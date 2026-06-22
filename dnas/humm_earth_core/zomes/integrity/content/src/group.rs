//! Validated group-membership infrastructure.
//!
//! Pushes the pass-2 hive-authority pattern one level down: a **group**
//! gains a cryptographic identity ([`GroupGenesis`] action hash) and a
//! validated role-grant chain ([`GroupMembership`]), rooted — ultimately
//! — in the hive owner's keypair. This closes the group/role/ACL
//! poisoning class against the standard modified-coordinator adversary
//! for group-scoped content (see `encrypted_content::AclSpec::HiveGroup`).
//!
//! ## Sovereignty model
//!
//! Authority over a group is satisfied by ANY of three independent
//! routes ([`check_group_authority`]):
//!
//! - **Path A — group author.** The agent who committed the
//!   [`GroupGenesis`] is the implicit group Owner. No membership entry
//!   required.
//! - **Path B — hive sovereignty.** A hive Admin+ (per pass-2
//!   [`check_hive_authority`]) of the group's *parent hive* controls
//!   every group in that hive. This encodes Nick's "everything is chosen
//!   by the hive owner" — the hive genesis author (root sovereign) needs
//!   no membership of any kind. Reached either as the hive genesis
//!   author (implicit Owner) or via a supplied `HiveMembership` witness.
//! - **Path C — explicit group membership.** A valid, unexpired
//!   [`GroupMembership`] granting the agent ≥ the required role.
//!
//! ## Why witness hashes live on the entry (not just the input)
//!
//! Both entries carry the grantor's/creator's authorising membership
//! hashes (`creator_hive_membership_hash`, `grantor_membership_hash`,
//! `grantor_hive_membership_hash`). The integrity validator only ever
//! sees the entry + action; to re-walk the authority chain at commit
//! time it MUST be able to fetch the witness the author relied on, so
//! the witness has to be persisted on the entry, not merely passed to
//! the coordinator extern.
//!
//! ## Validation cost
//!
//! All cost is paid at COMMIT time, never at consumer read time. A
//! group-authority check costs at most: 1 fetch for the group genesis,
//! the pass-2 hive two-fetch bound if Path B is exercised, and 1 fetch
//! for the claimed group membership if Path C is reached — O(1)
//! regardless of grant-chain depth (each membership's own grantor was
//! validated when *it* was committed).

use hdi::prelude::*;

use crate::hive::{check_hive_authority, role_satisfies, Role};

/// The per-group root-of-trust entry. Its **action hash** is the group's
/// cryptographic identity throughout the DNA (mirroring [`HiveGenesis`]
/// at the hive level).
///
/// Immutable: update + delete both reject. To deprecate a group, stop
/// granting memberships and let existing ones expire.
///
/// ## Fields
///
/// - `hive_genesis_hash` — the parent hive. Binds the group into a hive
///   trust domain so the hive owner is sovereign over it. MUST resolve
///   to a real [`HiveGenesis`].
/// - `display_id` — human alias = the legacy humm-tauri group squuid
///   (continuity). NEVER security-load-bearing; routing/display only.
/// - `hive_wide_role` — `Some(role)` marks a hive-wide *system role
///   group* (the admin/writer/reader groups created at hive setup);
///   `None` marks an ordinary custom group. Load-bearing: a system role
///   group may only be created by the hive Owner; a custom group needs
///   hive Admin+.
/// - `creator_hive_membership_hash` — the creator's authorising
///   [`crate::hive::HiveMembership`] in `hive_genesis_hash`. `None` =
///   the creator IS the hive genesis author (implicit Owner). Persisted
///   so the create validator can re-walk hive authority.
/// - `created_at_microseconds` — informational only (UI ordering); not
///   compared against `action.timestamp` by any validator.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct GroupGenesis {
    pub hive_genesis_hash: ActionHash,
    pub display_id: String,
    pub hive_wide_role: Option<Role>,
    pub creator_hive_membership_hash: Option<ActionHash>,
    pub created_at_microseconds: i64,
}

