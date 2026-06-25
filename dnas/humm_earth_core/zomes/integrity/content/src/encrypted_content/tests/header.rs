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
