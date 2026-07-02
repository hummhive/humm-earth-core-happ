use hdi::prelude::*;

/// The per-hive root-of-trust entry. Any agent may commit one to
/// establish a new hive; the entry's **action hash** then serves as the
/// hive's cryptographic identity throughout the DNA.
///
/// Immutable: `validate_update_hive_genesis` and
/// `validate_delete_hive_genesis` both return `Invalid`. To deprecate a
/// hive, stop granting memberships; existing data stays valid on the
/// DHT but the social graph withers.
///
/// ## Fields
///
/// - `display_id` — human-readable alias surfaced in UI ("Acme Corp",
///   "My DMs", or for migration: the old squuid `hive_id` string). NEVER
///   used by validators for security; routing/discovery only.
/// - `created_at_microseconds` — informational only. Validators do not
///   compare it against `action.timestamp` (the action timestamp is
///   already authoritative; this field exists for UI ordering when the
///   action is not in scope).
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct HiveGenesis {
    pub display_id: String,
    pub created_at_microseconds: i64,
}

/// Role granted by a membership entry. Ordered such that
/// `Owner > Admin > Writer > Reader` for permission containment
/// (`role_satisfies` enforces this).
///
/// Shared across the hive layer ([`HiveMembership`]) and the group
/// layer ([`crate::group::GroupMembership`]); matches the humm-tauri
/// `AclRole = owner|admin|writer|reader` vocabulary 1:1.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Owner,
    Admin,
    Writer,
    Reader,
}

/// Pass-2 compatibility alias. Identical to [`Role`] in every respect
/// (same variants, same serialization); retained so existing
/// `HiveRole` references across the integrity + coordinator crates
/// resolve unchanged after the rename.
pub use self::Role as HiveRole;

/// A role grant. Mirrors Moss `StewardPermission`: every grant carries a
/// reference to the grantor's own authorising membership (or `None` if
/// the grantor IS the genesis author), and validation walks one level
/// up the grant chain on every commit.
///
/// ## Immutability + revocation model
///
/// Updates and deletes both return `Invalid`. Revocation is via
/// `expiry: Some(ts)`: once `Timestamp::now() > expiry`, every consumer
/// validator that checks the membership returns `Invalid`. To revoke
/// permanently, set `expiry` to a past timestamp on the next grant
/// (effectively no-op grant + the prior membership ages out). To grant
/// a different role to the same agent, issue a fresh `HiveMembership`
/// with the new role — consumers use the most-recent valid one.
///
/// ## Fields
///
/// - `hive_genesis_hash` — the hive this grant applies in.
/// - `for_agent` — the grantee.
/// - `role` — `Owner` / `Admin` / `Writer` / `Reader`.
/// - `grantor_membership_hash` — `None` means `action.author` IS the
///   genesis author (no membership entry required); `Some(hash)` means
///   the validator must `must_get_valid_record(hash)` to fetch the
///   grantor's authorising membership.
/// - `expiry` — `None` = permanent; `Some(ts)` = invalid past this
///   timestamp. Mirrors Moss `permission_duration_until`.
/// - `grantor_owner_accept_hash` — pass-5; for `Admin` grants, cites the
///   grantor's owner-accept proving lineage ownership.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct HiveMembership {
    pub hive_genesis_hash: ActionHash,
    pub for_agent: AgentPubKey,
    pub role: Role,
    pub grantor_membership_hash: Option<ActionHash>,
    pub expiry: Option<Timestamp>,
    // serde(default): pass-4 HiveMembership wire data predates this field.
    #[serde(default)]
    pub grantor_owner_accept_hash: Option<ActionHash>,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct HiveOwnerHandoffOffer {
    pub hive_genesis_hash: ActionHash,
    pub to_agent: AgentPubKey,
    pub offerer_owner_accept_hash: Option<ActionHash>,
    pub created_at_microseconds: i64,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct HiveOwnerHandoffAccept {
    pub offer_hash: ActionHash,
}