/// A group role grant. Mirrors [`crate::hive::HiveMembership`] exactly,
/// plus the parent-hive witness needed for the hive-sovereign grant
/// route (Path B of [`check_group_authority`]).
///
/// Immutable; revocation is via `expiry` (issue a fresh membership for
/// the same agent with a past `expiry` — consumers use the most-recent
/// valid one). See [`crate::hive::HiveMembership`] for the rationale.
///
/// ## Fields
///
/// - `group_genesis_hash` — the group this grant applies in.
/// - `for_agent` — the grantee. May live in a *different hive* — the
///   field is just a holohash, which is what makes cross-hive group
///   membership (group chat across hives) representable.
/// - `role` — `Owner` / `Admin` / `Writer` / `Reader`.
/// - `grantor_membership_hash` — grantor's authorising
///   [`GroupMembership`] (Path C). `None` = grantor proved authority via
///   the group-author (Path A) or hive-sovereign (Path B) route.
/// - `grantor_hive_membership_hash` — grantor's authorising
///   [`crate::hive::HiveMembership`] in the group's parent hive (Path
///   B). `None` = not relying on a hive membership witness (uses Path A,
///   Path C, or the hive-genesis-author implicit-Owner case).
/// - `expiry` — `None` = permanent; `Some(ts)` = invalid once
///   `timestamp > ts`.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct GroupMembership {
    pub group_genesis_hash: ActionHash,
    pub for_agent: AgentPubKey,
    pub role: Role,
    pub grantor_membership_hash: Option<ActionHash>,
    pub grantor_hive_membership_hash: Option<ActionHash>,
    pub expiry: Option<Timestamp>,
}

/// Fetch and decode a [`GroupGenesis`] by action hash, returning
/// `(group_author, group)`. Mirrors [`crate::hive::fetch_genesis`].
pub fn fetch_group_genesis(
    group_genesis_hash: &ActionHash,
) -> ExternResult<(AgentPubKey, GroupGenesis)> {
    let record = must_get_valid_record(group_genesis_hash.clone())?;
    let author = record.action().author().clone();
    let entry: GroupGenesis = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "group_genesis_hash {group_genesis_hash} does not reference a GroupGenesis entry",
            )))
        })?;
    Ok((author, entry))
}

/// Fetch and decode a [`GroupMembership`] by action hash, returning
/// `(membership_author, membership)`.
pub fn fetch_group_membership(
    membership_hash: &ActionHash,
) -> ExternResult<(AgentPubKey, GroupMembership)> {
    let record = must_get_valid_record(membership_hash.clone())?;
    let author = record.action().author().clone();
    let entry: GroupMembership = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "membership_hash {membership_hash} does not reference a GroupMembership entry",
            )))
        })?;
    Ok((author, entry))
}

