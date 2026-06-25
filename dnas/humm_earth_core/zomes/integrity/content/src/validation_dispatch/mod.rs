mod activity;
mod entry;
mod links;

use hdi::prelude::*;

use crate::*;
use entry::{
    dispatch_create_entry, dispatch_delete_entry, dispatch_store_record_update_entry,
    dispatch_update_entry,
};
use links::{dispatch_create_link, dispatch_delete_link, dispatch_store_record_delete_link};

pub(crate) use activity::validate_agent_joining;

pub(crate) fn validate_op(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::StoreEntry(store_entry) => match store_entry {
            OpEntry::CreateEntry { app_entry, action } => {
                dispatch_create_entry(EntryCreationAction::Create(action), app_entry)
            }
            OpEntry::UpdateEntry {
                app_entry, action, ..
            } => dispatch_update_entry(action, app_entry),
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterUpdate(update_entry) => match update_entry {
            OpUpdate::Entry { app_entry, action } => dispatch_update_entry(action, app_entry),
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterDelete(OpDelete { action }) => dispatch_delete_entry(action),
        FlatOp::RegisterCreateLink {
            link_type,
            base_address,
            target_address,
            tag,
            action,
        } => dispatch_create_link(link_type, action, base_address, target_address, tag),
        FlatOp::RegisterDeleteLink {
            link_type,
            base_address,
            target_address,
            tag,
            original_action,
            action,
        } => dispatch_delete_link(
            link_type,
            action,
            original_action,
            base_address,
            target_address,
            tag,
        ),
        FlatOp::StoreRecord(store_record) => match store_record {
            OpRecord::CreateEntry { app_entry, action } => {
                dispatch_create_entry(EntryCreationAction::Create(action), app_entry)
            }
            OpRecord::UpdateEntry {
                app_entry, action, ..
            } => dispatch_store_record_update_entry(action, app_entry),
            OpRecord::DeleteEntry { action, .. } => dispatch_delete_entry(action),
            OpRecord::CreateLink {
                base_address,
                target_address,
                tag,
                link_type,
                action,
            } => dispatch_create_link(link_type, action, base_address, target_address, tag),
            OpRecord::DeleteLink {
                original_action_hash,
                base_address,
                action,
            } => dispatch_store_record_delete_link(action, original_action_hash, base_address),
            OpRecord::CreatePrivateEntry { .. }
            | OpRecord::UpdatePrivateEntry { .. }
            | OpRecord::CreateCapClaim { .. }
            | OpRecord::CreateCapGrant { .. }
            | OpRecord::UpdateCapClaim { .. }
            | OpRecord::UpdateCapGrant { .. }
            | OpRecord::Dna { .. }
            | OpRecord::OpenChain { .. }
            | OpRecord::CloseChain { .. }
            | OpRecord::InitZomesComplete { .. } => Ok(ValidateCallbackResult::Valid),
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterAgentActivity(agent_activity) => {
            activity::validate_register_agent_activity(agent_activity)
        }
    }
}
