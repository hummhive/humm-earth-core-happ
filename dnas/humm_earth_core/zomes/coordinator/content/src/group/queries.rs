//! Read-only externs for the group-authority infrastructure.
//!
//! - [`get_latest_group_membership`] resolves "what is `agent`'s
//!   most-recent valid (unexpired) [`GroupMembership`] in
//!   `group_genesis_hash`?" Used by content writes that need to stamp
//!   `author_group_membership_hash` into a future `AclSpec::HiveGroup`
//!   header (Phase C).
//! - [`list_group_members`] is **the authoritative cryptographic
//!   roster** — walks `GroupToGroupMemberships` links from
//!   `group_genesis_hash`, dedupes by `for_agent` taking the latest
//!   unexpired membership. Replaces the forgeable
//!   `GroupMemberList`-keyed roster lookups in humm-tauri.
//! - [`list_my_groups`] enumerates every group the local agent
//!   founded (self-Inbox `GroupInvite` links) or holds a membership
//!   in (durable `AgentToGroupMemberships` index).
//! - [`list_groups_in_hive`] enumerates every group in a hive, derived
//!   from the `HiveToGroups` link set on the hive genesis.
//! - [`get_group_genesis`] resolves a `GroupGenesis` by action hash for
//!   UI consumption.

use std::collections::{hash_map::Entry, HashMap};

use content_integrity::*;
use hdk::prelude::*;

use crate::group::crud::{GroupGenesisResponse, GroupMembershipResponse};

// =============================================================================
// get_latest_group_membership
// =============================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetLatestGroupMembershipInput {
    pub agent: AgentPubKey,
    pub group_genesis_hash: ActionHash,
}

/// Return the most-recent valid (unexpired) [`GroupMembership`] for
/// `agent` in the named group, or `None` if no such membership exists.
///
/// "Most recent" is decided by the membership entry's action
/// timestamp (later wins). Expiry is checked against `sys_time()` at
/// call time. Walks the forward index
/// (`AgentToGroupMemberships`) from `agent` — O(memberships granted to
/// agent), not O(total roster).
///
/// Caller workflow (Phase C `AclSpec::HiveGroup` content):
/// 1. Before any `create_encrypted_content` write into group G, the
///    coordinator (or upstream UI) calls this to fetch the local
///    agent's latest group membership.
/// 2. The returned `hash` is stamped into
///    `acl_spec.HiveGroup.author_group_membership_hash` so the
///    integrity validator can verify `action.author` holds Writer+ in
///    G.
#[hdk_extern]
pub fn get_latest_group_membership(
    input: GetLatestGroupMembershipInput,
) -> ExternResult<Option<GroupMembershipResponse>> {
    let links = get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(input.agent.clone()),
            LinkTypes::AgentToGroupMemberships,
        )?,
        GetStrategy::Network,
    )?;
    let now = sys_time()?;

    let mut best: Option<(Timestamp, GroupMembership, ActionHash)> = None;
    for link in links {
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some(record) = get(target_ah.clone(), GetOptions::network())? else {
            continue;
        };
        // Tolerate an undecodable target instead of failing the whole
        // query (defensive; AgentToGroupMemberships targets are
        // homogeneous by design, but a foreign/corrupt one must skip).
        let Some(membership) = record
            .entry()
            .to_app_option::<GroupMembership>()
            .ok()
            .flatten()
        else {
            continue;
        };
        if membership.group_genesis_hash != input.group_genesis_hash {
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

    Ok(best.map(|(_, membership, hash)| GroupMembershipResponse { membership, hash }))
}

// =============================================================================
// list_group_members — the cryptographic roster
// =============================================================================

/// Return every currently-valid (unexpired) [`GroupMembership`] in the
/// named group, deduplicated to the most-recent grant per agent.
///
/// **This is the authoritative roster.** Walks the reverse index
/// (`GroupToGroupMemberships`) from `group_genesis_hash`. The
/// `GroupMemberList` entry type from pass-1/pass-2 is demoted to a
/// display cache; every authorization decision and every recipient-set
/// derivation MUST consult this function (or `must_get_valid_record`
/// against a specific membership hash), never the `GroupMemberList`.
///
/// Dedup rule: when multiple memberships exist for the same
/// `for_agent` (e.g. after a role change or revocation), keep the
/// **latest-timestamped unexpired** one. Earlier valid memberships are
/// shadowed by later ones.
///
/// Per the index-vs-entry contract documented on
/// [`content_integrity::validate_delete_group_link`], a grantor MAY
/// later delete the index links pointing at a still-valid membership.
/// Consumers that need 100% accuracy in the presence of a hostile
/// grantor should cross-check by enumerating membership entries via
/// AgentActivity for known grantors. For the common case this function
/// is sufficient.
#[hdk_extern]
pub fn list_group_members(
    group_genesis_hash: ActionHash,
) -> ExternResult<Vec<GroupMembershipResponse>> {
    group_members_of(group_genesis_hash)
}

pub(crate) fn group_members_of(
    group_genesis_hash: ActionHash,
) -> ExternResult<Vec<GroupMembershipResponse>> {
    let links = group_roster_links(&group_genesis_hash)?;
    resolve_roster(&group_genesis_hash, links)
}

fn group_roster_links(group_genesis_hash: &ActionHash) -> ExternResult<Vec<Link>> {
    get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(group_genesis_hash.clone()),
            LinkTypes::GroupToGroupMemberships,
        )?,
        GetStrategy::Network,
    )
}

