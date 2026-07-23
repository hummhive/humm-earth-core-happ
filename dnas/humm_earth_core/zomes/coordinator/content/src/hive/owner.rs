//! Owner-handoff externs and deterministic current-owner resolution.

use std::collections::BTreeMap;

use content_integrity::*;
use hdk::prelude::*;

use crate::get_typed_entry;
use crate::hive::queries::{get_latest_membership, GetLatestMembershipInput};

const MAX_OWNER_HANDOFFS: usize = 256;

#[derive(Serialize, Deserialize, Debug)]
pub struct InitiateOwnerHandoffInput {
    pub hive_genesis_hash: ActionHash,
    pub to_agent: AgentPubKey,
    pub offerer_owner_accept_hash: Option<ActionHash>,
}

#[hdk_extern]
pub fn initiate_owner_handoff(input: InitiateOwnerHandoffInput) -> ExternResult<ActionHash> {
    let hive_genesis_hash = input.hive_genesis_hash.clone();
    let recipient = input.to_agent.clone();
    let offer = HiveOwnerHandoffOffer {
        hive_genesis_hash: input.hive_genesis_hash,
        to_agent: input.to_agent,
        offerer_owner_accept_hash: input.offerer_owner_accept_hash,
        created_at_microseconds: sys_time()?.as_micros() as i64,
    };
    let offer_hash = create_entry(&EntryTypes::HiveOwnerHandoffOffer(offer))?;
    create_link(
        AnyLinkableHash::from(recipient.clone()),
        AnyLinkableHash::from(offer_hash.clone()),
        LinkTypes::AgentToOwnerHandoffs,
        LinkTag::new(Vec::new()),
    )?;
    send_owner_handoff_offer_hint(&offer_hash, &hive_genesis_hash, recipient);
    Ok(offer_hash)
}

/// Fetch-hint that an owner-handoff offer awaits the recipient, so the
/// governance panel reacts without polling; carries identifiers only, the
/// recipient re-reads the durable offer for authority.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct OwnerHandoffOfferHint {
    pub offer_hash: ActionHash,
    pub hive_genesis_hash: ActionHash,
    /// Stamped by recv_remote_signal from call_info().provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_agent: Option<AgentPubKey>,
}

/// Best-effort: a failed hint send never blocks the committed offer.
fn send_owner_handoff_offer_hint(
    offer_hash: &ActionHash,
    hive_genesis_hash: &ActionHash,
    recipient: AgentPubKey,
) {
    let hint = OwnerHandoffOfferHint {
        offer_hash: offer_hash.clone(),
        hive_genesis_hash: hive_genesis_hash.clone(),
        from_agent: None,
    };
    let payload = match ExternIO::encode(&hint) {
        Ok(payload) => payload,
        Err(err) => {
            warn!("initiate_owner_handoff: offer hint encode failed (non-fatal): {err:?}");
            return;
        }
    };
    if let Err(err) = send_remote_signal(payload, vec![recipient]) {
        warn!("initiate_owner_handoff: offer hint send failed (non-fatal): {err:?}");
    }
}

#[hdk_extern]
pub fn cancel_owner_handoff(offer_hash: ActionHash) -> ExternResult<()> {
    crate::delete_own_links_targeting(AnyLinkableHash::from(offer_hash))?;
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AcceptOwnerHandoffInput {
    pub offer_hash: ActionHash,
}

#[hdk_extern]
pub fn accept_owner_handoff(input: AcceptOwnerHandoffInput) -> ExternResult<ActionHash> {
    let offer: HiveOwnerHandoffOffer = get_typed_entry(&input.offer_hash)?.ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "accept_owner_handoff: offer {} not found",
            input.offer_hash
        )))
    })?;
    let accept_hash = create_entry(&EntryTypes::HiveOwnerHandoffAccept(
        HiveOwnerHandoffAccept {
            offer_hash: input.offer_hash,
        },
    ))?;
    create_link(
        AnyLinkableHash::from(offer.hive_genesis_hash),
        AnyLinkableHash::from(accept_hash.clone()),
        LinkTypes::HiveToOwnerHandoffs,
        LinkTag::new(Vec::new()),
    )?;
    Ok(accept_hash)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PendingOwnerHandoff {
    pub offer_hash: ActionHash,
    pub offer: HiveOwnerHandoffOffer,
}

