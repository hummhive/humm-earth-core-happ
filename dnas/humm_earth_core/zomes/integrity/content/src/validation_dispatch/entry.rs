use hdi::prelude::*;

use crate::*;

pub(super) fn dispatch_create_entry(
    action: EntryCreationAction,
    app_entry: EntryTypes,
) -> ExternResult<ValidateCallbackResult> {
    match app_entry {
        EntryTypes::EncryptedContent(encrypted_content) => {
            validate_create_encrypted_content(action, encrypted_content)
        }
        EntryTypes::HiveGenesis(genesis) => validate_create_hive_genesis(action, genesis),
        EntryTypes::HiveMembership(membership) => {
            validate_create_hive_membership(action, membership)
        }
        EntryTypes::DmProbeLog(_) => Ok(ValidateCallbackResult::Valid),
        EntryTypes::GroupGenesis(genesis) => validate_create_group_genesis(action, genesis),
        EntryTypes::GroupMembership(membership) => {
            validate_create_group_membership(action, membership)
        }
        EntryTypes::HiveOwnerHandoffOffer(offer) => {
            validate_create_hive_owner_handoff_offer(action, offer)
        }
        EntryTypes::HiveOwnerHandoffAccept(accept) => {
            validate_create_hive_owner_handoff_accept(action, accept)
        }
        EntryTypes::InviteRedemption(redemption) => {
            validate_create_invite_redemption(action, redemption)
        }
    }
}

fn update_entry_type_verdict(
    original_entry_type: &EntryType,
    updated_entry_type: &EntryType,
) -> ValidateCallbackResult {
    if original_entry_type == updated_entry_type {
        return ValidateCallbackResult::Valid;
    }
    ValidateCallbackResult::Invalid(
        "Updates must preserve the original app entry type; create a new entry instead".into(),
    )
}

fn validate_update_targets_same_entry_type(
    action: &Update,
) -> ExternResult<ValidateCallbackResult> {
    let original_action = must_get_action(action.original_action_address.clone())?;
    let Some(original_entry_type) = original_action.action().entry_type() else {
        return Ok(ValidateCallbackResult::Invalid(
            "Update original action must be an entry action".into(),
        ));
    };
    Ok(update_entry_type_verdict(
        original_entry_type,
        &action.entry_type,
    ))
}

pub(super) fn dispatch_update_entry(
    action: Update,
    app_entry: EntryTypes,
) -> ExternResult<ValidateCallbackResult> {
    let type_verdict = validate_update_targets_same_entry_type(&action)?;
    if let invalid @ ValidateCallbackResult::Invalid(_) = type_verdict {
        return Ok(invalid);
    }
    match app_entry {
        EntryTypes::EncryptedContent(encrypted_content) => {
            validate_update_encrypted_content(action, encrypted_content)
        }
        EntryTypes::HiveGenesis(genesis) => validate_update_hive_genesis(action, genesis),
        EntryTypes::HiveMembership(membership) => {
            validate_update_hive_membership(action, membership)
        }
        EntryTypes::DmProbeLog(_) => Ok(ValidateCallbackResult::Valid),
        EntryTypes::GroupGenesis(genesis) => validate_update_group_genesis(action, genesis),
        EntryTypes::GroupMembership(membership) => {
            validate_update_group_membership(action, membership)
        }
        EntryTypes::HiveOwnerHandoffOffer(offer) => {
            validate_update_hive_owner_handoff_offer(action, offer)
        }
        EntryTypes::HiveOwnerHandoffAccept(accept) => {
            validate_update_hive_owner_handoff_accept(action, accept)
        }
        EntryTypes::InviteRedemption(redemption) => {
            validate_update_invite_redemption(action, redemption)
        }
    }
}

pub(super) fn dispatch_store_record_update_entry(
    action: Update,
    app_entry: EntryTypes,
) -> ExternResult<ValidateCallbackResult> {
    match app_entry {
        EntryTypes::EncryptedContent(encrypted_content) => {
            let create_result = validate_create_encrypted_content(
                EntryCreationAction::Update(action.clone()),
                encrypted_content.clone(),
            )?;
            if let ValidateCallbackResult::Valid = create_result {
                dispatch_update_entry(action, EntryTypes::EncryptedContent(encrypted_content))
            } else {
                Ok(create_result)
            }
        }
        other_entry => dispatch_update_entry(action, other_entry),
    }
}

