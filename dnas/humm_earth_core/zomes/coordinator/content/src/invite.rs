//! Approver-side invite redemption with an advisory `max_uses` soft cap.

use std::collections::BTreeSet;

use content_integrity::*;
use hdk::prelude::*;

use crate::get_typed_entry;
use crate::hive::crud::{
    create_hive_membership, CreateHiveMembershipInput, HiveMembershipResponse,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct RedeemInviteGrantInput {
    pub invite_action_hash: ActionHash,
    pub max_uses: Option<u32>,
    pub membership: CreateHiveMembershipInput,
}

#[hdk_extern]
pub fn redeem_invite_grant(input: RedeemInviteGrantInput) -> ExternResult<HiveMembershipResponse> {
    let redeemer = input.membership.for_agent.clone();
    let links = get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(input.invite_action_hash.clone()),
            LinkTypes::InviteToRedemptions,
        )?,
        GetStrategy::Network,
    )?;
    let mut redeemers: BTreeSet<AgentPubKey> = BTreeSet::new();
    for link in links {
        let Some(redemption_hash) = link.target.into_action_hash() else {
            continue;
        };
        if let Some(redemption) = get_typed_entry::<InviteRedemption>(&redemption_hash)? {
            redeemers.insert(redemption.redeemer);
        }
    }

    if !redeemers.contains(&redeemer) {
        if let Some(max) = input.max_uses {
            if redeemers.len() as u32 >= max {
                return Err(wasm_error!(WasmErrorInner::Guest(
                    "invite max_uses exhausted".into(),
                )));
            }
        }
        let redemption_hash = create_entry(&EntryTypes::InviteRedemption(InviteRedemption {
            invite_action_hash: input.invite_action_hash.clone(),
            redeemer,
        }))?;
        create_link(
            AnyLinkableHash::from(input.invite_action_hash),
            AnyLinkableHash::from(redemption_hash),
            LinkTypes::InviteToRedemptions,
            LinkTag::new(Vec::new()),
        )?;
    }

    create_hive_membership(input.membership)
}
