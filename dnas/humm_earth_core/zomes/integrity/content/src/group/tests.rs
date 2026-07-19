use super::*;
use crate::hive::Role;
use hdi::prelude::{
    ActionHash, AgentPubKey, AnyLinkableHash, AppEntryDef, Create, CreateLink, Delete, DeleteLink,
    EntryCreationAction, EntryHash, EntryType, EntryVisibility, LinkTag, Timestamp, Update,
    ValidateCallbackResult,
};

fn agent_pubkey(byte: u8) -> AgentPubKey {
    AgentPubKey::from_raw_36(vec![byte; 36])
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
            entry_index: 4.into(),
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
            entry_index: 4.into(),
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

fn sample_group_genesis() -> GroupGenesis {
    GroupGenesis {
        hive_genesis_hash: action_hash(9),
        display_id: "group-test".into(),
        hive_wide_role: None,
        creator_hive_membership_hash: None,
        created_at_microseconds: 0,
    }
}

fn sample_group_membership(
    for_agent: AgentPubKey,
    role: Role,
    grantor_membership_hash: Option<ActionHash>,
) -> GroupMembership {
    GroupMembership {
        group_genesis_hash: action_hash(9),
        for_agent,
        role,
        grantor_membership_hash,
        grantor_hive_membership_hash: None,
        expiry: None,
    }
}

// -----------------------------------------------------------------
// GroupGenesis immutability.
// -----------------------------------------------------------------

#[test]
fn group_genesis_update_is_invalid() {
    let alice = agent_pubkey(1);
    let result = validate_update_group_genesis(make_update(alice), sample_group_genesis())
        .expect("validator should not error in test");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.to_lowercase().contains("immutable"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn group_genesis_delete_author_gated() {
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let by_author = validate_delete_group_genesis(
        make_delete(alice.clone()),
        EntryCreationAction::Create(make_create(alice.clone())),
        sample_group_genesis(),
    )
    .expect("validator should not error in test");
    assert!(
        matches!(by_author, ValidateCallbackResult::Valid),
        "group creator may delete; got {by_author:?}",
    );

    let by_other = validate_delete_group_genesis(
        make_delete(bob),
        EntryCreationAction::Create(make_create(alice)),
        sample_group_genesis(),
    )
    .expect("validator should not error in test");
    assert!(
        matches!(by_other, ValidateCallbackResult::Invalid(_)),
        "non-creator rejected; got {by_other:?}",
    );
}

// -----------------------------------------------------------------
// GroupMembership immutability.
// -----------------------------------------------------------------

#[test]
fn group_membership_update_is_invalid() {
    let alice = agent_pubkey(1);
    let result = validate_update_group_membership(
        make_update(alice.clone()),
        sample_group_membership(alice, Role::Writer, None),
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
fn group_membership_delete_is_invalid() {
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let original = EntryCreationAction::Create(make_create(alice.clone()));
    let result = validate_delete_group_membership(
        make_delete(alice),
        original,
        sample_group_membership(bob, Role::Writer, None),
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
// GroupMembership create — Rule 1 (no self-grant). Reachable
// host-side because the self-grant short-circuit fires BEFORE any
// must_get_valid_record chain walk.
// -----------------------------------------------------------------

#[test]
fn group_membership_self_grant_is_invalid() {
    let alice = agent_pubkey(1);
    let action = EntryCreationAction::Create(make_create(alice.clone()));
    let membership = sample_group_membership(alice, Role::Writer, None);
    let result = validate_create_group_membership(action, membership)
        .expect("validator should not error before chain walk");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.to_lowercase().contains("self-grant"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn group_membership_self_grant_invalid_regardless_of_role() {
    let alice = agent_pubkey(1);
    for role in [Role::Owner, Role::Admin, Role::Writer, Role::Reader] {
        let action = EntryCreationAction::Create(make_create(alice.clone()));
        let membership = sample_group_membership(alice.clone(), role, None);
        let result = validate_create_group_membership(action, membership)
            .expect("validator should not error before chain walk");
        assert!(
            matches!(result, ValidateCallbackResult::Invalid(_)),
            "self-grant of {role:?} must be Invalid; got {result:?}",
        );
    }
}

// -----------------------------------------------------------------
// enforce_grant_window — the no-witness fast path (grantor relied on
// group-author / hive-sovereign route) is host-reachable: it returns
// Valid without any fetch.
// -----------------------------------------------------------------

#[test]
fn grant_window_unconstrained_without_grantor_membership() {
    // No grantor_membership_hash => Path A/B grantor => unconstrained.
    let bob = agent_pubkey(2);
    let membership = GroupMembership {
        group_genesis_hash: action_hash(9),
        for_agent: bob,
        role: Role::Writer,
        grantor_membership_hash: None,
        grantor_hive_membership_hash: None,
        expiry: Some(Timestamp(1_000)),
    };
    let result = enforce_grant_window(
        &agent_pubkey(3),
        &action_hash(9),
        &Timestamp(0),
        &membership,
    )
    .expect("no fetch on the None-witness path");
    assert!(matches!(result, ValidateCallbackResult::Valid));
}

// -----------------------------------------------------------------
// Link author guard — pure comparison, host-reachable.
// -----------------------------------------------------------------

#[test]
fn link_author_mismatch_is_invalid() {
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    assert!(matches!(
        require_link_author_is(&alice, &alice),
        ValidateCallbackResult::Valid
    ));
    assert!(matches!(
        require_link_author_is(&alice, &bob),
        ValidateCallbackResult::Invalid(_)
    ));
}

#[test]
fn group_link_delete_requires_link_author() {
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let create = CreateLink {
        author: alice.clone(),
        timestamp: Timestamp(0),
        action_seq: 1,
        prev_action: action_hash(0),
        base_address: AnyLinkableHash::from(alice.clone()),
        target_address: AnyLinkableHash::from(action_hash(5)),
        zome_index: 0.into(),
        link_type: 12.into(),
        tag: LinkTag::new(vec![]),
        weight: Default::default(),
    };
    let same_author_delete = DeleteLink {
        author: alice.clone(),
        timestamp: Timestamp(0),
        action_seq: 2,
        prev_action: action_hash(0),
        base_address: AnyLinkableHash::from(alice.clone()),
        link_add_address: action_hash(1),
    };
    assert!(matches!(
        validate_delete_group_link(same_author_delete, create.clone(), "HiveToGroups")
            .expect("pure path"),
        ValidateCallbackResult::Valid
    ));
    let other_author_delete = DeleteLink {
        author: bob,
        timestamp: Timestamp(0),
        action_seq: 2,
        prev_action: action_hash(0),
        base_address: AnyLinkableHash::from(alice),
        link_add_address: action_hash(1),
    };
    assert!(matches!(
        validate_delete_group_link(other_author_delete, create, "HiveToGroups").expect("pure path"),
        ValidateCallbackResult::Invalid(_)
    ));
}

// ---------------------------------------------------------------------
// Pass-7 M3 — system-role GroupGenesis uniqueness tuple
// ---------------------------------------------------------------------

#[test]
fn tuple_conflicts_on_same_hive_and_role() {
    let mut existing = sample_group_genesis();
    existing.hive_wide_role = Some(Role::Owner);
    let mut new = sample_group_genesis();
    new.hive_wide_role = Some(Role::Owner);
    assert!(genesis_tuple_conflicts(&existing, &new));
}

#[test]
fn tuple_no_conflict_on_different_role() {
    let mut existing = sample_group_genesis();
    existing.hive_wide_role = Some(Role::Owner);
    let mut new = sample_group_genesis();
    new.hive_wide_role = Some(Role::Writer);
    assert!(!genesis_tuple_conflicts(&existing, &new));
}

#[test]
fn tuple_no_conflict_on_different_hive() {
    let mut existing = sample_group_genesis();
    existing.hive_genesis_hash = action_hash(9);
    existing.hive_wide_role = Some(Role::Owner);
    let mut new = sample_group_genesis();
    new.hive_genesis_hash = action_hash(10);
    new.hive_wide_role = Some(Role::Owner);
    assert!(!genesis_tuple_conflicts(&existing, &new));
}

#[test]
fn tuple_custom_candidate_never_conflicts_with_system_role() {
    let existing = sample_group_genesis();
    let mut new = sample_group_genesis();
    new.hive_wide_role = Some(Role::Owner);
    assert!(!genesis_tuple_conflicts(&existing, &new));
}
