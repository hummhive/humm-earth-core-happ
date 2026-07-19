use hdk::prelude::*;

pub use content_integrity::MIGRATION_MARKER_CONTENT_TYPE_PREFIX;

/// Magic discriminator that identifies a [`MigrationMarkerV1`] payload
/// when deserialised from untrusted bytes — distinct enough that random
/// msgpack-decoded bytes will not collide with it.
pub const MIGRATION_MARKER_SCHEMA_TAG: &str = "humm-earth-core-happ/migration-marker";

/// Deterministic content-id of the ONE hive-identity marker entry a
/// founder writes per `HiveGenesis` (content-id path
/// `[genesis_b64, this]`). Create-based: the frozen integrity update
/// gate forbids cross-entry-type updates, so hive markers can never
/// ride the V1/V2 update-chain mechanism.
pub const HIVE_MIGRATION_MARKER_CONTENT_ID: &str = "hive-migration-marker-v2";

/// Pseudo original-content-type for hive-identity markers; becomes
/// `_migrated/hive-genesis` via `marker_content_type`.
pub const HIVE_GENESIS_MARKER_ORIGINAL_TYPE: &str = "hive-genesis";

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
pub(super) fn decode_marker(bytes: SerializedBytes) -> Option<MigrationMarker> {
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
