use super::super::*;
use super::support::*;
use hdi::prelude::*;

// ---------------------------------------------------------------------
// Pass-3 — variant-dispatch validators (pre-fetch branches only).
//
// Fetch-dependent branches (HiveGroup hive/group authority and
// OpenWrite target existence) require must_get_valid_record and are
// covered by Sweettest/conductor paths. Same host-side limitation as
// the pass-2 hive validator suite.
// ---------------------------------------------------------------------

fn dm_content(
    author: AgentPubKey,
    recipients: Vec<AgentPubKey>,
    public_key_acl: Acl,
) -> EncryptedContent {
    EncryptedContent {
        header: EncryptedContentHeader {
            id: "dm-id".into(),
            display_hive_id: "".into(),
            content_type: "dm".into(),
            acl_spec: AclSpec::DirectMessage { recipients },
            public_key_acl,
            revision_author_signing_public_key: author.to_string(),
            lineage: None,
        },
        bytes: SerializedBytes::from(UnsafeBytes::from(vec![])),
    }
}

fn empty_acl() -> Acl {
    Acl {
        owner: "".into(),
        admin: vec![],
        writer: vec![],
        reader: vec![],
    }
}

fn reader_acl(readers: &[AgentPubKey]) -> Acl {
    Acl {
        owner: "".into(),
        admin: vec![],
        writer: vec![],
        reader: readers.iter().map(|r| r.to_string()).collect(),
    }
}

#[test]
fn directmessage_rejects_zero_recipients() {
    let alice = agent_pubkey(1);
    let content = dm_content(alice.clone(), vec![], empty_acl());
    let result = run_content_validators(&alice, &Timestamp(0), &content)
        .expect("DM validator runs pre-fetch on cardinality check");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("recipients.len() = 0"), "got: {msg}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn directmessage_rejects_one_recipient() {
    let alice = agent_pubkey(1);
    let content = dm_content(
        alice.clone(),
        vec![alice.clone()],
        reader_acl(&[alice.clone()]),
    );
    let result = run_content_validators(&alice, &Timestamp(0), &content)
        .expect("DM validator runs pre-fetch on cardinality check");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("recipients.len() = 1"), "got: {msg}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn directmessage_rejects_over_max_recipients() {
    let alice = agent_pubkey(1);
    // DM_MAX_RECIPIENTS = 32; build 33.
    let recipients: Vec<AgentPubKey> = (0u8..33).map(agent_pubkey).collect();
    let pka = reader_acl(&recipients);
    let content = dm_content(alice.clone(), recipients, pka);
    let result = run_content_validators(&alice, &Timestamp(0), &content)
        .expect("DM validator runs pre-fetch on cardinality check");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("exceeds DM_MAX_RECIPIENTS"), "got: {msg}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn directmessage_rejects_author_not_in_recipients() {
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let carol = agent_pubkey(3);
    // Mallory (alice) tries to spoof a DM between bob and carol.
    let content = dm_content(
        alice.clone(),
        vec![bob.clone(), carol.clone()],
        reader_acl(&[bob, carol]),
    );
    let result = run_content_validators(&alice, &Timestamp(0), &content)
        .expect("DM validator runs pre-fetch on author-in-recipients");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("not in recipients"), "got: {msg}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn directmessage_rejects_reader_bucket_mismatch() {
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let mallory = agent_pubkey(99);
    // Recipient list is [alice, bob], but reader bucket has
    // [alice, mallory] — modified-coordinator forgery (Mallory
    // injects herself into the routing fan-out).
    let content = dm_content(
        alice.clone(),
        vec![alice.clone(), bob],
        reader_acl(&[alice.clone(), mallory]),
    );
    let result = run_content_validators(&alice, &Timestamp(0), &content)
        .expect("DM validator runs pre-fetch on reader-bucket equality");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("public_key_acl.reader"), "got: {msg}");
            assert!(msg.contains("does not match recipients"), "got: {msg}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn directmessage_rejects_nonempty_non_reader_buckets() {
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let mut pka = reader_acl(&[alice.clone(), bob.clone()]);
    pka.admin.push("trojan".into());
    let content = dm_content(alice.clone(), vec![alice.clone(), bob], pka);
    let result = run_content_validators(&alice, &Timestamp(0), &content)
        .expect("DM validator runs pre-fetch on owner/admin/writer empty");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(
                msg.contains("owner/admin/writer must be empty"),
                "got: {msg}"
            );
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn directmessage_accepts_2_recipients_in_any_order() {
    // Reader bucket sorting normalises order: validator should
    // accept regardless of insertion order. Sorted-equality is the
    // pre-fetch path; no must_get_valid_record fired.
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let pka = reader_acl(&[bob.clone(), alice.clone()]); // reversed
    let content = dm_content(alice.clone(), vec![alice, bob], pka);
    let result = run_content_validators(&agent_pubkey(1), &Timestamp(0), &content)
        .expect("DM validator runs pre-fetch on order-independent equality");
    assert!(matches!(result, ValidateCallbackResult::Valid));
}

fn openwrite_content(author: AgentPubKey, target: Option<ActionHash>) -> EncryptedContent {
    EncryptedContent {
        header: EncryptedContentHeader {
            id: "open-id".into(),
            display_hive_id: "".into(),
            content_type: "member-request".into(),
            acl_spec: AclSpec::OpenWrite {
                target_hive_genesis_hash: target,
            },
            public_key_acl: empty_acl(),
            revision_author_signing_public_key: author.to_string(),
            lineage: None,
        },
        bytes: SerializedBytes::from(UnsafeBytes::from(vec![])),
    }
}

#[test]
fn openwrite_with_no_target_accepts_without_fetch() {
    // OpenWrite { target: None } is the cross-network-discovery
    // case. Validator returns Valid pre-fetch (only the author-
    // identity check runs, and the header pubkey is built from
    // the author).
    let alice = agent_pubkey(1);
    let content = openwrite_content(alice.clone(), None);
    let result = run_content_validators(&alice, &Timestamp(0), &content)
        .expect("OpenWrite with no target runs pre-fetch");
    assert!(matches!(result, ValidateCallbackResult::Valid));
}

#[test]
fn openwrite_header_pubkey_mismatch_rejects_pre_fetch() {
    // The pass-1 check_author_matches_header guard fires before any
    // OpenWrite-specific logic.
    let alice = agent_pubkey(1);
    let bob = agent_pubkey(2);
    let mut content = openwrite_content(bob, None);
    // Override the header pubkey to alice's string; action.author
    // (agent_pubkey(99)) will not match.
    content.header.revision_author_signing_public_key = alice.to_string();
    let result = run_content_validators(&agent_pubkey(99), &Timestamp(0), &content)
        .expect("OpenWrite validator runs pre-fetch on author-vs-header");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(
                msg.contains("revision_author_signing_public_key"),
                "got: {msg}"
            );
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}
