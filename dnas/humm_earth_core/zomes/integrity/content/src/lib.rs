pub mod encrypted_content;
pub mod globals;
pub mod hive;
pub mod inbox;

pub use encrypted_content::*;
pub use globals::*;
pub use hive::*;
pub use inbox::*;

use hdi::prelude::*;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
#[hdk_entry_types]
#[unit_enum(EntryTypesUnit)]
pub enum EntryTypes {
    EncryptedContent(EncryptedContent),
    HiveGenesis(HiveGenesis),
    HiveMembership(HiveMembership),
    /// Private source-chain entry — never DHT-published. Validators
    /// allow any commit (a host should only ever write its own
    /// probe-log; the entry is invisible to peers anyway).
    #[entry_type(visibility = "private")]
    DmProbeLog(DmProbeLog),
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
    HummContentId,
    HummContentOwner,
    HummContentAdmin,
    HummContentWriter,
    HummContentReader,
    /// Inbox — recipient AgentPubKey → content ActionHash; tag = 1 byte
    /// [`InboxEvent`] discriminator. Appended at the END to keep all
    /// existing variant indices stable post-pass-2.
    Inbox,
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

/// Single-source dispatcher for an ACL link variant's create validation.
/// Reduces the four `LinkTypes::HummContent*` arms to one helper call
/// site per arm; preserves the link's class discriminator across the
/// dispatch boundary so the per-class entity-id membership check
/// (Owner/Admin/Writer/Reader) runs against the right ACL field set.
fn dispatch_acl_create_link(
    class: AclLinkClass,
    action: CreateLink,
    base: AnyLinkableHash,
    target: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    validate_create_link_humm_content_acl(action, base, target, tag, class)
}

/// Mirror of [`dispatch_acl_create_link`] for the delete side.
/// `class_label` is forwarded into the error message so the failing
/// link type is identifiable from the validation error alone.
fn dispatch_acl_delete_link(
    class_label: &str,
    action: DeleteLink,
    original_action: CreateLink,
    base: AnyLinkableHash,
    target: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    validate_delete_link_humm_content_acl(
        action,
        original_action,
        base,
        target,
        tag,
        class_label,
    )
}


/// Fetch the original record being deleted and dispatch to the
/// per-entry-type delete validator.
///
/// hdi 0.7.0's [`FlatOp::RegisterDelete`] only carries the [`Delete`]
/// action (`OpDelete { action }`); the original record + classified
/// entry must be fetched here. The same routing runs under
/// `FlatOp::StoreRecord::DeleteEntry` for chain-store validation
/// — the two arms share this helper so the semantics cannot drift.
fn dispatch_delete_entry(action: Delete) -> ExternResult<ValidateCallbackResult> {
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
    }
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
                EntryTypes::HiveGenesis(genesis) => {
                    validate_create_hive_genesis(
                        EntryCreationAction::Create(action),
                        genesis,
                    )
                }
                EntryTypes::HiveMembership(membership) => {
                    validate_create_hive_membership(
                        EntryCreationAction::Create(action),
                        membership,
                    )
                }
                EntryTypes::DmProbeLog(_) => Ok(ValidateCallbackResult::Valid),
            },
            OpEntry::UpdateEntry {
                app_entry, action, ..
            } => match app_entry {
                EntryTypes::EncryptedContent(encrypted_content) => {
                    validate_update_encrypted_content(action, encrypted_content)
                }
                EntryTypes::HiveGenesis(genesis) => {
                    validate_update_hive_genesis(action, genesis)
                }
                EntryTypes::HiveMembership(membership) => {
                    validate_update_hive_membership(action, membership)
                }
                EntryTypes::DmProbeLog(_) => Ok(ValidateCallbackResult::Valid),
            },
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterUpdate(update_entry) => match update_entry {
            OpUpdate::Entry { app_entry, action } => match app_entry {
                EntryTypes::EncryptedContent(encrypted_content) => {
                    validate_update_encrypted_content(action, encrypted_content)
                }
                EntryTypes::HiveGenesis(genesis) => {
                    validate_update_hive_genesis(action, genesis)
                }
                EntryTypes::HiveMembership(membership) => {
                    validate_update_hive_membership(action, membership)
                }
                EntryTypes::DmProbeLog(_) => Ok(ValidateCallbackResult::Valid),
            },
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterDelete(OpDelete { action }) => {
            // hdi 0.7.0's RegisterDelete only carries the Delete action;
            // the original record must be fetched + classified here. We
            // route through the same dispatch helper used by the
            // StoreRecord::DeleteEntry arm below to keep the two paths
            // semantically identical.
            dispatch_delete_entry(action)
        }
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
            LinkTypes::Hive => {
                validate_create_link_hive(action, base_address, target_address, tag)
            }
            LinkTypes::Dynamic => {
                validate_create_link_dynamic(action, base_address, target_address, tag)
            }
            LinkTypes::HummContentId => {
                validate_create_link_humm_content_id(action, base_address, target_address, tag)
            }
            LinkTypes::HummContentOwner => dispatch_acl_create_link(
                AclLinkClass::Owner,
                action,
                base_address,
                target_address,
                tag,
            ),
            LinkTypes::HummContentAdmin => dispatch_acl_create_link(
                AclLinkClass::Admin,
                action,
                base_address,
                target_address,
                tag,
            ),
            LinkTypes::HummContentWriter => dispatch_acl_create_link(
                AclLinkClass::Writer,
                action,
                base_address,
                target_address,
                tag,
            ),
            LinkTypes::HummContentReader => dispatch_acl_create_link(
                AclLinkClass::Reader,
                action,
                base_address,
                target_address,
                tag,
            ),
            LinkTypes::Inbox => {
                validate_create_link_inbox(action, base_address, target_address, tag)
            }
            // OriginalHashPointer is a self-link / chain pointer; no
            // structural recompute is feasible without re-deriving the
            // entire EncryptedContent header — and the link only
            // points within an entry's own update chain so impact is
            // self-contained. TimePath/TimeItem are bookkeeping links
            // for the time_indexing crate (currently commented-out at
            // coordinator); validating without crate context would be
            // brittle. Both remain `Valid` until called for.
            LinkTypes::OriginalHashPointer => Ok(ValidateCallbackResult::Valid),
            LinkTypes::TimePath => Ok(ValidateCallbackResult::Valid),
            LinkTypes::TimeItem => Ok(ValidateCallbackResult::Valid),
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
            LinkTypes::Hive => validate_delete_link_hive(
                action,
                original_action,
                base_address,
                target_address,
                tag,
            ),
            LinkTypes::Dynamic => validate_delete_link_dynamic(
                action,
                original_action,
                base_address,
                target_address,
                tag,
            ),
            LinkTypes::HummContentId => validate_delete_link_humm_content_id(
                action,
                original_action,
                base_address,
                target_address,
                tag,
            ),
            LinkTypes::HummContentOwner => dispatch_acl_delete_link(
                "HummContentOwner",
                action,
                original_action,
                base_address,
                target_address,
                tag,
            ),
            LinkTypes::HummContentAdmin => dispatch_acl_delete_link(
                "HummContentAdmin",
                action,
                original_action,
                base_address,
                target_address,
                tag,
            ),
            LinkTypes::HummContentWriter => dispatch_acl_delete_link(
                "HummContentWriter",
                action,
                original_action,
                base_address,
                target_address,
                tag,
            ),
            LinkTypes::HummContentReader => dispatch_acl_delete_link(
                "HummContentReader",
                action,
                original_action,
                base_address,
                target_address,
                tag,
            ),
            LinkTypes::Inbox => validate_delete_link_inbox(
                action,
                original_action,
                base_address,
                target_address,
                tag,
            ),
            LinkTypes::OriginalHashPointer => Ok(ValidateCallbackResult::Valid),
            LinkTypes::TimePath => Ok(ValidateCallbackResult::Valid),
            LinkTypes::TimeItem => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::StoreRecord(store_record) => match store_record {
            OpRecord::CreateEntry { app_entry, action } => match app_entry {
                EntryTypes::EncryptedContent(encrypted_content) => {
                    validate_create_encrypted_content(
                        EntryCreationAction::Create(action),
                        encrypted_content,
                    )
                }
                EntryTypes::HiveGenesis(genesis) => {
                    validate_create_hive_genesis(
                        EntryCreationAction::Create(action),
                        genesis,
                    )
                }
                EntryTypes::HiveMembership(membership) => {
                    validate_create_hive_membership(
                        EntryCreationAction::Create(action),
                        membership,
                    )
                }
                EntryTypes::DmProbeLog(_) => Ok(ValidateCallbackResult::Valid),
            },
            OpRecord::UpdateEntry {
                app_entry, action, ..
            } => match app_entry {
                EntryTypes::EncryptedContent(encrypted_content) => {
                    let create_result = validate_create_encrypted_content(
                        EntryCreationAction::Update(action.clone()),
                        encrypted_content.clone(),
                    )?;
                    if let ValidateCallbackResult::Valid = create_result {
                        validate_update_encrypted_content(action, encrypted_content)
                    } else {
                        Ok(create_result)
                    }
                }
                EntryTypes::HiveGenesis(genesis) => {
                    validate_update_hive_genesis(action, genesis)
                }
                EntryTypes::HiveMembership(membership) => {
                    validate_update_hive_membership(action, membership)
                }
                EntryTypes::DmProbeLog(_) => Ok(ValidateCallbackResult::Valid),
            },
            OpRecord::DeleteEntry { action, .. } => dispatch_delete_entry(action),
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
                LinkTypes::Hive => {
                    validate_create_link_hive(action, base_address, target_address, tag)
                }
                LinkTypes::Dynamic => {
                    validate_create_link_dynamic(action, base_address, target_address, tag)
                }
                LinkTypes::HummContentId => validate_create_link_humm_content_id(
                    action,
                    base_address,
                    target_address,
                    tag,
                ),
                LinkTypes::HummContentOwner => dispatch_acl_create_link(
                    AclLinkClass::Owner,
                    action,
                    base_address,
                    target_address,
                    tag,
                ),
                LinkTypes::HummContentAdmin => dispatch_acl_create_link(
                    AclLinkClass::Admin,
                    action,
                    base_address,
                    target_address,
                    tag,
                ),
                LinkTypes::HummContentWriter => dispatch_acl_create_link(
                    AclLinkClass::Writer,
                    action,
                    base_address,
                    target_address,
                    tag,
                ),
                LinkTypes::HummContentReader => dispatch_acl_create_link(
                    AclLinkClass::Reader,
                    action,
                    base_address,
                    target_address,
                    tag,
                ),
                LinkTypes::Inbox => {
                    validate_create_link_inbox(action, base_address, target_address, tag)
                }
                LinkTypes::OriginalHashPointer => Ok(ValidateCallbackResult::Valid),
                LinkTypes::TimePath => Ok(ValidateCallbackResult::Valid),
                LinkTypes::TimeItem => Ok(ValidateCallbackResult::Valid),
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
                    create_link.zome_index,
                    create_link.link_type,
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
                    LinkTypes::Hive => validate_delete_link_hive(
                        action,
                        create_link.clone(),
                        base_address,
                        create_link.target_address,
                        create_link.tag,
                    ),
                    LinkTypes::Dynamic => validate_delete_link_dynamic(
                        action,
                        create_link.clone(),
                        base_address,
                        create_link.target_address,
                        create_link.tag,
                    ),
                    LinkTypes::HummContentId => validate_delete_link_humm_content_id(
                        action,
                        create_link.clone(),
                        base_address,
                        create_link.target_address,
                        create_link.tag,
                    ),
                    LinkTypes::HummContentOwner => dispatch_acl_delete_link(
                        "HummContentOwner",
                        action,
                        create_link.clone(),
                        base_address,
                        create_link.target_address,
                        create_link.tag,
                    ),
                    LinkTypes::HummContentAdmin => dispatch_acl_delete_link(
                        "HummContentAdmin",
                        action,
                        create_link.clone(),
                        base_address,
                        create_link.target_address,
                        create_link.tag,
                    ),
                    LinkTypes::HummContentWriter => dispatch_acl_delete_link(
                        "HummContentWriter",
                        action,
                        create_link.clone(),
                        base_address,
                        create_link.target_address,
                        create_link.tag,
                    ),
                    LinkTypes::HummContentReader => dispatch_acl_delete_link(
                        "HummContentReader",
                        action,
                        create_link.clone(),
                        base_address,
                        create_link.target_address,
                        create_link.tag,
                    ),
                    LinkTypes::Inbox => validate_delete_link_inbox(
                        action,
                        create_link.clone(),
                        base_address,
                        create_link.target_address,
                        create_link.tag,
                    ),
                    LinkTypes::OriginalHashPointer => Ok(ValidateCallbackResult::Valid),
                    LinkTypes::TimePath => Ok(ValidateCallbackResult::Valid),
                    LinkTypes::TimeItem => Ok(ValidateCallbackResult::Valid),
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
                    Action::AgentValidationPkg(AgentValidationPkg { membrane_proof, .. }) => {
                        validate_agent_joining(agent, membrane_proof)
                    }
                    _ => Ok(ValidateCallbackResult::Invalid(
                        "The previous action for a `CreateAgent` action must be an `AgentValidationPkg`"
                            .to_string(),
                    )),
                }
            }
            _ => Ok(ValidateCallbackResult::Valid),
        },
    }
}
