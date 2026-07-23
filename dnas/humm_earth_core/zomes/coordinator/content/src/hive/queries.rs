//! Read-only externs for the hive-membership infrastructure.
//!
//! - [`get_latest_membership`] resolves "what is `agent`'s most-recent
//!   valid (i.e. unexpired) membership in `hive_genesis_hash`?", used
//!   by coordinator writes to stamp `author_membership_hash` into
//!   `EncryptedContentHeader`.
//! - [`list_my_hives`] enumerates every hive the local agent founded
//!   or holds a membership in, derived from the agent's durable
//!   `HiveMembershipIndex` link set.
//!
//! Both queries walk `HiveMembershipIndex` links (pass-7: rerouted from
//! Inbox `HiveInvite`, which a DM sweep could retract), resolve the
//! targets to either `HiveGenesis` (self-founded) or `HiveMembership`
//! (granted), and project the result.

use std::collections::{HashMap, HashSet};

use content_integrity::*;
use hdk::prelude::*;

use crate::hive::crud::HiveMembershipResponse;

/// Discriminate `record` as a `HiveGenesis` using the action's
/// `EntryType::App` entry type via `EntryTypes::deserialize_from_type`
/// dispatch, NOT msgpack shape. `GroupGenesis` is a strict field-superset
/// of `HiveGenesis` (shares `display_id` and `created_at_microseconds`),
/// so `to_app_option::<HiveGenesis>()` on a `GroupGenesis` entry succeeds
/// and silently false-positives every device-set / role-group as a "hive".
/// Returns `None` for any other entry type so callers can fall through to
/// a sibling-type decode. A deserialize failure on a recognised app type
/// (entry bytes inconsistent with their declared type — practically
/// unreachable on validator-gated data) is `warn!`-logged and treated as
/// `None`, so a single corrupt entry degrades to a skip rather than
/// failing the whole read (mirrors `lib.rs::signal_entry_deleted`).
pub(crate) fn try_decode_hive_genesis(record: &Record) -> Option<HiveGenesis> {
    let Some(EntryType::App(AppEntryDef {
        zome_index,
        entry_index,
        ..
    })) = record.action().entry_type()
    else {
        return None;
    };
    let entry = record.entry().as_option()?;
    match EntryTypes::deserialize_from_type(*zome_index, *entry_index, entry) {
        Ok(Some(EntryTypes::HiveGenesis(genesis))) => Some(genesis),
        Ok(Some(
            EntryTypes::EncryptedContent(_)
            | EntryTypes::HiveMembership(_)
            | EntryTypes::DmProbeLog(_)
            | EntryTypes::GroupGenesis(_)
            | EntryTypes::GroupMembership(_)
            | EntryTypes::HiveOwnerHandoffOffer(_)
            | EntryTypes::HiveOwnerHandoffAccept(_)
            | EntryTypes::InviteRedemption(_),
        )) => None,
        Ok(None) => None,
        Err(e) => {
            warn!(
                "try_decode_hive_genesis: deserialize_from_type failed on entry tagged App(zome_index={zome_index:?}, entry_index={entry_index:?}): {e}; treating as not-a-HiveGenesis (skip)"
            );
            None
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetLatestMembershipInput {
    pub agent: AgentPubKey,
    pub hive_genesis_hash: ActionHash,
}

fn membership_index_links(agent: &AgentPubKey, strategy: GetStrategy) -> ExternResult<Vec<Link>> {
    get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(agent.clone()),
            LinkTypes::HiveMembershipIndex,
        )?,
        strategy,
    )
}

fn consider_latest_membership(
    best: &mut Option<(Timestamp, HiveMembershipResponse)>,
    membership: HiveMembership,
    timestamp: Timestamp,
    hash: ActionHash,
    agent: &AgentPubKey,
    hive_genesis_hash: &ActionHash,
    now: Timestamp,
) {
    if &membership.hive_genesis_hash != hive_genesis_hash {
        return;
    }
    if &membership.for_agent != agent {
        return;
    }
    if let Some(expiry) = membership.expiry {
        if expiry < now {
            return;
        }
    }
    if best
        .as_ref()
        .map(|(previous_timestamp, _)| timestamp > *previous_timestamp)
        .unwrap_or(true)
    {
        *best = Some((timestamp, HiveMembershipResponse { membership, hash }));
    }
}

/// Shared walk behind both membership twins: enumerate the caller's
/// HiveMembershipIndex links, decode targets as HiveMembership (skip
/// non-memberships), filter to the (hive, agent) unexpired at now, and
/// return the newest by action timestamp. `strategy`/`options` select the
/// network vs local-store read.
fn latest_membership_via(
    input: GetLatestMembershipInput,
    strategy: GetStrategy,
    options: GetOptions,
) -> ExternResult<Option<HiveMembershipResponse>> {
    let links = membership_index_links(&input.agent, strategy)?;
    let now = sys_time()?;

    let mut best: Option<(Timestamp, HiveMembershipResponse)> = None;
    for link in links {
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some((membership, ts)) =
            crate::get_typed_entry_with_timestamp::<HiveMembership>(&target_ah, options.clone())?
        else {
            continue;
        };
        consider_latest_membership(
            &mut best,
            membership,
            ts,
            target_ah,
            &input.agent,
            &input.hive_genesis_hash,
            now,
        );
    }

    Ok(best.map(|(_, response)| response))
}

/// Return the most-recent valid (unexpired) [`HiveMembership`] for
/// `agent` in the named hive, or `None` if no such membership exists.
///
/// "Most recent" is decided by the membership entry's action timestamp
/// (later wins). Expiry is checked against `sys_time()` at call time.
///
/// Caller workflow:
/// 1. Before any `create_encrypted_content` write into hive H, the
///    coordinator (or upstream UI) calls this to fetch the local
///    agent's latest membership.
/// 2. The returned `hash` is stamped into
///    `EncryptedContentHeader.author_membership_hash` so the integrity
///    validator can verify `action.author` actually holds Writer+ in H.
/// 3. If `None`, the caller MUST either acquire a fresh membership
///    or fail the write — committing without a membership when the
///    author is not the genesis author will be rejected by the
///    integrity layer.
#[hdk_extern]
pub fn get_latest_membership(
    input: GetLatestMembershipInput,
) -> ExternResult<Option<HiveMembershipResponse>> {
    latest_membership_via(input, GetStrategy::Network, GetOptions::network())
}

/// Dormancy-proof twin of [`get_latest_membership`]: reads the caller's
/// granted membership from the local DHT store only
/// (`GetStrategy::Local` / `GetOptions::local()`), so the non-owner
/// content-stamping path resolves a membership on a peerless cell.
///
/// Best-effort: returns `Some` only if the membership link + entry
/// integrated locally while the agent was online; if the agent went
/// dormant before the grant ever reached its store, this returns
/// `None` (mirrors `get_latest_membership`'s semantics — "no recent
/// valid membership"). Issues NO network calls.
#[hdk_extern]
pub fn get_latest_membership_local(
    input: GetLatestMembershipInput,
) -> ExternResult<Option<HiveMembershipResponse>> {
    latest_membership_via(input, GetStrategy::Local, GetOptions::local())
}

pub const MEMBERSHIP_BATCH_MAX: usize = 64;

#[derive(Serialize, Deserialize, Debug)]
pub struct GetLatestMembershipsLocalManyInput {
    pub hive_genesis_hashes: Vec<ActionHash>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LatestMembershipBucket {
    pub hive_genesis_hash: ActionHash,
    #[serde(default)]
    pub membership: Option<HiveMembershipResponse>,
}

type LatestMembershipByHive = HashMap<ActionHash, Option<(Timestamp, HiveMembershipResponse)>>;

#[hdk_extern]
pub fn get_latest_memberships_local_many(
    input: GetLatestMembershipsLocalManyInput,
) -> ExternResult<Vec<LatestMembershipBucket>> {
    if input.hive_genesis_hashes.len() > MEMBERSHIP_BATCH_MAX {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "membership batch accepts at most 64 hives".into()
        )));
    }

    let agent = agent_info()?.agent_initial_pubkey;
    let best_by_hive = latest_memberships_local_by_hive(&agent, &input.hive_genesis_hashes)?;
    Ok(input
        .hive_genesis_hashes
        .into_iter()
        .map(|hive_genesis_hash| LatestMembershipBucket {
            membership: best_by_hive
                .get(&hive_genesis_hash)
                .and_then(|best| best.as_ref().map(|(_, response)| response.clone())),
            hive_genesis_hash,
        })
        .collect())
}

