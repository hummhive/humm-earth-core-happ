use hdi::prelude::*;

use super::authority::{check_group_authority, fetch_group_genesis, fetch_group_membership};
use super::types::{GroupGenesis, GroupMembership};
use crate::hive::{check_hive_authority, Role};

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
pub(super) fn enforce_grant_window(
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
