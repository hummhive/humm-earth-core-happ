//! Forward-pointer migration markers — let old-DNA clients detect "this
//! data has moved" after a DNA-hash-bumping upgrade.
//!
//! The marker mechanism, security model, and receiver-side trust
//! requirements are documented in the project migration guide. Read
//! it before extending this module.

use content_integrity::{EncryptedContent, EncryptedContentHeader};
use hdk::prelude::*;

use super::EncryptedContentResponse;

/// Sentinel prefix on `EncryptedContentHeader.content_type` that marks an
/// entry as having been forward-migrated to a new DNA. The original
/// content_type is preserved after the prefix so a `split_once('/')`
/// recovers it cheaply.
pub const MIGRATION_MARKER_CONTENT_TYPE_PREFIX: &str = "_migrated/";

/// Magic discriminator that identifies a [`MigrationMarkerV1`] payload
/// when deserialised from untrusted bytes — distinct enough that random
/// msgpack-decoded bytes will not collide with it.
pub const MIGRATION_MARKER_SCHEMA_TAG: &str = "humm-earth-core-happ/migration-marker";

/// Versioned migration-marker payload stored as msgpack-encoded bytes in
/// the `EncryptedContent.bytes` field of a marker update.
///
/// `schema_version` is the explicit forward-compat hinge. As of
/// pass-2.5, [`MigrationMarkerV2`] ships alongside the
/// [`get_migration_marker_v2`] reader that handles BOTH V1 and V2
/// bytes; pre-pass-2.5 V1-only readers see a V2 marker as `Ok(None)`
/// (well_formed mismatch on `schema_version`). Bump only with the
/// reader update in the same release.
#[hdk_entry_helper]
#[derive(Clone, PartialEq, Eq)]
pub struct MigrationMarkerV1 {
    /// Always equal to [`MIGRATION_MARKER_SCHEMA_TAG`]. Set by
    /// [`MigrationMarkerV1::new`]; verified by
    /// [`MigrationMarkerV1::is_well_formed`].
    pub schema_tag: String,

    /// Always `1` for `MigrationMarkerV1`.
    pub schema_version: u32,

    /// Multibase holohash of the NEW DNA the data now lives in
    /// (`'u' + URL_SAFE_NO_PAD(39)`, matching `hc dna hash` output).
    pub new_dna_hash_base64: String,

    /// Multibase holohash of the migrated entry on the NEW DNA. The host
    /// uses this to redirect queries from the old AH to the new AH —
    /// only AFTER the human approves the new DNA (see migration guide
    /// "Mandatory host-side defenses" B).
    pub new_action_hash_base64: String,

    /// `installed_app_id` of the NEW hApp on the user's conductor. Lets
    /// the host detect whether the new hApp is locally installed and
    /// prompt installation if not.
    pub new_app_id: String,

    /// Microsecond Unix timestamp of when the marker was written.
    pub migrated_at_microseconds: i64,
}

impl MigrationMarkerV1 {
    /// Construct a marker with the schema tag pre-filled. The boundary
    /// for code-side construction; pairs with
    /// [`MigrationMarkerV1::is_well_formed`] which is the boundary for
    /// wire-side deserialisation of untrusted bytes.
    pub fn new(
        new_dna_hash_base64: String,
        new_action_hash_base64: String,
        new_app_id: String,
        migrated_at_microseconds: i64,
    ) -> Self {
        Self {
            schema_tag: MIGRATION_MARKER_SCHEMA_TAG.into(),
            schema_version: 1,
            new_dna_hash_base64,
            new_action_hash_base64,
            new_app_id,
            migrated_at_microseconds,
        }
    }

    /// Cheap guard against treating random `bytes` that happen to decode
    /// as `MigrationMarkerV1` as an actual marker.
    pub fn is_well_formed(&self) -> bool {
        self.schema_tag == MIGRATION_MARKER_SCHEMA_TAG && self.schema_version == 1
    }
}

