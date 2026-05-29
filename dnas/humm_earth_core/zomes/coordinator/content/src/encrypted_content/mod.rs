//! `encrypted_content` — coordinator-side externs and helpers for the
//! `EncryptedContent` integrity entry.
//!
//! Split out of the original monolithic `encrypted_content.rs` (which had
//! grown past the file/function size cap that the repo enforces) into:
//!
//! - `crud.rs` — create / get / get_many / update / delete externs.
//! - `queries.rs` — every `list_by_*` / `count_links_by_hive` /
//!   `get_by_content_id_link` / `get_encrypted_content_by_time_and_author` /
//!   `fetch_pair_ss_with_hive_check` (C4).
//! - `signals.rs` — `EncryptedContentSignal` + new `DmRemoteSignal`
//!   (C6 delete-request, C7 WebRTC) + the `send_dm_*` externs and the
//!   `remote_signal_acl_readers` fan-out helper.
//! - `get_helpers.rs` — generic DHT-get helpers
//!   (`get_eh`, `get_record`, `get_latest_typed_from_eh`, `sah_to_ah`).
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
pub mod queries;
pub mod signals;

// --- Shared wire-shape types -------------------------------------------------
//
// These were originally at the top of `encrypted_content.rs`; they live here
// because more than one submodule needs them.

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
    pub hive_id: String,
    pub content_type: String,
    pub revision_author_signing_public_key: String,
    pub bytes: SerializedBytes,
    pub acl: Acl,
    pub public_key_acl: Acl,
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
pub use queries::{
    count_links_by_hive, fetch_pair_ss_with_hive_check, get_by_content_id_link,
    get_encrypted_content_by_time_and_author, list_by_acl_link, list_by_author,
    list_by_dynamic_link, list_by_hive_link, CountByHiveInput, FetchPairWithHiveCheckInput,
    GetEncryptedContentByTimeAndAuthorInput, ListByAclInput, ListByAuthorInput,
    ListByContentIdInput, ListByDynamicLinkInput, ListByHiveInput,
};
pub use signals::{
    remote_signal_acl_readers, send_dm_call_init_accept, send_dm_call_init_request,
    send_dm_call_sdp_data, send_dm_delete_request, DmCallSignal, DmDeleteRequestSignal,
    DmRemoteSignal, EncryptedContentSignal, EncryptedContentSignalType,
    SendDmCallInitAcceptInput, SendDmCallInitRequestInput, SendDmCallSdpDataInput,
    SendDmDeleteRequestInput,
};
