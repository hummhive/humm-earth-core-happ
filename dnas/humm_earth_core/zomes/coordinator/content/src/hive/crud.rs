//! Create externs for [`HiveGenesis`] and [`HiveMembership`] entries.
//!
//! Each create:
//! 1. Commits the integrity entry.
//! 2. Publishes an `Inbox` link tagged `InboxEvent::HiveInvite` to the
//!    target agent (self for HiveGenesis; the grantee for
//!    HiveMembership). The recipient discovers their hives by walking
//!    `get_links(my_pubkey, Inbox, ...)` filtered by tag byte 2.
//!
//! The integrity validator already enforces all of:
//! - `HiveGenesis`: any author (permissionless).
//! - `HiveMembership`: cannot self-grant; grantor must hold ≥Admin in
//!   the hive; only Owner may grant Owner; expiry honoured at commit
//!   time.
//!
//! Coordinator side simply assembles the payloads from caller input and
//! defers the security decisions to the integrity layer.

use crate::get_typed_entry;
use content_integrity::*;
use hdk::prelude::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateHiveGenesisInput {
    /// Optional human-friendly alias (e.g. a hive name or, for
    /// migration, the old squuid `hive_id` string). Not used for
    /// security; routing/UI only.
    pub display_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HiveGenesisResponse {
    pub genesis: HiveGenesis,
    pub hash: ActionHash,
}

/// Commit a new [`HiveGenesis`] entry and surface the action hash to
/// the caller. Also writes a self-tagged `Inbox` link so the founding
/// agent can enumerate this hive in `list_my_hives`.
///
/// Permissionless: any agent on the DNA may call this. The integrity
/// validator returns `Valid` for every author.
#[hdk_extern]
pub fn create_hive_genesis(input: CreateHiveGenesisInput) -> ExternResult<HiveGenesisResponse> {
    let now = sys_time()?;
    let genesis = HiveGenesis {
        display_id: input.display_id,
        created_at_microseconds: now.as_micros() as i64,
    };
    let hash = create_entry(&EntryTypes::HiveGenesis(genesis.clone()))?;

    // Self-write an Inbox HiveInvite link so list_my_hives surfaces
    // this hive without a chain replay.
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    create_link(
        AnyLinkableHash::from(my_pubkey),
        AnyLinkableHash::from(hash.clone()),
        LinkTypes::Inbox,
        LinkTag::new(vec![InboxEvent::HiveInvite.as_byte()]),
    )?;

    Ok(HiveGenesisResponse { genesis, hash })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateHiveMembershipInput {
    pub hive_genesis_hash: ActionHash,
    pub for_agent: AgentPubKey,
    pub role: HiveRole,
    /// `None` = the calling agent IS the genesis author for the hive
    /// (implicit Owner). `Some(hash)` = caller's own authorising
    /// membership hash.
    pub grantor_membership_hash: Option<ActionHash>,
    pub expiry: Option<Timestamp>,
    #[serde(default)]
    pub grantor_owner_accept_hash: Option<ActionHash>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HiveMembershipResponse {
    pub membership: HiveMembership,
    pub hash: ActionHash,
}

/// Commit a [`HiveMembership`] grant + the grantee's `Inbox` discovery link.
#[hdk_extern]
pub fn create_hive_membership(
    input: CreateHiveMembershipInput,
) -> ExternResult<HiveMembershipResponse> {
    if input.role == HiveRole::Admin {
        // Integrity proves only ever-owner; current-owner is resolvable only here.
        let my_pubkey = agent_info()?.agent_initial_pubkey;
        if crate::hive::owner::resolve_current_owner(&input.hive_genesis_hash)? != my_pubkey {
            return Err(wasm_error!(WasmErrorInner::Guest(
                "only the current hive owner may grant the Admin role".into(),
            )));
        }
    }
    let membership = HiveMembership {
        hive_genesis_hash: input.hive_genesis_hash,
        for_agent: input.for_agent.clone(),
        role: input.role,
        grantor_membership_hash: input.grantor_membership_hash,
        expiry: input.expiry,
        grantor_owner_accept_hash: input.grantor_owner_accept_hash,
    };
    let hash = create_entry(&EntryTypes::HiveMembership(membership.clone()))?;

    create_link(
        AnyLinkableHash::from(input.for_agent),
        AnyLinkableHash::from(hash.clone()),
        LinkTypes::Inbox,
        LinkTag::new(vec![InboxEvent::HiveInvite.as_byte()]),
    )?;

    Ok(HiveMembershipResponse { membership, hash })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RevokeHiveMembershipInput {
    pub membership_hash: ActionHash,
    pub new_expiry: Timestamp,
    pub grantor_membership_hash: Option<ActionHash>,
    #[serde(default)]
    pub grantor_owner_accept_hash: Option<ActionHash>,
}

/// Revoke by re-issuing the same `(hive, for_agent, role)` grant with a past
/// `expiry`; the current owner's own membership is protected.
#[hdk_extern]
pub fn revoke_hive_membership(
    input: RevokeHiveMembershipInput,
) -> ExternResult<HiveMembershipResponse> {
    let original: HiveMembership = get_typed_entry(&input.membership_hash)?.ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "revoke_hive_membership: membership {} not found",
            input.membership_hash,
        )))
    })?;
    if crate::hive::owner::resolve_current_owner(&original.hive_genesis_hash)? == original.for_agent
    {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "refusing to revoke the current hive owner's membership".into(),
        )));
    }
    create_hive_membership(CreateHiveMembershipInput {
        hive_genesis_hash: original.hive_genesis_hash,
        for_agent: original.for_agent,
        role: original.role,
        grantor_membership_hash: input.grantor_membership_hash,
        grantor_owner_accept_hash: input.grantor_owner_accept_hash,
        expiry: Some(input.new_expiry),
    })
}
