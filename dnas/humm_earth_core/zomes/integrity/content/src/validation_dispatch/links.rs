use hdi::prelude::*;

use crate::*;

pub(super) fn dispatch_create_link(
    link_type: LinkTypes,
    action: CreateLink,
    base: AnyLinkableHash,
    target: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    match link_type {
        LinkTypes::EncryptedContentUpdates => {
            validate_create_link_encrypted_content_updates(action, base, target, tag)
        }
        LinkTypes::Hive => validate_create_link_hive(action, base, target, tag),
        LinkTypes::Dynamic => validate_create_link_dynamic(action, base, target, tag),
        LinkTypes::HummContentId => validate_create_link_humm_content_id(action, base, target, tag),
        LinkTypes::HummContentOwner => {
            dispatch_acl_create_link(AclLinkClass::Owner, action, base, target, tag)
        }
        LinkTypes::HummContentAdmin => {
            dispatch_acl_create_link(AclLinkClass::Admin, action, base, target, tag)
        }
        LinkTypes::HummContentWriter => {
            dispatch_acl_create_link(AclLinkClass::Writer, action, base, target, tag)
        }
        LinkTypes::HummContentReader => {
            dispatch_acl_create_link(AclLinkClass::Reader, action, base, target, tag)
        }
        LinkTypes::Inbox => validate_create_link_inbox(action, base, target, tag),
        LinkTypes::AgentToGroupMemberships => {
            validate_create_link_agent_to_group_memberships(action, base, target, tag)
        }
        LinkTypes::GroupToGroupMemberships => {
            validate_create_link_group_to_group_memberships(action, base, target, tag)
        }
        LinkTypes::HiveToGroups => validate_create_link_hive_to_groups(action, base, target, tag),
        LinkTypes::AgentToOwnerHandoffs => {
            validate_create_link_agent_to_owner_handoffs(action, base, target, tag)
        }
        LinkTypes::HiveToOwnerHandoffs => {
            validate_create_link_hive_to_owner_handoffs(action, base, target, tag)
        }
        LinkTypes::InviteToRedemptions => {
            validate_create_link_invite_to_redemptions(action, base, target, tag)
        }
        LinkTypes::OriginalHashPointer => {
            validate_create_link_original_hash_pointer(action, base, target, tag)
        }
        LinkTypes::TimePath | LinkTypes::TimeItem => Ok(ValidateCallbackResult::Valid),
    }
}

pub(super) fn dispatch_delete_link(
    link_type: LinkTypes,
    action: DeleteLink,
    original_action: CreateLink,
    base: AnyLinkableHash,
    target: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    match link_type {
        LinkTypes::EncryptedContentUpdates => validate_delete_link_encrypted_content_updates(
            action,
            original_action,
            base,
            target,
            tag,
        ),
        LinkTypes::Hive => validate_delete_link_hive(action, original_action, base, target, tag),
        LinkTypes::Dynamic => {
            validate_delete_link_dynamic(action, original_action, base, target, tag)
        }
        LinkTypes::HummContentId => {
            validate_delete_link_humm_content_id(action, original_action, base, target, tag)
        }
        LinkTypes::HummContentOwner => dispatch_acl_delete_link(
            "HummContentOwner",
            action,
            original_action,
            base,
            target,
            tag,
        ),
        LinkTypes::HummContentAdmin => dispatch_acl_delete_link(
            "HummContentAdmin",
            action,
            original_action,
            base,
            target,
            tag,
        ),
        LinkTypes::HummContentWriter => dispatch_acl_delete_link(
            "HummContentWriter",
            action,
            original_action,
            base,
            target,
            tag,
        ),
        LinkTypes::HummContentReader => dispatch_acl_delete_link(
            "HummContentReader",
            action,
            original_action,
            base,
            target,
            tag,
        ),
        LinkTypes::Inbox => validate_delete_link_inbox(action, original_action, base, target, tag),
        LinkTypes::AgentToGroupMemberships => {
            validate_delete_group_link(action, original_action, "AgentToGroupMemberships")
        }
        LinkTypes::GroupToGroupMemberships => {
            validate_delete_group_link(action, original_action, "GroupToGroupMemberships")
        }
        LinkTypes::HiveToGroups => {
            validate_delete_group_link(action, original_action, "HiveToGroups")
        }
        LinkTypes::AgentToOwnerHandoffs => {
            validate_delete_group_link(action, original_action, "AgentToOwnerHandoffs")
        }
        LinkTypes::HiveToOwnerHandoffs => {
            validate_delete_group_link(action, original_action, "HiveToOwnerHandoffs")
        }
        LinkTypes::InviteToRedemptions => {
            validate_delete_group_link(action, original_action, "InviteToRedemptions")
        }
        LinkTypes::OriginalHashPointer => {
            validate_delete_link_original_hash_pointer(action, original_action, base, target, tag)
        }
        LinkTypes::TimePath | LinkTypes::TimeItem => Ok(ValidateCallbackResult::Valid),
    }
}

pub(super) fn dispatch_store_record_delete_link(
    action: DeleteLink,
    original_action_hash: ActionHash,
    base: AnyLinkableHash,
) -> ExternResult<ValidateCallbackResult> {
    let record = must_get_valid_record(original_action_hash)?;
    let create_link = match record.action() {
        Action::CreateLink(create_link) => create_link.clone(),
        _ => {
            return Ok(ValidateCallbackResult::Invalid(
                "The action that a DeleteLink deletes must be a CreateLink".to_string(),
            ));
        }
    };
    let Some(link_type) = LinkTypes::from_type(create_link.zome_index, create_link.link_type)?
    else {
        return Ok(ValidateCallbackResult::Valid);
    };
    let target = create_link.target_address.clone();
    let tag = create_link.tag.clone();
    dispatch_delete_link(link_type, action, create_link, base, target, tag)
}

fn dispatch_acl_create_link(
    class: AclLinkClass,
    action: CreateLink,
    base: AnyLinkableHash,
    target: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    validate_create_link_humm_content_acl(action, base, target, tag, class)
}

fn dispatch_acl_delete_link(
    class_label: &str,
    action: DeleteLink,
    original_action: CreateLink,
    base: AnyLinkableHash,
    target: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    validate_delete_link_humm_content_acl(action, original_action, base, target, tag, class_label)
}
