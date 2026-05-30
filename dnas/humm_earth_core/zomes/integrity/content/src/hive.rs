//! Validated hive-membership infrastructure (pass-2 I-H).
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

/// Role granted by a [`HiveMembership`]. Ordered such that
/// `Owner > Admin > Writer > Reader` for permission containment
/// (`role_satisfies` enforces this).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum HiveRole {
    Owner,
    Admin,
    Writer,
    Reader,
}

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
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct HiveMembership {
    pub hive_genesis_hash: ActionHash,
    pub for_agent: AgentPubKey,
    pub role: HiveRole,
    pub grantor_membership_hash: Option<ActionHash>,
    pub expiry: Option<Timestamp>,
}

/// Permission containment: does `held` satisfy `required`? Higher roles
/// satisfy lower ones; equal satisfies equal. The order
/// `Owner > Admin > Writer > Reader` is hard-coded here; this function
/// is THE single source of truth for the ordering.
pub fn role_satisfies(held: HiveRole, required: HiveRole) -> bool {
    fn rank(r: HiveRole) -> u8 {
        match r {
            HiveRole::Owner => 4,
            HiveRole::Admin => 3,
            HiveRole::Writer => 2,
            HiveRole::Reader => 1,
        }
    }
    rank(held) >= rank(required)
}

/// Fetch and decode a [`HiveGenesis`] entry by action hash, returning
/// `(genesis_author, genesis_entry)`. Wraps `must_get_valid_record` +
/// a typed `to_app_option` decode with explicit error context so the
/// "wrong entry type at that hash" failure mode produces a useful
/// validation message instead of a generic deserialization error.
pub fn fetch_genesis(
    genesis_hash: &ActionHash,
) -> ExternResult<(AgentPubKey, HiveGenesis)> {
    let record = must_get_valid_record(genesis_hash.clone())?;
    let author = record.action().author().clone();
    let entry: HiveGenesis = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "hive_genesis_hash {genesis_hash} does not reference a HiveGenesis entry",
            )))
        })?;
    Ok((author, entry))
}

/// Fetch and decode a [`HiveMembership`] entry by action hash, returning
/// `(membership_author, membership_entry)`.
pub fn fetch_membership(
    membership_hash: &ActionHash,
) -> ExternResult<(AgentPubKey, HiveMembership)> {
    let record = must_get_valid_record(membership_hash.clone())?;
    let author = record.action().author().clone();
    let entry: HiveMembership = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "membership_hash {membership_hash} does not reference a HiveMembership entry",
            )))
        })?;
    Ok((author, entry))
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
    required_role: HiveRole,
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
        HiveRole::Admin,
        timestamp,
    )?;
    if !matches!(grantor_check, ValidateCallbackResult::Valid) {
        return Ok(grantor_check);
    }

    // Rule 3 — grantor cannot grant a role higher than their own. We
    // already know the grantor satisfies Admin; if they're attempting
    // to grant Owner, they must themselves be Owner.
    if matches!(membership.role, HiveRole::Owner) {
        let owner_check = check_hive_authority(
            grantor,
            &membership.hive_genesis_hash,
            membership.grantor_membership_hash.as_ref(),
            HiveRole::Owner,
            timestamp,
        )?;
        if !matches!(owner_check, ValidateCallbackResult::Valid) {
            return Ok(ValidateCallbackResult::Invalid(
                "only an Owner may grant the Owner role".into(),
            ));
        }
    }

    Ok(ValidateCallbackResult::Valid)
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
}