/// Verify `agent` holds at least `required_role` in the group at
/// `timestamp`, via Path A (group author), Path B (hive sovereignty),
/// or Path C (explicit group membership). Returns `Valid` on the first
/// satisfied route, a contextual `Invalid` if none hold, and propagates
/// host errors from the chain walk.
///
/// `membership_hash` backs Path C (the agent's own group membership);
/// `hive_membership_hash` backs Path B (the agent's hive membership in
/// the group's parent hive). Either may be `None`.
///
/// Evaluation order is A → B → C, matching the conceptual primacy of the
/// hive sovereign. Path B is attempted even when `hive_membership_hash`
/// is `None` because that is how the hive *genesis author* (the root
/// sovereign, who holds no membership entry) is recognised.
pub fn check_group_authority(
    agent: &AgentPubKey,
    group_genesis_hash: &ActionHash,
    membership_hash: Option<&ActionHash>,
    hive_membership_hash: Option<&ActionHash>,
    required_role: Role,
    timestamp: &Timestamp,
) -> ExternResult<ValidateCallbackResult> {
    // Anchor on the group genesis. Catches "group_genesis_hash points at
    // a non-GroupGenesis entry" before anything else, and yields the
    // parent hive needed for Path B.
    let (group_author, group) = fetch_group_genesis(group_genesis_hash)?;

    // Path A — group author is the implicit group Owner.
    if &group_author == agent {
        return Ok(ValidateCallbackResult::Valid);
    }

    // Path B — hive sovereignty. A hive Admin+ of the parent hive (or the
    // hive genesis author, caught inside check_hive_authority even with a
    // None witness) controls every group in their hive. `required_role` is
    // intentionally NOT forwarded here: hive Admin+ confers FULL group
    // authority, so it satisfies every group-level role including Owner.
    // Forwarding it would wrongly demand hive Owner to grant a group Owner.
    let hive_check = check_hive_authority(
        agent,
        &group.hive_genesis_hash,
        hive_membership_hash,
        Role::Admin,
        timestamp,
    )?;
    if matches!(hive_check, ValidateCallbackResult::Valid) {
        return Ok(ValidateCallbackResult::Valid);
    }

    // Path C — explicit group membership.
    let Some(hash) = membership_hash else {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "agent {agent} is neither the group author of {group_genesis_hash}, \
             nor a hive Admin+ of its parent hive, and supplied no authorising \
             GroupMembership",
        )));
    };
    let (_membership_author, membership) = fetch_group_membership(hash)?;
    if &membership.group_genesis_hash != group_genesis_hash {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "group membership {hash} is for group {} but caller claimed group {group_genesis_hash}",
            membership.group_genesis_hash,
        )));
    }
    if &membership.for_agent != agent {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "group membership {hash} grants role to {} but action author is {agent}",
            membership.for_agent,
        )));
    }
    if let Some(expiry) = membership.expiry {
        if timestamp > &expiry {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "group membership {hash} expired at {expiry:?}; action timestamp {timestamp:?}",
            )));
        }
    }
    if !role_satisfies(membership.role, required_role) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "group membership {hash} grants role {:?}, required {:?}",
            membership.role, required_role,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

// =============================================================================
// GroupGenesis validators
// =============================================================================

/// A [`GroupGenesis`] create requires hive authority over the parent
/// hive: a hive-wide *system role group* (`hive_wide_role.is_some()`)
/// demands hive **Owner**; an ordinary custom group demands hive
/// **Admin+**. `check_hive_authority` also enforces that
/// `hive_genesis_hash` resolves to a real [`HiveGenesis`].
pub fn validate_create_group_genesis(
    action: EntryCreationAction,
    genesis: GroupGenesis,
) -> ExternResult<ValidateCallbackResult> {
    let required_role = if genesis.hive_wide_role.is_some() {
        Role::Owner
    } else {
        Role::Admin
    };
    check_hive_authority(
        action.author(),
        &genesis.hive_genesis_hash,
        genesis.creator_hive_membership_hash.as_ref(),
        required_role,
        action.timestamp(),
    )
}

/// GroupGenesis is immutable. Updating one would let the creator
/// retroactively redefine the group's hive binding or system-role
/// status while its cryptographic identity (action hash) survives.
pub fn validate_update_group_genesis(
    _action: Update,
    _entry: GroupGenesis,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "GroupGenesis entries are immutable; found a new group instead".into(),
    ))
}

/// Author-gated cosmetic tombstone: a deleted `GroupGenesis` still resolves
/// via `must_get_valid_record`, so this prunes listings without revoking
/// authority (the coordinator refuses deletion while a group has live members).
pub fn validate_delete_group_genesis(
    action: Delete,
    original_action: EntryCreationAction,
    _original_entry: GroupGenesis,
) -> ExternResult<ValidateCallbackResult> {
    if &action.author == original_action.author() {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "GroupGenesis delete must be authored by the group creator \
         (creator: {}, attempted by: {})",
        original_action.author(),
        action.author,
    )))
}

// =============================================================================
// GroupMembership validators
// =============================================================================