#[hdk_extern]
pub fn list_pending_owner_handoffs(_: ()) -> ExternResult<Vec<PendingOwnerHandoff>> {
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    let links = get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(my_pubkey),
            LinkTypes::AgentToOwnerHandoffs,
        )?,
        GetStrategy::Network,
    )?;
    let mut pending = Vec::new();
    for link in links {
        let Some(offer_hash) = link.target.into_action_hash() else {
            continue;
        };
        let Some(offer) = get_typed_entry::<HiveOwnerHandoffOffer>(&offer_hash)? else {
            continue;
        };
        pending.push(PendingOwnerHandoff { offer_hash, offer });
    }
    Ok(pending)
}

/// Resolve the current owner AND whether the lineage is contested. The owner is
/// the genesis author folded forward through the validated offer→accept lineage,
/// taking the smallest offer `ActionHash` at any fork. A malicious PAST owner can
/// fork the lineage to re-seize ownership (an irreducible cross-chain double-spend
/// — see the pass-5 residual); the fork is detected (`contested`) + `warn!`ed but
/// cannot be prevented here, only mitigated by the current-owner prechecks.
fn resolve_owner_state(hive_genesis_hash: &ActionHash) -> ExternResult<(AgentPubKey, bool)> {
    let genesis_record =
        get(hive_genesis_hash.clone(), GetOptions::network())?.ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "resolve_current_owner: hive genesis {hive_genesis_hash} not found"
            )))
        })?;
    let genesis_author = genesis_record.action().author().clone();

    let mut links = get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(hive_genesis_hash.clone()),
            LinkTypes::HiveToOwnerHandoffs,
        )?,
        GetStrategy::Network,
    )?;
    // Deterministic bound: every node must keep the SAME subset under the cap,
    // so order by target before truncating the unordered get_links result.
    links.sort_by(|a, b| a.target.cmp(&b.target));

    let mut accepted_offers: BTreeMap<ActionHash, HiveOwnerHandoffOffer> = BTreeMap::new();
    let mut accept_to_offer: BTreeMap<ActionHash, ActionHash> = BTreeMap::new();
    for link in links.into_iter().take(MAX_OWNER_HANDOFFS) {
        let Some(accept_hash) = link.target.into_action_hash() else {
            continue;
        };
        let Some(accept) = get_typed_entry::<HiveOwnerHandoffAccept>(&accept_hash)? else {
            continue;
        };
        let offer_hash = accept.offer_hash;
        if !accepted_offers.contains_key(&offer_hash) {
            let Some(offer) = get_typed_entry::<HiveOwnerHandoffOffer>(&offer_hash)? else {
                continue;
            };
            if &offer.hive_genesis_hash != hive_genesis_hash {
                continue;
            }
            accepted_offers.insert(offer_hash.clone(), offer);
        }
        accept_to_offer.insert(accept_hash, offer_hash);
    }

    Ok(fold_current_owner(
        genesis_author,
        &accepted_offers,
        &accept_to_offer,
        hive_genesis_hash,
    ))
}

/// The current hive owner (genesis author or the validly-descended handoff
/// recipient). See [`resolve_owner_state`] for the contested-fork residual.
pub(crate) fn resolve_current_owner(hive_genesis_hash: &ActionHash) -> ExternResult<AgentPubKey> {
    Ok(resolve_owner_state(hive_genesis_hash)?.0)
}

/// True iff the ownership lineage has a contested position (>1 accepted onward
/// offer somewhere — only reachable via a past-owner fork). Detection only: the
/// fork is irreducible (cross-chain double-spend), so the UI surfaces a warning
/// for out-of-band review rather than the zome preventing it.
#[hdk_extern]
pub fn is_ownership_contested(hive_genesis_hash: ActionHash) -> ExternResult<bool> {
    Ok(resolve_owner_state(&hive_genesis_hash)?.1)
}

