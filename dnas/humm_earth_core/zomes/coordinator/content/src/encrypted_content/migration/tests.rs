use super::markers::decode_marker;
use super::*;
use content_integrity::{
    Acl, AclByGroupGenesis, AclSpec, EncryptedContent, EncryptedContentHeader,
};
use hdk::prelude::*;

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