fn latest_memberships_local_by_hive(
    agent: &AgentPubKey,
    hive_genesis_hashes: &[ActionHash],
) -> ExternResult<LatestMembershipByHive> {
    let links = membership_index_links(agent, GetStrategy::Local)?;
    let now = sys_time()?;
    let options = GetOptions::local();
    let mut best_by_hive: LatestMembershipByHive = hive_genesis_hashes
        .iter()
        .cloned()
        .map(|hive_genesis_hash| (hive_genesis_hash, None))
        .collect();

    for link in links {
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some((membership, timestamp)) =
            crate::get_typed_entry_with_timestamp::<HiveMembership>(&target_ah, options.clone())?
        else {
            continue;
        };
        let hive_genesis_hash = membership.hive_genesis_hash.clone();
        let Some(best) = best_by_hive.get_mut(&hive_genesis_hash) else {
            continue;
        };
        consider_latest_membership(
            best,
            membership,
            timestamp,
            target_ah,
            agent,
            &hive_genesis_hash,
            now,
        );
    }

    Ok(best_by_hive)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ListedHive {
    /// The hive's cryptographic identity.
    pub hive_genesis_hash: ActionHash,
    /// Pulled from the resolved `HiveGenesis` entry for UI display.
    pub display_id: String,
    /// `None` = local agent is the hive's genesis author (implicit
    /// Owner); `Some(role)` = role granted via a `HiveMembership`.
    pub role: Option<HiveRole>,
}

fn genesis_resolves_as_hive(
    hive_genesis_hash: &ActionHash,
    cache: &mut HashMap<ActionHash, bool>,
) -> ExternResult<bool> {
    if let Some(known) = cache.get(hive_genesis_hash) {
        return Ok(*known);
    }
    let resolved = match get(hive_genesis_hash.clone(), GetOptions::network())? {
        Some(record) => try_decode_hive_genesis(&record).is_some(),
        None => false,
    };
    cache.insert(hive_genesis_hash.clone(), resolved);
    Ok(resolved)
}

/// Return the distinct valid hive ids represented by the caller's
/// network-visible membership index without resolving display metadata.
pub(crate) fn my_hive_ids_network() -> ExternResult<Vec<ActionHash>> {
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    let links = get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(my_pubkey.clone()),
            LinkTypes::HiveMembershipIndex,
        )?,
        GetStrategy::Network,
    )?;
    let now = sys_time()?;
    let mut seen = HashSet::new();
    let mut hive_ids = Vec::new();
    let mut genesis_resolvable: HashMap<ActionHash, bool> = HashMap::new();

    for link in links {
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some(record) = get(target_ah.clone(), GetOptions::network())? else {
            continue;
        };
        if try_decode_hive_genesis(&record).is_some() {
            if record.action().author() != &my_pubkey {
                continue;
            }
            if seen.insert(target_ah.clone()) {
                hive_ids.push(target_ah);
            }
            continue;
        }
        let Some(membership) = record
            .entry()
            .to_app_option::<HiveMembership>()
            .ok()
            .flatten()
        else {
            continue;
        };
        if membership.for_agent != my_pubkey {
            continue;
        }
        if let Some(expiry) = membership.expiry {
            if expiry < now {
                continue;
            }
        }
        let hive_genesis_hash = membership.hive_genesis_hash;
        if !genesis_resolves_as_hive(&hive_genesis_hash, &mut genesis_resolvable)? {
            continue;
        }
        if seen.insert(hive_genesis_hash.clone()) {
            hive_ids.push(hive_genesis_hash);
        }
    }

    Ok(hive_ids)
}

