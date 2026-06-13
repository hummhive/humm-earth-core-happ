//! Read-only externs for the hive-membership infrastructure.
//!
//! - [`get_latest_membership`] resolves "what is `agent`'s most-recent
//!   valid (i.e. unexpired) membership in `hive_genesis_hash`?", used
//!   by coordinator writes to stamp `author_membership_hash` into
//!   `EncryptedContentHeader`.
//! - [`list_my_hives`] enumerates every hive the local agent founded
//!   or holds a membership in, derived from the agent's Inbox
//!   `HiveInvite` link set.
//!
//! Both queries walk Inbox links tagged `InboxEvent::HiveInvite`,
//! resolve the link targets to either `HiveGenesis` (self-founded) or
//! `HiveMembership` (granted), and project the result. This keeps the
//! discovery layer uniform with the I-C inbox infrastructure.

use content_integrity::*;
use hdk::prelude::*;

use crate::hive::crud::HiveMembershipResponse;

#[derive(Serialize, Deserialize, Debug)]
pub struct GetLatestMembershipInput {
    pub agent: AgentPubKey,
    pub hive_genesis_hash: ActionHash,
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
    let invite_byte = InboxEvent::HiveInvite.as_byte();
    let links = get_links(
        LinkQuery::try_new(AnyLinkableHash::from(input.agent.clone()), LinkTypes::Inbox)?,
        GetStrategy::Network,
    )?;
    let now = sys_time()?;

    let mut best: Option<(Timestamp, HiveMembership, ActionHash)> = None;
    for link in links {
        // Filter by tag byte.
        if link.tag.0.first().copied() != Some(invite_byte) {
            continue;
        }
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some(record) = get(target_ah.clone(), GetOptions::network())? else {
            continue;
        };
        // Targets are either HiveGenesis or HiveMembership; only
        // HiveMembership concerns this query. Decode-as-membership and
        // skip on failure.
        let Some(membership) =
            record.entry().to_app_option::<HiveMembership>().ok().flatten()
        else {
            continue;
        };
        if membership.hive_genesis_hash != input.hive_genesis_hash {
            continue;
        }
        if membership.for_agent != input.agent {
            continue;
        }
        if let Some(expiry) = membership.expiry {
            if expiry < now {
                continue;
            }
        }
        let ts = record.action().timestamp();
        if best.as_ref().map(|(prev_ts, _, _)| ts > *prev_ts).unwrap_or(true) {
            best = Some((ts, membership, target_ah));
        }
    }

    Ok(best.map(|(_, membership, hash)| HiveMembershipResponse { membership, hash }))
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

/// Enumerate every hive the local agent participates in, derived from
/// the agent's Inbox `HiveInvite` link set.
///
/// For each `HiveInvite` link:
/// - If the target is a `HiveGenesis` entry, the agent founded that
///   hive → `role: None` (implicit Owner).
/// - If the target is a `HiveMembership` entry whose `for_agent`
///   matches the local agent, surface the genesis hash + role.
///
/// Memberships pointing to other agents are filtered out (defensive —
/// shouldn't happen with well-behaved coordinators, but a modified
/// peer could in principle write a HiveInvite link with a foreign
/// target).
#[hdk_extern]
pub fn list_my_hives(_: ()) -> ExternResult<Vec<ListedHive>> {
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    let invite_byte = InboxEvent::HiveInvite.as_byte();
    let links = get_links(
        LinkQuery::try_new(AnyLinkableHash::from(my_pubkey.clone()), LinkTypes::Inbox)?,
        GetStrategy::Network,
    )?;

    let now = sys_time()?;
    let mut out: Vec<ListedHive> = Vec::new();

    for link in links {
        if link.tag.0.first().copied() != Some(invite_byte) {
            continue;
        }
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some(record) = get(target_ah.clone(), GetOptions::network())? else {
            continue;
        };
        // Try HiveGenesis first. The Inbox is open-write by design (any
        // peer can publish a HiveInvite link at any pubkey), so we MUST
        // verify the local agent actually authored the genesis before
        // surfacing it as "I founded this hive". Without this check, a
        // hostile peer can pollute the local agent's hive list with
        // arbitrary `role: None` entries (UI confusion / griefing; no
        // privilege escalation since integrity validators still gate
        // every action against the real author identity).
        // Cross-type tolerant: the Inbox carries BOTH HiveGenesis and
        // HiveMembership targets, so decoding a membership target as a
        // genesis fails by design — treat it as None and fall through,
        // never `?`-propagate (that broke the whole list for any joiner).
        if let Some(genesis) =
            record.entry().to_app_option::<HiveGenesis>().ok().flatten()
        {
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
        if let Some(membership) =
            record.entry().to_app_option::<HiveMembership>().ok().flatten()
        {
            if membership.for_agent != my_pubkey {
                continue;
            }
            if let Some(expiry) = membership.expiry {
                if expiry < now {
                    continue;
                }
            }
            // Resolve the genesis to get the display_id.
            let Some(genesis_record) = get(membership.hive_genesis_hash.clone(), GetOptions::network())? else {
                continue;
            };
            let Some(genesis) = genesis_record
                .entry()
                .to_app_option::<HiveGenesis>()
                .map_err(|e| wasm_error!(e))?
            else {
                continue;
            };
            out.push(ListedHive {
                hive_genesis_hash: membership.hive_genesis_hash,
                display_id: genesis.display_id,
                role: Some(membership.role),
            });
        }
    }

    Ok(out)
}
