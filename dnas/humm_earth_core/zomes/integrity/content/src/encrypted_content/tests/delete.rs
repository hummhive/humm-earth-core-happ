use super::super::*;
use super::support::*;
use hdi::prelude::*;

// ---------------------------------------------------------------------
// I-A — validate_delete_encrypted_content
// ---------------------------------------------------------------------
//
// These tests construct `Delete` and `EntryCreationAction` shapes
// manually. We avoid faking entire actions; instead we test the
// function's *decision* logic by setting up the relevant fields the
// function reads.

#[test]
fn delete_accepts_original_author() {
    let alice = agent_pubkey(1);
    let action = make_delete(alice.clone());
    let original = EntryCreationAction::Create(make_create(alice));
    let content = sample_content_with_acl(Acl {
        owner: "x".into(),
        admin: vec![],
        writer: vec![],
        reader: vec![],
    });
    let result = validate_delete_encrypted_content(action, original, content)
        .expect("validator should not error in test");
    assert!(matches!(result, ValidateCallbackResult::Valid));
}

#[test]
fn delete_rejects_stranger_with_empty_public_key_acl() {
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let action = make_delete(bob);
    let original = EntryCreationAction::Create(make_create(alice));
    let content = sample_content_with_acl(Acl {
        owner: "not-bob".into(),
        admin: vec![],
        writer: vec![],
        reader: vec![],
    });
    let result = validate_delete_encrypted_content(action, original, content)
        .expect("validator should not error in test");
    assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
}

#[test]
fn delete_dm_recipient_in_reader_bucket_is_valid() {
    let bob = agent_pubkey(2);
    let content = content_with_spec(
        AclSpec::DirectMessage {
            recipients: vec![bob.clone()],
        },
        Acl {
            owner: "x".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![bob.to_string()],
        },
    );
    let result = validate_delete_encrypted_content(
        make_delete(bob),
        EntryCreationAction::Create(make_create(agent_pubkey(1))),
        content,
    )
    .expect("validator should not error in test");
    assert!(
        matches!(result, ValidateCallbackResult::Valid),
        "got {result:?}"
    );
}

#[test]
fn delete_hive_group_reader_is_rejected() {
    let bob = agent_pubkey(2);
    let content = sample_content_with_acl(Acl {
        owner: "x".into(),
        admin: vec![],
        writer: vec![],
        reader: vec![bob.to_string()],
    });
    let result = validate_delete_encrypted_content(
        make_delete(bob),
        EntryCreationAction::Create(make_create(agent_pubkey(1))),
        content,
    )
    .expect("validator should not error in test");
    assert!(
        matches!(result, ValidateCallbackResult::Invalid(_)),
        "got {result:?}"
    );
}

#[test]
fn delete_accepts_listed_in_public_key_acl_admin() {
    let alice = agent_pubkey(1);
    let admin = agent_pubkey(3);
    let action = make_delete(admin.clone());
    let original = EntryCreationAction::Create(make_create(alice));
    let content = sample_content_with_acl(Acl {
        owner: "x".into(),
        admin: vec![admin.to_string()],
        writer: vec![],
        reader: vec![],
    });
    let result = validate_delete_encrypted_content(action, original, content)
        .expect("validator should not error in test");
    assert!(matches!(result, ValidateCallbackResult::Valid));
}

#[test]
fn delete_accepts_listed_in_public_key_acl_writer() {
    let alice = agent_pubkey(1);
    let writer = agent_pubkey(4);
    let action = make_delete(writer.clone());
    let original = EntryCreationAction::Create(make_create(alice));
    let content = sample_content_with_acl(Acl {
        owner: "x".into(),
        admin: vec![],
        writer: vec![writer.to_string()],
        reader: vec![],
    });
    let result = validate_delete_encrypted_content(action, original, content)
        .expect("validator should not error in test");
    assert!(matches!(result, ValidateCallbackResult::Valid));
}

#[test]
fn delete_accepts_listed_in_public_key_acl_owner() {
    let alice = agent_pubkey(1);
    let owner = agent_pubkey(5);
    let action = make_delete(owner.clone());
    let original = EntryCreationAction::Create(make_create(alice));
    let content = sample_content_with_acl(Acl {
        owner: owner.to_string(),
        admin: vec![],
        writer: vec![],
        reader: vec![],
    });
    let result = validate_delete_encrypted_content(action, original, content)
        .expect("validator should not error in test");
    assert!(matches!(result, ValidateCallbackResult::Valid));
}

