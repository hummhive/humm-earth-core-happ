//! `encrypted_content` — coordinator-side externs and helpers for the
//! `EncryptedContent` integrity entry.
//!
//! Split out of the original monolithic `encrypted_content.rs` (which had
//! grown past the file/function size cap that the repo enforces) into:
//!
//! - `crud.rs` — create / get / get_many / update / delete externs.
//! - `queries.rs` — every `list_by_*` / `count_links_by_hive` /
//!   `get_by_content_id_link` / `fetch_pair_ss_with_hive_check` (C4).
//! - `signals.rs` — `EncryptedContentSignal` + new `DmRemoteSignal`
//!   (C6 delete-request, C7 WebRTC) + the `send_dm_*` externs and the
//!   `remote_signal_acl_readers` fan-out helper.
//! - `get_helpers.rs` — generic DHT-get helpers
//!   (`get_eh`, `get_record`, `get_latest_typed_from_eh`, `sah_to_ah`).
//! - `migration.rs` — forward-pointer migration markers
//!   for the pass-2 DNA-hash change (`mark_migrated` / `mark_migrated_v2`
//!   write externs + `get_migration_marker` / `get_migration_marker_v2`
//!   readers; coordinator-only, no DNA-hash impact).
//!
//! Public-API guarantee: every `#[hdk_extern]` and shared struct exposed
//! by the original file is re-exported from this module so existing
//! callsites and the conductor's WASM symbol table are unaffected. The
//! integrity zome is NOT touched by any of this — the DNA hash stays
//! byte-identical, which is the load-bearing constraint for shipping
//! this work via the coordinator hot-swap path.

use content_integrity::*;
use hdk::prelude::*;

pub mod crud;
pub mod get_helpers;
pub mod migration;
pub mod queries;
pub mod signals;

// --- Shared wire-shape types -------------------------------------------------
//
// Multiple submodules require these wire-shape types at the module root.

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct EncryptedContentResponse {
    pub encrypted_content: EncryptedContent,
    pub hash: String,
    pub original_hash: String,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct CreateEncryptedContentInput {
    pub id: String,
    /// Display alias for the hive (preserves the legacy squuid
    /// `hive_id` semantics for UX continuity). NOT load-bearing for
    /// security — see `acl_spec` for the cryptographic authority
    /// contract. May be the empty string for hive-discovery /
    /// open-write entries that intentionally sit OUTSIDE any hive's
    /// content path.
    pub display_hive_id: String,
    pub content_type: String,
    pub revision_author_signing_public_key: String,
    pub bytes: SerializedBytes,
    /// Pass-3: the per-scope authority contract. The integrity
    /// validator variant-dispatches off this field — see
    /// [`content_integrity::AclSpec`] for the four variants
    /// (HiveGroup, DirectMessage, Public, OpenWrite) and what each
    /// validator enforces.
    pub acl_spec: AclSpec,
    pub public_key_acl: Acl,
    /// Optional Dynamic link labels. Only published for variants that
    /// bind a hive context (HiveGroup, Public, OpenWrite with target);
    /// silently ignored for DirectMessage and OpenWrite-without-target
    /// (the integrity validator rejects Dynamic links targeting
    /// non-hive-context entries).
    pub dynamic_links: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdateEncryptedContentInput {
    pub previous_encrypted_content_hash: ActionHash,
    pub updated_encrypted_content: EncryptedContent,
}

// --- Re-exports for external (lib.rs / tests / older import paths) consumers -

pub use crud::{
    create_encrypted_content, delete_encrypted_content, get_encrypted_content,
    get_many_encrypted_content, update_encrypted_content,
};
pub use migration::{
    build_marker_payload, build_marker_v2_payload, get_migration_marker, get_migration_marker_v2,
    mark_migrated, mark_migrated_v2, MarkMigratedInput, MarkMigratedV2Input, MigrationMarker,
    MigrationMarkerV1, MigrationMarkerV2, MIGRATION_MARKER_CONTENT_TYPE_PREFIX,
    MIGRATION_MARKER_SCHEMA_TAG,
};
pub use queries::{
    count_links_by_hive, fetch_pair_ss_with_hive_check, get_by_content_id_link, list_by_acl_link,
    list_by_author, list_by_dynamic_link, list_by_hive_link, CountByHiveInput,
    FetchPairWithHiveCheckInput, ListByAclInput, ListByAuthorInput, ListByContentIdInput,
    ListByDynamicLinkInput, ListByHiveInput,
};
pub use signals::{
    remote_signal_acl_readers, send_dm_call_init_accept, send_dm_call_init_request,
    send_dm_call_sdp_data, send_dm_delete_request, DmCallSignal, DmDeleteRequestSignal,
    DmRemoteSignal, EncryptedContentSignal, EncryptedContentSignalType, SendDmCallInitAcceptInput,
    SendDmCallInitRequestInput, SendDmCallSdpDataInput, SendDmDeleteRequestInput,
};