/// Walk the validated lineage from the genesis root, taking the smallest-offer-hash
/// child at each step. Returns `(owner, contested)`; `contested` (also `warn!`ed)
/// flags a position with >1 accepted onward-offer — a past-owner fork.
fn fold_current_owner(
    genesis_author: AgentPubKey,
    accepted_offers: &BTreeMap<ActionHash, HiveOwnerHandoffOffer>,
    accept_to_offer: &BTreeMap<ActionHash, ActionHash>,
    hive_genesis_hash: &ActionHash,
) -> (AgentPubKey, bool) {
    let mut owner = genesis_author;
    let mut current_offer: Option<ActionHash> = None;
    let mut contested = false;
    for _ in 0..=accepted_offers.len() {
        let mut chosen: Option<(&ActionHash, &HiveOwnerHandoffOffer)> = None;
        let mut match_count = 0usize;
        for (offer_hash, offer) in accepted_offers.iter() {
            // A missing/unresolved parent accept is an INCOMPLETE lineage, never a
            // root: collapsing it to None would let a partially-synced node promote
            // an orphan offer to root and resolve a different owner.
            let is_child = match offer.offerer_owner_accept_hash.as_ref() {
                None => current_offer.is_none(),
                Some(accept_hash) => match accept_to_offer.get(accept_hash) {
                    Some(parent_offer) => current_offer.as_ref() == Some(parent_offer),
                    None => false,
                },
            };
            if is_child {
                match_count += 1;
                // BTreeMap iterates ascending by offer hash, so the first match is
                // the smallest-offer-hash child (the deterministic fork tiebreak).
                if chosen.is_none() {
                    chosen = Some((offer_hash, offer));
                }
            }
        }
        let Some((offer_hash, offer)) = chosen else {
            break;
        };
        if match_count > 1 {
            contested = true;
            warn!(
                "resolve_current_owner: contested ownership for hive {hive_genesis_hash}; a lineage position has multiple accepted onward-offers (possible past-owner fork)"
            );
        }
        owner = offer.to_agent.clone();
        current_offer = Some(offer_hash.clone());
    }
    (owner, contested)
}

fn membership_role(
    hive_genesis_hash: &ActionHash,
    agent: &AgentPubKey,
) -> ExternResult<Option<HiveRole>> {
    let latest = get_latest_membership(GetLatestMembershipInput {
        hive_genesis_hash: hive_genesis_hash.clone(),
        agent: agent.clone(),
    })?;
    Ok(latest
        .map(|response| response.membership.role)
        .filter(|role| *role != HiveRole::Owner))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetMemberHiveRoleInput {
    pub hive_genesis_hash: ActionHash,
    pub agent: AgentPubKey,
}

#[hdk_extern]
pub fn get_member_hive_role(input: GetMemberHiveRoleInput) -> ExternResult<Option<HiveRole>> {
    if resolve_current_owner(&input.hive_genesis_hash)? == input.agent {
        return Ok(Some(HiveRole::Owner));
    }
    membership_role(&input.hive_genesis_hash, &input.agent)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListMemberHiveRolesInput {
    pub hive_genesis_hash: ActionHash,
    pub agents: Vec<AgentPubKey>,
}

#[hdk_extern]
pub fn list_member_hive_roles(
    input: ListMemberHiveRolesInput,
) -> ExternResult<Vec<(AgentPubKey, Option<HiveRole>)>> {
    let owner = resolve_current_owner(&input.hive_genesis_hash)?;
    let mut roles = Vec::with_capacity(input.agents.len());
    for agent in input.agents {
        let role = if agent == owner {
            Some(HiveRole::Owner)
        } else {
            membership_role(&input.hive_genesis_hash, &agent)?
        };
        roles.push((agent, role));
    }
    Ok(roles)
}

#[hdk_extern]
pub fn get_hive_owner(hive_genesis_hash: ActionHash) -> ExternResult<AgentPubKey> {
    resolve_current_owner(&hive_genesis_hash)
}