/// Enumerate every hive the local agent participates in, derived from
/// the agent's durable `HiveMembershipIndex` link set (pass-7: rerouted
/// from Inbox `HiveInvite` so a DM sweep or invite retraction no longer
/// erases hive discovery; Inbox links remain as transient notifications).
///
/// For each index link:
/// - If the target is a `HiveGenesis` entry, the agent founded that
///   hive → `role: None` (implicit Owner).
/// - If the target is a `HiveMembership` entry whose `for_agent`
///   matches the local agent, surface the genesis hash + role.
///
/// Memberships pointing to other agents are filtered out (defensive —
/// the index validator pins base = `for_agent`, so a mismatch implies
/// a link that should never have validated).
#[hdk_extern]
pub fn list_my_hives(_: ()) -> ExternResult<Vec<ListedHive>> {
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    let links = get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(my_pubkey.clone()),
            LinkTypes::HiveMembershipIndex,
        )?,
        GetStrategy::Network,
    )?;

    let now = sys_time()?;
    let mut out: Vec<ListedHive> = Vec::new();
    let mut display_cache: HashMap<ActionHash, Option<String>> = HashMap::new();

    for link in links {
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some(record) = get(target_ah.clone(), GetOptions::network())? else {
            continue;
        };
        // Defense in depth: the index validator pins base = genesis author,
        // but a foreign-authored row must never surface as "I founded this".
        if let Some(genesis) = try_decode_hive_genesis(&record) {
            if record.action().author() != &my_pubkey {
                continue;
            }
            out.push(ListedHive {
                hive_genesis_hash: target_ah,
                display_id: genesis.display_id,
                role: None,
            });
            continue;
        }
        // Fall through to HiveMembership.
        if let Some(membership) = record
            .entry()
            .to_app_option::<HiveMembership>()
            .ok()
            .flatten()
        {
            if membership.for_agent != my_pubkey {
                continue;
            }
            if let Some(expiry) = membership.expiry {
                if expiry < now {
                    continue;
                }
            }
            let hive_genesis_hash = membership.hive_genesis_hash.clone();
            let display_id = if let Some(cached) = display_cache.get(&hive_genesis_hash) {
                cached.clone()
            } else {
                let resolved = match get(hive_genesis_hash.clone(), GetOptions::network())? {
                    Some(genesis_record) => match try_decode_hive_genesis(&genesis_record) {
                        Some(genesis) => Some(genesis.display_id),
                        None => {
                            warn!(
                                "list_my_hives: hash {} for a valid membership is not a HiveGenesis entry; skipping (DHT corruption or membership grantor authored a foreign type?)",
                                hive_genesis_hash
                            );
                            None
                        }
                    },
                    None => None,
                };
                display_cache.insert(hive_genesis_hash, resolved.clone());
                resolved
            };
            let Some(display_id) = display_id else {
                continue;
            };
            out.push(ListedHive {
                hive_genesis_hash: membership.hive_genesis_hash,
                display_id,
                role: Some(membership.role),
            });
        }
    }

    Ok(out)
}

