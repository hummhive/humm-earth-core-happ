//! Link validators for the durable per-agent hive-membership index.

use hdi::prelude::*;

use crate::group::{require_link_author_is, target_action_hash};
use crate::EntryTypes;

/// `HiveMembershipIndex`: base = member `AgentPubKey`, target =
/// `HiveMembership` (grant; grantor authored both) or `HiveGenesis`
/// (founder self-index). Tag must be empty; link author must equal the
/// target entry's author. Unlike `Inbox`, deletion is author-only, so a
/// DM-sweeping recipient can never erase hive-membership discovery.
pub fn validate_create_link_hive_membership_index(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    if !tag.0.is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "HiveMembershipIndex tag must be empty".into(),
        ));
    }
    let record = must_get_valid_record(target_action_hash(&target_address)?)?;
    let target_author = record.action().author().clone();
    let author_check = require_link_author_is(&action.author, &target_author);
    if !matches!(author_check, ValidateCallbackResult::Valid) {
        return Ok(author_check);
    }
    let Some(EntryType::App(AppEntryDef {
        zome_index,
        entry_index,
        ..
    })) = record.action().entry_type()
    else {
        return Ok(invalid_index_target());
    };
    let Some(entry) = record.entry().as_option() else {
        return Ok(invalid_index_target());
    };
    // Entry-def-index dispatch, NEVER shape-decode: GroupGenesis is a serde
    // field-superset of HiveGenesis, so to_app_option would false-positive.
    match EntryTypes::deserialize_from_type(*zome_index, *entry_index, entry)? {
        Some(EntryTypes::HiveMembership(membership)) => {
            if base_address != AnyLinkableHash::from(membership.for_agent.clone()) {
                return Ok(ValidateCallbackResult::Invalid(
                    "HiveMembershipIndex base must be the membership's for_agent".into(),
                ));
            }
            Ok(ValidateCallbackResult::Valid)
        }
        Some(EntryTypes::HiveGenesis(_)) => {
            if base_address != AnyLinkableHash::from(target_author) {
                return Ok(ValidateCallbackResult::Invalid(
                    "HiveMembershipIndex base must be the hive genesis author".into(),
                ));
            }
            Ok(ValidateCallbackResult::Valid)
        }
        Some(
            EntryTypes::EncryptedContent(_)
            | EntryTypes::DmProbeLog(_)
            | EntryTypes::GroupGenesis(_)
            | EntryTypes::GroupMembership(_)
            | EntryTypes::HiveOwnerHandoffOffer(_)
            | EntryTypes::HiveOwnerHandoffAccept(_)
            | EntryTypes::InviteRedemption(_),
        )
        | None => Ok(invalid_index_target()),
    }
}

fn invalid_index_target() -> ValidateCallbackResult {
    ValidateCallbackResult::Invalid(
        "HiveMembershipIndex target must be a HiveMembership or HiveGenesis".into(),
    )
}

/// Author-gated delete: only the index link's creator may retract it.
/// This is the load-bearing difference from the `Inbox` delete validator
/// (which lets the recipient consume) — the durable index survives sweeps.
pub fn validate_delete_link_hive_membership_index(
    action: DeleteLink,
    original_action: CreateLink,
) -> ExternResult<ValidateCallbackResult> {
    if action.author == original_action.author {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "HiveMembershipIndex link may only be deleted by its author \
         (creator: {}, attempted by: {})",
        original_action.author, action.author,
    )))
}