/// Validate a [`GroupMembership`] commit. The grantor (= `action.author`)
/// must:
///
/// 1. NOT be granting to themselves (no self-grants — the bootstrap path
///    is the group author / hive sovereign implicit-Owner).
/// 2. Hold Admin+ authority in the group ([`check_group_authority`]).
/// 3. NOT grant a role higher than their own (only an Owner may grant
///    Owner).
/// 4. **G-4.4 grant-window containment.** If the grantor proved authority
///    via an *expiring* group membership, the new membership must itself
///    expire no later than the grantor's. Closes the
///    delegation-window-extension attack (matrix #10).
pub fn validate_create_group_membership(
    action: EntryCreationAction,
    membership: GroupMembership,
) -> ExternResult<ValidateCallbackResult> {
    let grantor = action.author();
    let timestamp = action.timestamp();

    // Rule 1 — no self-grants.
    if grantor == &membership.for_agent {
        return Ok(ValidateCallbackResult::Invalid(
            "self-grant is prohibited; the grantor cannot be the grantee".into(),
        ));
    }

    // Rule 2 — grantor must have Admin+ authority in the group.
    let grantor_check = check_group_authority(
        grantor,
        &membership.group_genesis_hash,
        membership.grantor_membership_hash.as_ref(),
        membership.grantor_hive_membership_hash.as_ref(),
        Role::Admin,
        timestamp,
    )?;
    if !matches!(grantor_check, ValidateCallbackResult::Valid) {
        return Ok(grantor_check);
    }

    // Rule 3 — no escalation above the grantor's own role.
    if matches!(membership.role, Role::Owner) {
        let owner_check = check_group_authority(
            grantor,
            &membership.group_genesis_hash,
            membership.grantor_membership_hash.as_ref(),
            membership.grantor_hive_membership_hash.as_ref(),
            Role::Owner,
            timestamp,
        )?;
        if !matches!(owner_check, ValidateCallbackResult::Valid) {
            return Ok(ValidateCallbackResult::Invalid(
                "granting the Owner role requires group Owner or hive Admin+ authority".into(),
            ));
        }
    }
    // Rule 4 — grant-window containment (G-4.4). Needs the group's parent
    // hive (for the safe Path-B re-verification inside enforce_grant_window),
    // which check_group_authority already fetched but did not return. The
    // fetch is conductor-cached against the same key.
    let (_group_author, group) = fetch_group_genesis(&membership.group_genesis_hash)?;
    enforce_grant_window(
        grantor,
        &group.hive_genesis_hash,
        action.timestamp(),
        &membership,
    )
}