fn resolve_roster(
    group_genesis_hash: &ActionHash,
    links: Vec<Link>,
) -> ExternResult<Vec<GroupMembershipResponse>> {
    let now = sys_time()?;

    let mut best: HashMap<AgentPubKey, (Timestamp, GroupMembership, ActionHash)> = HashMap::new();
    for link in links {
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some(record) = get(target_ah.clone(), GetOptions::network())? else {
            continue;
        };
        let Some(membership) = record
            .entry()
            .to_app_option::<GroupMembership>()
            .map_err(|e| wasm_error!(e))?
        else {
            continue;
        };
        if membership.group_genesis_hash != *group_genesis_hash {
            continue;
        }
        if let Some(expiry) = membership.expiry {
            if expiry < now {
                continue;
            }
        }
        let timestamp = record.action().timestamp();
        let key = membership.for_agent.clone();
        let install = best
            .get(&key)
            .map(|(previous_timestamp, _, _)| timestamp > *previous_timestamp)
            .unwrap_or(true);
        if install {
            best.insert(key, (timestamp, membership, target_ah));
        }
    }

    Ok(best
        .into_values()
        .map(|(_, membership, hash)| GroupMembershipResponse { membership, hash })
        .collect())
}

pub const GROUP_MEMBERS_BATCH_MAX: usize = 64;

#[derive(Serialize, Deserialize, Debug)]
pub struct GroupMembersBucket {
    pub group_genesis_hash: ActionHash,
    pub members: Vec<GroupMembershipResponse>,
}

pub const GROUP_MEMBERS_LINK_BUDGET: usize = 4096;

/// Rosters stay COMPLETE (ACL derivation needs every member), so an over-budget
/// batch REJECTS instead of truncating; the caller falls back to per-group calls.
fn enforce_roster_link_budget(total_links: usize) -> ExternResult<()> {
    if total_links > GROUP_MEMBERS_LINK_BUDGET {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "group-members batch roster links exceed the 4096 budget".into()
        )));
    }
    Ok(())
}

#[hdk_extern]
pub fn list_group_members_many(
    group_genesis_hashes: Vec<ActionHash>,
) -> ExternResult<Vec<GroupMembersBucket>> {
    if group_genesis_hashes.len() > GROUP_MEMBERS_BATCH_MAX {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "group-members batch accepts at most 64 groups".into()
        )));
    }
    let mut rosters = Vec::with_capacity(group_genesis_hashes.len());
    let mut total_links: usize = 0;
    for group_genesis_hash in group_genesis_hashes {
        let links = group_roster_links(&group_genesis_hash)?;
        total_links = total_links.saturating_add(links.len());
        enforce_roster_link_budget(total_links)?;
        rosters.push((group_genesis_hash, links));
    }
    rosters
        .into_iter()
        .map(|(group_genesis_hash, links)| {
            let members = resolve_roster(&group_genesis_hash, links)?;
            Ok(GroupMembersBucket {
                group_genesis_hash,
                members,
            })
        })
        .collect()
}

// =============================================================================
// list_my_groups
// =============================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ListedGroup {
    /// The group's cryptographic identity.
    pub group_genesis_hash: ActionHash,
    /// The parent hive (recovered from the resolved `GroupGenesis`).
    pub hive_genesis_hash: ActionHash,
    /// Pulled from the resolved `GroupGenesis` entry for UI display.
    pub display_id: String,
    /// `Some(role)` for hive-wide system role groups (admin/writer/reader
    /// system groups); `None` for ordinary custom groups.
    pub hive_wide_role: Option<HiveRole>,
    /// `None` = local agent is the group's genesis author (implicit
    /// Owner); `Some(role)` = role granted via a `GroupMembership`.
    pub role: Option<HiveRole>,
}

