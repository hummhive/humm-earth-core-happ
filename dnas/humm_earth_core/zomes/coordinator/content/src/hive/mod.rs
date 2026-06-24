//! Coordinator-side externs and helpers for the per-hive cryptographic
//! identity infrastructure introduced in pass-2 (I-H).
//!
//! Two integrity entry types power this:
//! - [`content_integrity::HiveGenesis`] — the per-hive root of trust;
//!   its action hash is the cryptographic hive identity.
//! - [`content_integrity::HiveMembership`] — role grants
//!   (Owner/Admin/Writer/Reader) keyed off a genesis hash; validated
//!   inductively (Moss-style chain walk at commit time).
//!
//! Discovery convention: every `create_hive_genesis` and
//! `create_hive_membership` ALSO writes an `Inbox` link tagged
//! `InboxEvent::HiveInvite` so the recipient agent can enumerate "hives
//! I'm part of" via `get_links(my_pubkey, Inbox, ...)` filtered by tag
//! byte. This piggybacks on the I-C inbox infrastructure (single link
//! type, simple validator).

pub mod crud;
pub mod owner;
pub mod queries;

pub use crud::{
    create_hive_genesis, create_hive_membership, CreateHiveGenesisInput, CreateHiveMembershipInput,
    HiveGenesisResponse, HiveMembershipResponse,
};
pub use owner::*;
pub use queries::{get_latest_membership, list_my_hives, GetLatestMembershipInput, ListedHive};
