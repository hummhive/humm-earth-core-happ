mod support;

use holochain::sweettest::await_consistency_s;
use holochain_types::prelude::UnsafeBytes;
use holo_hash::{ActionHash, AgentPubKey};
use support::{
    create_hive, setup_cells, single_conductor_cell_app, Acl, AclBucket, AclByGroupGenesis,
    AclSpec,
    CreateEncryptedContentInput, CreateGroupGenesisInput, CreateGroupMembershipInput,
    CreateResponse, GenesisResponse, MembershipResponse, RecipientWitness,
};

const DISJOINT_REJECT: &str = "HiveGroup group_acl buckets must be disjoint";

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
                        reader: vec![],
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

/// Build a HiveGroup create input whose `group_acl` lists the same
/// group in `owner` AND `admin` — the disjointness violation under test.
fn overlapping_bucket_input(
    hive: ActionHash,
    group: ActionHash,
    author_pubkey: &AgentPubKey,
) -> CreateEncryptedContentInput {
    CreateEncryptedContentInput {
        id: "disjoint-content".to_string(),
        display_hive_id: "disjoint-hive".to_string(),
        content_type: "sweettest-disjoint".to_string(),
        revision_author_signing_public_key: author_pubkey.to_string(),
        bytes: UnsafeBytes::from(vec![7u8]).into(),
        acl_spec: AclSpec::HiveGroup {
            hive_genesis_hash: hive,
            author_membership_hash: None,
            group_acl: AclByGroupGenesis {
                owner: group.clone(),
                admin: vec![group],
                writer: vec![],
                reader: vec![],
            },
            author_group_membership_hash: None,
            recipient_witnesses: vec![],
        },
        public_key_acl: Acl {
            owner: String::new(),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        },
        dynamic_links: None,
    }
}

/// Step 1.5 (pass-7 M13): a `group_acl` that lists one group in two
/// buckets is rejected pre-fetch, before any hive/group authority walk.
#[tokio::test(flavor = "multi_thread")]
async fn overlapping_group_acl_buckets_reject() {
    holochain_trace::test_run();
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().clone();
    let hive_hash = create_hive(&conductor, &zome, "disjoint-hive").await;

    let group: GenesisResponse = conductor
        .call(
            &zome,
            "create_group_genesis",
            CreateGroupGenesisInput {
                hive_genesis_hash: hive_hash.clone(),
                display_id: "disjoint-group".to_string(),
                hive_wide_role: None,
                creator_hive_membership_hash: None,
            },
        )
        .await;

    let rejected: Result<CreateResponse, _> = conductor
        .call_fallible(
            &zome,
            "create_encrypted_content",
            overlapping_bucket_input(hive_hash, group.hash, &author),
        )
        .await;
    let err = format!(
        "{:?}",
        rejected.expect_err("overlapping group_acl buckets must reject")
    );
    assert!(err.contains(DISJOINT_REJECT), "unexpected error: {err}");
}
