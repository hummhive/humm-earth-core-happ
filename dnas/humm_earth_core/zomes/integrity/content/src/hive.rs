//! Validated hive-membership infrastructure.
//!
//! Closes H-1 cryptographically by establishing a per-hive root of trust:
//! every hive is anchored to a [`HiveGenesis`] entry whose **action hash**
//! is the unforgeable hive identity. Authorship of any hive-scoped data is
//! validated against either:
//!
//! 1. The hive author's own pubkey matching `HiveGenesis.action.author`
//!    (implicit Owner role — no membership entry required), OR
//! 2. A valid [`HiveMembership`] entry granting the agent a role in the
//!    hive, where the membership's own grantor must be either the genesis
//!    author or an Admin/Owner membership-holder for the same hive
//!    (chain-walked inductive validation, Moss-style).
//!
//! ## Why genesis action-hash (not DNA properties)
//!
//! Moss's `progenitor` pattern bakes a single root-of-trust pubkey into
//! `DnaModifiers.properties`. That requires ONE DNA PER HIVE because the
//! DNA hash includes `properties`. The Humm architecture is the opposite:
//! ONE shared DNA, N hives identified at runtime. To get the same
//! cryptographic identity without forcing per-hive DNAs, we anchor each
//! hive to a `HiveGenesis` entry. The action hash of that entry is
//! cryptographically bound to `(author, prev_action, entry_hash,
//! timestamp)` — just as unforgeable as a DNA-properties root, but
//! per-hive within a shared DNA.
//!
//! ## Validation cost
//!
//! Each authority check costs at most TWO `must_get_valid_record` calls:
//! one for the genesis (to identify the implicit Owner), one for the
//! claimed membership entry. The membership entry's own validation
//! recursively walks at most one more level (the grantor's membership),
//! and so on — but that walk happens at COMMIT time, not at consumer
//! validate time. A consumer validating "Alice writes content under hive
//! H" pays exactly: `must_get_valid_record(H)` + `must_get_valid_record(
//! alice_membership_in_H)`. O(1) regardless of grant-chain depth.

use crate::group::link_authors_target_entry;
use hdi::hash_path::path::Component;
use hdi::prelude::*;

/// The per-hive root-of-trust entry. Any agent may commit one to
/// establish a new hive; the entry's **action hash** then serves as the
/// hive's cryptographic identity throughout the DNA.
///
/// Immutable: `validate_update_hive_genesis` and
/// `validate_delete_hive_genesis` both return `Invalid`. To deprecate a
/// hive, stop granting memberships; existing data stays valid on the
/// DHT but the social graph withers.
///
/// ## Fields
///
/// - `display_id` — human-readable alias surfaced in UI ("Acme Corp",
///   "My DMs", or for migration: the old squuid `hive_id` string). NEVER
///   used by validators for security; routing/discovery only.
/// - `created_at_microseconds` — informational only. Validators do not
///   compare it against `action.timestamp` (the action timestamp is
///   already authoritative; this field exists for UI ordering when the
///   action is not in scope).
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct HiveGenesis {
    pub display_id: String,
    pub created_at_microseconds: i64,
}

/// Role granted by a membership entry. Ordered such that
/// `Owner > Admin > Writer > Reader` for permission containment
/// (`role_satisfies` enforces this).
///
/// Shared across the hive layer ([`HiveMembership`]) and the group
/// layer ([`crate::group::GroupMembership`]); matches the humm-tauri
/// `AclRole = owner|admin|writer|reader` vocabulary 1:1.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Owner,
    Admin,
    Writer,
    Reader,
}

/// Pass-2 compatibility alias. Identical to [`Role`] in every respect
/// (same variants, same serialization); retained so existing
/// `HiveRole` references across the integrity + coordinator crates
/// resolve unchanged after the rename.
pub use self::Role as HiveRole;