/// Construct an `EncryptedContent` payload that, fed to
/// `update_encrypted_content`, marks the original entry as migrated.
///
/// Preserves `id`, `hive_id`, `acl`, `public_key_acl`, and
/// `revision_author_signing_public_key` verbatim via struct-update so
/// the marker stays under the same hive / ACL anchors as the original
/// entry, and any future `EncryptedContentHeader` fields are carried
/// forward automatically.
pub fn build_marker_payload(
    original: &EncryptedContent,
    marker: &MigrationMarkerV1,
) -> ExternResult<EncryptedContent> {
    let bytes = SerializedBytes::try_from(marker.clone())
        .map_err(|err| wasm_error!(WasmErrorInner::Serialize(err)))?;
    let original_content_type = &original.header.content_type;
    let marker_content_type =
        if original_content_type.starts_with(MIGRATION_MARKER_CONTENT_TYPE_PREFIX) {
            // Idempotent on the prefix: re-marking an already-marked
            // entry preserves the sentinel exactly once. Bytes ARE still
            // overwritten with the supplied marker; caller decides which
            // marker is authoritative (latest-from-trusted-author wins
            // on the reader side).
            original_content_type.clone()
        } else {
            format!("{MIGRATION_MARKER_CONTENT_TYPE_PREFIX}{original_content_type}")
        };
    Ok(EncryptedContent {
        header: EncryptedContentHeader {
            content_type: marker_content_type,
            ..original.header.clone()
        },
        bytes,
    })
}

/// Input for [`mark_migrated`].
#[derive(Serialize, Deserialize, Debug)]
pub struct MarkMigratedInput {
    /// Action hash of the original entry. Typically the original
    /// `Create` action hash the host stored at first ingest.
    pub original_action_hash: ActionHash,
    /// The forward-pointer payload to write onto the original entry's
    /// chain.
    pub marker: MigrationMarkerV1,
}

