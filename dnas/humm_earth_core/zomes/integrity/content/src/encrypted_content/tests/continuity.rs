use super::super::*;
use super::support::*;
use hdi::prelude::*;

fn base_header() -> EncryptedContentHeader {
    sample_header_pubkey(&agent_pubkey(1).to_string())
}

fn header_with_spec(acl_spec: AclSpec) -> EncryptedContentHeader {
    let mut header = base_header();
    header.acl_spec = acl_spec;
    header
}

fn header_with_content_type(content_type: &str) -> EncryptedContentHeader {
    let mut header = base_header();
    header.content_type = content_type.into();
    header
}

fn open_write_to(target: Option<ActionHash>) -> AclSpec {
    AclSpec::OpenWrite {
        target_hive_genesis_hash: target,
    }
}

fn assert_continuity_reject(
    old: &EncryptedContentHeader,
    new: &EncryptedContentHeader,
    expected_fragment: &str,
) {
    match validate_update_continuity(old, new) {
        ValidateCallbackResult::Invalid(msg) => assert!(
            msg.contains(expected_fragment),
            "expected {expected_fragment:?} in {msg:?}",
        ),
        other => panic!("expected Invalid containing {expected_fragment:?}, got {other:?}"),
    }
}

#[test]
fn identical_headers_are_continuous() {
    let header = base_header();
    assert!(matches!(
        validate_update_continuity(&header, &header),
        ValidateCallbackResult::Valid,
    ));
}

#[test]
fn id_is_byte_exact_immutable() {
    let old = base_header();
    let mut new = base_header();
    new.id = "Id".into();
    assert_continuity_reject(
        &old,
        &new,
        "EncryptedContent updates must not change the id",
    );
}

#[test]
fn same_variant_hive_retarget_rejects() {
    let old = header_with_spec(open_write_to(Some(action_hash(1))));
    let new = header_with_spec(open_write_to(Some(action_hash(2))));
    assert_continuity_reject(
        &old,
        &new,
        "EncryptedContent updates must not change the hive context",
    );
}

#[test]
fn variant_swap_with_equal_hive_context_rejects() {
    let old = base_header();
    let new = header_with_spec(AclSpec::Public {
        hive_genesis_hash: action_hash(9),
        author_membership_hash: None,
    });
    assert_eq!(old.hive_context(), new.hive_context());
    assert_continuity_reject(
        &old,
        &new,
        "EncryptedContent updates must not change the acl_spec variant",
    );
}

#[test]
fn variant_swap_with_equal_none_context_rejects() {
    let old = header_with_spec(AclSpec::DirectMessage {
        recipients: vec![agent_pubkey(1), agent_pubkey(2)],
    });
    let new = header_with_spec(open_write_to(None));
    assert_eq!(old.hive_context(), new.hive_context());
    assert_continuity_reject(
        &old,
        &new,
        "EncryptedContent updates must not change the acl_spec variant",
    );
}

#[test]
fn content_type_permits_exactly_one_migration_stamp() {
    let cases = [
        ("post", "post", true),
        ("post", "_migrated/post", true),
        ("_migrated/post", "_migrated/post", true),
        ("post", "_migrated/blog", false),
        ("_migrated/post", "_migrated/_migrated/post", false),
        ("post", "blog", false),
    ];
    for (old_type, new_type, expected_valid) in cases {
        let old = header_with_content_type(old_type);
        let new = header_with_content_type(new_type);
        let result = validate_update_continuity(&old, &new);
        if expected_valid {
            assert!(
                matches!(result, ValidateCallbackResult::Valid),
                "{old_type} -> {new_type} should be continuous, got {result:?}",
            );
        } else {
            assert_continuity_reject(
                &old,
                &new,
                "EncryptedContent updates may only stamp content_type with the _migrated/ prefix",
            );
        }
    }
}

#[test]
fn acl_display_and_revision_key_stay_mutable() {
    let old = base_header();
    let mut new = base_header();
    new.display_hive_id = "renamed-hive".into();
    new.revision_author_signing_public_key = agent_pubkey(3).to_string();
    new.public_key_acl.reader = vec!["*".into()];
    new.public_key_acl.admin = vec!["added-admin".into()];
    assert!(matches!(
        validate_update_continuity(&old, &new),
        ValidateCallbackResult::Valid,
    ));
}