/// G-4.4 — grant-window containment. If the grantor's authority for THIS
/// grant rests on an *expiring* group membership (Path C), the new
/// membership must itself expire no later than the grantor's window.
/// Closes the delegation-window-extension attack (matrix #10).
///
/// The constraint applies ONLY when Path C is the grantor's actual
/// authority basis. A grantor who could prove authority via the
/// group-author (Path A) or hive-sovereign (Path B) route is
/// unconstrained — they may mint permanent group memberships even while
/// personally holding an expiring group membership.
///
/// Path attribution rule (closes the both-witnesses-present hole from
/// the security review):
/// - No `grantor_membership_hash` → Path A or B; unconstrained.
/// - `grantor_membership_hash` present but witness is for a different
///   agent / group → witness does NOT back this grantor's Path-C
///   authority; unconstrained.
/// - `grantor_membership_hash` present AND `grantor_hive_membership_hash`
///   present AND that hive witness independently satisfies hive
///   Admin+ → Path B was viable; unconstrained.
/// - Otherwise → Path C is the only basis; the window must be contained.
///
/// The Path-B re-verification calls `check_hive_authority` directly with
/// the validated parent hive and the supplied hive witness; it is NOT
/// bypassable by a forged witness because the function itself is the
/// authoritative hive-authority oracle (already used by pass-2 for
/// hive-content commits and by `check_group_authority` Path B).
///
/// Note: this re-fetches the grantor membership that
/// [`check_group_authority`] may already have fetched on Path C. The
/// conductor's validation-package cache deduplicates the underlying
/// `must_get_valid_record`, and keeping this rule separate from the
/// reusable authority helper preserves a single source of truth for
/// "does agent hold role X".
fn enforce_grant_window(
    grantor: &AgentPubKey,
    parent_hive: &ActionHash,
    timestamp: &Timestamp,
    membership: &GroupMembership,
) -> ExternResult<ValidateCallbackResult> {
    let Some(grantor_hash) = membership.grantor_membership_hash.as_ref() else {
        // Path A/B grantor — no group-membership window to contain.
        return Ok(ValidateCallbackResult::Valid);
    };
    let (_author, grantor_membership) = fetch_group_membership(grantor_hash)?;
    if &grantor_membership.for_agent != grantor
        || grantor_membership.group_genesis_hash != membership.group_genesis_hash
    {
        // Witness does not back this grantor's Path-C authority for this
        // group; their authority must have come via Path A/B.
        return Ok(ValidateCallbackResult::Valid);
    }
    // Path-C witness IS for this grantor in this group. But the grantor
    // may ALSO have Path-B authority — check independently. If they do,
    // their permanent hive-Admin+ authority dominates the expiring
    // group-Admin witness and the window is unconstrained.
    if let Some(hive_witness) = membership.grantor_hive_membership_hash.as_ref() {
        let hive_check = check_hive_authority(
            grantor,
            parent_hive,
            Some(hive_witness),
            Role::Admin,
            timestamp,
        )?;
        if matches!(hive_check, ValidateCallbackResult::Valid) {
            return Ok(ValidateCallbackResult::Valid);
        }
    }
    let Some(grantor_expiry) = grantor_membership.expiry else {
        // Permanent Path-C authority — unconstrained.
        return Ok(ValidateCallbackResult::Valid);
    };
    match membership.expiry {
        Some(new_expiry) if new_expiry <= grantor_expiry => Ok(ValidateCallbackResult::Valid),
        Some(new_expiry) => Ok(ValidateCallbackResult::Invalid(format!(
            "granted expiry {new_expiry:?} exceeds the grantor membership's expiry \
             {grantor_expiry:?}; an expiring grantor may not extend the delegation window",
        ))),
        None => Ok(ValidateCallbackResult::Invalid(
            "an expiring grantor may not mint a permanent (no-expiry) membership".into(),
        )),
    }
}

/// GroupMembership entries are immutable. Role changes happen by issuing
/// a fresh membership; revocation happens via `expiry`.
pub fn validate_update_group_membership(
    _action: Update,
    _entry: GroupMembership,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "GroupMembership entries are immutable; issue a new membership instead".into(),
    ))
}

/// GroupMembership entries cannot be deleted; revocation is via expiry.
pub fn validate_delete_group_membership(
    _action: Delete,
    _original_action: EntryCreationAction,
    _original_entry: GroupMembership,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "GroupMembership entries cannot be deleted; use the `expiry` field at create time".into(),
    ))
}

// =============================================================================
// Discovery link validators
// =============================================================================
//
// Three index links hang off the group entries. Each create validator
// fetches the target entry, recovers its authoritative fields, and binds
// the link's base (and, where present, tag) to those fields. The link
// author MUST be the target entry's author — only the grantor (for
// membership links) or the group creator (for HiveToGroups) publishes
// the index, mirroring the pass-2 link-validator discipline.

/// Resolve a link `target_address` to its `ActionHash`, erroring if the
/// target is not action-addressed.
pub(crate) fn target_action_hash(target_address: &AnyLinkableHash) -> ExternResult<ActionHash> {
    target_address.clone().into_action_hash().ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "link target {target_address} must be an ActionHash",
        )))
    })
}

/// `link_author == target_author` guard shared by the three group link
/// create validators.
pub(crate) fn require_link_author_is(
    link_author: &AgentPubKey,
    target_author: &AgentPubKey,
) -> ValidateCallbackResult {
    if link_author != target_author {
        return ValidateCallbackResult::Invalid(format!(
            "link author {link_author} does not match target entry author {target_author}",
        ));
    }
    ValidateCallbackResult::Valid
}