/// A role grant. Mirrors Moss `StewardPermission`: every grant carries a
/// reference to the grantor's own authorising membership (or `None` if
/// the grantor IS the genesis author), and validation walks one level
/// up the grant chain on every commit.
///
/// ## Immutability + revocation model
///
/// Updates and deletes both return `Invalid`. Revocation is via
/// `expiry: Some(ts)`: once `Timestamp::now() > expiry`, every consumer
/// validator that checks the membership returns `Invalid`. To revoke
/// permanently, set `expiry` to a past timestamp on the next grant
/// (effectively no-op grant + the prior membership ages out). To grant
/// a different role to the same agent, issue a fresh `HiveMembership`
/// with the new role — consumers use the most-recent valid one.
///
/// ## Fields
///
/// - `hive_genesis_hash` — the hive this grant applies in.
/// - `for_agent` — the grantee.
/// - `role` — `Owner` / `Admin` / `Writer` / `Reader`.
/// - `grantor_membership_hash` — `None` means `action.author` IS the
///   genesis author (no membership entry required); `Some(hash)` means
///   the validator must `must_get_valid_record(hash)` to fetch the
///   grantor's authorising membership.
/// - `expiry` — `None` = permanent; `Some(ts)` = invalid past this
///   timestamp. Mirrors Moss `permission_duration_until`.
/// - `grantor_owner_accept_hash` — pass-5; for `Admin` grants, cites the
///   grantor's owner-accept proving lineage ownership.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct HiveMembership {
    pub hive_genesis_hash: ActionHash,
    pub for_agent: AgentPubKey,
    pub role: Role,
    pub grantor_membership_hash: Option<ActionHash>,
    pub expiry: Option<Timestamp>,
    // serde(default): pass-4 HiveMembership wire data predates this field.
    #[serde(default)]
    pub grantor_owner_accept_hash: Option<ActionHash>,
}

/// Permission containment: does `held` satisfy `required`? Higher roles
/// satisfy lower ones; equal satisfies equal. The order
/// `Owner > Admin > Writer > Reader` is hard-coded here; this function
/// is THE single source of truth for the ordering.
pub fn role_satisfies(held: Role, required: Role) -> bool {
    fn rank(r: Role) -> u8 {
        match r {
            Role::Owner => 4,
            Role::Admin => 3,
            Role::Writer => 2,
            Role::Reader => 1,
        }
    }
    rank(held) >= rank(required)
}

/// Fetch + decode an entry by action hash into `(author, entry)`, with an
/// explicit "wrong entry type" message (`type_label`) rather than a bare
/// deserialization error. Shared by the typed fetchers below.
fn fetch_authored_entry<T: TryFrom<SerializedBytes, Error = SerializedBytesError>>(
    action_hash: &ActionHash,
    type_label: &str,
) -> ExternResult<(AgentPubKey, T)> {
    let record = must_get_valid_record(action_hash.clone())?;
    let author = record.action().author().clone();
    let entry = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "{action_hash} does not reference a {type_label} entry"
            )))
        })?;
    Ok((author, entry))
}

/// `(genesis_author, genesis_entry)` for a [`HiveGenesis`] action hash.
pub fn fetch_genesis(genesis_hash: &ActionHash) -> ExternResult<(AgentPubKey, HiveGenesis)> {
    fetch_authored_entry(genesis_hash, "HiveGenesis")
}

/// `(membership_author, membership_entry)` for a [`HiveMembership`] action hash.
pub fn fetch_membership(
    membership_hash: &ActionHash,
) -> ExternResult<(AgentPubKey, HiveMembership)> {
    fetch_authored_entry(membership_hash, "HiveMembership")
}

/// Verify `agent` holds at least `required_role` for `genesis_hash` at
/// `timestamp`, where the claim is backed either by:
///
/// - `membership_hash.is_none()` AND `agent` IS the genesis author
///   (implicit Owner), OR
/// - `membership_hash.is_some()` AND that membership grants `agent` a
///   role >= `required_role`, is for the correct hive, and has not
///   expired at `timestamp`.
///
/// Returns `Valid` on success and a contextual `Invalid` on every
/// failure branch. Caller is expected to short-circuit on `Invalid` /
/// propagate `Err`.
pub fn check_hive_authority(
    agent: &AgentPubKey,
    genesis_hash: &ActionHash,
    membership_hash: Option<&ActionHash>,
    required_role: Role,
    timestamp: &Timestamp,
) -> ExternResult<ValidateCallbackResult> {
    // Always anchor to the genesis. This catches "genesis_hash points at
    // a non-HiveGenesis entry" cleanly before we look at anything else.
    let (genesis_author, _genesis) = fetch_genesis(genesis_hash)?;

    // Path 1: implicit-Owner. The genesis author always has Owner role,
    // no membership entry needed.
    if &genesis_author == agent {
        return Ok(ValidateCallbackResult::Valid);
    }

    // Path 2: explicit-membership. Caller must supply a membership hash;
    // we fetch it, verify it's for the right hive + right grantee, and
    // check role + expiry.
    let Some(hash) = membership_hash else {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "agent {agent} is not the genesis author of {genesis_hash} \
             and supplied no authorising HiveMembership",
        )));
    };
    let (_membership_author, membership) = fetch_membership(hash)?;
    if &membership.hive_genesis_hash != genesis_hash {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "membership {hash} is for hive {} but caller claimed hive {genesis_hash}",
            membership.hive_genesis_hash,
        )));
    }
    if &membership.for_agent != agent {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "membership {hash} grants role to {} but action author is {agent}",
            membership.for_agent,
        )));
    }
    if let Some(expiry) = membership.expiry {
        if timestamp > &expiry {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "membership {hash} expired at {expiry:?}; action timestamp {timestamp:?}",
            )));
        }
    }
    if !role_satisfies(membership.role, required_role) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "membership {hash} grants role {:?}, required {:?}",
            membership.role, required_role,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

