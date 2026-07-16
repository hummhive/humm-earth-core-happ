//! Create externs for [`GroupGenesis`] and [`GroupMembership`] entries,
//! plus the [`revoke_group_membership`] ergonomic helper.
//!
//! Each create:
//! 1. Commits the integrity entry.
//! 2. Publishes the canonical discovery links so consumers can enumerate
//!    the new group / membership without a chain replay:
//!    - `GroupGenesis` ⇒ `HiveToGroups` (base = parent hive) + self
//!      `Inbox::GroupInvite` (so the creator surfaces in
//!      `list_my_groups`).
//!    - `GroupMembership` ⇒ `AgentToGroupMemberships` (base = grantee),
//!      `GroupToGroupMemberships` (base = group_genesis_hash, tag =
//!      for_agent), and an `Inbox::GroupInvite` to the grantee.
//!
//! Validation lives entirely in the integrity layer (see
//! `content_integrity::validate_create_group_genesis` /
//! `validate_create_group_membership` and `check_group_authority`).
//! The coordinator just assembles the payload + indexes.

use content_integrity::*;
use hdk::prelude::*;

use super::queries::{get_latest_group_membership, GetLatestGroupMembershipInput};

// `hdk::prelude::*` exports a `Role` symbol from
// `holochain_integrity_types` (capability-token role-based access) that
// shadows our `content_integrity::Role` membership-role enum at the
// coordinator's namespace. The `pub use Role as HiveRole;` alias in
// `integrity/.../hive.rs` exists precisely for this: every coordinator
// reference to the membership-role enum uses `HiveRole` (unambiguous;
// no hdk shadow) instead of `Role`. The integrity zome itself is fine
// — `hdi::prelude::*` does not re-export the conflicting symbol in a
// way that shadows the locally-defined enum.

// =============================================================================
// create_group_genesis
// =============================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateGroupGenesisInput {
    pub hive_genesis_hash: ActionHash,
    /// Human alias = the legacy humm-tauri group squuid (continuity).
    /// Never security-load-bearing; routing/display only.
    pub display_id: String,
    /// `Some(role)` => a hive-wide *system role group* (requires hive
    /// Owner); `None` => an ordinary custom group (requires hive
    /// Admin+).
    pub hive_wide_role: Option<HiveRole>,
    /// The creator's authorising [`HiveMembership`] hash in
    /// `hive_genesis_hash`. `None` = creator IS the hive genesis author
    /// (implicit Owner). Persisted onto `GroupGenesis` so the integrity
    /// validator can re-walk hive authority.
    pub creator_hive_membership_hash: Option<ActionHash>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupGenesisResponse {
    pub genesis: GroupGenesis,
    pub hash: ActionHash,
}

/// Commit a new [`GroupGenesis`] entry and write its canonical
/// discovery indexes (`HiveToGroups` + self `Inbox::GroupInvite`).
///
/// Authority requirement: enforced by
/// `validate_create_group_genesis`. A system role group
/// (`hive_wide_role.is_some()`) demands hive Owner; a custom group
/// demands hive Admin+. The integrity validator returns Invalid on
/// authority failure.
#[hdk_extern]
pub fn create_group_genesis(input: CreateGroupGenesisInput) -> ExternResult<GroupGenesisResponse> {
    let now = sys_time()?;
    let genesis = GroupGenesis {
        hive_genesis_hash: input.hive_genesis_hash.clone(),
        display_id: input.display_id,
        hive_wide_role: input.hive_wide_role,
        creator_hive_membership_hash: input.creator_hive_membership_hash,
        created_at_microseconds: now.as_micros() as i64,
    };
    let hash = create_entry(&EntryTypes::GroupGenesis(genesis.clone()))?;

    // `HiveToGroups` — enables `list_groups_in_hive(hive_genesis_hash)`.
    create_link(
        AnyLinkableHash::from(input.hive_genesis_hash),
        AnyLinkableHash::from(hash.clone()),
        LinkTypes::HiveToGroups,
        LinkTag::new(vec![]),
    )?;

    // Self `Inbox::GroupInvite` — enables `list_my_groups` to surface
    // founded groups without an AgentActivity scan, mirroring the hive
    // pattern.
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    create_link(
        AnyLinkableHash::from(my_pubkey),
        AnyLinkableHash::from(hash.clone()),
        LinkTypes::Inbox,
        LinkTag::new(vec![InboxEvent::GroupInvite.as_byte()]),
    )?;

    Ok(GroupGenesisResponse { genesis, hash })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FindOrCreateGroupGenesisResponse {
    pub response: GroupGenesisResponse,
    pub was_created: bool,
}

/// Idempotent [`create_group_genesis`]: returns the caller's existing
/// group in `hive_genesis_hash` when one matches, else creates.
///
/// Match key — system role groups (`hive_wide_role: Some`) are hive
/// singletons matched on role alone (display drift tolerated); custom
/// groups (`None`) match on `display_id`. Author-scoped: only
/// caller-authored geneses count (crash-resume semantics; cross-agent
/// duplicate prevention stays client-side canonical-pick until the
/// pass-7 A11 uniqueness validators). Found ⇒ no write, no signal;
/// multiple candidates ⇒ lowest-hash wins (selectCanonicalByHash rule).
/// NOT cap-granted (mutator).
#[hdk_extern]
pub fn find_or_create_group_genesis(
    input: CreateGroupGenesisInput,
) -> ExternResult<FindOrCreateGroupGenesisResponse> {
    let me = agent_info()?.agent_initial_pubkey;
    let query = LinkQuery::try_new(
        AnyLinkableHash::from(input.hive_genesis_hash.clone()),
        LinkTypes::HiveToGroups,
    )?
    .author(me.clone());
    let mut links = get_links(query, GetStrategy::Network)?;
    links.retain(|link| link.author == me);

    let mut candidates: Vec<GroupGenesisResponse> = Vec::new();
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
            .ok()
            .flatten()
        else {
            debug!("find_or_create_group_genesis: skipping undecodable HiveToGroups target {target_ah}");
            continue;
        };
        if genesis.hive_genesis_hash != input.hive_genesis_hash {
            continue;
        }
        let role_matches = genesis.hive_wide_role == input.hive_wide_role;
        let display_matches =
            input.hive_wide_role.is_some() || genesis.display_id == input.display_id;
        if role_matches && display_matches {
            candidates.push(GroupGenesisResponse {
                genesis,
                hash: target_ah,
            });
        }
    }

    if let Some(existing) = candidates.into_iter().min_by(|a, b| a.hash.cmp(&b.hash)) {
        return Ok(FindOrCreateGroupGenesisResponse {
            response: existing,
            was_created: false,
        });
    }
    let response = create_group_genesis(input)?;
    Ok(FindOrCreateGroupGenesisResponse {
        response,
        was_created: true,
    })
}