/// Write a forward-pointer marker onto an original entry by delegating
/// to `update_encrypted_content`. Returns the full
/// [`EncryptedContentResponse`] for the marker update so callers can
/// inspect the new action hash and signal-emitted state.
///
/// Side-effects (inherited from `update_encrypted_content`):
/// local `emit_signal` carrying the marker payload to the author's own
/// UI; cross-host `send_remote_signal` fan-out via
/// `remote_signal_acl_readers` to every agent in the entry's
/// `public_key_acl.reader` (minus self), with `from_agent` stamped on
/// the receiver by `recv_remote_signal`; standard `EncryptedContentUpdates`
/// + `OriginalHashPointer` link plumbing.
///
/// **Not in `set_cap_tokens`** — local-only by design. A remote-callable
/// `mark_migrated` would let attackers pollute another agent's chain
/// AND fan out a spurious cross-host migration signal in that agent's
/// name. Local-only matches the precedent of `update_encrypted_content`
/// itself.
#[hdk_extern]
pub fn mark_migrated(input: MarkMigratedInput) -> ExternResult<EncryptedContentResponse> {
    let original = super::crud::get_encrypted_content(input.original_action_hash.clone())?;
    let marker_payload = build_marker_payload(&original.encrypted_content, &input.marker)?;
    super::crud::update_encrypted_content(super::UpdateEncryptedContentInput {
        previous_encrypted_content_hash: input.original_action_hash,
        updated_encrypted_content: marker_payload,
    })
}

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
    let Some(original_record) = get(
        AnyDhtHash::from(action_hash),
        GetOptions {
            strategy: GetStrategy::Network,
        },
    )?
    else {
        return Ok(None);
    };
    let trusted_author = original_record.action().author().clone();
    let Some(entry_hash) = original_record.action().entry_hash() else {
        return Ok(None);
    };
    let entry_hash = entry_hash.clone();

    let details = match get_details(
        entry_hash,
        GetOptions {
            strategy: GetStrategy::Network,
        },
    )? {
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
    let Some(update_record) = get(
        update_action_hash,
        GetOptions {
            strategy: GetStrategy::Network,
        },
    )?
    else {
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

// ---------------------------------------------------------------------------
// V2 marker — additive forward-compatible superset of V1.
// ---------------------------------------------------------------------------

/// Forward-compatible superset of [`MigrationMarkerV1`].
///
/// Two additive optional fields carry the per-hive identity continuity
/// info that pass-2 needs to migrate a hive's cryptographic identity
/// (the `hive_genesis_hash` introduced in pass-2 integrity):
///
/// - `new_hive_genesis_hash_base64` — when the marker is written on an
///   old-DNA "hive setup" entry, this points members at the new-DNA
///   `HiveGenesis` action hash they should join. `None` on per-entry
///   content markers.
/// - `new_hive_genesis_display_id` — display alias stamped on the new
///   `HiveGenesis` (typically the old squuid `hive_id` for continuity).
///   `None` on per-entry content markers.
///
/// ## Cross-version decode rules (load-bearing for migration rollout)
///
/// - **V2 bytes decoded as V1 struct**: succeeds (msgpack with
///   `with_struct_map` ignores unknown fields), but
///   [`MigrationMarkerV1::is_well_formed`] returns `false`
///   (`schema_version == 2`), so the V1 reader returns `Ok(None)` —
///   pre-pass-2 hosts simply don't see V2 markers. Documented: V2
///   markers require V2-aware hosts to discover.
/// - **V1 bytes decoded as V2 struct**: succeeds (`#[serde(default)]`
///   yields `None` for the two new fields), but
///   [`MigrationMarkerV2::is_well_formed`] returns `false`
///   (`schema_version == 1`), so V2 readers fall back to the V1 decode
///   path via [`MigrationMarker`]. V2 readers see EVERY V1 marker.
///
/// `schema_version` is the only discriminator. Future `MigrationMarkerV3`
/// additions MUST preserve this contract: extra fields with
/// `#[serde(default)]` AND a bumped `schema_version` AND a paired
/// reader update.
#[hdk_entry_helper]
#[derive(Clone, PartialEq, Eq)]
pub struct MigrationMarkerV2 {
    /// Always equal to [`MIGRATION_MARKER_SCHEMA_TAG`]. Set by
    /// [`MigrationMarkerV2::new`]; verified by
    /// [`MigrationMarkerV2::is_well_formed`].
    pub schema_tag: String,

    /// Always `2` for `MigrationMarkerV2`.
    pub schema_version: u32,

    /// Multibase holohash of the NEW DNA the data now lives in.
    pub new_dna_hash_base64: String,

    /// Multibase holohash of the migrated entry on the NEW DNA (for
    /// per-entry content markers) OR multibase holohash of the new-DNA
    /// `HiveGenesis` entry (for hive-identity markers). Disambiguated
    /// by the presence of `new_hive_genesis_hash_base64`.
    pub new_action_hash_base64: String,

    /// `installed_app_id` of the NEW hApp on the user's conductor.
    pub new_app_id: String,

    /// Microsecond Unix timestamp of when the marker was written.
    pub migrated_at_microseconds: i64,

    /// `Some(hash)` for hive-identity markers: action-hash multibase of
    /// the new-DNA `HiveGenesis` entry. Members read this to discover
    /// which `HiveGenesis` they should join on the new DNA. `None` for
    /// per-entry content markers.
    ///
    /// `#[serde(default)]` is load-bearing: lets V2 readers decode V1
    /// bytes without missing-field errors.
    #[serde(default)]
    pub new_hive_genesis_hash_base64: Option<String>,

    /// `Some(id)` for hive-identity markers: display alias stamped on
    /// the new `HiveGenesis` (typically the old squuid `hive_id`).
    /// `None` for per-entry content markers.
    #[serde(default)]
    pub new_hive_genesis_display_id: Option<String>,
}

impl MigrationMarkerV2 {
    /// Construct a marker with `schema_tag` and `schema_version`
    /// pre-filled. Pairs with [`MigrationMarkerV2::is_well_formed`] as
    /// the wire-side validation boundary.
    pub fn new(
        new_dna_hash_base64: String,
        new_action_hash_base64: String,
        new_app_id: String,
        migrated_at_microseconds: i64,
        new_hive_genesis_hash_base64: Option<String>,
        new_hive_genesis_display_id: Option<String>,
    ) -> Self {
        Self {
            schema_tag: MIGRATION_MARKER_SCHEMA_TAG.into(),
            schema_version: 2,
            new_dna_hash_base64,
            new_action_hash_base64,
            new_app_id,
            migrated_at_microseconds,
            new_hive_genesis_hash_base64,
            new_hive_genesis_display_id,
        }
    }

    /// Cheap guard against treating random bytes (or a V1 marker
    /// successfully decoded into the V2 struct via `#[serde(default)]`)
    /// as a real V2 marker.
    pub fn is_well_formed(&self) -> bool {
        self.schema_tag == MIGRATION_MARKER_SCHEMA_TAG && self.schema_version == 2
    }
}

/// Reader-side tagged enum spanning V1 + V2 markers.
///
/// Serializes via serde's default external tagging: `{ "V1": {...} }`
/// or `{ "V2": {...} }`. TS/JS consumers switch on the single key.
/// Adding a V3 variant is purely additive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationMarker {
    V1(MigrationMarkerV1),
    V2(MigrationMarkerV2),
}

/// Construct an `EncryptedContent` payload that, fed to
/// `update_encrypted_content`, marks the original entry as migrated
/// with a V2 marker. Twin of [`build_marker_payload`].
///
/// See `build_marker_payload` for the full carry-forward contract; V2
/// only changes the bytes payload. The carry-forward set already
/// includes the pass-2-added `hive_genesis_hash` and
/// `author_membership_hash` fields via the struct-update spread, so
/// the marker stays validly anchored in the same hive even when the
/// underlying integrity schema gains fields.
pub fn build_marker_v2_payload(
    original: &EncryptedContent,
    marker: &MigrationMarkerV2,
) -> ExternResult<EncryptedContent> {
    let bytes = SerializedBytes::try_from(marker.clone())
        .map_err(|err| wasm_error!(WasmErrorInner::Serialize(err)))?;
    let original_content_type = &original.header.content_type;
    let marker_content_type =
        if original_content_type.starts_with(MIGRATION_MARKER_CONTENT_TYPE_PREFIX) {
            // Idempotent on the prefix; see `build_marker_payload`.
            original_content_type.clone()
        } else {
            format!("{MIGRATION_MARKER_CONTENT_TYPE_PREFIX}{original_content_type}")
        };
    Ok(EncryptedContent {
        header: EncryptedContentHeader {
            content_type: marker_content_type,
            ..original.header.clone()
        },
        bytes,
    })
}

/// Input for [`mark_migrated_v2`].
#[derive(Debug, Serialize, Deserialize)]
pub struct MarkMigratedV2Input {
    /// Action hash of the original entry. Typically the original
    /// `Create` action hash the host stored at first ingest.
    pub original_action_hash: ActionHash,
    /// The forward-pointer payload to write onto the original entry's
    /// chain.
    pub marker: MigrationMarkerV2,
}

/// V2 twin of [`mark_migrated`]. Same security envelope: local-only by
/// design (NOT in `set_cap_tokens`); see the V1 doc-comment for the
/// rationale.
///
/// Use this for the **hive-identity** migration markers that carry
/// `new_hive_genesis_hash_base64`. For per-entry content markers, V1
/// is sufficient when V1-only readers still exist in the wild; pass-2.5
/// hosts can write V2 here too without breaking V2-aware readers.
#[hdk_extern]
pub fn mark_migrated_v2(input: MarkMigratedV2Input) -> ExternResult<EncryptedContentResponse> {
    let original = super::crud::get_encrypted_content(input.original_action_hash.clone())?;
    let marker_payload = build_marker_v2_payload(&original.encrypted_content, &input.marker)?;
    super::crud::update_encrypted_content(super::UpdateEncryptedContentInput {
        previous_encrypted_content_hash: input.original_action_hash,
        updated_encrypted_content: marker_payload,
    })
}

/// Pure decode: try V2 first, fall back to V1; return `None` if neither
/// shape is well-formed.
///
/// V2 is tried first because well-formedness is decided by
/// `schema_version` embedded in the bytes — trying V1 first would
/// successfully decode V2 bytes into a V1 struct (extra fields ignored)
/// but then reject them at `is_well_formed`, short-circuiting before V2
/// ever sees them.
///
/// Extracted as a private helper so the test module can exercise
/// decode-priority semantics without a DHT.
fn decode_marker(bytes: SerializedBytes) -> Option<MigrationMarker> {
    if let Ok(m2) = MigrationMarkerV2::try_from(bytes.clone()) {
        if m2.is_well_formed() {
            return Some(MigrationMarker::V2(m2));
        }
    }
    if let Ok(m1) = MigrationMarkerV1::try_from(bytes) {
        if m1.is_well_formed() {
            return Some(MigrationMarker::V1(m1));
        }
    }
    None
}

/// V2 reader. Same DHT walk + author-binding filter as
/// [`get_migration_marker`] but returns the polymorphic
/// [`MigrationMarker`] so callers handle both V1 and V2 markers
/// uniformly.
///
/// Use this in pass-2-aware hosts. The V1 reader remains in place for
/// callers that have not yet learned the V2 shape.
///
/// Return semantics mirror the V1 reader; see its doc-comment for the
/// full table.
#[hdk_extern]
pub fn get_migration_marker_v2(action_hash: ActionHash) -> ExternResult<Option<MigrationMarker>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use content_integrity::{Acl, AclByGroupGenesis, AclSpec};

    fn sample_acl() -> Acl {
        Acl {
            owner: "owner".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        }
    }

    fn sample_original() -> EncryptedContent {
        EncryptedContent {
            header: EncryptedContentHeader {
                id: "msg-1".into(),
                display_hive_id: "hive-1".into(),
                content_type: "dm".into(),
                revision_author_signing_public_key: "uhCAk-original-author".into(),
                acl_spec: AclSpec::HiveGroup {
                    hive_genesis_hash: ActionHash::from_raw_36(vec![7u8; 36]),
                    author_membership_hash: None,
                    group_acl: AclByGroupGenesis {
                        owner: ActionHash::from_raw_36(vec![8u8; 36]),
                        admin: vec![],
                        writer: vec![],
                        reader: vec![],
                    },
                    author_group_membership_hash: None,
                    recipient_witnesses: vec![],
                },
                public_key_acl: sample_acl(),
            },
            bytes: UnsafeBytes::from(vec![0xDE, 0xAD, 0xBE, 0xEF]).into(),
        }
    }

    /// Round-trip: build a marker payload, then decode it back via the
    /// same `TryFrom<SerializedBytes>` that `get_migration_marker` uses
    /// internally. Does NOT exercise the extern's content_type gate or
    /// author-binding — those require a DHT and are covered by the
    /// TR-MIG-* tryorama tests once the harness is paired.
    #[test]
    fn marker_payload_round_trips() {
        let original = sample_original();
        let marker = MigrationMarkerV1::new(
            "uhC0k-new-dna-hash".into(),
            "uhCkk-new-action-hash".into(),
            "humm-earth-core@2".into(),
            1_700_000_000_000_000,
        );
        let payload = build_marker_payload(&original, &marker).expect("build");
        assert_eq!(payload.header.content_type, "_migrated/dm");
        assert_eq!(payload.header.id, original.header.id);
        assert_eq!(
            payload.header.display_hive_id,
            original.header.display_hive_id
        );
        assert_eq!(
            payload.header.revision_author_signing_public_key,
            original.header.revision_author_signing_public_key
        );
        let decoded = MigrationMarkerV1::try_from(payload.bytes).expect("decode");
        assert!(decoded.is_well_formed());
        assert_eq!(decoded, marker);
    }

    /// Re-marking an already-marked entry's content_type keeps the
    /// prefix at exactly one — bytes are still overwritten.
    #[test]
    fn marker_payload_is_idempotent_on_content_type_prefix() {
        let mut original = sample_original();
        original.header.content_type = "_migrated/dm".into();
        let marker = MigrationMarkerV1::new(
            "uhC0k-new-dna-hash".into(),
            "uhCkk-new-action-hash".into(),
            "humm-earth-core@2".into(),
            1_700_000_000_000_000,
        );
        let payload = build_marker_payload(&original, &marker).expect("build");
        assert_eq!(payload.header.content_type, "_migrated/dm");
    }

    /// Well-formed marker passes the schema-tag+version check.
    #[test]
    fn well_formed_marker_passes_check() {
        let m = MigrationMarkerV1::new("uhC0k".into(), "uhCkk".into(), "app".into(), 0);
        assert!(m.is_well_formed());
    }

    /// A struct with the same field shape but a different `schema_tag`
    /// fails the well-formed check.
    #[test]
    fn malformed_marker_fails_check() {
        let m = MigrationMarkerV1 {
            schema_tag: "something-else".into(),
            schema_version: 1,
            new_dna_hash_base64: String::new(),
            new_action_hash_base64: String::new(),
            new_app_id: String::new(),
            migrated_at_microseconds: 0,
        };
        assert!(!m.is_well_formed());
    }

    /// A future V2 marker (different schema_version) also fails the V1
    /// check. The reader returns `Ok(None)` for V2 markers — a V1
    /// reader cannot interpret V2; a future reader must handle both.
    #[test]
    fn unknown_schema_version_fails_well_formed_check() {
        let m = MigrationMarkerV1 {
            schema_tag: MIGRATION_MARKER_SCHEMA_TAG.into(),
            schema_version: 2,
            new_dna_hash_base64: String::new(),
            new_action_hash_base64: String::new(),
            new_app_id: String::new(),
            migrated_at_microseconds: 0,
        };
        assert!(!m.is_well_formed());
    }

    // -----------------------------------------------------------------------
    // V2 marker tests
    // -----------------------------------------------------------------------

    /// Round-trip a V2 marker through the same serialization path used
    /// by `build_marker_v2_payload`.
    #[test]
    fn marker_v2_payload_round_trips() {
        let original = sample_original();
        let marker = MigrationMarkerV2::new(
            "uhC0k-new-dna-hash".into(),
            "uhCkk-new-action-hash".into(),
            "humm-earth-core@2".into(),
            1_700_000_000_000_000,
            Some("uhCkk-new-genesis-hash".into()),
            Some("hive-1".into()),
        );
        let payload = build_marker_v2_payload(&original, &marker).expect("build");
        assert_eq!(payload.header.content_type, "_migrated/dm");
        assert_eq!(payload.header.id, original.header.id);
        assert_eq!(
            payload.header.display_hive_id,
            original.header.display_hive_id
        );
        assert_eq!(payload.header.acl_spec, original.header.acl_spec);
        assert_eq!(
            payload.header.revision_author_signing_public_key,
            original.header.revision_author_signing_public_key,
        );
        let decoded = MigrationMarkerV2::try_from(payload.bytes).expect("decode");
        assert!(decoded.is_well_formed());
        assert_eq!(decoded, marker);
    }

    /// Each failure mode of `MigrationMarkerV2::is_well_formed`.
    #[test]
    fn marker_v2_well_formed_check() {
        let good = MigrationMarkerV2::new("u".into(), "u".into(), "app".into(), 0, None, None);
        assert!(good.is_well_formed());

        let wrong_tag = MigrationMarkerV2 {
            schema_tag: "something-else".into(),
            ..MigrationMarkerV2::new("u".into(), "u".into(), "app".into(), 0, None, None)
        };
        assert!(!wrong_tag.is_well_formed());

        let wrong_version = MigrationMarkerV2 {
            schema_version: 1,
            ..MigrationMarkerV2::new("u".into(), "u".into(), "app".into(), 0, None, None)
        };
        assert!(!wrong_version.is_well_formed());
    }

    /// V2-authored bytes decode into the V1 struct (struct-map msgpack
    /// ignores unknown fields), but V1's `is_well_formed` returns false
    /// because `schema_version == 2`. Documents and guards the
    /// degradation path for pre-pass-2 hosts reading V2 markers.
    #[test]
    fn v2_marker_bytes_decode_as_v1_struct_but_fail_well_formed_check() {
        let v2 = MigrationMarkerV2::new(
            "uhC0k".into(),
            "uhCkk".into(),
            "app".into(),
            1,
            Some("uhCkk-genesis".into()),
            Some("hive-1".into()),
        );
        let bytes = SerializedBytes::try_from(v2).expect("serialize V2");
        let v1_view = MigrationMarkerV1::try_from(bytes).expect("decode V1");
        assert_eq!(v1_view.schema_tag, MIGRATION_MARKER_SCHEMA_TAG);
        assert_eq!(v1_view.schema_version, 2);
        assert!(!v1_view.is_well_formed());
    }

    /// V1-authored bytes decode into the V2 struct via
    /// `#[serde(default)]` on the V2-only optional fields, but V2's
    /// `is_well_formed` returns false because `schema_version == 1`.
    /// This forward-compat property is what makes `decode_marker`'s
    /// V1 fallback work for V1 bytes.
    #[test]
    fn v1_marker_bytes_decode_as_v2_struct_with_none_fields() {
        let v1 = MigrationMarkerV1::new("uhC0k".into(), "uhCkk".into(), "app".into(), 1);
        let bytes = SerializedBytes::try_from(v1).expect("serialize V1");
        let v2_view = MigrationMarkerV2::try_from(bytes).expect("decode V2");
        assert_eq!(v2_view.schema_version, 1);
        assert_eq!(v2_view.new_hive_genesis_hash_base64, None);
        assert_eq!(v2_view.new_hive_genesis_display_id, None);
        assert!(!v2_view.is_well_formed());
    }

    /// `build_marker_v2_payload` preserves the V1 carry-forward
    /// contract AND embeds the V2-only genesis fields into the bytes.
    #[test]
    fn build_marker_v2_payload_carries_genesis_fields() {
        let original = sample_original();
        let marker = MigrationMarkerV2::new(
            "uhC0k-new-dna".into(),
            "uhCkk-new-action".into(),
            "humm-earth-core@2".into(),
            42,
            Some("uhCkk-genesis-on-new-dna".into()),
            Some("hive-1-display".into()),
        );
        let payload = build_marker_v2_payload(&original, &marker).expect("build");
        assert_eq!(
            payload.header.display_hive_id,
            original.header.display_hive_id
        );
        assert_eq!(payload.header.acl_spec, original.header.acl_spec);
        let decoded = MigrationMarkerV2::try_from(payload.bytes).expect("decode");
        assert_eq!(
            decoded.new_hive_genesis_hash_base64.as_deref(),
            Some("uhCkk-genesis-on-new-dna"),
        );
        assert_eq!(
            decoded.new_hive_genesis_display_id.as_deref(),
            Some("hive-1-display"),
        );
    }

    /// `MigrationMarker::V1` and `::V2` round-trip through the same
    /// msgpack `with_struct_map` encoding used by SerializedBytes
    /// internally. Pins serde's default external tagging behaviour
    /// (`{"V1": {...}}` / `{"V2": {...}}`) so a swap to internal
    /// tagging via `#[serde(tag = ...)]` is a deliberate breaking
    /// change rather than an accident — TS callers switch on the
    /// outer single key, and internal tagging would break them.
    #[test]
    fn migration_marker_enum_round_trip() {
        let v1 = MigrationMarker::V1(MigrationMarkerV1::new(
            "uhC0k".into(),
            "uhCkk".into(),
            "app".into(),
            1,
        ));
        let v2 = MigrationMarker::V2(MigrationMarkerV2::new(
            "uhC0k".into(),
            "uhCkk".into(),
            "app".into(),
            2,
            Some("uhCkk-genesis".into()),
            Some("hive-1".into()),
        ));
        for variant in [v1, v2] {
            let bytes = holochain_serialized_bytes::encode(&variant).expect("ser");
            // External-tagging wire-shape guard: first byte is a
            // msgpack 1-fixmap (0x81). Internal tagging would prefix
            // a larger fixmap (one entry per inner field plus the
            // tag) — e.g. 0x87 for the 6-field V2 + tag.
            assert_eq!(
                bytes.first().copied(),
                Some(0x81),
                "MigrationMarker must serialize as a 1-element msgpack fixmap \
                 (external tagging). Got first byte: {:?}. A change to internal \
                 tagging would break TS callers that switch on the outer key.",
                bytes.first(),
            );
            let back: MigrationMarker = holochain_serialized_bytes::decode(&bytes).expect("de");
            assert_eq!(back, variant);
        }
    }

    /// `decode_marker` prefers V2 when the bytes are V2.
    #[test]
    fn decode_marker_prefers_v2_for_v2_bytes() {
        let v2 = MigrationMarkerV2::new(
            "uhC0k".into(),
            "uhCkk".into(),
            "app".into(),
            1,
            Some("uhCkk-genesis".into()),
            Some("hive-1".into()),
        );
        let bytes = SerializedBytes::try_from(v2.clone()).expect("ser");
        let decoded = decode_marker(bytes).expect("decoded");
        match decoded {
            MigrationMarker::V2(got) => assert_eq!(got, v2),
            MigrationMarker::V1(_) => panic!("expected V2 variant"),
        }
    }

    /// `decode_marker` falls back to V1 when the bytes are V1 — V2
    /// decode succeeds via `#[serde(default)]` but fails well_formed.
    #[test]
    fn decode_marker_falls_back_to_v1_for_v1_bytes() {
        let v1 = MigrationMarkerV1::new("uhC0k".into(), "uhCkk".into(), "app".into(), 1);
        let bytes = SerializedBytes::try_from(v1.clone()).expect("ser");
        let decoded = decode_marker(bytes).expect("decoded");
        match decoded {
            MigrationMarker::V1(got) => assert_eq!(got, v1),
            MigrationMarker::V2(_) => panic!("expected V1 variant"),
        }
    }

    /// `decode_marker` returns `None` when neither V1 nor V2 well_formed
    /// checks pass (e.g. wrong schema_tag).
    #[test]
    fn decode_marker_returns_none_for_bad_schema_tag() {
        let bad = MigrationMarkerV1 {
            schema_tag: "something-else".into(),
            schema_version: 1,
            new_dna_hash_base64: String::new(),
            new_action_hash_base64: String::new(),
            new_app_id: String::new(),
            migrated_at_microseconds: 0,
        };
        let bytes = SerializedBytes::try_from(bad).expect("ser");
        assert!(decode_marker(bytes).is_none());
    }

    /// `build_marker_v2_payload` keeps the content_type prefix at
    /// exactly one when the original is already prefixed (twin of
    /// `marker_payload_is_idempotent_on_content_type_prefix` for V1).
    /// A copy-paste error in the V2 builder's prefix logic would
    /// otherwise sneak through.
    #[test]
    fn marker_v2_payload_is_idempotent_on_content_type_prefix() {
        let mut original = sample_original();
        original.header.content_type = "_migrated/dm".into();
        let marker =
            MigrationMarkerV2::new("uhC0k".into(), "uhCkk".into(), "app".into(), 0, None, None);
        let payload = build_marker_v2_payload(&original, &marker).expect("build");
        assert_eq!(payload.header.content_type, "_migrated/dm");
    }

    /// `decode_marker` returns `None` for a marker whose schema_version
    /// is neither 1 nor 2 (e.g. a hypothetical V3 written by a future
    /// release before pass-2.5 readers learn the V3 shape). Matches
    /// the debug-log path in `get_migration_marker_v2` that says the
    /// "author may be running a newer schema".
    #[test]
    fn decode_marker_returns_none_for_unknown_schema_version() {
        let v3 = MigrationMarkerV2 {
            schema_version: 3,
            ..MigrationMarkerV2::new("uhC0k".into(), "uhCkk".into(), "app".into(), 0, None, None)
        };
        let bytes = SerializedBytes::try_from(v3).expect("ser");
        assert!(decode_marker(bytes).is_none());
    }
}