// =============================================================================
// HiveGenesis validators
// =============================================================================

/// A [`HiveGenesis`] entry is permissionless: any agent may found a hive.
/// The cryptographic identity is the action hash, which inherently binds
/// authorship — no further validation is needed at create time.
pub fn validate_create_hive_genesis(
    _action: EntryCreationAction,
    _entry: HiveGenesis,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}

/// HiveGenesis is immutable. Updating one would let the genesis author
/// retroactively redefine `display_id`; the hive identity (action hash)
/// would survive but its display would shift in a confusing way.
pub fn validate_update_hive_genesis(
    _action: Update,
    _entry: HiveGenesis,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "HiveGenesis entries are immutable; create a new hive instead".into(),
    ))
}

/// HiveGenesis is non-deletable. Deletion would orphan every membership
/// + content entry rooted in it.
pub fn validate_delete_hive_genesis(
    _action: Delete,
    _original_action: EntryCreationAction,
    _original_entry: HiveGenesis,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "HiveGenesis entries cannot be deleted".into(),
    ))
}

// =============================================================================
// HiveMembership validators
// =============================================================================

/// Validate a [`HiveMembership`] commit. The grantor (= `action.author`)
/// must:
///
/// 1. NOT be granting to themselves (no self-grants — bootstrap path is
///    via `genesis_author` implicit-Owner). This blocks the trivial
///    "Mallory commits a HiveMembership giving herself Admin in bob's
///    hive" forge.
/// 2. Either be the hive's genesis author, OR hold an Admin/Owner
///    HiveMembership in the same hive (chain-walked via
///    `check_hive_authority` with `required_role = Admin`).
/// 3. NOT grant a role HIGHER than their own (Owner is the only role
///    that can grant Owner; Admin can grant Admin/Writer/Reader; etc.).
/// 4. **G-4.4 grant-window containment (pass-4 back-port).** If the
///    grantor proved authority via an *expiring* hive membership
///    (Path 2), the new membership must itself expire no later than
///    the grantor's. Closes the parallel delegation-window-extension
///    attack at the hive layer (matrix #10 hive-layer analogue;
///    pass-3's group-layer rule G-4.4 is mirrored here).
pub fn validate_create_hive_membership(
    action: EntryCreationAction,
    membership: HiveMembership,
) -> ExternResult<ValidateCallbackResult> {
    let grantor = action.author();
    let timestamp = action.timestamp();

    // Rule 1 — no self-grants. The only way to GET a role is to be
    // granted by someone with that authority (or to be the genesis
    // author, who is implicitly Owner).
    if grantor == &membership.for_agent {
        return Ok(ValidateCallbackResult::Invalid(
            "self-grant is prohibited; the grantor cannot be the grantee".into(),
        ));
    }

    // Rule 2 — grantor must have Admin+ authority in the hive.
    let grantor_check = check_hive_authority(
        grantor,
        &membership.hive_genesis_hash,
        membership.grantor_membership_hash.as_ref(),
        Role::Admin,
        timestamp,
    )?;
    if !matches!(grantor_check, ValidateCallbackResult::Valid) {
        return Ok(grantor_check);
    }

    // Rule 3 — Owner is never membership-grantable (only the handoff
    // handshake confers it); only a lineage owner may grant Admin.
    match membership.role {
        Role::Owner => {
            return Ok(ValidateCallbackResult::Invalid(
                "the Owner role cannot be granted via membership; use the owner-handoff handshake"
                    .into(),
            ));
        }
        Role::Admin => {
            if !is_lineage_owner(
                grantor,
                &membership.hive_genesis_hash,
                membership.grantor_owner_accept_hash.as_ref(),
            )? {
                return Ok(ValidateCallbackResult::Invalid(
                    "only the hive Owner may grant the Admin role".into(),
                ));
            }
        }
        Role::Writer | Role::Reader => {}
    }

    let (genesis_author, _) = fetch_genesis(&membership.hive_genesis_hash)?;
    if membership.for_agent == genesis_author {
        return Ok(ValidateCallbackResult::Invalid(
            "cannot assign a membership role to the hive's founding owner".into(),
        ));
    }

    // Rule 4 — grant-window containment (G-4.4 hive-layer back-port).
    // Mirrors the pass-3 group-layer enforce_grant_window. The
    // hive-layer flavour drops the `timestamp` parameter — the only
    // re-verification needed (grantor-IS-genesis-author) is
    // timestamp-independent, unlike the group version's Path-B
    // re-verification which calls check_hive_authority with the
    // action timestamp.
    enforce_hive_grant_window(grantor, &membership)
}