// =============================================================================
// create_group_membership
// =============================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateGroupMembershipInput {
    pub group_genesis_hash: ActionHash,
    pub for_agent: AgentPubKey,
    pub role: HiveRole,
    /// Grantor's authorising [`GroupMembership`] hash (Path C). `None` =
    /// grantor proves authority via the group-author (Path A) or
    /// hive-sovereign (Path B) route.
    pub grantor_membership_hash: Option<ActionHash>,
    /// Grantor's authorising [`HiveMembership`] hash in the group's
    /// parent hive (Path B). `None` = not relying on a hive witness.
    pub grantor_hive_membership_hash: Option<ActionHash>,
    /// `None` = permanent; `Some(ts)` = invalid past this timestamp.
    pub expiry: Option<Timestamp>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupMembershipResponse {
    pub membership: GroupMembership,
    pub hash: ActionHash,
}

/// Commit a new [`GroupMembership`] and write its three canonical
/// discovery indexes (`AgentToGroupMemberships`,
/// `GroupToGroupMemberships`, `Inbox::GroupInvite` to the grantee).
///
/// Authority requirement: enforced by
/// `validate_create_group_membership` — no self-grant, grantor must
/// hold Admin+ via Path A/B/C, only Owner grants Owner, and the G-4.4
/// grant-window-containment rule applies when the grantor's authority
/// rests on an expiring group membership.
#[hdk_extern]
pub fn create_group_membership(
    input: CreateGroupMembershipInput,
) -> ExternResult<GroupMembershipResponse> {
    let membership = GroupMembership {
        group_genesis_hash: input.group_genesis_hash.clone(),
        for_agent: input.for_agent.clone(),
        role: input.role,
        grantor_membership_hash: input.grantor_membership_hash,
        grantor_hive_membership_hash: input.grantor_hive_membership_hash,
        expiry: input.expiry,
    };
    let hash = create_entry(&EntryTypes::GroupMembership(membership.clone()))?;

    // Forward index (`AgentToGroupMemberships`) — base = grantee. Powers
    // `list_my_groups` for granted memberships.
    create_link(
        AnyLinkableHash::from(input.for_agent.clone()),
        AnyLinkableHash::from(hash.clone()),
        LinkTypes::AgentToGroupMemberships,
        LinkTag::new(vec![]),
    )?;

    // Reverse index (`GroupToGroupMemberships`) — base = group genesis,
    // tag = for_agent. Powers `list_group_members(group_genesis_hash)`
    // — the cryptographic roster.
    create_link(
        AnyLinkableHash::from(input.group_genesis_hash),
        AnyLinkableHash::from(hash.clone()),
        LinkTypes::GroupToGroupMemberships,
        LinkTag::new(input.for_agent.to_string().into_bytes()),
    )?;

    // Grantee notification via Inbox.
    create_link(
        AnyLinkableHash::from(input.for_agent),
        AnyLinkableHash::from(hash.clone()),
        LinkTypes::Inbox,
        LinkTag::new(vec![InboxEvent::GroupInvite.as_byte()]),
    )?;

    Ok(GroupMembershipResponse { membership, hash })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FindOrCreateMembershipResponse {
    pub response: GroupMembershipResponse,
    pub was_created: bool,
}

/// Idempotent [`create_group_membership`]: returns the grantee's latest
/// unexpired membership when its role equals `input.role`, else creates.
/// A different role, an expired grant, or no grant falls through to
/// create — a role change is a legitimate new grant, and validator
/// errors (self-grant, authority) propagate unchanged. Found ⇒ no
/// write, no signal. NOT cap-granted (mutator).
#[hdk_extern]
pub fn find_or_create_group_membership(
    input: CreateGroupMembershipInput,
) -> ExternResult<FindOrCreateMembershipResponse> {
    let existing = get_latest_group_membership(GetLatestGroupMembershipInput {
        agent: input.for_agent.clone(),
        group_genesis_hash: input.group_genesis_hash.clone(),
    })?;
    if let Some(found) = existing {
        if found.membership.role == input.role {
            return Ok(FindOrCreateMembershipResponse {
                response: found,
                was_created: false,
            });
        }
    }
    let response = create_group_membership(input)?;
    Ok(FindOrCreateMembershipResponse {
        response,
        was_created: true,
    })
}

// =============================================================================
// revoke_group_membership — ergonomic helper
// =============================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RevokeGroupMembershipInput {
    /// Action hash of the [`GroupMembership`] being revoked. Used to
    /// recover `(group_genesis_hash, for_agent, role)` for the
    /// replacement entry; the original entry remains on the DHT
    /// (immutable).
    pub membership_hash: ActionHash,
    /// The expiry to stamp on the replacement membership. Typically
    /// `sys_time()?.into()` (revoke immediately) or a near-past
    /// timestamp to age the prior membership out without a deletion
    /// race.
    pub new_expiry: Timestamp,
    /// Caller's authorising group-membership witness (Path C). MUST be
    /// supplied even though the original entry already records who
    /// granted the membership: the integrity validator binds authority
    /// to `action.author` (= the calling agent), not to the original
    /// grantor.
    pub grantor_membership_hash: Option<ActionHash>,
    /// Caller's authorising hive-membership witness (Path B).
    pub grantor_hive_membership_hash: Option<ActionHash>,
}