/// Decoded link target, or the `Invalid` verdict when the link author is not
/// the target entry's author.
pub(crate) fn link_authors_target_entry<T>(
    link_action: &CreateLink,
    target_address: &AnyLinkableHash,
) -> ExternResult<Result<T, ValidateCallbackResult>>
where
    T: TryFrom<SerializedBytes, Error = SerializedBytesError>,
{
    let record = must_get_valid_record(target_action_hash(target_address)?)?;
    if let invalid @ ValidateCallbackResult::Invalid(_) =
        require_link_author_is(&link_action.author, record.action().author())
    {
        return Ok(Err(invalid));
    }
    let entry = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "link target {} references an unexpected entry type",
                record.action_address(),
            )))
        })?;
    Ok(Ok(entry))
}

/// `AgentToGroupMemberships`: base = grantee `AgentPubKey`, target =
/// [`GroupMembership`]. Forward index ("my group memberships"). Base must
/// equal `membership.for_agent`; link author must be the membership
/// author (the grantor).
pub fn validate_create_link_agent_to_group_memberships(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    let target_ah = target_action_hash(&target_address)?;
    let record = must_get_valid_record(target_ah)?;
    let target_author = record.action().author().clone();
    let author_check = require_link_author_is(&action.author, &target_author);
    if !matches!(author_check, ValidateCallbackResult::Valid) {
        return Ok(author_check);
    }
    let membership: GroupMembership = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "AgentToGroupMemberships target {target_address} is not a GroupMembership",
            )))
        })?;
    let expected_base = AnyLinkableHash::from(membership.for_agent.clone());
    if base_address != expected_base {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "AgentToGroupMemberships base {base_address} does not match membership.for_agent {}",
            membership.for_agent,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// `GroupToGroupMemberships`: base = `group_genesis_hash`, target =
/// [`GroupMembership`], tag = `for_agent` (multibase string bytes).
/// Reverse index — the cryptographic roster. Base must equal
/// `membership.group_genesis_hash`; tag must equal `membership.for_agent`;
/// link author must be the membership author.
pub fn validate_create_link_group_to_group_memberships(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    let target_ah = target_action_hash(&target_address)?;
    let record = must_get_valid_record(target_ah)?;
    let target_author = record.action().author().clone();
    let author_check = require_link_author_is(&action.author, &target_author);
    if !matches!(author_check, ValidateCallbackResult::Valid) {
        return Ok(author_check);
    }
    let membership: GroupMembership = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "GroupToGroupMemberships target {target_address} is not a GroupMembership",
            )))
        })?;
    let expected_base = AnyLinkableHash::from(membership.group_genesis_hash.clone());
    if base_address != expected_base {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "GroupToGroupMemberships base {base_address} does not match \
             membership.group_genesis_hash {}",
            membership.group_genesis_hash,
        )));
    }
    let tag_str = match String::from_utf8(tag.0) {
        Ok(s) => s,
        Err(e) => {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "GroupToGroupMemberships tag is not valid UTF-8: {e}",
            )))
        }
    };
    if tag_str != membership.for_agent.to_string() {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "GroupToGroupMemberships tag {tag_str} does not match membership.for_agent {}",
            membership.for_agent,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// `HiveToGroups`: base = `hive_genesis_hash`, target = [`GroupGenesis`].
/// Enumerate a hive's groups. Base must equal `genesis.hive_genesis_hash`;
/// link author must be the genesis author (the group creator); tag MUST
/// be empty (reserved for future use; constrained now so a rogue group
/// creator cannot poison the field for display/routing consumers).
pub fn validate_create_link_hive_to_groups(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    let target_ah = target_action_hash(&target_address)?;
    let record = must_get_valid_record(target_ah)?;
    let target_author = record.action().author().clone();
    let author_check = require_link_author_is(&action.author, &target_author);
    if !matches!(author_check, ValidateCallbackResult::Valid) {
        return Ok(author_check);
    }
    if !tag.0.is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "HiveToGroups link tag must be empty (reserved for future use)".into(),
        ));
    }
    let genesis: GroupGenesis = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "HiveToGroups target {target_address} is not a GroupGenesis",
            )))
        })?;
    let expected_base = AnyLinkableHash::from(genesis.hive_genesis_hash.clone());
    if base_address != expected_base {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveToGroups base {base_address} does not match genesis.hive_genesis_hash {}",
            genesis.hive_genesis_hash,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Author-gated delete shared by the group index, owner-handoff, and
/// invite-redemption links: only the link creator may delete.
///
/// **Index-vs-entry contract (security-relevant).** A link's `delete`
/// is the link author's prerogative, which means a grantor (who is the
/// only legal author of `AgentToGroupMemberships` /
/// `GroupToGroupMemberships`) can later remove the discovery links
/// pointing at a grantee's still-valid `GroupMembership` entry. The
/// `GroupMembership` entry itself is immutable and remains cryptographically
/// valid — only the index loses the row. Coordinator + downstream
/// consumers MUST treat the discovery links as a *cache*, not as the
/// authoritative roster: every authority decision MUST be made by
/// `must_get_valid_record` against the entry hash directly. The links
/// exist solely to enumerate "which membership hashes does this group /
/// agent currently advertise"; a missing link does NOT prove a missing
/// membership. (Mirrors the pass-2 hive-link discipline; documented
/// here so future consumers in this repo and humm-tauri don't
/// mistakenly index-gate access.)
pub fn validate_delete_group_link(
    action: DeleteLink,
    original_action: CreateLink,
    link_label: &str,
) -> ExternResult<ValidateCallbackResult> {
    if action.author == original_action.author {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "{link_label} link delete must be authored by the link creator \
         (creator: {}, attempted by: {})",
        original_action.author, action.author,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_pubkey(byte: u8) -> AgentPubKey {
        AgentPubKey::from_raw_36(vec![byte; 36])
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
                entry_index: 4.into(),
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
                entry_index: 4.into(),
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

    fn sample_group_genesis() -> GroupGenesis {
        GroupGenesis {
            hive_genesis_hash: action_hash(9),
            display_id: "group-test".into(),
            hive_wide_role: None,
            creator_hive_membership_hash: None,
            created_at_microseconds: 0,
        }
    }

    fn sample_group_membership(
        for_agent: AgentPubKey,
        role: Role,
        grantor_membership_hash: Option<ActionHash>,
    ) -> GroupMembership {
        GroupMembership {
            group_genesis_hash: action_hash(9),
            for_agent,
            role,
            grantor_membership_hash,
            grantor_hive_membership_hash: None,
            expiry: None,
        }
    }

    // -----------------------------------------------------------------
    // GroupGenesis immutability.
    // -----------------------------------------------------------------

    #[test]
    fn group_genesis_update_is_invalid() {
        let alice = agent_pubkey(1);
        let result = validate_update_group_genesis(make_update(alice), sample_group_genesis())
            .expect("validator should not error in test");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.to_lowercase().contains("immutable"));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn group_genesis_delete_author_gated() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let by_author = validate_delete_group_genesis(
            make_delete(alice.clone()),
            EntryCreationAction::Create(make_create(alice.clone())),
            sample_group_genesis(),
        )
        .expect("validator should not error in test");
        assert!(
            matches!(by_author, ValidateCallbackResult::Valid),
            "group creator may delete; got {by_author:?}",
        );

        let by_other = validate_delete_group_genesis(
            make_delete(bob),
            EntryCreationAction::Create(make_create(alice)),
            sample_group_genesis(),
        )
        .expect("validator should not error in test");
        assert!(
            matches!(by_other, ValidateCallbackResult::Invalid(_)),
            "non-creator rejected; got {by_other:?}",
        );
    }

    // -----------------------------------------------------------------
    // GroupMembership immutability.
    // -----------------------------------------------------------------

    #[test]
    fn group_membership_update_is_invalid() {
        let alice = agent_pubkey(1);
        let result = validate_update_group_membership(
            make_update(alice.clone()),
            sample_group_membership(alice, Role::Writer, None),
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
    fn group_membership_delete_is_invalid() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let original = EntryCreationAction::Create(make_create(alice.clone()));
        let result = validate_delete_group_membership(
            make_delete(alice),
            original,
            sample_group_membership(bob, Role::Writer, None),
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
    // GroupMembership create — Rule 1 (no self-grant). Reachable
    // host-side because the self-grant short-circuit fires BEFORE any
    // must_get_valid_record chain walk.
    // -----------------------------------------------------------------

    #[test]
    fn group_membership_self_grant_is_invalid() {
        let alice = agent_pubkey(1);
        let action = EntryCreationAction::Create(make_create(alice.clone()));
        let membership = sample_group_membership(alice, Role::Writer, None);
        let result = validate_create_group_membership(action, membership)
            .expect("validator should not error before chain walk");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.to_lowercase().contains("self-grant"));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn group_membership_self_grant_invalid_regardless_of_role() {
        let alice = agent_pubkey(1);
        for role in [Role::Owner, Role::Admin, Role::Writer, Role::Reader] {
            let action = EntryCreationAction::Create(make_create(alice.clone()));
            let membership = sample_group_membership(alice.clone(), role, None);
            let result = validate_create_group_membership(action, membership)
                .expect("validator should not error before chain walk");
            assert!(
                matches!(result, ValidateCallbackResult::Invalid(_)),
                "self-grant of {role:?} must be Invalid; got {result:?}",
            );
        }
    }

    // -----------------------------------------------------------------
    // enforce_grant_window — the no-witness fast path (grantor relied on
    // group-author / hive-sovereign route) is host-reachable: it returns
    // Valid without any fetch.
    // -----------------------------------------------------------------

    #[test]
    fn grant_window_unconstrained_without_grantor_membership() {
        // No grantor_membership_hash => Path A/B grantor => unconstrained.
        let bob = agent_pubkey(2);
        let membership = GroupMembership {
            group_genesis_hash: action_hash(9),
            for_agent: bob,
            role: Role::Writer,
            grantor_membership_hash: None,
            grantor_hive_membership_hash: None,
            expiry: Some(Timestamp(1_000)),
        };
        let result = enforce_grant_window(
            &agent_pubkey(3),
            &action_hash(9),
            &Timestamp(0),
            &membership,
        )
        .expect("no fetch on the None-witness path");
        assert!(matches!(result, ValidateCallbackResult::Valid));
    }

    // -----------------------------------------------------------------
    // Link author guard — pure comparison, host-reachable.
    // -----------------------------------------------------------------

    #[test]
    fn link_author_mismatch_is_invalid() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        assert!(matches!(
            require_link_author_is(&alice, &alice),
            ValidateCallbackResult::Valid
        ));
        assert!(matches!(
            require_link_author_is(&alice, &bob),
            ValidateCallbackResult::Invalid(_)
        ));
    }

    #[test]
    fn group_link_delete_requires_link_author() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let create = CreateLink {
            author: alice.clone(),
            timestamp: Timestamp(0),
            action_seq: 1,
            prev_action: action_hash(0),
            base_address: AnyLinkableHash::from(alice.clone()),
            target_address: AnyLinkableHash::from(action_hash(5)),
            zome_index: 0.into(),
            link_type: 12.into(),
            tag: LinkTag::new(vec![]),
            weight: Default::default(),
        };
        let same_author_delete = DeleteLink {
            author: alice.clone(),
            timestamp: Timestamp(0),
            action_seq: 2,
            prev_action: action_hash(0),
            base_address: AnyLinkableHash::from(alice.clone()),
            link_add_address: action_hash(1),
        };
        assert!(matches!(
            validate_delete_group_link(same_author_delete, create.clone(), "HiveToGroups")
                .expect("pure path"),
            ValidateCallbackResult::Valid
        ));
        let other_author_delete = DeleteLink {
            author: bob,
            timestamp: Timestamp(0),
            action_seq: 2,
            prev_action: action_hash(0),
            base_address: AnyLinkableHash::from(alice),
            link_add_address: action_hash(1),
        };
        assert!(matches!(
            validate_delete_group_link(other_author_delete, create, "HiveToGroups")
                .expect("pure path"),
            ValidateCallbackResult::Invalid(_)
        ));
    }
}
