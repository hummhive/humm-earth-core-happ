use hdi::prelude::*;

use super::authority::{fetch_authored_entry, fetch_genesis};
use super::types::{HiveOwnerHandoffAccept, HiveOwnerHandoffOffer};
use crate::group::link_authors_target_entry;

fn fetch_owner_handoff_offer(
    offer_hash: &ActionHash,
) -> ExternResult<(AgentPubKey, HiveOwnerHandoffOffer)> {
    fetch_authored_entry(offer_hash, "HiveOwnerHandoffOffer")
}

fn fetch_owner_handoff_accept(
    accept_hash: &ActionHash,
) -> ExternResult<(AgentPubKey, HiveOwnerHandoffAccept)> {
    fetch_authored_entry(accept_hash, "HiveOwnerHandoffAccept")
}

/// EVER-owner, not current-owner: a validator cannot detect a completed
/// downstream transfer without forbidden link-enumeration, so this proves
/// only that `agent` is the genesis root or a past handoff recipient. The
/// coordinator's resolve_current_owner folds the lineage to the live owner.
pub fn is_lineage_owner(
    agent: &AgentPubKey,
    genesis_hash: &ActionHash,
    owner_accept_hash: Option<&ActionHash>,
) -> ExternResult<bool> {
    let Some(accept_hash) = owner_accept_hash else {
        let (genesis_author, _) = fetch_genesis(genesis_hash)?;
        return Ok(&genesis_author == agent);
    };
    let (_, accept) = fetch_owner_handoff_accept(accept_hash)?;
    let (_, offer) = fetch_owner_handoff_offer(&accept.offer_hash)?;
    Ok(&offer.to_agent == agent && &offer.hive_genesis_hash == genesis_hash)
}

pub fn validate_create_hive_owner_handoff_offer(
    action: EntryCreationAction,
    offer: HiveOwnerHandoffOffer,
) -> ExternResult<ValidateCallbackResult> {
    let offerer = action.author();
    if &offer.to_agent == offerer {
        return Ok(ValidateCallbackResult::Invalid(
            "cannot hand off ownership to yourself".into(),
        ));
    }
    let offerer_is_owner = is_lineage_owner(
        offerer,
        &offer.hive_genesis_hash,
        offer.offerer_owner_accept_hash.as_ref(),
    )?;
    if !offerer_is_owner {
        return Ok(ValidateCallbackResult::Invalid(
            "offer author is not an owner of the hive".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_update_hive_owner_handoff_offer(
    _action: Update,
    _entry: HiveOwnerHandoffOffer,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "HiveOwnerHandoffOffer entries are immutable".into(),
    ))
}

pub fn validate_delete_hive_owner_handoff_offer(
    _action: Delete,
    _original_action: EntryCreationAction,
    _original_entry: HiveOwnerHandoffOffer,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "HiveOwnerHandoffOffer is immutable; cancel a pending offer by deleting its AgentToOwnerHandoffs link"
            .into(),
    ))
}

pub fn validate_create_hive_owner_handoff_accept(
    action: EntryCreationAction,
    accept: HiveOwnerHandoffAccept,
) -> ExternResult<ValidateCallbackResult> {
    // No one-accept-per-offer check: forks are tolerated and the coordinator
    // de-duplicates by offer hash when resolving the current owner.
    let (_, offer) = fetch_owner_handoff_offer(&accept.offer_hash)?;
    if &offer.to_agent != action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "accept author is not the offer's to_agent".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_update_hive_owner_handoff_accept(
    _action: Update,
    _entry: HiveOwnerHandoffAccept,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "HiveOwnerHandoffAccept entries are immutable".into(),
    ))
}

pub fn validate_delete_hive_owner_handoff_accept(
    _action: Delete,
    _original_action: EntryCreationAction,
    _original_entry: HiveOwnerHandoffAccept,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "HiveOwnerHandoffAccept entries cannot be deleted".into(),
    ))
}

pub fn validate_create_link_agent_to_owner_handoffs(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    if !tag.0.is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "AgentToOwnerHandoffs link tag must be empty".into(),
        ));
    }
    let offer: HiveOwnerHandoffOffer = match link_authors_target_entry(&action, &target_address)? {
        Ok(offer) => offer,
        Err(invalid) => return Ok(invalid),
    };
    if base_address != AnyLinkableHash::from(offer.to_agent.clone()) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "AgentToOwnerHandoffs base {base_address} does not match offer.to_agent {}",
            offer.to_agent,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_link_hive_to_owner_handoffs(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    if !tag.0.is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "HiveToOwnerHandoffs link tag must be empty".into(),
        ));
    }
    let accept: HiveOwnerHandoffAccept = match link_authors_target_entry(&action, &target_address)?
    {
        Ok(accept) => accept,
        Err(invalid) => return Ok(invalid),
    };
    let (_, offer) = fetch_owner_handoff_offer(&accept.offer_hash)?;
    if base_address != AnyLinkableHash::from(offer.hive_genesis_hash.clone()) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveToOwnerHandoffs base {base_address} does not match offer.hive_genesis_hash {}",
            offer.hive_genesis_hash,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}
