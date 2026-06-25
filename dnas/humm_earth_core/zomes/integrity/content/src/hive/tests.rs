use super::*;
use hdi::hash_path::path::Component;
use hdi::prelude::*;

fn agent_pubkey(byte: u8) -> AgentPubKey {
    AgentPubKey::from_raw_36(vec![byte; 36])
}

#[test]
fn role_satisfies_diagonal_and_below() {
    // Owner satisfies everything.
    assert!(role_satisfies(HiveRole::Owner, HiveRole::Owner));
    assert!(role_satisfies(HiveRole::Owner, HiveRole::Admin));
    assert!(role_satisfies(HiveRole::Owner, HiveRole::Writer));
    assert!(role_satisfies(HiveRole::Owner, HiveRole::Reader));
    // Admin satisfies Admin and below.
    assert!(!role_satisfies(HiveRole::Admin, HiveRole::Owner));
    assert!(role_satisfies(HiveRole::Admin, HiveRole::Admin));
    assert!(role_satisfies(HiveRole::Admin, HiveRole::Writer));
    assert!(role_satisfies(HiveRole::Admin, HiveRole::Reader));
    // Writer satisfies Writer and below.
    assert!(!role_satisfies(HiveRole::Writer, HiveRole::Owner));
    assert!(!role_satisfies(HiveRole::Writer, HiveRole::Admin));
    assert!(role_satisfies(HiveRole::Writer, HiveRole::Writer));
    assert!(role_satisfies(HiveRole::Writer, HiveRole::Reader));
    // Reader satisfies only Reader.
    assert!(!role_satisfies(HiveRole::Reader, HiveRole::Owner));
    assert!(!role_satisfies(HiveRole::Reader, HiveRole::Admin));
    assert!(!role_satisfies(HiveRole::Reader, HiveRole::Writer));
    assert!(role_satisfies(HiveRole::Reader, HiveRole::Reader));
}

#[test]
fn recompute_path_hash_matches_path_entry_hash() {
    // Sanity-pin: recompute_path_hash must agree with the same Path
    // constructed manually, otherwise every link validator's
    // recompute check would silently disagree with the writer's
    // path construction.
    let manual = Path::from(vec![
        Component::from("hive-x"),
        Component::from("content-y"),
    ]);
    let manual_hash: AnyLinkableHash = manual
        .path_entry_hash()
        .expect("manual path hash should compute in test")
        .into();
    let recomputed = recompute_path_hash(&["hive-x", "content-y"])
        .expect("recompute_path_hash should compute in test");
    assert_eq!(manual_hash, recomputed);
}

#[test]
fn agent_pubkey_helper_constructs_consistent_pubkeys() {
    // Trivial guard: any change to the test helper would silently
    // break every other test in this module that relies on stable
    // pubkey identity across calls.
    let alice = agent_pubkey(1);
    let alice_again = agent_pubkey(1);
    let bob = agent_pubkey(2);
    assert_eq!(alice, alice_again);
    assert_ne!(alice, bob);
}

fn action_hash(byte: u8) -> ActionHash {
    ActionHash::from_raw_36(vec![byte; 36])
}

fn entry_hash(byte: u8) -> EntryHash {
    EntryHash::from_raw_36(vec![byte; 36])
}

fn make_create(author: AgentPubKey) -> Create {
    Create {
        author,
        timestamp: Timestamp(0),
        action_seq: 0,
        prev_action: action_hash(0),
        entry_type: EntryType::App(AppEntryDef {
            entry_index: 0.into(),
            zome_index: 0.into(),
            visibility: EntryVisibility::Public,
        }),
        entry_hash: entry_hash(0),
        weight: Default::default(),
    }
}

fn make_update(author: AgentPubKey) -> Update {
    Update {
        author,
        timestamp: Timestamp(0),
        action_seq: 1,
        prev_action: action_hash(0),
        original_action_address: action_hash(7),
        original_entry_address: entry_hash(7),
        entry_type: EntryType::App(AppEntryDef {
            entry_index: 0.into(),
            zome_index: 0.into(),
            visibility: EntryVisibility::Public,
        }),
        entry_hash: entry_hash(8),
        weight: Default::default(),
    }
}

fn make_delete(author: AgentPubKey) -> Delete {
    Delete {
        author,
        timestamp: Timestamp(0),
        action_seq: 1,
        prev_action: action_hash(0),
        deletes_address: action_hash(7),
        deletes_entry_address: entry_hash(7),
        weight: Default::default(),
    }
}

fn sample_genesis() -> HiveGenesis {
    HiveGenesis {
        display_id: "hive-test".into(),
        created_at_microseconds: 0,
    }
}

fn sample_membership(
    for_agent: AgentPubKey,
    role: HiveRole,
    grantor_membership_hash: Option<ActionHash>,
    expiry: Option<Timestamp>,
) -> HiveMembership {
    HiveMembership {
        hive_genesis_hash: action_hash(9),
        for_agent,
        role,
        grantor_membership_hash,
        expiry,
        grantor_owner_accept_hash: None,
    }
}

// -----------------------------------------------------------------
// HiveGenesis immutability — update and delete unconditionally
// reject.
// -----------------------------------------------------------------

