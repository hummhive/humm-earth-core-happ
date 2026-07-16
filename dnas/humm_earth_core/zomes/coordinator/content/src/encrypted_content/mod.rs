//! `encrypted_content` — coordinator-side externs and helpers for the
//! `EncryptedContent` integrity entry.
//!
//! Split out of the original monolithic `encrypted_content.rs` into:
//!
//! - `crud.rs` — create / get / get_many / update / delete externs.
//! - `queries.rs` — every `list_by_*` / `count_links_by_hive` /
//!   `get_by_content_id_link` / `fetch_pair_ss_with_hive_check` (C4).
//! - `signals/` — `EncryptedContentSignal`, `DmRemoteSignal`, `send_dm_*`
//!   externs, and ACL-reader fan-out.
//! - `get_helpers.rs` — generic DHT-get helpers.
//! - `migration/` — forward-pointer migration markers and marker readers/writers.
//!
//! Public-API guarantee: every `#[hdk_extern]` and shared struct exposed
//! by the original file is re-exported from this module so existing
//! callsites and the conductor's WASM symbol table are unaffected. Coordinator
//! module splits remain hot-swappable; the sibling pass-6 integrity split is a
//! sanctioned DNA-changing refactor tracked in the pass lineage docs.

use content_integrity::*;
use hdk::prelude::*;

pub mod crud;
pub mod get_helpers;
pub mod migration;
pub mod paging;
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
    /// Micros timestamp of the SELECTED action — the create for a
    /// never-updated entry, the latest update otherwise. `None` on the
    /// create-extern response (no Record fetch happens there; consumers
    /// must not fabricate a time). Additive pass-6-pinned-hosts field:
    /// old consumers deserialize it away, absent decodes to `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_action_micros: Option<i64>,
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
pub use paging::{
    get_my_content_by_id_link, list_by_author_page, list_by_dynamic_link_page,
    list_by_hive_link_page, AuthorLinkPageInput, BoundedLinkPage, DynamicLinkPageInput,
    HiveLinkPageInput, MyContentByIdInput, OwnContentRecords, SourcePosition,
};
pub use queries::{
    count_links_by_hive, fetch_pair_ss_with_hive_check, get_by_content_id_link, list_by_acl_link,
    list_by_author, list_by_dynamic_link, list_by_hive_link, CountByHiveInput,
    FetchPairWithHiveCheckInput, ListByAclInput, ListByAuthorInput, ListByContentIdInput,
    ListByDynamicLinkInput, ListByHiveInput,
};
pub use signals::{
    remote_signal_acl_readers, send_blob_pin_signal, send_dm_call_init_accept,
    send_dm_call_init_request, send_dm_call_sdp_data, send_dm_delete_request, BlobPinHint,
    BlobPinSignal, DmCallSignal, DmDeleteRequestSignal, DmRemoteSignal, EncryptedContentSignal,
    EncryptedContentSignalType, SendBlobPinSignalInput, SendDmCallInitAcceptInput,
    SendDmCallInitRequestInput, SendDmCallSdpDataInput, SendDmDeleteRequestInput,
};
