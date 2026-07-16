use content_integrity::{EncryptedContent, HiveGenesis};
use hdk::prelude::*;

use super::markers::{
    decode_marker, MigrationMarker, MigrationMarkerV1, HIVE_GENESIS_MARKER_ORIGINAL_TYPE,
    HIVE_MIGRATION_MARKER_CONTENT_ID, MIGRATION_MARKER_CONTENT_TYPE_PREFIX,
    MIGRATION_MARKER_SCHEMA_TAG,
};
use super::payload::marker_content_type;
use crate::encrypted_content::paging::content_id_records_by_author;

/// Walk the update chain of `action_hash` and return the latest
/// trusted-author update's `EncryptedContent` envelope iff it carries
/// the migration sentinel `content_type` prefix. Shared by V1 and V2
/// readers — bytes-level decoding is the caller's responsibility.
///
/// "Trusted-author" = the author of the action passed in. Mirrors the
/// security invariant documented on `get_migration_marker`: only the
/// original author's updates count as valid markers.
///
/// Returns `Ok(None)` for every "not a marker" branch (action hash
/// does not resolve, action carries no entry hash, entry is not Live,
/// no trusted-author updates, latest update lacks the sentinel
/// content_type). Errors propagate transport failures unchanged.
fn fetch_latest_marker_envelope(action_hash: ActionHash) -> ExternResult<Option<EncryptedContent>> {
    let Some(original_record) = get(AnyDhtHash::from(action_hash), GetOptions::network())? else {
        return Ok(None);
    };
    let trusted_author = original_record.action().author().clone();
    let Some(entry_hash) = original_record.action().entry_hash() else {
        return Ok(None);
    };
    let entry_hash = entry_hash.clone();

    let details = match get_details(entry_hash, GetOptions::network())? {
        Some(Details::Entry(d)) => d,
        _ => return Ok(None),
    };
    if details.entry_dht_status != EntryDhtStatus::Live {
        return Ok(None);
    }
    let Some(latest_marker_update) = details
        .updates
        .iter()
        .filter(|u| u.action().author() == &trusted_author)
        .max_by_key(|u| u.action().timestamp())
    else {
        return Ok(None);
    };

    let update_action_hash = latest_marker_update.as_hash().clone();
    let Some(update_record) = get(update_action_hash, GetOptions::network())? else {
        return Ok(None);
    };
    let typed = update_record
        .entry()
        .to_app_option::<EncryptedContent>()
        .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?;
    let Some(typed_entry) = typed else {
        return Ok(None);
    };

    if !typed_entry
        .header
        .content_type
        .starts_with(MIGRATION_MARKER_CONTENT_TYPE_PREFIX)
    {
        return Ok(None);
    }
    Ok(Some(typed_entry))
}

