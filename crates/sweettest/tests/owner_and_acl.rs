//! Multi-user conductor proof for the pass-5 hive Owner role + ACL fork.
//!
//! Exercises the must_get-backed paths host tests cannot reach: the
//! offer/accept handshake, lineage-based current-owner resolution across
//! nodes, the only-owner-grants-Admin hierarchy (integrity ever-owner floor +
//! coordinator current-owner precheck), and the revoke owner-protect.

use std::path::Path;

use holo_hash::{ActionHash, AgentPubKey};
use holochain::sweettest::{
    await_consistency_s, SweetCell, SweetConductorBatch, SweetDnaFile,
};
use serde::{Deserialize, Serialize};

fn dna_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dnas/humm_earth_core/workdir/humm_earth_core.dna")
}

#[derive(Debug, Serialize)]
struct CreateHiveGenesisInput {
    display_id: String,
}

#[derive(Debug, Deserialize)]
struct GenesisResponse {
    hash: ActionHash,
}

#[derive(Debug, Serialize)]
struct CreateHiveMembershipInput {
    hive_genesis_hash: ActionHash,
    for_agent: AgentPubKey,
    role: String,
    grantor_membership_hash: Option<ActionHash>,
    expiry: Option<i64>,
    grantor_owner_accept_hash: Option<ActionHash>,
}

#[derive(Debug, Deserialize)]
struct MembershipResponse {
    hash: ActionHash,
}

#[derive(Debug, Serialize)]
struct InitiateOwnerHandoffInput {
    hive_genesis_hash: ActionHash,
    to_agent: AgentPubKey,
    offerer_owner_accept_hash: Option<ActionHash>,
}

#[derive(Debug, Serialize)]
struct AcceptOwnerHandoffInput {
    offer_hash: ActionHash,
}

#[derive(Debug, Serialize)]
struct GetMemberHiveRoleInput {
    hive_genesis_hash: ActionHash,
    agent: AgentPubKey,
}

#[derive(Debug, Serialize)]
struct RevokeHiveMembershipInput {
    membership_hash: ActionHash,
    new_expiry: i64,
    grantor_membership_hash: Option<ActionHash>,
    grantor_owner_accept_hash: Option<ActionHash>,
}

async fn setup(agents: usize) -> (SweetConductorBatch, Vec<SweetCell>) {
    let dna = SweetDnaFile::from_bundle(&dna_path())
        .await
        .expect("load humm_earth_core.dna (build: npm run build:zomes && hc dna pack)");
    let mut conductors = SweetConductorBatch::from_standard_config_rendezvous(agents).await;
    let apps = conductors
        .setup_app("test-app", &[("humm_earth_core".into(), dna)])
        .await
        .unwrap();
    let cells = apps.cells_flattened();
    (conductors, cells)
}

