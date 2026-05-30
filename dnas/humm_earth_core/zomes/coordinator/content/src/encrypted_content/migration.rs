//! Forward-pointer migration markers — let old-DNA clients detect "this
//! data has moved" after a DNA-hash-bumping upgrade.
//!
//! The marker mechanism, security model, GUI integration flow, and the
//! receiver-side trust requirements live in `docs/DNA_MIGRATION_GUIDE.md`.
//! Read it before extending this module.

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
/// `schema_version` is the explicit forward-compat hinge: a future
/// `MigrationMarkerV2` ships a paired reader update that handles both
/// V1 and V2; until that ships, a V2-only marker is invisible to a V1
/// reader. Bump only with the reader update in the same release.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
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
/// the receiver by the C7b dispatcher; standard `EncryptedContentUpdates`
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
pub fn get_migration_marker(
    action_hash: ActionHash,
) -> ExternResult<Option<MigrationMarkerV1>> {
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

    let marker = match MigrationMarkerV1::try_from(typed_entry.bytes) {
        Ok(m) if m.is_well_formed() => m,
        Ok(bad) => {
            debug!(
                "get_migration_marker: sentinel content_type but schema_tag/version mismatch: \
                 got tag={:?} ver={}; expected tag={:?} ver=1",
                bad.schema_tag, bad.schema_version, MIGRATION_MARKER_SCHEMA_TAG,
            );
            return Ok(None);
        }
        Err(err) => {
            debug!(
                "get_migration_marker: sentinel content_type but bytes failed to decode: {err:?}"
            );
            return Ok(None);
        }
    };

    Ok(Some(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use content_integrity::Acl;

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
                hive_id: "hive-1".into(),
                hive_genesis_hash: ActionHash::from_raw_36(vec![7u8; 36]),
                author_membership_hash: None,
                content_type: "dm".into(),
                revision_author_signing_public_key: "uhCAk-original-author".into(),
                acl: sample_acl(),
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
        assert_eq!(payload.header.hive_id, original.header.hive_id);
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
}