#[test]
fn hive_genesis_update_is_invalid() {
    let alice = agent_pubkey(1);
    let result = validate_update_hive_genesis(make_update(alice), sample_genesis())
        .expect("validator should not error in test");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.to_lowercase().contains("immutable"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn hive_genesis_delete_is_invalid() {
    let alice = agent_pubkey(1);
    let original = EntryCreationAction::Create(make_create(alice.clone()));
    let result = validate_delete_hive_genesis(make_delete(alice), original, sample_genesis())
        .expect("validator should not error in test");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.to_lowercase().contains("cannot be deleted"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

// -----------------------------------------------------------------
// HiveMembership immutability — update and delete unconditionally
// reject.
// -----------------------------------------------------------------

#[test]
fn hive_membership_update_is_invalid() {
    let alice = agent_pubkey(1);
    let result = validate_update_hive_membership(
        make_update(alice.clone()),
        sample_membership(alice, HiveRole::Writer, None, None),
    )
    .expect("validator should not error in test");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.to_lowercase().contains("immutable"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn hive_membership_delete_is_invalid() {
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let original = EntryCreationAction::Create(make_create(alice.clone()));
    let result = validate_delete_hive_membership(
        make_delete(alice),
        original,
        sample_membership(bob, HiveRole::Writer, None, None),
    )
    .expect("validator should not error in test");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.to_lowercase().contains("cannot be deleted"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

// -----------------------------------------------------------------
// HiveMembership create — Rule 1 (no self-grant). Reachable
// host-side because the self-grant short-circuit fires BEFORE any
// `must_get_valid_record` chain walk.
// -----------------------------------------------------------------

#[test]
fn hive_membership_self_grant_is_invalid() {
    let alice = agent_pubkey(1);
    let action = EntryCreationAction::Create(make_create(alice.clone()));
    // Alice grants Alice a role — self-grant. No chain walk needed
    // to reject; rule 1 fires immediately.
    let membership = sample_membership(alice, HiveRole::Writer, None, None);
    let result = validate_create_hive_membership(action, membership)
        .expect("validator should not error before chain walk");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.to_lowercase().contains("self-grant"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn hive_membership_self_grant_invalid_regardless_of_role() {
    // Confirm Rule 1 fires across every role variant — guards
    // against a future refactor that conditions the self-grant
    // check on role.
    let alice = agent_pubkey(1);
    for role in [
        HiveRole::Owner,
        HiveRole::Admin,
        HiveRole::Writer,
        HiveRole::Reader,
    ] {
        let action = EntryCreationAction::Create(make_create(alice.clone()));
        let membership = sample_membership(alice.clone(), role, None, None);
        let result = validate_create_hive_membership(action, membership)
            .expect("validator should not error before chain walk");
        assert!(
            matches!(result, ValidateCallbackResult::Invalid(_)),
            "self-grant of {role:?} must be Invalid; got {result:?}",
        );
    }
}

// -----------------------------------------------------------------
// Pass-4 — G-4.4 hive grant-window back-port (pre-fetch branch).
//
// The no-witness fast path (grantor relied on the hive-genesis-
// author Path 1) is host-reachable: enforce_hive_grant_window
// returns Valid without any fetch. The fetch-dependent branches
// (witness-backed Path-2 expiry containment, Path-1 re-verification
// when both witnesses are present) require a live conductor and
// are covered by Sweettest/conductor behavior tests.
// -----------------------------------------------------------------

#[test]
fn hive_grant_window_unconstrained_without_grantor_membership() {
    // No grantor_membership_hash → Path 1 grantor (genesis author);
    // enforce_hive_grant_window short-circuits to Valid without
    // any fetch. Even an expiring grant is permitted because the
    // grantor's permanent Owner role dominates.
    let bob = agent_pubkey(2);
    let membership = HiveMembership {
        hive_genesis_hash: action_hash(9),
        for_agent: bob,
        role: HiveRole::Writer,
        grantor_membership_hash: None,
        expiry: Some(Timestamp(1_000)),
        grantor_owner_accept_hash: None,
    };
    let result = enforce_hive_grant_window(&agent_pubkey(3), &membership)
        .expect("Path 1 short-circuit requires no fetch");
    assert!(
        matches!(result, ValidateCallbackResult::Valid),
        "expected Valid, got {result:?}",
    );
}

#[test]
fn hive_grant_window_unconstrained_for_permanent_new_grant_when_no_witness() {
    // Same fast path with `expiry: None` on the new grant —
    // permanent grant from a Path-1 grantor is the canonical
    // bootstrap pattern (the hive owner mints a permanent member).
    // enforce_hive_grant_window must accept regardless of what
    // `expiry` value the new grant carries when the witness is
    // None.
    let bob = agent_pubkey(2);
    let membership = HiveMembership {
        hive_genesis_hash: action_hash(9),
        for_agent: bob,
        role: HiveRole::Owner,
        grantor_membership_hash: None,
        expiry: None,
        grantor_owner_accept_hash: None,
    };
    let result = enforce_hive_grant_window(&agent_pubkey(3), &membership)
        .expect("Path 1 short-circuit requires no fetch");
    assert!(
        matches!(result, ValidateCallbackResult::Valid),
        "expected Valid, got {result:?}",
    );
}