/// G-4.4 (hive-layer back-port) — grant-window containment. If the
/// grantor's authority for THIS grant rests on an *expiring* hive
/// membership (Path 2), the new membership must itself expire no
/// later than the grantor's window. Closes the
/// delegation-window-extension attack at the hive layer.
///
/// The constraint applies ONLY when Path 2 is the grantor's actual
/// authority basis. A grantor who could prove authority via the
/// hive-genesis-author route (Path 1) is unconstrained — they may
/// mint permanent hive memberships even while personally carrying an
/// (irrelevant) expiring witness.
///
/// Path attribution rule (mirrors the hardened group version):
/// - No `grantor_membership_hash` → Path 1 (the grantor IS the
///   genesis author; Rule 2's check_hive_authority validated that).
///   Unconstrained.
/// - `grantor_membership_hash` present but witness is for a
///   different agent / different hive → witness does NOT back this
///   grantor's Path-2 authority; Path 1 was the actual basis.
///   Unconstrained.
/// - `grantor_membership_hash` present AND witness backs Path 2 AND
///   the grantor is independently the hive genesis author → Path 1
///   was ALSO viable, and Owner dominates the expiring Path-2
///   witness. Unconstrained.
/// - Otherwise → Path 2 is the only basis; the window must be
///   contained.
///
/// The Path-1 re-verification re-fetches the hive genesis. The
/// conductor's validation-package cache deduplicates the underlying
/// `must_get_valid_record` against Rule 2's earlier fetch.
///
/// Note: no `timestamp` parameter — unlike the group analogue
/// (`crate::group::enforce_grant_window`), this routine performs no
/// timestamp-bearing authority checks. The grantor's witness expiry
/// was already validated against the action timestamp by Rule 2's
/// `check_hive_authority`; here we only compare grant windows
/// (witness expiry vs new-grant expiry), both of which are
/// timestamps-as-data on entries.
fn enforce_hive_grant_window(
    grantor: &AgentPubKey,
    membership: &HiveMembership,
) -> ExternResult<ValidateCallbackResult> {
    let Some(grantor_hash) = membership.grantor_membership_hash.as_ref() else {
        // Path 1 grantor — no membership window to contain.
        return Ok(ValidateCallbackResult::Valid);
    };
    let (_witness_author, grantor_membership) = fetch_membership(grantor_hash)?;
    if &grantor_membership.for_agent != grantor
        || grantor_membership.hive_genesis_hash != membership.hive_genesis_hash
    {
        // Witness does not back this grantor's Path-2 authority for
        // this hive; their authority must have come via Path 1.
        return Ok(ValidateCallbackResult::Valid);
    }
    // Path-1 re-verification: even if the witness backs Path 2, the
    // grantor may ALSO be the hive genesis author (Path 1 viable);
    // their permanent Owner role dominates the expiring Path-2
    // witness and the window is unconstrained.
    let (genesis_author, _) = fetch_genesis(&membership.hive_genesis_hash)?;
    if &genesis_author == grantor {
        return Ok(ValidateCallbackResult::Valid);
    }
    let Some(grantor_expiry) = grantor_membership.expiry else {
        // Permanent Path-2 authority — unconstrained.
        return Ok(ValidateCallbackResult::Valid);
    };
    match membership.expiry {
        Some(new_expiry) if new_expiry <= grantor_expiry => Ok(ValidateCallbackResult::Valid),
        Some(new_expiry) => Ok(ValidateCallbackResult::Invalid(format!(
            "granted expiry {new_expiry:?} exceeds the grantor membership's \
             expiry {grantor_expiry:?}; an expiring grantor may not extend \
             the delegation window",
        ))),
        None => Ok(ValidateCallbackResult::Invalid(
            "an expiring grantor may not mint a permanent (no-expiry) \
             membership"
                .into(),
        )),
    }
}

