//! Coordinator-side externs and helpers for the per-group cryptographic
//! authority infrastructure introduced in pass-3 (G-series).
//!
//! Three integrity entry types + three discovery link types power this:
//! - [`content_integrity::GroupGenesis`] — the per-group root of trust;
//!   its action hash is the cryptographic group identity. Bound to a
//!   parent [`content_integrity::HiveGenesis`] so the hive owner is
//!   sovereign over every group in their hive.
//! - [`content_integrity::GroupMembership`] — role grants
//!   (Owner/Admin/Writer/Reader) keyed off a `group_genesis_hash` and
//!   validated by `check_group_authority` (Path A = group author,
//!   Path B = hive sovereign, Path C = explicit group membership).
//!   Immutable; revocation is via a fresh entry with past `expiry`.
//! - Links: `AgentToGroupMemberships` (forward index, base = grantee),
//!   `GroupToGroupMemberships` (reverse index = the cryptographic
//!   roster, base = group_genesis_hash, tag = for_agent), `HiveToGroups`
//!   (enumeration of a hive's groups).
//!
//! Discovery convention: every `create_group_genesis` and
//! `create_group_membership` ALSO writes an `Inbox` link tagged
//! `InboxEvent::GroupInvite` (transient notification; self for genesis,
//! grantee for membership). `list_my_groups` reads founded groups from
//! the self-Inbox links and granted groups from the durable
//! `AgentToGroupMemberships` index.

pub mod crud;
pub mod queries;

pub use crud::{
    create_group_genesis, create_group_membership, find_or_create_group_genesis,
    find_or_create_group_membership, revoke_group_membership, CreateGroupGenesisInput,
    CreateGroupMembershipInput, FindOrCreateGroupGenesisResponse,
    FindOrCreateGroupMembershipResponse, GroupGenesisResponse, GroupMembershipResponse,
    RevokeGroupMembershipInput,
};
pub use queries::{
    get_group_genesis, get_latest_group_membership, list_group_members, list_groups_in_hive,
    list_my_groups, role_key_closure, GetLatestGroupMembershipInput, ListedGroup, RoleClosureEntry,
    RoleKeyClosure, RoleKeyClosureInput,
};
