use hdi::prelude::*;

use super::authority::{check_hive_authority, fetch_genesis, fetch_membership};
use super::owner::is_lineage_owner;
use super::types::{HiveGenesis, HiveMembership, Role};

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
/// 3. NOT grant `Owner`. HiveMembership can grant Admin/Writer/Reader;
///    only the owner-handoff handshake confers Owner, and the current
///    lineage owner is the only authority that may grant Admin.
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
pub(super) fn enforce_hive_grant_window(
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
