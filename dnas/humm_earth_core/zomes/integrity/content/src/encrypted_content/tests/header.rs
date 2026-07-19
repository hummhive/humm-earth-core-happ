use super::super::*;
use super::support::*;
use hdi::prelude::*;

// ---------------------------------------------------------------------
// Pass-1 invariants — header pubkey check
// ---------------------------------------------------------------------

#[test]
fn check_rejects_when_header_pubkey_does_not_match_action_author() {
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let bob_b64 = bob.to_string();
    let result = check_author_matches_header(&alice, &bob_b64);
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("revision_author_signing_public_key"));
            assert!(msg.contains(&bob_b64));
            assert!(msg.contains(&alice.to_string()));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn check_accepts_when_header_pubkey_matches_action_author() {
    let alice = agent_pubkey(1);
    let alice_b64 = alice.to_string();
    let result = check_author_matches_header(&alice, &alice_b64);
    assert!(
        matches!(result, ValidateCallbackResult::Valid),
        "expected Valid, got {result:?}",
    );
}

#[test]
fn check_rejects_empty_header_pubkey() {
    let alice = agent_pubkey(1);
    let result = check_author_matches_header(&alice, "");
    assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
}

#[test]
fn check_rejects_legacy_placeholder_header() {
    let alice = agent_pubkey(1);
    let result = check_author_matches_header(&alice, "test-revision-author-signing-public-key");
    assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
}

#[test]
fn header_struct_round_trips_through_the_check() {
    let alice = agent_pubkey(7);
    let valid_header = sample_header_pubkey(&alice.to_string());
    assert!(matches!(
        check_author_matches_header(&alice, &valid_header.revision_author_signing_public_key),
        ValidateCallbackResult::Valid,
    ));
    let forged_header = sample_header_pubkey(&agent_pubkey(8).to_string());
    assert!(matches!(
        check_author_matches_header(&alice, &forged_header.revision_author_signing_public_key),
        ValidateCallbackResult::Invalid(_),
    ));
}

// ---------------------------------------------------------------------
// Pass-7 M1 — header bounds
// ---------------------------------------------------------------------

fn bounded_header() -> EncryptedContentHeader {
    sample_header_pubkey(&agent_pubkey(1).to_string())
}

fn unique_keys(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("k{i:04}")).collect()
}

fn assert_bounds_reject(header: &EncryptedContentHeader, expected_fragment: &str) {
    match validate_header_bounds(header) {
        ValidateCallbackResult::Invalid(msg) => assert!(
            msg.contains(expected_fragment),
            "expected {expected_fragment:?} in {msg:?}",
        ),
        other => panic!("expected Invalid containing {expected_fragment:?}, got {other:?}"),
    }
}

#[test]
fn bounds_accept_a_header_at_every_limit_simultaneously() {
    let mut header = bounded_header();
    header.id = "i".repeat(256);
    header.content_type = "c".repeat(128);
    header.display_hive_id = "d".repeat(256);
    header.public_key_acl.owner = "o".repeat(64);
    header.public_key_acl.reader = unique_keys(256);
    assert!(matches!(
        validate_header_bounds(&header),
        ValidateCallbackResult::Valid,
    ));
}

#[test]
fn bounds_count_chars_not_bytes() {
    let mut header = bounded_header();
    header.id = "é".repeat(256);
    assert!(matches!(
        validate_header_bounds(&header),
        ValidateCallbackResult::Valid,
    ));
}

#[test]
fn bounds_reject_id_out_of_range() {
    let mut header = bounded_header();
    header.id = "i".repeat(257);
    assert_bounds_reject(&header, "header id must be 1-256 chars");
    header.id = String::new();
    assert_bounds_reject(&header, "header id must be 1-256 chars");
}

#[test]
fn bounds_reject_content_type_out_of_range() {
    let mut header = bounded_header();
    header.content_type = "c".repeat(129);
    assert_bounds_reject(&header, "header content_type must be 1-128 chars");
    header.content_type = String::new();
    assert_bounds_reject(&header, "header content_type must be 1-128 chars");
}

#[test]
fn bounds_allow_empty_display_hive_id_but_reject_overlong() {
    let mut header = bounded_header();
    header.display_hive_id = String::new();
    assert!(matches!(
        validate_header_bounds(&header),
        ValidateCallbackResult::Valid,
    ));
    header.display_hive_id = "d".repeat(257);
    assert_bounds_reject(&header, "header display_hive_id must be at most 256 chars");
}

#[test]
fn bounds_allow_empty_owner_but_reject_overlong() {
    let mut header = bounded_header();
    header.public_key_acl.owner = String::new();
    assert!(matches!(
        validate_header_bounds(&header),
        ValidateCallbackResult::Valid,
    ));
    header.public_key_acl.owner = "o".repeat(65);
    assert_bounds_reject(&header, "public_key_acl owner must be at most 64 chars");
}

#[test]
fn bounds_reject_oversized_bucket() {
    let mut header = bounded_header();
    header.public_key_acl.reader = unique_keys(257);
    assert_bounds_reject(&header, "public_key_acl buckets accept at most 256 entries");
}

#[test]
fn bounds_reject_key_length_out_of_range() {
    let mut header = bounded_header();
    header.public_key_acl.writer = vec!["k".repeat(65)];
    assert_bounds_reject(&header, "public_key_acl keys must be 1-64 chars");
    header.public_key_acl.writer = vec![String::new()];
    assert_bounds_reject(&header, "public_key_acl keys must be 1-64 chars");
    header.public_key_acl.writer = vec!["k".repeat(64)];
    assert!(matches!(
        validate_header_bounds(&header),
        ValidateCallbackResult::Valid,
    ));
}

#[test]
fn bounds_reject_duplicates_within_a_bucket_only() {
    let mut header = bounded_header();
    header.public_key_acl.reader = vec!["same-key".into(), "same-key".into()];
    assert_bounds_reject(
        &header,
        "public_key_acl buckets must not contain duplicate keys",
    );
    header.public_key_acl.reader = vec!["same-key".into()];
    header.public_key_acl.writer = vec!["same-key".into()];
    assert!(matches!(
        validate_header_bounds(&header),
        ValidateCallbackResult::Valid,
    ));
}

#[test]
fn author_mismatch_reports_before_bounds_violation() {
    let alice = agent_pubkey(1);
    let mallory = agent_pubkey(2);
    let mut content = content_with_spec(
        AclSpec::OpenWrite {
            target_hive_genesis_hash: None,
        },
        Acl {
            owner: "owner".into(),
            admin: vec![],
            writer: vec![],
            reader: unique_keys(257),
        },
    );
    content.header.revision_author_signing_public_key = alice.to_string();
    let timestamp = Timestamp::from_micros(0);
    match run_content_validators(&mallory, &timestamp, &content)
        .expect("openwrite validation is fetch-free")
    {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("revision_author_signing_public_key"));
        }
        other => panic!("expected author-mismatch Invalid, got {other:?}"),
    }
    match run_content_validators(&alice, &timestamp, &content)
        .expect("openwrite validation is fetch-free")
    {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("public_key_acl buckets accept at most 256 entries"));
        }
        other => panic!("expected bounds Invalid, got {other:?}"),
    }
}
