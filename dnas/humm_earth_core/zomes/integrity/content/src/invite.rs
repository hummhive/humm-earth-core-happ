//! Advisory invite-redemption marker backing the coordinator's `max_uses` soft cap.

use hdi::prelude::*;

use crate::group::link_authors_target_entry;

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct InviteRedemption {
    pub invite_action_hash: ActionHash,
    pub redeemer: AgentPubKey,
}

pub fn validate_create_invite_redemption(
    _action: EntryCreationAction,
    _entry: InviteRedemption,
) -> ExternResult<ValidateCallbackResult> {
    // Permissionless: `redeemer` is approver-authored, so the derived count is
    // an ADVISORY cap, not authority (the validated HiveMembership is the gate).
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_update_invite_redemption(
    _action: Update,
    _entry: InviteRedemption,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "InviteRedemption entries are immutable".into(),
    ))
}

pub fn validate_delete_invite_redemption(
    _action: Delete,
    _original_action: EntryCreationAction,
    _original_entry: InviteRedemption,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "InviteRedemption entries cannot be deleted".into(),
    ))
}

pub fn validate_create_link_invite_to_redemptions(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    if !tag.0.is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "InviteToRedemptions link tag must be empty".into(),
        ));
    }
    let redemption: InviteRedemption = match link_authors_target_entry(&action, &target_address)? {
        Ok(redemption) => redemption,
        Err(invalid) => return Ok(invalid),
    };
    if base_address != AnyLinkableHash::from(redemption.invite_action_hash.clone()) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "InviteToRedemptions base {base_address} does not match redemption.invite_action_hash {}",
            redemption.invite_action_hash,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}