/// Return the migration marker for the given action hash, if any.
///
/// Walks `get_details(entry_hash).updates`, filters them to only those
/// whose `action.author` equals the author of the action hash passed in,
/// picks the latest by `action.timestamp`, fetches that update's record,
/// and returns the parsed marker iff the update carries the
/// `_migrated/` sentinel and decodes as a well-formed
/// [`MigrationMarkerV1`]. The author-binding filter is the load-bearing
/// security closure for the marker-forge attack documented in the
/// migration guide.
///
/// ## Caller contract
///
/// Pass the action hash the host stored at first ingest — typically the
/// original `Create`. Passing an `Update` action hash is permitted but
/// will only find markers in the chain rooted at THAT update's author,
/// which is usually not what the host wants.
///
/// ## Return semantics
///
/// - `Ok(Some(marker))` — entry has a well-formed marker authored by
///   the same agent as the passed action hash.
/// - `Ok(None)` — the entry resolved on the DHT but does NOT carry a
///   recognised marker. Covers: action hash does not resolve; the
///   action carries no entry hash (e.g. `CreateLink`); entry is not
///   Live (deleted); no trusted-author Updates exist; the latest
///   trusted-author Update lacks the `_migrated/` sentinel; the bytes
///   fail to decode as `MigrationMarkerV1` or decode as a malformed /
///   unknown-schema marker.
/// - `Err(_)` — transport-level failure that prevented the reader from
///   completing the lookup (DHT unreachable, `get` / `get_details` host
///   call failed, or the EncryptedContent envelope decode of a
///   resolved record failed). Retry on `Err` rather than treating it
///   as `Ok(None)`, which would silently hide a real migration during
///   a transient network blip.
#[hdk_extern]
pub fn get_migration_marker(action_hash: ActionHash) -> ExternResult<Option<MigrationMarkerV1>> {
    let Some(typed_entry) = fetch_latest_marker_envelope(action_hash)? else {
        return Ok(None);
    };
    match MigrationMarkerV1::try_from(typed_entry.bytes) {
        Ok(m) if m.is_well_formed() => Ok(Some(m)),
        Ok(bad) => {
            debug!(
                "get_migration_marker: sentinel content_type but schema_tag/version mismatch: \
                 got tag={:?} ver={}; expected tag={:?} ver=1",
                bad.schema_tag, bad.schema_version, MIGRATION_MARKER_SCHEMA_TAG,
            );
            Ok(None)
        }
        Err(err) => {
            debug!(
                "get_migration_marker: sentinel content_type but bytes failed to decode: {err:?}"
            );
            Ok(None)
        }
    }
}

/// V2 reader. Same DHT walk + author-binding filter as
/// [`get_migration_marker`] but returns the polymorphic
/// [`MigrationMarker`] so callers handle both V1 and V2 markers
/// uniformly.
///
/// Use this in pass-2-aware hosts. The V1 reader remains in place for
/// callers that have not yet learned the V2 shape.
///
/// A `HiveGenesis` action hash resolves via the create-based hive
/// marker path (pass-6-idempotent-writes): founder-authored entries on
/// `[genesis_b64, "hive-migration-marker-v2"]`, freshest wins. All
/// other hashes keep the V1 update-chain walk; return semantics mirror
/// the V1 reader.
#[hdk_extern]
pub fn get_migration_marker_v2(action_hash: ActionHash) -> ExternResult<Option<MigrationMarker>> {
    if let Some(record) = get(AnyDhtHash::from(action_hash.clone()), GetOptions::network())? {
        if record
            .entry()
            .to_app_option::<HiveGenesis>()
            .ok()
            .flatten()
            .is_some()
        {
            return hive_genesis_marker(&action_hash, record.action().author());
        }
    }
    let Some(typed_entry) = fetch_latest_marker_envelope(action_hash)? else {
        return Ok(None);
    };
    let decoded = decode_marker(typed_entry.bytes);
    if decoded.is_none() {
        debug!(
            "get_migration_marker_v2: sentinel content_type but bytes are neither \
             a well-formed V1 nor V2 marker — author may be running a newer schema",
        );
    }
    Ok(decoded)
}

/// Trust rule: only markers link-authored AND entry-authored by the
/// genesis author count — mirrors the update-chain trusted-author
/// filter.
fn hive_genesis_marker(
    genesis_hash: &ActionHash,
    genesis_author: &AgentPubKey,
) -> ExternResult<Option<MigrationMarker>> {
    let (records, _truncated) = content_id_records_by_author(
        genesis_hash,
        HIVE_MIGRATION_MARKER_CONTENT_ID,
        genesis_author,
    )?;
    let marker_type = marker_content_type(HIVE_GENESIS_MARKER_ORIGINAL_TYPE);
    let Some(envelope) = records
        .into_iter()
        .filter(|record| record.encrypted_content.header.content_type == marker_type)
        .max_by_key(|record| record.latest_action_micros)
    else {
        return Ok(None);
    };
    let decoded = decode_marker(envelope.encrypted_content.bytes);
    if decoded.is_none() {
        debug!(
            "get_migration_marker_v2: hive marker bytes are neither a well-formed \
             V1 nor V2 marker — author may be running a newer schema",
        );
    }
    Ok(decoded)
}
