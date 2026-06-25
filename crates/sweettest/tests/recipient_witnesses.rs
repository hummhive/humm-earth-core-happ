mod support;

use holo_hash::{ActionHash, AgentPubKey};
use holochain::sweettest::await_consistency_s;
use holochain_types::prelude::{SerializedBytes, UnsafeBytes};
use serde::{Deserialize, Serialize};
use support::{create_hive, setup_cells, GenesisResponse, MembershipResponse};

#[derive(Debug, Serialize)]
struct CreateGroupGenesisInput {
    hive_genesis_hash: ActionHash,
    display_id: String,
    hive_wide_role: Option<String>,
    creator_hive_membership_hash: Option<ActionHash>,
}

#[derive(Debug, Serialize)]
struct CreateGroupMembershipInput {
    group_genesis_hash: ActionHash,
    for_agent: AgentPubKey,
    role: String,
    grantor_membership_hash: Option<ActionHash>,
    grantor_hive_membership_hash: Option<ActionHash>,
    expiry: Option<i64>,
}

#[derive(Debug, Serialize)]
struct Acl {
    owner: String,
    admin: Vec<String>,
    writer: Vec<String>,
    reader: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AclByGroupGenesis {
    owner: ActionHash,
    admin: Vec<ActionHash>,
    writer: Vec<ActionHash>,
    reader: Vec<ActionHash>,
}

#[derive(Debug, Serialize)]
enum AclBucket {
    Reader,
}

#[derive(Debug, Serialize)]
struct RecipientWitness {
    pubkey: AgentPubKey,
    bucket: AclBucket,
    membership_hash: ActionHash,
}

#[derive(Debug, Serialize)]
enum AclSpec {
    HiveGroup {
        hive_genesis_hash: ActionHash,
        author_membership_hash: Option<ActionHash>,
        group_acl: AclByGroupGenesis,
        author_group_membership_hash: Option<ActionHash>,
        recipient_witnesses: Vec<RecipientWitness>,
    },
}

#[derive(Debug, Serialize)]
struct CreateEncryptedContentInput {
    id: String,
    display_hive_id: String,
    content_type: String,
    revision_author_signing_public_key: String,
    bytes: SerializedBytes,
    acl_spec: AclSpec,
    public_key_acl: Acl,
    dynamic_links: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct CreateResponse {
    hash: String,
}

#[tokio::test(flavor = "multi_thread")]
async fn hivegroup_recipient_witness_accepts_real_group_membership() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_pubkey = alice.agent_pubkey().clone();
    let bob_pubkey = bob.agent_pubkey().clone();

    await_consistency_s(30, [alice, bob]).await.unwrap();

    let hive_hash = create_hive(&conductors[0], &alice.zome("content"), "witness-hive").await;

    let group: GenesisResponse = conductors[0]
        .call(
            &alice.zome("content"),
            "create_group_genesis",
            CreateGroupGenesisInput {
                hive_genesis_hash: hive_hash.clone(),
                display_id: "private-group".to_string(),
                hive_wide_role: None,
                creator_hive_membership_hash: None,
            },
        )
        .await;

    let bob_membership: MembershipResponse = conductors[0]
        .call(
            &alice.zome("content"),
            "create_group_membership",
            CreateGroupMembershipInput {
                group_genesis_hash: group.hash.clone(),
                for_agent: bob_pubkey.clone(),
                role: "Reader".to_string(),
                grantor_membership_hash: None,
                grantor_hive_membership_hash: None,
                expiry: None,
            },
        )
        .await;

    await_consistency_s(60, [alice, bob]).await.unwrap();

    let created: CreateResponse = conductors[0]
        .call(
            &alice.zome("content"),
            "create_encrypted_content",
            CreateEncryptedContentInput {
                id: "witness-content".to_string(),
                display_hive_id: "witness-hive".to_string(),
                content_type: "sweettest-witness".to_string(),
                revision_author_signing_public_key: alice_pubkey.to_string(),
                bytes: UnsafeBytes::from(vec![1u8, 2, 3]).into(),
                acl_spec: AclSpec::HiveGroup {
                    hive_genesis_hash: hive_hash,
                    author_membership_hash: None,
                    group_acl: AclByGroupGenesis {
                        owner: group.hash.clone(),
                        admin: vec![],
                        writer: vec![],
                        reader: vec![group.hash],
                    },
                    author_group_membership_hash: None,
                    recipient_witnesses: vec![RecipientWitness {
                        pubkey: bob_pubkey.clone(),
                        bucket: AclBucket::Reader,
                        membership_hash: bob_membership.hash,
                    }],
                },
                public_key_acl: Acl {
                    owner: String::new(),
                    admin: vec![],
                    writer: vec![],
                    reader: vec![bob_pubkey.to_string()],
                },
                dynamic_links: None,
            },
        )
        .await;

    assert!(!created.hash.is_empty(), "content hash must be returned");
}