/// Enumerate every group the local agent founded or holds a
/// (still-valid) membership in. Mirrors `list_my_hives` one level down
/// the sovereignty hierarchy.
///
/// Founded groups come from the agent's self-Inbox `GroupInvite` links
/// (author-guarded; see [`resolve_genesis_invite`]). Granted groups come
/// from the durable `AgentToGroupMemberships` index, so a swept or
/// retracted Inbox no longer erases group discovery; each unexpired
/// membership for the caller surfaces its group genesis + role.
///
/// **Dedup contract.** When a member's role changes, each
/// `create_group_membership` writes a fresh `AgentToGroupMemberships` link,
/// so the same `group_genesis_hash` may appear multiple times in the
/// output (one entry per membership issuance). humm-tauri SHOULD
/// deduplicate on `group_genesis_hash` callsite-side and pair with
/// `get_latest_group_membership` to resolve the current role per
/// group. The mirroring `list_my_hives` carries the same contract.
#[hdk_extern]
pub fn list_my_groups(_: ()) -> ExternResult<Vec<ListedGroup>> {
    list_my_groups_via(GetStrategy::Network, GetOptions::network())
}

#[hdk_extern]
pub fn list_my_groups_local(_: ()) -> ExternResult<Vec<ListedGroup>> {
    list_my_groups_via(GetStrategy::Local, GetOptions::local())
}

fn list_my_groups_via(
    link_strategy: GetStrategy,
    options: GetOptions,
) -> ExternResult<Vec<ListedGroup>> {
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    let now = sys_time()?;
    let mut out = Vec::new();
    append_founded_groups(&mut out, &my_pubkey, link_strategy, &options)?;
    append_membership_groups(&mut out, &my_pubkey, &now, link_strategy, &options)?;
    Ok(out)
}

fn append_founded_groups(
    out: &mut Vec<ListedGroup>,
    my_pubkey: &AgentPubKey,
    link_strategy: GetStrategy,
    options: &GetOptions,
) -> ExternResult<()> {
    let invite_byte = InboxEvent::GroupInvite.as_byte();
    let invite_links = get_links(
        LinkQuery::try_new(AnyLinkableHash::from(my_pubkey.clone()), LinkTypes::Inbox)?,
        link_strategy,
    )?;
    for link in invite_links {
        if link.tag.0.first().copied() != Some(invite_byte) {
            continue;
        }
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        if let Some(listed) = resolve_genesis_invite(&target_ah, my_pubkey, options)? {
            out.push(listed);
        }
    }
    Ok(())
}

fn append_membership_groups(
    out: &mut Vec<ListedGroup>,
    my_pubkey: &AgentPubKey,
    now: &Timestamp,
    link_strategy: GetStrategy,
    options: &GetOptions,
) -> ExternResult<()> {
    let membership_links = get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(my_pubkey.clone()),
            LinkTypes::AgentToGroupMemberships,
        )?,
        link_strategy,
    )?;
    let mut group_genesis_cache: HashMap<ActionHash, Option<GroupGenesis>> = HashMap::new();
    for link in membership_links {
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        if let Some(listed) = resolve_membership_invite(
            &target_ah,
            my_pubkey,
            now,
            &mut group_genesis_cache,
            options,
        )? {
            out.push(listed);
        }
    }
    Ok(())
}

/// The author guard prevents open-write Inbox links from spoofing a founder claim.
fn resolve_genesis_invite(
    target_ah: &ActionHash,
    my_pubkey: &AgentPubKey,
    options: &GetOptions,
) -> ExternResult<Option<ListedGroup>> {
    let Some(record) = get(target_ah.clone(), options.clone())? else {
        return Ok(None);
    };
    // Inbox targets are heterogeneous, so decode mismatches skip instead of failing the list.
    let Some(genesis) = record
        .entry()
        .to_app_option::<GroupGenesis>()
        .ok()
        .flatten()
    else {
        return Ok(None);
    };
    if record.action().author() != my_pubkey {
        return Ok(None);
    }
    Ok(Some(ListedGroup {
        group_genesis_hash: target_ah.clone(),
        hive_genesis_hash: genesis.hive_genesis_hash,
        display_id: genesis.display_id,
        hive_wide_role: genesis.hive_wide_role,
        role: None,
    }))
}

fn cached_group_genesis<'a>(
    cache: &'a mut HashMap<ActionHash, Option<GroupGenesis>>,
    group_genesis_hash: &ActionHash,
    options: &GetOptions,
) -> ExternResult<Option<&'a GroupGenesis>> {
    let cached = match cache.entry(group_genesis_hash.clone()) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            let genesis = match get(group_genesis_hash.clone(), options.clone())? {
                Some(record) => record
                    .entry()
                    .to_app_option::<GroupGenesis>()
                    .ok()
                    .flatten(),
                None => None,
            };
            entry.insert(genesis)
        }
    };
    Ok(cached.as_ref())
}