fn founds_membership(
    hive: ActionHash,
    for_agent: AgentPubKey,
    role: &str,
) -> CreateHiveMembershipInput {
    CreateHiveMembershipInput {
        hive_genesis_hash: hive,
        for_agent,
        role: role.to_string(),
        grantor_membership_hash: None,
        expiry: None,
        grantor_owner_accept_hash: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn owner_handshake_admin_authority_and_owner_reject() {
    holochain_trace::test_run();
    let (conductors, cells) = setup(3).await;
    let (alice, bob, carol) = (&cells[0], &cells[1], &cells[2]);
    let alice_key = alice.agent_pubkey().clone();
    let bob_key = bob.agent_pubkey().clone();
    let carol_key = carol.agent_pubkey().clone();
    await_consistency_s(30, [alice, bob, carol]).await.unwrap();

    let hive: GenesisResponse = conductors[0]
        .call(
            &alice.zome("content"),
            "create_hive_genesis",
            CreateHiveGenesisInput {
                display_id: "owner-hive".into(),
            },
        )
        .await;

    let owner_via_membership: Result<MembershipResponse, _> = conductors[0]
        .call_fallible(
            &alice.zome("content"),
            "create_hive_membership",
            founds_membership(hive.hash.clone(), bob_key.clone(), "Owner"),
        )
        .await;
    let rejection = format!("{:?}", owner_via_membership.expect_err("Owner grant must reject"));
    assert!(
        rejection.contains("Owner role cannot be granted via membership"),
        "got {rejection}"
    );

    let offer: ActionHash = conductors[0]
        .call(
            &alice.zome("content"),
            "initiate_owner_handoff",
            InitiateOwnerHandoffInput {
                hive_genesis_hash: hive.hash.clone(),
                to_agent: bob_key.clone(),
                offerer_owner_accept_hash: None,
            },
        )
        .await;
    let bob_admin: MembershipResponse = conductors[0]
        .call(
            &alice.zome("content"),
            "create_hive_membership",
            founds_membership(hive.hash.clone(), bob_key.clone(), "Admin"),
        )
        .await;
    await_consistency_s(60, [alice, bob, carol]).await.unwrap();

    let bob_accept: ActionHash = conductors[1]
        .call(
            &bob.zome("content"),
            "accept_owner_handoff",
            AcceptOwnerHandoffInput { offer_hash: offer },
        )
        .await;
    await_consistency_s(60, [alice, bob, carol]).await.unwrap();

    let bob_role: Option<String> = conductors[0]
        .call(
            &alice.zome("content"),
            "get_member_hive_role",
            GetMemberHiveRoleInput {
                hive_genesis_hash: hive.hash.clone(),
                agent: bob_key.clone(),
            },
        )
        .await;
    assert_eq!(bob_role.as_deref(), Some("Owner"), "Bob owns the hive post-accept");

    let alice_role: Option<String> = conductors[0]
        .call(
            &alice.zome("content"),
            "get_member_hive_role",
            GetMemberHiveRoleInput {
                hive_genesis_hash: hive.hash.clone(),
                agent: alice_key.clone(),
            },
        )
        .await;
    assert_ne!(alice_role.as_deref(), Some("Owner"), "former owner Alice steps down");

    let former_owner_grant: Result<MembershipResponse, _> = conductors[0]
        .call_fallible(
            &alice.zome("content"),
            "create_hive_membership",
            founds_membership(hive.hash.clone(), carol_key.clone(), "Admin"),
        )
        .await;
    let rejection = format!(
        "{:?}",
        former_owner_grant.expect_err("former owner cannot grant Admin")
    );
    assert!(
        rejection.contains("only the current hive owner may grant the Admin role"),
        "got {rejection}"
    );

    let owner_grant: MembershipResponse = conductors[1]
        .call(
            &bob.zome("content"),
            "create_hive_membership",
            CreateHiveMembershipInput {
                hive_genesis_hash: hive.hash.clone(),
                for_agent: carol_key.clone(),
                role: "Admin".into(),
                grantor_membership_hash: Some(bob_admin.hash),
                expiry: None,
                grantor_owner_accept_hash: Some(bob_accept),
            },
        )
        .await;
    assert!(
        !owner_grant.hash.get_raw_39().is_empty(),
        "current owner Bob grants Admin"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn two_transfers_resolve_to_same_owner_on_every_node() {
    holochain_trace::test_run();
    let (conductors, cells) = setup(3).await;
    let (alice, bob, carol) = (&cells[0], &cells[1], &cells[2]);
    let bob_key = bob.agent_pubkey().clone();
    let carol_key = carol.agent_pubkey().clone();
    await_consistency_s(30, [alice, bob, carol]).await.unwrap();

    let hive: GenesisResponse = conductors[0]
        .call(
            &alice.zome("content"),
            "create_hive_genesis",
            CreateHiveGenesisInput {
                display_id: "lineage-hive".into(),
            },
        )
        .await;

    let offer_bob: ActionHash = conductors[0]
        .call(
            &alice.zome("content"),
            "initiate_owner_handoff",
            InitiateOwnerHandoffInput {
                hive_genesis_hash: hive.hash.clone(),
                to_agent: bob_key.clone(),
                offerer_owner_accept_hash: None,
            },
        )
        .await;
    await_consistency_s(60, [alice, bob, carol]).await.unwrap();
    let bob_accept: ActionHash = conductors[1]
        .call(
            &bob.zome("content"),
            "accept_owner_handoff",
            AcceptOwnerHandoffInput {
                offer_hash: offer_bob,
            },
        )
        .await;
    await_consistency_s(60, [alice, bob, carol]).await.unwrap();

    let offer_carol: ActionHash = conductors[1]
        .call(
            &bob.zome("content"),
            "initiate_owner_handoff",
            InitiateOwnerHandoffInput {
                hive_genesis_hash: hive.hash.clone(),
                to_agent: carol_key.clone(),
                offerer_owner_accept_hash: Some(bob_accept),
            },
        )
        .await;
    await_consistency_s(60, [alice, bob, carol]).await.unwrap();
    let _carol_accept: ActionHash = conductors[2]
        .call(
            &carol.zome("content"),
            "accept_owner_handoff",
            AcceptOwnerHandoffInput {
                offer_hash: offer_carol,
            },
        )
        .await;
    await_consistency_s(90, [alice, bob, carol]).await.unwrap();

    for (idx, cell) in [alice, bob, carol].iter().enumerate() {
        let role: Option<String> = conductors[idx]
            .call(
                &cell.zome("content"),
                "get_member_hive_role",
                GetMemberHiveRoleInput {
                    hive_genesis_hash: hive.hash.clone(),
                    agent: carol_key.clone(),
                },
            )
            .await;
        assert_eq!(
            role.as_deref(),
            Some("Owner"),
            "node {idx} must resolve Carol as the owner after two transfers"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn revoke_refuses_the_current_owner_membership() {
    holochain_trace::test_run();
    let (conductors, cells) = setup(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let bob_key = bob.agent_pubkey().clone();
    await_consistency_s(30, [alice, bob]).await.unwrap();

    let hive: GenesisResponse = conductors[0]
        .call(
            &alice.zome("content"),
            "create_hive_genesis",
            CreateHiveGenesisInput {
                display_id: "revoke-hive".into(),
            },
        )
        .await;
    let offer: ActionHash = conductors[0]
        .call(
            &alice.zome("content"),
            "initiate_owner_handoff",
            InitiateOwnerHandoffInput {
                hive_genesis_hash: hive.hash.clone(),
                to_agent: bob_key.clone(),
                offerer_owner_accept_hash: None,
            },
        )
        .await;
    let bob_membership: MembershipResponse = conductors[0]
        .call(
            &alice.zome("content"),
            "create_hive_membership",
            founds_membership(hive.hash.clone(), bob_key.clone(), "Admin"),
        )
        .await;
    await_consistency_s(60, [alice, bob]).await.unwrap();
    let _: ActionHash = conductors[1]
        .call(
            &bob.zome("content"),
            "accept_owner_handoff",
            AcceptOwnerHandoffInput { offer_hash: offer },
        )
        .await;
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let revoke: Result<MembershipResponse, _> = conductors[0]
        .call_fallible(
            &alice.zome("content"),
            "revoke_hive_membership",
            RevokeHiveMembershipInput {
                membership_hash: bob_membership.hash,
                new_expiry: 1,
                grantor_membership_hash: None,
                grantor_owner_accept_hash: None,
            },
        )
        .await;
    let rejection = format!("{:?}", revoke.expect_err("owner membership is protected"));
    assert!(
        rejection.contains("refusing to revoke the current hive owner's membership"),
        "got {rejection}"
    );
}