#[test]
fn delete_rejects_when_author_string_is_substring_of_acl_value() {
    // The `public_key_acl.owner` field is a `String` and the
    // admin/writer/reader fields are `Vec<String>`. The validator
    // uses `==` (exact-string) comparison everywhere, so a stranger
    // whose pubkey string happens to be a strict substring of an
    // ACL value MUST NOT false-match. This test pins the exact-
    // match guarantee in every bucket.
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let bob_str = bob.to_string();
    // Bob's pubkey string with an appended suffix — bob is NOT
    // exactly listed; the suffix-bearing string is the ACL value.
    let bob_with_suffix = format!("{bob_str}EXTRA");
    // Note: the inverse direction (ACL value is a strict prefix of
    // bob's pubkey) is covered by
    // `delete_rejects_when_acl_value_is_substring_of_author_string`.
    for acl in [
        // owner field carries a string that CONTAINS bob's pubkey.
        Acl {
            owner: bob_with_suffix.clone(),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        },
        // admin vec carries a containing-string (admin entry holds bob+suffix).
        Acl {
            owner: "z".into(),
            admin: vec![bob_with_suffix.clone()],
            writer: vec![],
            reader: vec![],
        },
        // writer vec carries a containing-string.
        Acl {
            owner: "z".into(),
            admin: vec![],
            writer: vec![bob_with_suffix.clone()],
            reader: vec![],
        },
        // reader vec carries a containing-string (reader entry holds bob+suffix).
        Acl {
            owner: "z".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![bob_with_suffix.clone()],
        },
    ] {
        let action = make_delete(bob.clone());
        let original = EntryCreationAction::Create(make_create(alice.clone()));
        let content = sample_content_with_acl(acl);
        let result = validate_delete_encrypted_content(action, original, content)
            .expect("validator should not error in test");
        assert!(
            matches!(result, ValidateCallbackResult::Invalid(_)),
            "substring-but-not-exact ACL match must NOT permit delete; got {result:?}",
        );
    }
}

#[test]
fn delete_rejects_when_acl_value_is_substring_of_author_string() {
    // Inverse of the previous test: an ACL value that is a strict
    // prefix of the deleter's pubkey string must NOT false-match.
    // Exact-string semantics in both directions.
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let bob_str = bob.to_string();
    let bob_prefix_only = bob_str[..bob_str.len() - 1].to_string();
    let action = make_delete(bob.clone());
    let original = EntryCreationAction::Create(make_create(alice));
    let content = sample_content_with_acl(Acl {
        owner: bob_prefix_only.clone(),
        admin: vec![bob_prefix_only.clone()],
        writer: vec![bob_prefix_only.clone()],
        reader: vec![bob_prefix_only],
    });
    let result = validate_delete_encrypted_content(action, original, content)
        .expect("validator should not error in test");
    assert!(
        matches!(result, ValidateCallbackResult::Invalid(_)),
        "ACL value that is a strict prefix of author pubkey must NOT permit delete; got {result:?}",
    );
}

#[test]
fn delete_hive_group_writer_admin_owner_are_valid() {
    let bob = agent_pubkey(2);
    let bob_b64 = bob.to_string();
    let buckets = [
        Acl {
            owner: bob_b64.clone(),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        },
        Acl {
            owner: "x".into(),
            admin: vec![bob_b64.clone()],
            writer: vec![],
            reader: vec![],
        },
        Acl {
            owner: "x".into(),
            admin: vec![],
            writer: vec![bob_b64.clone()],
            reader: vec![],
        },
    ];
    for acl in buckets {
        let content = sample_content_with_acl(acl);
        let result = validate_delete_encrypted_content(
            make_delete(bob.clone()),
            EntryCreationAction::Create(make_create(agent_pubkey(1))),
            content,
        )
        .expect("validator should not error in test");
        assert!(
            matches!(result, ValidateCallbackResult::Valid),
            "got {result:?}"
        );
    }
}

#[test]
fn delete_dm_non_recipient_is_rejected() {
    let bob = agent_pubkey(2);
    let stranger = agent_pubkey(5);
    let content = content_with_spec(
        AclSpec::DirectMessage {
            recipients: vec![stranger.clone()],
        },
        Acl {
            owner: "x".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![stranger.to_string()],
        },
    );
    let result = validate_delete_encrypted_content(
        make_delete(bob),
        EntryCreationAction::Create(make_create(agent_pubkey(1))),
        content,
    )
    .expect("validator should not error in test");
    assert!(
        matches!(result, ValidateCallbackResult::Invalid(_)),
        "got {result:?}"
    );
}