/// HiveMembership entries are immutable. Role changes happen by issuing
/// a fresh membership (consumers read the most-recent valid one);
/// revocation happens via `expiry` set at create time.
pub fn validate_update_hive_membership(
    _action: Update,
    _entry: HiveMembership,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "HiveMembership entries are immutable; issue a new membership instead".into(),
    ))
}

/// HiveMembership entries cannot be deleted; revocation is via expiry.
/// Deletion would create a window where consumers who fetched an entry
/// pre-deletion still trust it while post-deletion fetchers don't.
pub fn validate_delete_hive_membership(
    _action: Delete,
    _original_action: EntryCreationAction,
    _original_entry: HiveMembership,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "HiveMembership entries cannot be deleted; use the `expiry` field at create time".into(),
    ))
}

// =============================================================================
// Path-recompute helpers — used by link validators (hive/dynamic/ACL).
// =============================================================================

/// Recompute the hash of a path constructed from string components.
///
/// Used by link validators to verify the link's claimed `base_address`
/// matches the path the link CLAIMS to be off of, after recomputing that
/// path from validated data (the target entry's header fields).
pub fn recompute_path_hash(components: &[&str]) -> ExternResult<AnyLinkableHash> {
    let path = Path::from(
        components
            .iter()
            .map(|c| Component::from(*c))
            .collect::<Vec<_>>(),
    );
    Ok(path.path_entry_hash()?.into())
}

// =============================================================================
// Hive ownership: single-owner, transferred by offer/accept handshake (pass-5)
// =============================================================================

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct HiveOwnerHandoffOffer {
    pub hive_genesis_hash: ActionHash,
    pub to_agent: AgentPubKey,
    pub offerer_owner_accept_hash: Option<ActionHash>,
    pub created_at_microseconds: i64,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct HiveOwnerHandoffAccept {
    pub offer_hash: ActionHash,
}

fn fetch_owner_handoff_offer(
    offer_hash: &ActionHash,
) -> ExternResult<(AgentPubKey, HiveOwnerHandoffOffer)> {
    fetch_authored_entry(offer_hash, "HiveOwnerHandoffOffer")
}

fn fetch_owner_handoff_accept(
    accept_hash: &ActionHash,
) -> ExternResult<(AgentPubKey, HiveOwnerHandoffAccept)> {
    fetch_authored_entry(accept_hash, "HiveOwnerHandoffAccept")
}

/// EVER-owner, not current-owner: a validator cannot detect a completed
/// downstream transfer without forbidden link-enumeration, so this proves
/// only that `agent` is the genesis root or a past handoff recipient. The
/// coordinator's resolve_current_owner folds the lineage to the live owner.
pub fn is_lineage_owner(
    agent: &AgentPubKey,
    genesis_hash: &ActionHash,
    owner_accept_hash: Option<&ActionHash>,
) -> ExternResult<bool> {
    let Some(accept_hash) = owner_accept_hash else {
        let (genesis_author, _) = fetch_genesis(genesis_hash)?;
        return Ok(&genesis_author == agent);
    };
    let (_, accept) = fetch_owner_handoff_accept(accept_hash)?;
    let (_, offer) = fetch_owner_handoff_offer(&accept.offer_hash)?;
    Ok(&offer.to_agent == agent && &offer.hive_genesis_hash == genesis_hash)
}

pub fn validate_create_hive_owner_handoff_offer(
    action: EntryCreationAction,
    offer: HiveOwnerHandoffOffer,
) -> ExternResult<ValidateCallbackResult> {
    let offerer = action.author();
    if &offer.to_agent == offerer {
        return Ok(ValidateCallbackResult::Invalid(
            "cannot hand off ownership to yourself".into(),
        ));
    }
    let offerer_is_owner = is_lineage_owner(
        offerer,
        &offer.hive_genesis_hash,
        offer.offerer_owner_accept_hash.as_ref(),
    )?;
    if !offerer_is_owner {
        return Ok(ValidateCallbackResult::Invalid(
            "offer author is not an owner of the hive".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_update_hive_owner_handoff_offer(
    _action: Update,
    _entry: HiveOwnerHandoffOffer,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "HiveOwnerHandoffOffer entries are immutable".into(),
    ))
}

pub fn validate_delete_hive_owner_handoff_offer(
    _action: Delete,
    _original_action: EntryCreationAction,
    _original_entry: HiveOwnerHandoffOffer,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "HiveOwnerHandoffOffer is immutable; cancel a pending offer by deleting its AgentToOwnerHandoffs link"
            .into(),
    ))
}

pub fn validate_create_hive_owner_handoff_accept(
    action: EntryCreationAction,
    accept: HiveOwnerHandoffAccept,
) -> ExternResult<ValidateCallbackResult> {
    // No one-accept-per-offer check: forks are tolerated and the coordinator
    // de-duplicates by offer hash when resolving the current owner.
    let (_, offer) = fetch_owner_handoff_offer(&accept.offer_hash)?;
    if &offer.to_agent != action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "accept author is not the offer's to_agent".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_update_hive_owner_handoff_accept(
    _action: Update,
    _entry: HiveOwnerHandoffAccept,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "HiveOwnerHandoffAccept entries are immutable".into(),
    ))
}