/// Dormancy-proof twin of [`list_my_hives`]. Reads the caller's OWN
/// source chain (founder hives) and local DHT store (joined hives)
/// only — no network authority is consulted, so it returns the
/// agent's hives even on a peerless cell where `list_my_hives`
/// (`GetStrategy::Network`) returns `[]`.
///
/// Founder branch (deterministic): `query(ChainQueryFilter)` reads the
/// caller's source chain synchronously; every `HiveGenesis` it returns
/// was self-authored → `role: None`. `HiveGenesis` is immutable, so no
/// liveness filtering is needed.
///
/// Joiner branch (best-effort): walks the local `HiveMembershipIndex`
/// link store with `GetStrategy::Local` / `GetOptions::local()`. The
/// agent is an authority for its own pubkey base, so its index links +
/// targets integrate locally while online. A grant that never reached the
/// local store before dormancy is invisible here — that is correct
/// "best-effort" semantics and is documented in the rescue handoff.
#[hdk_extern]
pub fn list_my_hives_local(_: ()) -> ExternResult<Vec<ListedHive>> {
    let mut out: Vec<ListedHive> = Vec::new();

    // Founder hives: self-authored HiveGenesis on the local source chain.
    let records = query(ChainQueryFilter::new().include_entries(true))?;
    for record in &records {
        if !matches!(record.action(), Action::Create(_)) {
            continue;
        }
        let Some(genesis) = try_decode_hive_genesis(record) else {
            continue;
        };
        out.push(ListedHive {
            hive_genesis_hash: record.action_address().clone(),
            display_id: genesis.display_id,
            role: None,
        });
    }

    // Joiner hives: granted memberships in the local DHT store. The agent
    // is an authority for its own pubkey base, so its HiveMembershipIndex
    // links integrated locally while online (dormancy-proof, best-effort).
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    let now = sys_time()?;
    let links = get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(my_pubkey.clone()),
            LinkTypes::HiveMembershipIndex,
        )?,
        GetStrategy::Local,
    )?;
    let options = GetOptions::local();
    for link in links {
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some((membership, _)) =
            crate::get_typed_entry_with_timestamp::<HiveMembership>(&target_ah, options.clone())?
        else {
            // Founder self-link (HiveGenesis target) or undecodable → skip.
            continue;
        };
        if membership.for_agent != my_pubkey {
            continue;
        }
        if let Some(expiry) = membership.expiry {
            if expiry < now {
                continue;
            }
        }
        if out
            .iter()
            .any(|h| h.hive_genesis_hash == membership.hive_genesis_hash)
        {
            // Already added by the founder branch or a prior membership.
            continue;
        }
        let Some(genesis_record) = get(membership.hive_genesis_hash.clone(), GetOptions::local())?
        else {
            continue;
        };
        let Some(genesis) = try_decode_hive_genesis(&genesis_record) else {
            warn!(
                "list_my_hives_local: hash {} for a valid local membership is not a HiveGenesis entry; skipping (local-store corruption or membership grantor authored a foreign type?)",
                membership.hive_genesis_hash
            );
            continue;
        };
        out.push(ListedHive {
            hive_genesis_hash: membership.hive_genesis_hash,
            display_id: genesis.display_id,
            role: Some(membership.role),
        });
    }

    Ok(out)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChangesSinceInput {
    pub hive_genesis_hash: ActionHash,
    pub content_types: Vec<String>,
    pub since_seq: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChangesSinceSummary {
    pub new_action_count: usize,
    pub latest_seq: u32,
}

/// Local-source-chain delta probe for the caller's OWN commits to this hive
/// after `since_seq`. Peers' content isn't visible here (use content_summary).
#[hdk_extern]
pub fn changes_since(input: ChangesSinceInput) -> ExternResult<ChangesSinceSummary> {
    let hive_paths: Vec<AnyLinkableHash> = input
        .content_types
        .into_iter()
        .map(|content_type| {
            Path::from(vec![
                Component::from(input.hive_genesis_hash.to_string()),
                Component::from(content_type),
            ])
            .path_entry_hash()
            .map(AnyLinkableHash::from)
        })
        .collect::<ExternResult<Vec<_>>>()?;

    let recent = query(
        ChainQueryFilter::new()
            .sequence_range(ChainQueryFilterRange::ActionSeqRange(
                input.since_seq.saturating_add(1),
                u32::MAX,
            ))
            .include_entries(false),
    )?;
    let new_action_count = recent
        .iter()
        .filter(|record| {
            matches!(record.action(), Action::CreateLink(cl) if hive_paths.contains(&cl.base_address))
        })
        .count();

    let latest_seq = query(ChainQueryFilter::new().include_entries(false))?
        .last()
        .map(|record| record.action().action_seq())
        .unwrap_or(0);

    Ok(ChangesSinceSummary {
        new_action_count,
        latest_seq,
    })
}
