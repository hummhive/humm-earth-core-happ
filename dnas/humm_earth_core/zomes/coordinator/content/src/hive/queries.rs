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
/// failing the whole dormancy-rescue read (mirrors the containment in
/// `lib.rs::signal_entry_deleted`).
fn try_decode_hive_genesis(record: &Record) -> Option<HiveGenesis> {
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
            | EntryTypes::GroupMembership(_),
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
        let Some(membership) = record
            .entry()
            .to_app_option::<HiveMembership>()
            .ok()
            .flatten()
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
        if best
            .as_ref()
            .map(|(prev_ts, _, _)| ts > *prev_ts)
            .unwrap_or(true)
        {
            best = Some((ts, membership, target_ah));
        }
    }

    Ok(best.map(|(_, membership, hash)| HiveMembershipResponse { membership, hash }))
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
    let invite_byte = InboxEvent::HiveInvite.as_byte();
    let links = get_links(
        LinkQuery::try_new(AnyLinkableHash::from(input.agent.clone()), LinkTypes::Inbox)?,
        GetStrategy::Local,
    )?;
    let now = sys_time()?;

    let mut best: Option<(Timestamp, HiveMembership, ActionHash)> = None;
    for link in links {
        if link.tag.0.first().copied() != Some(invite_byte) {
            continue;
        }
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some(record) = get(target_ah.clone(), GetOptions::local())? else {
            continue;
        };
        let Some(membership) = record
            .entry()
            .to_app_option::<HiveMembership>()
            .ok()
            .flatten()
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
        if best
            .as_ref()
            .map(|(prev_ts, _, _)| ts > *prev_ts)
            .unwrap_or(true)
        {
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
            // Resolve the genesis to get the display_id.
            let Some(genesis_record) =
                get(membership.hive_genesis_hash.clone(), GetOptions::network())?
            else {
                continue;
            };
            let Some(genesis) = try_decode_hive_genesis(&genesis_record) else {
                warn!(
                    "list_my_hives: hash {} for a valid membership is not a HiveGenesis entry; skipping (DHT corruption or membership grantor authored a foreign type?)",
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
/// Joiner branch (best-effort): walks the local Inbox link store with
/// `GetStrategy::Local` / `GetOptions::local()`. The agent is an
/// authority for its own pubkey base, so its Inbox links + targets
/// integrate locally while online. A grant that never reached the
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

    // Joiner hives: granted memberships in the local DHT store. The
    // agent is an authority for its own pubkey base, so its Inbox
    // links were integrated locally while online; GetStrategy::Local
    // reads them with NO network call (dormancy-proof). Best-effort:
    // a joined hive is recovered only if its membership link + entry
    // integrated locally before the cell went dormant.
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    let invite_byte = InboxEvent::HiveInvite.as_byte();
    let now = sys_time()?;
    let links = get_links(
        LinkQuery::try_new(AnyLinkableHash::from(my_pubkey.clone()), LinkTypes::Inbox)?,
        GetStrategy::Local,
    )?;
    for link in links {
        if link.tag.0.first().copied() != Some(invite_byte) {
            continue;
        }
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some(record) = get(target_ah, GetOptions::local())? else {
            continue;
        };
        let Some(membership) = record
            .entry()
            .to_app_option::<HiveMembership>()
            .ok()
            .flatten()
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
