use super::super::*;
use super::support::*;
use hdi::hash_path::path::Component;
use hdi::prelude::*;

// ---------------------------------------------------------------------
// Path-recompute sanity
// ---------------------------------------------------------------------

#[test]
fn recompute_base_matches_path_constructed_manually() {
    let manual = Path::from(vec![
        Component::from("a"),
        Component::from("b"),
        Component::from("c"),
    ]);
    let manual_hash: AnyLinkableHash = manual
        .path_entry_hash()
        .expect("path hash should compute in test")
        .into();
    let recomputed =
        recompute_base(&["a", "b", "c"]).expect("recompute_base should compute in test");
    assert_eq!(manual_hash, recomputed);
}

// ---------------------------------------------------------------------
// Delete-link author-equality — Hive / Dynamic / HummContentId /
// HummContentAcl all share the contract "only the link's author may
// delete it". Pure logic, no host calls.
// ---------------------------------------------------------------------

#[test]
fn delete_link_hive_accepts_original_author() {
    let alice = agent_pubkey(1);
    let (base, target, tag) = link_args();
    let result = validate_delete_link_hive(
        make_delete_link(alice.clone()),
        make_create_link(alice),
        base,
        target,
        tag,
    )
    .expect("validator should not error in test");
    assert!(matches!(result, ValidateCallbackResult::Valid));
}

#[test]
fn delete_link_hive_rejects_third_party() {
    let alice = agent_pubkey(1);
    let mallory = agent_pubkey(99);
    let (base, target, tag) = link_args();
    let result = validate_delete_link_hive(
        make_delete_link(mallory),
        make_create_link(alice),
        base,
        target,
        tag,
    )
    .expect("validator should not error in test");
    assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
}

#[test]
fn delete_link_dynamic_enforces_author_equality() {
    let alice = agent_pubkey(1);
    let mallory = agent_pubkey(99);
    let (base, target, tag) = link_args();
    let accept = validate_delete_link_dynamic(
        make_delete_link(alice.clone()),
        make_create_link(alice.clone()),
        base.clone(),
        target.clone(),
        tag.clone(),
    )
    .expect("validator should not error in test");
    assert!(matches!(accept, ValidateCallbackResult::Valid));
    let reject = validate_delete_link_dynamic(
        make_delete_link(mallory),
        make_create_link(alice),
        base,
        target,
        tag,
    )
    .expect("validator should not error in test");
    assert!(matches!(reject, ValidateCallbackResult::Invalid(_)));
}

#[test]
fn delete_link_humm_content_id_enforces_author_equality() {
    let alice = agent_pubkey(1);
    let mallory = agent_pubkey(99);
    let (base, target, tag) = link_args();
    let accept = validate_delete_link_humm_content_id(
        make_delete_link(alice.clone()),
        make_create_link(alice.clone()),
        base.clone(),
        target.clone(),
        tag.clone(),
    )
    .expect("validator should not error in test");
    assert!(matches!(accept, ValidateCallbackResult::Valid));
    let reject = validate_delete_link_humm_content_id(
        make_delete_link(mallory),
        make_create_link(alice),
        base,
        target,
        tag,
    )
    .expect("validator should not error in test");
    assert!(matches!(reject, ValidateCallbackResult::Invalid(_)));
}

#[test]
fn delete_link_humm_content_acl_enforces_author_equality_per_class() {
    // ACL delete-link validator takes a `class_label: &str` for
    // error messaging; iterate every ACL class to confirm uniform
    // author-equality across the four variants.
    let alice = agent_pubkey(1);
    let mallory = agent_pubkey(99);
    for class_label in [
        "HummContentOwner",
        "HummContentAdmin",
        "HummContentWriter",
        "HummContentReader",
    ] {
        let (base, target, tag) = link_args();
        let accept = validate_delete_link_humm_content_acl(
            make_delete_link(alice.clone()),
            make_create_link(alice.clone()),
            base.clone(),
            target.clone(),
            tag.clone(),
            class_label,
        )
        .expect("validator should not error in test");
        assert!(
            matches!(accept, ValidateCallbackResult::Valid),
            "{class_label} delete by original author must be Valid; got {accept:?}",
        );
        let reject = validate_delete_link_humm_content_acl(
            make_delete_link(mallory.clone()),
            make_create_link(alice.clone()),
            base,
            target,
            tag,
            class_label,
        )
        .expect("validator should not error in test");
        match reject {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(
                    msg.contains(class_label),
                    "{class_label}: error message must identify the link class; got {msg:?}",
                );
            }
            other => panic!("{class_label}: expected Invalid, got {other:?}"),
        }
    }
}

#[test]
fn delete_link_encrypted_content_updates_is_invalid() {
    // EncryptedContentUpdates is the only link type that
    // unconditionally rejects deletes (preserves the update chain
    // integrity).
    let alice = agent_pubkey(1);
    let (base, target, tag) = link_args();
    let result = validate_delete_link_encrypted_content_updates(
        make_delete_link(alice.clone()),
        make_create_link(alice),
        base,
        target,
        tag,
    )
    .expect("validator should not error in test");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("cannot be deleted"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn create_link_original_hash_pointer_rejects_non_action_target() {
    let alice = agent_pubkey(1);
    let base = AnyLinkableHash::from(action_hash(1));
    let target = AnyLinkableHash::from(EntryHash::from_raw_36(vec![4u8; 36]));
    let result = validate_create_link_original_hash_pointer(
        make_create_link(alice),
        base,
        target,
        LinkTag::new(vec![]),
    )
    .expect("malformed target should be classified, not error");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("OriginalHashPointer target must be an ActionHash"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn delete_link_original_hash_pointer_enforces_author_equality() {
    let alice = agent_pubkey(1);
    let mallory = agent_pubkey(99);
    let (base, target, tag) = link_args();
    let accept = validate_delete_link_original_hash_pointer(
        make_delete_link(alice.clone()),
        make_create_link(alice.clone()),
        base.clone(),
        target.clone(),
        tag.clone(),
    )
    .expect("validator should not error in test");
    assert!(matches!(accept, ValidateCallbackResult::Valid));
    let reject = validate_delete_link_original_hash_pointer(
        make_delete_link(mallory),
        make_create_link(alice),
        base,
        target,
        tag,
    )
    .expect("validator should not error in test");
    match reject {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("OriginalHashPointer"));
            assert!(msg.contains("attempted by"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}