pub fn validate_delete_hive_owner_handoff_accept(
    _action: Delete,
    _original_action: EntryCreationAction,
    _original_entry: HiveOwnerHandoffAccept,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "HiveOwnerHandoffAccept entries cannot be deleted".into(),
    ))
}

pub fn validate_create_link_agent_to_owner_handoffs(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    if !tag.0.is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "AgentToOwnerHandoffs link tag must be empty".into(),
        ));
    }
    let offer: HiveOwnerHandoffOffer = match link_authors_target_entry(&action, &target_address)? {
        Ok(offer) => offer,
        Err(invalid) => return Ok(invalid),
    };
    if base_address != AnyLinkableHash::from(offer.to_agent.clone()) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "AgentToOwnerHandoffs base {base_address} does not match offer.to_agent {}",
            offer.to_agent,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_link_hive_to_owner_handoffs(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    if !tag.0.is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "HiveToOwnerHandoffs link tag must be empty".into(),
        ));
    }
    let accept: HiveOwnerHandoffAccept = match link_authors_target_entry(&action, &target_address)?
    {
        Ok(accept) => accept,
        Err(invalid) => return Ok(invalid),
    };
    let (_, offer) = fetch_owner_handoff_offer(&accept.offer_hash)?;
    if base_address != AnyLinkableHash::from(offer.hive_genesis_hash.clone()) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveToOwnerHandoffs base {base_address} does not match offer.hive_genesis_hash {}",
            offer.hive_genesis_hash,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_pubkey(byte: u8) -> AgentPubKey {
        AgentPubKey::from_raw_36(vec![byte; 36])
    }

    #[test]
    fn role_satisfies_diagonal_and_below() {
        // Owner satisfies everything.
        assert!(role_satisfies(HiveRole::Owner, HiveRole::Owner));
        assert!(role_satisfies(HiveRole::Owner, HiveRole::Admin));
        assert!(role_satisfies(HiveRole::Owner, HiveRole::Writer));
        assert!(role_satisfies(HiveRole::Owner, HiveRole::Reader));
        // Admin satisfies Admin and below.
        assert!(!role_satisfies(HiveRole::Admin, HiveRole::Owner));
        assert!(role_satisfies(HiveRole::Admin, HiveRole::Admin));
        assert!(role_satisfies(HiveRole::Admin, HiveRole::Writer));
        assert!(role_satisfies(HiveRole::Admin, HiveRole::Reader));
        // Writer satisfies Writer and below.
        assert!(!role_satisfies(HiveRole::Writer, HiveRole::Owner));
        assert!(!role_satisfies(HiveRole::Writer, HiveRole::Admin));
        assert!(role_satisfies(HiveRole::Writer, HiveRole::Writer));
        assert!(role_satisfies(HiveRole::Writer, HiveRole::Reader));
        // Reader satisfies only Reader.
        assert!(!role_satisfies(HiveRole::Reader, HiveRole::Owner));
        assert!(!role_satisfies(HiveRole::Reader, HiveRole::Admin));
        assert!(!role_satisfies(HiveRole::Reader, HiveRole::Writer));
        assert!(role_satisfies(HiveRole::Reader, HiveRole::Reader));
    }

    #[test]
    fn recompute_path_hash_matches_path_entry_hash() {
        // Sanity-pin: recompute_path_hash must agree with the same Path
        // constructed manually, otherwise every link validator's
        // recompute check would silently disagree with the writer's
        // path construction.
        let manual = Path::from(vec![
            Component::from("hive-x"),
            Component::from("content-y"),
        ]);
        let manual_hash: AnyLinkableHash = manual
            .path_entry_hash()
            .expect("manual path hash should compute in test")
            .into();
        let recomputed = recompute_path_hash(&["hive-x", "content-y"])
            .expect("recompute_path_hash should compute in test");
        assert_eq!(manual_hash, recomputed);
    }

    #[test]
    fn agent_pubkey_helper_constructs_consistent_pubkeys() {
        // Trivial guard: any change to the test helper would silently
        // break every other test in this module that relies on stable
        // pubkey identity across calls.
        let alice = agent_pubkey(1);
        let alice_again = agent_pubkey(1);
        let bob = agent_pubkey(2);
        assert_eq!(alice, alice_again);
        assert_ne!(alice, bob);
    }

    fn action_hash(byte: u8) -> ActionHash {
        ActionHash::from_raw_36(vec![byte; 36])
    }

    fn entry_hash(byte: u8) -> EntryHash {
        EntryHash::from_raw_36(vec![byte; 36])
    }

    fn make_create(author: AgentPubKey) -> Create {
        Create {
            author,
            timestamp: Timestamp(0),
            action_seq: 0,
            prev_action: action_hash(0),
            entry_type: EntryType::App(AppEntryDef {
                entry_index: 0.into(),
                zome_index: 0.into(),
                visibility: EntryVisibility::Public,
            }),
            entry_hash: entry_hash(0),
            weight: Default::default(),
        }
    }

    fn make_update(author: AgentPubKey) -> Update {
        Update {
            author,
            timestamp: Timestamp(0),
            action_seq: 1,
            prev_action: action_hash(0),
            original_action_address: action_hash(7),
            original_entry_address: entry_hash(7),
            entry_type: EntryType::App(AppEntryDef {
                entry_index: 0.into(),
                zome_index: 0.into(),
                visibility: EntryVisibility::Public,
            }),
            entry_hash: entry_hash(8),
            weight: Default::default(),
        }
    }

    fn make_delete(author: AgentPubKey) -> Delete {
        Delete {
            author,
            timestamp: Timestamp(0),
            action_seq: 1,
            prev_action: action_hash(0),
            deletes_address: action_hash(7),
            deletes_entry_address: entry_hash(7),
            weight: Default::default(),
        }
    }

    fn sample_genesis() -> HiveGenesis {
        HiveGenesis {
            display_id: "hive-test".into(),
            created_at_microseconds: 0,
        }
    }

    fn sample_membership(
        for_agent: AgentPubKey,
        role: HiveRole,
        grantor_membership_hash: Option<ActionHash>,
        expiry: Option<Timestamp>,
    ) -> HiveMembership {
        HiveMembership {
            hive_genesis_hash: action_hash(9),
            for_agent,
            role,
            grantor_membership_hash,
            expiry,
            grantor_owner_accept_hash: None,
        }
    }

    // -----------------------------------------------------------------
    // HiveGenesis immutability — update and delete unconditionally
    // reject.
    // -----------------------------------------------------------------

    #[test]
    fn hive_genesis_update_is_invalid() {
        let alice = agent_pubkey(1);
        let result = validate_update_hive_genesis(make_update(alice), sample_genesis())
            .expect("validator should not error in test");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.to_lowercase().contains("immutable"));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn hive_genesis_delete_is_invalid() {
        let alice = agent_pubkey(1);
        let original = EntryCreationAction::Create(make_create(alice.clone()));
        let result = validate_delete_hive_genesis(make_delete(alice), original, sample_genesis())
            .expect("validator should not error in test");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.to_lowercase().contains("cannot be deleted"));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------
    // HiveMembership immutability — update and delete unconditionally
    // reject.
    // -----------------------------------------------------------------

    #[test]
    fn hive_membership_update_is_invalid() {
        let alice = agent_pubkey(1);
        let result = validate_update_hive_membership(
            make_update(alice.clone()),
            sample_membership(alice, HiveRole::Writer, None, None),
        )
        .expect("validator should not error in test");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.to_lowercase().contains("immutable"));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn hive_membership_delete_is_invalid() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let original = EntryCreationAction::Create(make_create(alice.clone()));
        let result = validate_delete_hive_membership(
            make_delete(alice),
            original,
            sample_membership(bob, HiveRole::Writer, None, None),
        )
        .expect("validator should not error in test");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.to_lowercase().contains("cannot be deleted"));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------
    // HiveMembership create — Rule 1 (no self-grant). Reachable
    // host-side because the self-grant short-circuit fires BEFORE any
    // `must_get_valid_record` chain walk.
    // -----------------------------------------------------------------

    #[test]
    fn hive_membership_self_grant_is_invalid() {
        let alice = agent_pubkey(1);
        let action = EntryCreationAction::Create(make_create(alice.clone()));
        // Alice grants Alice a role — self-grant. No chain walk needed
        // to reject; rule 1 fires immediately.
        let membership = sample_membership(alice, HiveRole::Writer, None, None);
        let result = validate_create_hive_membership(action, membership)
            .expect("validator should not error before chain walk");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.to_lowercase().contains("self-grant"));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn hive_membership_self_grant_invalid_regardless_of_role() {
        // Confirm Rule 1 fires across every role variant — guards
        // against a future refactor that conditions the self-grant
        // check on role.
        let alice = agent_pubkey(1);
        for role in [
            HiveRole::Owner,
            HiveRole::Admin,
            HiveRole::Writer,
            HiveRole::Reader,
        ] {
            let action = EntryCreationAction::Create(make_create(alice.clone()));
            let membership = sample_membership(alice.clone(), role, None, None);
            let result = validate_create_hive_membership(action, membership)
                .expect("validator should not error before chain walk");
            assert!(
                matches!(result, ValidateCallbackResult::Invalid(_)),
                "self-grant of {role:?} must be Invalid; got {result:?}",
            );
        }
    }

    // -----------------------------------------------------------------
    // Pass-4 — G-4.4 hive grant-window back-port (pre-fetch branch).
    //
    // The no-witness fast path (grantor relied on the hive-genesis-
    // author Path 1) is host-reachable: enforce_hive_grant_window
    // returns Valid without any fetch. The fetch-dependent branches
    // (witness-backed Path-2 expiry containment, Path-1 re-verification
    // when both witnesses are present) require a live conductor and
    // are covered by Tryorama (`tryorama-grant-window.test.ts`).
    // -----------------------------------------------------------------

    #[test]
    fn hive_grant_window_unconstrained_without_grantor_membership() {
        // No grantor_membership_hash → Path 1 grantor (genesis author);
        // enforce_hive_grant_window short-circuits to Valid without
        // any fetch. Even an expiring grant is permitted because the
        // grantor's permanent Owner role dominates.
        let bob = agent_pubkey(2);
        let membership = HiveMembership {
            hive_genesis_hash: action_hash(9),
            for_agent: bob,
            role: HiveRole::Writer,
            grantor_membership_hash: None,
            expiry: Some(Timestamp(1_000)),
            grantor_owner_accept_hash: None,
        };
        let result = enforce_hive_grant_window(&agent_pubkey(3), &membership)
            .expect("Path 1 short-circuit requires no fetch");
        assert!(
            matches!(result, ValidateCallbackResult::Valid),
            "expected Valid, got {result:?}",
        );
    }

    #[test]
    fn hive_grant_window_unconstrained_for_permanent_new_grant_when_no_witness() {
        // Same fast path with `expiry: None` on the new grant —
        // permanent grant from a Path-1 grantor is the canonical
        // bootstrap pattern (the hive owner mints a permanent member).
        // enforce_hive_grant_window must accept regardless of what
        // `expiry` value the new grant carries when the witness is
        // None.
        let bob = agent_pubkey(2);
        let membership = HiveMembership {
            hive_genesis_hash: action_hash(9),
            for_agent: bob,
            role: HiveRole::Owner,
            grantor_membership_hash: None,
            expiry: None,
            grantor_owner_accept_hash: None,
        };
        let result = enforce_hive_grant_window(&agent_pubkey(3), &membership)
            .expect("Path 1 short-circuit requires no fetch");
        assert!(
            matches!(result, ValidateCallbackResult::Valid),
            "expected Valid, got {result:?}",
        );
    }
}