/// Revoke a [`GroupMembership`] by issuing a fresh one for the same
/// agent + group + role with a past (or near-past) `expiry`. The
/// original entry stays immutable on the DHT; consumers use the
/// most-recent valid one (`get_latest_group_membership`) so the
/// stale grant ages out naturally.
///
/// The caller (`action.author`) must independently hold Admin+
/// authority in the group, same as for a fresh grant — revocation
/// is not a different operation at the validator layer, it is just
/// another membership issuance with the expiry pinned to the past.
///
/// This helper is provided so humm-tauri does not have to re-derive
/// `(group_genesis_hash, for_agent, role)` from a `get` call for
/// every remove-member action.
///
/// **Self-revocation is NOT supported.** Rule 1 of
/// `validate_create_group_membership` unconditionally rejects
/// `action.author == for_agent` (no carve-out for past-expiry entries
/// or Admin-level callers). A member cannot revoke their own
/// membership through this extern. Leave-group must be implemented
/// as a remove-member request issued by an Admin+ holder of the
/// group (humm-tauri's UI flow should route "leave" through an
/// admin-side intermediation, not a direct self-call).
#[hdk_extern]
pub fn revoke_group_membership(
    input: RevokeGroupMembershipInput,
) -> ExternResult<GroupMembershipResponse> {
    let original_record =
        get(input.membership_hash.clone(), GetOptions::network())?.ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "revoke_group_membership: original membership {} not found",
                input.membership_hash,
            )))
        })?;
    let original: GroupMembership = original_record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "revoke_group_membership: action {} is not a GroupMembership",
                input.membership_hash,
            )))
        })?;

    let revocation = CreateGroupMembershipInput {
        group_genesis_hash: original.group_genesis_hash,
        for_agent: original.for_agent,
        role: original.role,
        grantor_membership_hash: input.grantor_membership_hash,
        grantor_hive_membership_hash: input.grantor_hive_membership_hash,
        expiry: Some(input.new_expiry),
    };
    create_group_membership(revocation)
}

/// Author-gated tombstone of an empty group's genesis; refuses while members remain.
#[hdk_extern]
pub fn delete_group_genesis(group_genesis_hash: ActionHash) -> ExternResult<ActionHash> {
    if !crate::group::queries::list_group_members(group_genesis_hash.clone())?.is_empty() {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "refusing to delete a group with live members".into(),
        )));
    }
    let deleted = delete_entry(group_genesis_hash.clone())?;
    crate::delete_own_links_targeting(AnyLinkableHash::from(group_genesis_hash))?;
    Ok(deleted)
}
