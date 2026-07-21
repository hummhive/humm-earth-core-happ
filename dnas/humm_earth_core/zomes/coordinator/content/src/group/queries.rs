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
    let links = get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(group_genesis_hash.clone()),
            LinkTypes::GroupToGroupMemberships,
        )?,
        GetStrategy::Network,
    )?;
    let now = sys_time()?;

    // Best-per-agent: walk all links, keep latest-timestamped unexpired
    // membership per `for_agent`. Final pass returns the dedup'd set.
    let mut best: std::collections::HashMap<AgentPubKey, (Timestamp, GroupMembership, ActionHash)> =
        std::collections::HashMap::new();
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
        if membership.group_genesis_hash != group_genesis_hash {
            continue;
        }
        if let Some(expiry) = membership.expiry {
            if expiry < now {
                continue;
            }
        }
        let ts = record.action().timestamp();
        let key = membership.for_agent.clone();
        let install = best
            .get(&key)
            .map(|(prev_ts, _, _)| ts > *prev_ts)
            .unwrap_or(true);
        if install {
            best.insert(key, (ts, membership, target_ah));
        }
    }

    Ok(best
        .into_values()
        .map(|(_, membership, hash)| GroupMembershipResponse { membership, hash })
        .collect())
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
/// from the durable `AgentToGroupMemberships` index (pass-7: rerouted
/// from Inbox `GroupInvite` so a swept or retracted inbox no longer
/// erases group discovery); each unexpired membership for the caller
/// surfaces its group genesis + role.
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
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    let invite_byte = InboxEvent::GroupInvite.as_byte();
    let now = sys_time()?;
    let mut out: Vec<ListedGroup> = Vec::new();

    // Founded rows stay Inbox-based (Wave-2 residual): the shipped sweep
    // consumes only DmCreate, and a founder re-derives from their own chain.
    let invite_links = get_links(
        LinkQuery::try_new(AnyLinkableHash::from(my_pubkey.clone()), LinkTypes::Inbox)?,
        GetStrategy::Network,
    )?;
    for link in invite_links {
        if link.tag.0.first().copied() != Some(invite_byte) {
            continue;
        }
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some(record) = get(target_ah.clone(), GetOptions::network())? else {
            continue;
        };
        if let Some(listed) = resolve_genesis_invite(&record, &target_ah, &my_pubkey)? {
            out.push(listed);
        }
    }

    let membership_links = get_links(
        LinkQuery::try_new(
            AnyLinkableHash::from(my_pubkey.clone()),
            LinkTypes::AgentToGroupMemberships,
        )?,
        GetStrategy::Network,
    )?;
    for link in membership_links {
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        let Some(record) = get(target_ah, GetOptions::network())? else {
            continue;
        };
        if let Some(listed) = resolve_membership_invite(&record, &my_pubkey, &now)? {
            out.push(listed);
        }
    }

    Ok(out)
}

/// Decode `record` as a [`GroupGenesis`]; if it is one, verify the
/// record's author matches `my_pubkey` (the open-write Inbox surface
/// means any peer can target an arbitrary genesis with a GroupInvite
/// link on the local agent's pubkey, so the author guard is what makes
/// "I founded this group" a real claim rather than a UI-spoofable
/// hint). Returns `Some(ListedGroup { role: None })` for a valid
/// founded group; `None` if the record isn't a genesis or the author
/// guard fails.
fn resolve_genesis_invite(
    record: &Record,
    target_ah: &ActionHash,
    my_pubkey: &AgentPubKey,
) -> ExternResult<Option<ListedGroup>> {
    // Cross-type tolerant: list_my_groups feeds BOTH GroupGenesis and
    // GroupMembership inbox targets through here, so a membership target
    // failing the genesis decode must fall through (Ok(None)) — never
    // `?`-propagate, which broke the whole list once any group was joined.
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

/// Decode `record` as a [`GroupMembership`]; if it grants `my_pubkey` a
/// currently-unexpired role, fetch the referenced `GroupGenesis` for
/// the display fields and return a populated [`ListedGroup`]. Returns
/// `None` if the record isn't a membership, isn't for `my_pubkey`, has
/// expired, or its referenced genesis is not resolvable / not a
/// `GroupGenesis`.
fn resolve_membership_invite(
    record: &Record,
    my_pubkey: &AgentPubKey,
    now: &Timestamp,
) -> ExternResult<Option<ListedGroup>> {
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
    let Some(genesis_record) = get(membership.group_genesis_hash.clone(), GetOptions::network())?
    else {
        return Ok(None);
    };
    let Some(genesis) = genesis_record
        .entry()
        .to_app_option::<GroupGenesis>()
        .ok()
        .flatten()
    else {
        return Ok(None);
    };
    Ok(Some(ListedGroup {
        group_genesis_hash: membership.group_genesis_hash,
        hive_genesis_hash: genesis.hive_genesis_hash,
        display_id: genesis.display_id,
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
/// action hash (`None` = no system-role group for that role exists yet).
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
        .min_by(|a, b| {
            a.group_genesis_hash
                .to_string()
                .cmp(&b.group_genesis_hash.to_string())
        })
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
