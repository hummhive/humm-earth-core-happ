pub mod encrypted_content;
pub mod globals;
pub use encrypted_content::*;
pub use globals::*;
use hdi::prelude::*;
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
#[hdk_entry_types]
#[unit_enum(EntryTypesUnit)]
pub enum EntryTypes {
    EncryptedContent(EncryptedContent),
}
#[derive(Serialize, Deserialize)]
#[hdk_link_types]
pub enum LinkTypes {
    OriginalHashPointer,
    EncryptedContentUpdates,
    TimePath,
    TimeItem,
    Hive,
    Dynamic,
    HummContentId, // TODO
    HummContentOwner,
    HummContentAdmin,
    HummContentWriter,
    HummContentReader,
}
#[hdk_extern]
pub fn genesis_self_check(_data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}
pub fn validate_agent_joining(
    _agent_pub_key: AgentPubKey,
    _membrane_proof: &Option<MembraneProof>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}
#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::StoreEntry(store_entry) => match store_entry {
            OpEntry::CreateEntry { app_entry, action } => match app_entry {
                EntryTypes::EncryptedContent(encrypted_content) => {
                    validate_create_encrypted_content(
                        EntryCreationAction::Create(action),
                        encrypted_content,
                    )
                }
            },
            OpEntry::UpdateEntry {
                app_entry, action, ..
            } => match app_entry {
                EntryTypes::EncryptedContent(encrypted_content) => {
                    validate_create_encrypted_content(
                        EntryCreationAction::Update(action),
                        encrypted_content,
                    )
                }
            },
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterUpdate(update_entry) => match update_entry {
            OpUpdate::Entry {
                app_entry,
                action,
            } => match app_entry {
                EntryTypes::EncryptedContent(encrypted_content)
                 => validate_update_encrypted_content(
                    action,
                    encrypted_content
                ),
                // _ => Ok(ValidateCallbackResult::Invalid(
                //     "Original and updated entry types must be the same".to_string(),
                // )),
            },
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterDelete(delete_entry) => match delete_entry {
            // OpDelete::Entry {
            //     original_action,
            //     original_app_entry,
            //     action,
            // } => match original_app_entry {
            //     EntryTypes::EncryptedContent(encrypted_content) => {
            //         validate_delete_encrypted_content(action, original_action, encrypted_content)
            //     }
            // },
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterCreateLink {
            link_type,
            base_address,
            target_address,
            tag,
            action,
        } => match link_type {
            LinkTypes::EncryptedContentUpdates => validate_create_link_encrypted_content_updates(
                action,
                base_address,
                target_address,
                tag,
            ),
            // TODO
            LinkTypes::OriginalHashPointer => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::HummContentOwner => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::HummContentAdmin => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::HummContentWriter => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::HummContentReader => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::Hive => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::HummContentId => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::TimePath => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::TimeItem => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::Dynamic => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterDeleteLink {
            link_type,
            base_address,
            target_address,
            tag,
            original_action,
            action,
        } => match link_type {
            LinkTypes::EncryptedContentUpdates => validate_delete_link_encrypted_content_updates(
                action,
                original_action,
                base_address,
                target_address,
                tag,
            ),
            // TODO
            LinkTypes::OriginalHashPointer => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::HummContentOwner => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::HummContentAdmin => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::HummContentWriter => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::HummContentReader => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::Hive => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::HummContentId => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::TimePath => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::TimeItem => Ok(ValidateCallbackResult::Valid),
            // TODO
            LinkTypes::Dynamic => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::StoreRecord(store_record) => match store_record {
            OpRecord::CreateEntry { app_entry, action } => match app_entry {
                EntryTypes::EncryptedContent(encrypted_content) => {
                    validate_create_encrypted_content(
                        EntryCreationAction::Create(action),
                        encrypted_content,
                    )
                }
            },
            OpRecord::UpdateEntry {
                original_action_hash,
                app_entry,
                action,
                ..
            } => {
                // let original_record = must_get_valid_record(original_action_hash)?;
                // let original_action = original_record.action().clone();

                match app_entry {
                    EntryTypes::EncryptedContent(encrypted_content) => {
                        let result = validate_create_encrypted_content(
                            EntryCreationAction::Update(action.clone()),
                            encrypted_content.clone(),
                        )?;
                        if let ValidateCallbackResult::Valid = result {
                            // let original_encrypted_content: Option<EncryptedContent> =
                            //     original_record
                            //         .entry()
                            //         .to_app_option()
                            //         .map_err(|e| wasm_error!(e))?;
                            // let original_encrypted_content = match original_encrypted_content {
                            //     Some(encrypted_content) => encrypted_content,
                            //     None => {
                            //         return Ok(
                            //                 ValidateCallbackResult::Invalid(
                            //                     "The updated entry type must be the same as the original entry type"
                            //                         .to_string(),
                            //                 ),
                            //             );
                            //     }
                            // };
                            validate_update_encrypted_content(
                                action,
                                encrypted_content,
                            )
                        } else {
                            Ok(result)
                        }
                    }
                }
            }
            OpRecord::DeleteEntry {
                original_action_hash,
                action,
                ..
            } => {
                let original_record = must_get_valid_record(original_action_hash)?;
                let original_action = original_record.action().clone();
                let original_action = match original_action {
                    Action::Create(create) => EntryCreationAction::Create(create),
                    Action::Update(update) => EntryCreationAction::Update(update),
                    _ => {
                        return Ok(ValidateCallbackResult::Invalid(
                            "Original action for a delete must be a Create or Update action"
                                .to_string(),
                        ));
                    }
                };
                let app_entry_type = match original_action.entry_type() {
                    EntryType::App(app_entry_type) => app_entry_type,
                    _ => {
                        return Ok(ValidateCallbackResult::Valid);
                    }
                };
                let entry = match original_record.entry().as_option() {
                    Some(entry) => entry,
                    None => {
                        if original_action.entry_type().visibility().is_public() {
                            return Ok(
                                    ValidateCallbackResult::Invalid(
                                        "Original record for a delete of a public entry must contain an entry"
                                            .to_string(),
                                    ),
                                );
                        } else {
                            return Ok(ValidateCallbackResult::Valid);
                        }
                    }
                };
                let original_app_entry = match EntryTypes::deserialize_from_type(
                    app_entry_type.zome_index.clone(),
                    app_entry_type.entry_index.clone(),
                    &entry,
                )? {
                    Some(app_entry) => app_entry,
                    None => {
                        return Ok(
                                ValidateCallbackResult::Invalid(
                                    "Original app entry must be one of the defined entry types for this zome"
                                        .to_string(),
                                ),
                            );
                    }
                };
                match original_app_entry {
                    EntryTypes::EncryptedContent(original_encrypted_content) => {
                        validate_delete_encrypted_content(
                            action,
                            original_action,
                            original_encrypted_content,
                        )
                    }
                }
            }
            OpRecord::CreateLink {
                base_address,
                target_address,
                tag,
                link_type,
                action,
            } => match link_type {
                LinkTypes::EncryptedContentUpdates => {
                    validate_create_link_encrypted_content_updates(
                        action,
                        base_address,
                        target_address,
                        tag,
                    )
                }
                // TODO
                LinkTypes::OriginalHashPointer => Ok(ValidateCallbackResult::Valid),
                // TODO
                LinkTypes::HummContentOwner => Ok(ValidateCallbackResult::Valid),
                // TODO
                LinkTypes::HummContentAdmin => Ok(ValidateCallbackResult::Valid),
                // TODO
                LinkTypes::HummContentWriter => Ok(ValidateCallbackResult::Valid),
                // TODO
                LinkTypes::HummContentReader => Ok(ValidateCallbackResult::Valid),
                // TODO
                LinkTypes::Hive => Ok(ValidateCallbackResult::Valid),
                // TODO
                LinkTypes::HummContentId => Ok(ValidateCallbackResult::Valid),
                // TODO
                LinkTypes::TimePath => Ok(ValidateCallbackResult::Valid),
                // TODO
                LinkTypes::TimeItem => Ok(ValidateCallbackResult::Valid),
                // TODO
                LinkTypes::Dynamic => Ok(ValidateCallbackResult::Valid),
            },
            OpRecord::DeleteLink {
                original_action_hash,
                base_address,
                action,
            } => {
                let record = must_get_valid_record(original_action_hash)?;
                let create_link = match record.action() {
                    Action::CreateLink(create_link) => create_link.clone(),
                    _ => {
                        return Ok(ValidateCallbackResult::Invalid(
                            "The action that a DeleteLink deletes must be a CreateLink".to_string(),
                        ));
                    }
                };
                let link_type = match LinkTypes::from_type(
                    create_link.zome_index.clone(),
                    create_link.link_type.clone(),
                )? {
                    Some(lt) => lt,
                    None => {
                        return Ok(ValidateCallbackResult::Valid);
                    }
                };
                match link_type {
                    LinkTypes::EncryptedContentUpdates => {
                        validate_delete_link_encrypted_content_updates(
                            action,
                            create_link.clone(),
                            base_address,
                            create_link.target_address,
                            create_link.tag,
                        )
                    }
                    // TODO
                    LinkTypes::OriginalHashPointer => Ok(ValidateCallbackResult::Valid),
                    // TODO
                    LinkTypes::HummContentOwner => Ok(ValidateCallbackResult::Valid),
                    // TODO
                    LinkTypes::HummContentAdmin => Ok(ValidateCallbackResult::Valid),
                    // TODO
                    LinkTypes::HummContentWriter => Ok(ValidateCallbackResult::Valid),
                    // TODO
                    LinkTypes::HummContentReader => Ok(ValidateCallbackResult::Valid),
                    // TODO
                    LinkTypes::Hive => Ok(ValidateCallbackResult::Valid),
                    // TODO
                    LinkTypes::HummContentId => Ok(ValidateCallbackResult::Valid),
                    // TODO
                    LinkTypes::TimePath => Ok(ValidateCallbackResult::Valid),
                    // TODO
                    LinkTypes::TimeItem => Ok(ValidateCallbackResult::Valid),
                    // TODO
                    LinkTypes::Dynamic => Ok(ValidateCallbackResult::Valid),
                }
            }
            OpRecord::CreatePrivateEntry { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::UpdatePrivateEntry { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::CreateCapClaim { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::CreateCapGrant { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::UpdateCapClaim { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::UpdateCapGrant { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::Dna { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::OpenChain { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::CloseChain { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::InitZomesComplete { .. } => Ok(ValidateCallbackResult::Valid),
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterAgentActivity(agent_activity) => match agent_activity {
            OpActivity::CreateAgent { agent, action } => {
                let previous_action = must_get_action(action.prev_action)?;
                match previous_action.action() {
                        Action::AgentValidationPkg(
                            AgentValidationPkg { membrane_proof, .. },
                        ) => validate_agent_joining(agent, membrane_proof),
                        _ => {
                            Ok(
                                ValidateCallbackResult::Invalid(
                                    "The previous action for a `CreateAgent` action must be an `AgentValidationPkg`"
                                        .to_string(),
                                ),
                            )
                        }
                    }
            }
            _ => Ok(ValidateCallbackResult::Valid),
        },
    }
}