/// Fetch the original record being deleted and dispatch to the
/// per-entry-type delete validator.
///
/// hdi 0.7.0's [`FlatOp::RegisterDelete`] only carries the [`Delete`]
/// action (`OpDelete { action }`); the original record + classified
/// entry must be fetched here. The same routing runs under
/// `FlatOp::StoreRecord::DeleteEntry` for chain-store validation
/// — the two arms share this helper so the semantics cannot drift.
pub(super) fn dispatch_delete_entry(action: Delete) -> ExternResult<ValidateCallbackResult> {
    let original_record = must_get_valid_record(action.deletes_address.clone())?;
    let original_action = match original_record.action().clone() {
        Action::Create(create) => EntryCreationAction::Create(create),
        Action::Update(update) => EntryCreationAction::Update(update),
        _ => {
            return Ok(ValidateCallbackResult::Invalid(
                "Original action for a delete must be a Create or Update action".into(),
            ));
        }
    };
    let app_entry_type = match original_action.entry_type() {
        EntryType::App(app_entry_type) => app_entry_type,
        _ => return Ok(ValidateCallbackResult::Valid),
    };
    let entry = match original_record.entry().as_option() {
        Some(entry) => entry,
        None => {
            if original_action.entry_type().visibility().is_public() {
                return Ok(ValidateCallbackResult::Invalid(
                    "Original record for a delete of a public entry must contain an entry".into(),
                ));
            }
            return Ok(ValidateCallbackResult::Valid);
        }
    };
    let original_app_entry = match EntryTypes::deserialize_from_type(
        app_entry_type.zome_index,
        app_entry_type.entry_index,
        entry,
    )? {
        Some(app_entry) => app_entry,
        None => {
            return Ok(ValidateCallbackResult::Invalid(
                "Original app entry must be one of the defined entry types for this zome".into(),
            ));
        }
    };
    match original_app_entry {
        EntryTypes::EncryptedContent(original_encrypted_content) => {
            validate_delete_encrypted_content(action, original_action, original_encrypted_content)
        }
        EntryTypes::HiveGenesis(genesis) => {
            validate_delete_hive_genesis(action, original_action, genesis)
        }
        EntryTypes::HiveMembership(membership) => {
            validate_delete_hive_membership(action, original_action, membership)
        }
        EntryTypes::DmProbeLog(_) => Ok(ValidateCallbackResult::Valid),
        EntryTypes::GroupGenesis(genesis) => {
            validate_delete_group_genesis(action, original_action, genesis)
        }
        EntryTypes::GroupMembership(membership) => {
            validate_delete_group_membership(action, original_action, membership)
        }
        EntryTypes::HiveOwnerHandoffOffer(offer) => {
            validate_delete_hive_owner_handoff_offer(action, original_action, offer)
        }
        EntryTypes::HiveOwnerHandoffAccept(accept) => {
            validate_delete_hive_owner_handoff_accept(action, original_action, accept)
        }
        EntryTypes::InviteRedemption(redemption) => {
            validate_delete_invite_redemption(action, original_action, redemption)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::update_entry_type_verdict;
    use hdi::prelude::*;

    fn app_entry_type(entry_index: u8) -> EntryType {
        EntryType::App(AppEntryDef {
            entry_index: entry_index.into(),
            zome_index: 0.into(),
            visibility: EntryVisibility::Public,
        })
    }

    #[test]
    fn allows_encrypted_content_to_encrypted_content_update_type_gate() {
        let encrypted_content = app_entry_type(0);
        let result = update_entry_type_verdict(&encrypted_content, &encrypted_content);
        assert!(matches!(result, ValidateCallbackResult::Valid));
    }

    #[test]
    fn rejects_hive_genesis_to_encrypted_content_update() {
        let result = update_entry_type_verdict(&app_entry_type(1), &app_entry_type(0));
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.contains("preserve the original app entry type"));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn rejects_immutable_originals_to_encrypted_content_update() {
        let encrypted_content = app_entry_type(0);
        for original_entry_index in [1, 2, 4, 5, 6, 7, 8] {
            let result = update_entry_type_verdict(
                &app_entry_type(original_entry_index),
                &encrypted_content,
            );
            assert!(
                matches!(result, ValidateCallbackResult::Invalid(_)),
                "entry index {original_entry_index} must not update into EncryptedContent; got {result:?}",
            );
        }
    }
}