fn resolve_membership_invite(
    target_ah: &ActionHash,
    my_pubkey: &AgentPubKey,
    now: &Timestamp,
    group_genesis_cache: &mut HashMap<ActionHash, Option<GroupGenesis>>,
    options: &GetOptions,
) -> ExternResult<Option<ListedGroup>> {
    let Some(record) = get(target_ah.clone(), options.clone())? else {
        return Ok(None);
    };
    let Some(membership) = record
        .entry()
        .to_app_option::<GroupMembership>()
        .ok()
        .flatten()
    else {
        return Ok(None);
    };
    if &membership.for_agent != my_pubkey {
        return Ok(None);
    }
    if let Some(expiry) = membership.expiry {
        if &expiry < now {
            return Ok(None);
        }
    }
    let Some(genesis) =
        cached_group_genesis(group_genesis_cache, &membership.group_genesis_hash, options)?
    else {
        return Ok(None);
    };
    Ok(Some(ListedGroup {
        group_genesis_hash: membership.group_genesis_hash,
        hive_genesis_hash: genesis.hive_genesis_hash.clone(),
        display_id: genesis.display_id.clone(),
        hive_wide_role: genesis.hive_wide_role,
        role: Some(membership.role),
    }))
}

// =============================================================================
// list_groups_in_hive
// =============================================================================

/// Enumerate every group in `hive_genesis_hash`, derived from the
/// `HiveToGroups` link set on the hive genesis. Used by humm-tauri to
/// populate the "all groups in this hive" UI (Members & Groups pane).
///
/// `role: None` for every entry — this query does NOT resolve the
/// caller's per-group membership. Pair with `get_latest_group_membership`
/// per-group if the caller's role matters for the UI surface.
#[hdk_extern]
pub fn list_groups_in_hive(hive_genesis_hash: ActionHash) -> ExternResult<Vec<ListedGroup>> {
    let links = get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(hive_genesis_hash.clone()),
            LinkTypes::HiveToGroups,
        )?,
        GetStrategy::Network,
    )?;
    let mut out: Vec<ListedGroup> = Vec::new();
    for link in links {
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some(record) = get(target_ah.clone(), GetOptions::network())? else {
            continue;
        };
        let Some(genesis) = record
            .entry()
            .to_app_option::<GroupGenesis>()
            .map_err(|e| wasm_error!(e))?
        else {
            continue;
        };
        if genesis.hive_genesis_hash != hive_genesis_hash {
            // Defensive — link bound on integrity-side, but skip on
            // mismatch rather than trust.
            continue;
        }
        out.push(ListedGroup {
            group_genesis_hash: target_ah,
            hive_genesis_hash: genesis.hive_genesis_hash,
            display_id: genesis.display_id,
            hive_wide_role: genesis.hive_wide_role,
            role: None,
        });
    }
    Ok(out)
}

// =============================================================================
// role_key_closure
// =============================================================================

#[derive(Serialize, Deserialize, Debug)]
pub struct RoleKeyClosureInput {
    pub hive_genesis_hash: ActionHash,
    pub granted_role: HiveRole,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RoleClosureEntry {
    pub role: HiveRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_genesis_hash: Option<ActionHash>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RoleKeyClosure {
    pub entries: Vec<RoleClosureEntry>,
}

/// Owner⊇Admin⊇Writer⊇Reader, ordered highest→lowest. Exhaustive on
/// purpose: a future role variant must force a compile error here.
fn dominated_roles(granted: HiveRole) -> Vec<HiveRole> {
    match granted {
        HiveRole::Owner => vec![
            HiveRole::Owner,
            HiveRole::Admin,
            HiveRole::Writer,
            HiveRole::Reader,
        ],
        HiveRole::Admin => vec![HiveRole::Admin, HiveRole::Writer, HiveRole::Reader],
        HiveRole::Writer => vec![HiveRole::Writer, HiveRole::Reader],
        HiveRole::Reader => vec![HiveRole::Reader],
    }
}

/// Downward role-K closure: the dominated role set for `granted_role`,
/// each paired with the hive's canonical system-role `GroupGenesis`
/// action hash (`None` = no system-role group for that role is visible
/// from this node yet — the walk is eventually consistent; a premature
/// duplicate mint is absorbed by the canonical-pick contract).
/// Returns owner-attested IDENTITIES only — no key material: the client
/// holds one INDEPENDENT SharedSecret per returned genesis; no role's K
/// is ever derived from another's. Cross-agent duplicate system-role
/// groups resolve deterministically to the lowest b64 action-hash STRING
/// (the shared canonical-pick contract).
#[hdk_extern]
pub fn role_key_closure(input: RoleKeyClosureInput) -> ExternResult<RoleKeyClosure> {
    let groups = list_groups_in_hive(input.hive_genesis_hash)?;
    let entries = dominated_roles(input.granted_role)
        .into_iter()
        .map(|role| RoleClosureEntry {
            role,
            group_genesis_hash: canonical_role_group(&groups, role),
        })
        .collect();
    Ok(RoleKeyClosure { entries })
}

/// Lowest-b64-STRING pick among the hive's groups carrying `role` (JS
/// parity — the same contract as `canonical_lowest_hash`).
fn canonical_role_group(groups: &[ListedGroup], role: HiveRole) -> Option<ActionHash> {
    groups
        .iter()
        .filter(|g| g.hive_wide_role == Some(role))
        .min_by_key(|g| g.group_genesis_hash.to_string())
        .map(|g| g.group_genesis_hash.clone())
}

// =============================================================================
// get_group_genesis
// =============================================================================

/// Resolve a [`GroupGenesis`] by action hash, returning `None` if the
/// hash does not reference a valid `GroupGenesis` entry. Convenience for
/// UI consumers that already hold a group identity and need display
/// fields.
#[hdk_extern]
pub fn get_group_genesis(action_hash: ActionHash) -> ExternResult<Option<GroupGenesisResponse>> {
    let Some(record) = get(action_hash.clone(), GetOptions::network())? else {
        return Ok(None);
    };
    let Some(genesis) = record
        .entry()
        .to_app_option::<GroupGenesis>()
        .map_err(|e| wasm_error!(e))?
    else {
        return Ok(None);
    };
    Ok(Some(GroupGenesisResponse {
        genesis,
        hash: action_hash,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_link_budget_accepts_at_budget_and_rejects_over() {
        assert!(enforce_roster_link_budget(GROUP_MEMBERS_LINK_BUDGET).is_ok());
        let over = enforce_roster_link_budget(GROUP_MEMBERS_LINK_BUDGET + 1)
            .expect_err("over-budget roster batch must reject");
        assert!(format!("{over:?}").contains("roster links exceed the 4096 budget"));
    }

    fn listed_group(role: Option<HiveRole>, hash_byte: u8) -> ListedGroup {
        ListedGroup {
            group_genesis_hash: ActionHash::from_raw_36(vec![hash_byte; 36]),
            hive_genesis_hash: ActionHash::from_raw_36(vec![9; 36]),
            display_id: format!("group-{hash_byte}"),
            hive_wide_role: role,
            role: None,
        }
    }

    #[test]
    fn canonical_role_group_picks_lowest_b64_string() {
        let first = listed_group(Some(HiveRole::Admin), 1);
        let second = listed_group(Some(HiveRole::Admin), 2);
        let expected = [
            first.group_genesis_hash.to_string(),
            second.group_genesis_hash.to_string(),
        ]
        .iter()
        .min()
        .cloned()
        .expect("two candidates");
        let picked = canonical_role_group(&[first, second], HiveRole::Admin)
            .expect("a candidate matches the role");
        assert_eq!(picked.to_string(), expected);
    }

    #[test]
    fn canonical_role_group_ignores_other_roles_and_custom_groups() {
        let admin = listed_group(Some(HiveRole::Admin), 3);
        let custom = listed_group(None, 1);
        assert_eq!(
            canonical_role_group(&[custom.clone(), admin.clone()], HiveRole::Admin),
            Some(admin.group_genesis_hash),
        );
        assert_eq!(canonical_role_group(&[custom], HiveRole::Writer), None);
    }

    #[test]
    fn dominated_roles_encode_downward_closure_highest_first() {
        assert_eq!(
            dominated_roles(HiveRole::Owner),
            vec![
                HiveRole::Owner,
                HiveRole::Admin,
                HiveRole::Writer,
                HiveRole::Reader,
            ],
        );
        assert_eq!(
            dominated_roles(HiveRole::Admin),
            vec![HiveRole::Admin, HiveRole::Writer, HiveRole::Reader],
        );
        assert_eq!(
            dominated_roles(HiveRole::Writer),
            vec![HiveRole::Writer, HiveRole::Reader],
        );
        assert_eq!(dominated_roles(HiveRole::Reader), vec![HiveRole::Reader]);
    }
}
