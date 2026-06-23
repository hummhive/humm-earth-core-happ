//! Behavior proof for the pass-4-migration-rescue coordinator: dormancy-proof
//! `*_local` reads + `mark_migrated_v2` fail-soft.
//!
//! Sweettest cannot reproduce live-iroh `@4` dormancy (the in-process conductor
//! is its own authority for its own basis, so the Network strategy resolves
//! locally too). These tests therefore pin local-path correctness and the
//! fail-soft return shape; the dormant-network differential lives in
//! humm-tauri's tryorama e2e and is captured here by Test B's `#[ignore]`.

use std::path::Path;

use holo_hash::{ActionHash, AgentPubKey};
use holochain::sweettest::{
    await_consistency, SweetCell, SweetConductor, SweetConductorBatch, SweetDnaFile, SweetZome,
};
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};

fn dna_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dnas/humm_earth_core/workdir/humm_earth_core.dna")
}

async fn single_conductor_app() -> (SweetConductor, SweetZome) {
    holochain_trace::test_run();
    let dna = SweetDnaFile::from_bundle(&dna_path())
        .await
        .expect("load humm_earth_core.dna (must be built: npm run build:zomes && hc app pack)");
    let mut conductor = SweetConductor::from_standard_config().await;
    let app = conductor
        .setup_app("test-app", &[("humm_earth_core".into(), dna)])
        .await
        .unwrap();
    let (cell,): (SweetCell,) = app.into_tuple();
    let zome = cell.zome("content");
    (conductor, zome)
}

#[derive(Debug, Deserialize)]
struct ListedHive {
    display_id: String,
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GenesisResponse {
    hash: ActionHash,
}

#[derive(Debug, Serialize)]
struct CreateHiveGenesisInput {
    display_id: String,
}

#[derive(Debug, Serialize)]
struct CreateHiveMembershipInput {
    hive_genesis_hash: ActionHash,
    for_agent: AgentPubKey,
    role: String,
    grantor_membership_hash: Option<ActionHash>,
    expiry: Option<i64>,
}

#[derive(Debug, Serialize)]
struct MarkMigratedV2Input {
    original_action_hash: ActionHash,
    marker: MigrationMarkerV2,
}

#[derive(Debug, Serialize)]
struct MigrationMarkerV2 {
    schema_tag: String,
    schema_version: u32,
    new_dna_hash_base64: String,
    new_action_hash_base64: String,
    new_app_id: String,
    migrated_at_microseconds: i64,
    new_hive_genesis_hash_base64: Option<String>,
    new_hive_genesis_display_id: Option<String>,
}

fn dummy_v2_marker() -> MigrationMarkerV2 {
    MigrationMarkerV2 {
        schema_tag: "humm-earth-core-happ/migration-marker".to_string(),
        schema_version: 2,
        new_dna_hash_base64: "uhC0kPLACEHOLDER".to_string(),
        new_action_hash_base64: "uhCkkPLACEHOLDER".to_string(),
        new_app_id: "humm-earth-core@new".to_string(),
        migrated_at_microseconds: 0,
        new_hive_genesis_hash_base64: None,
        new_hive_genesis_display_id: None,
    }
}

async fn create_hive(conductor: &SweetConductor, zome: &SweetZome, display_id: &str) -> ActionHash {
    let response: GenesisResponse = conductor
        .call(
            zome,
            "create_hive_genesis",
            CreateHiveGenesisInput {
                display_id: display_id.to_string(),
            },
        )
        .await;
    response.hash
}

/// Founder local enumeration via source-chain `query()` — the migration-critical
/// regression anchor (dormant founder MUST see their hives).
#[tokio::test(flavor = "multi_thread")]
async fn founder_lists_own_hives_via_local_path() {
    let (conductor, zome) = single_conductor_app().await;
    create_hive(&conductor, &zome, "DevHive").await;
    create_hive(&conductor, &zome, "Hive2").await;

    let hives: Vec<ListedHive> = conductor.call(&zome, "list_my_hives_local", ()).await;

    assert_eq!(
        hives.len(),
        2,
        "list_my_hives_local must enumerate both founder hives; got {hives:?}"
    );
    let mut display_ids: Vec<&str> = hives.iter().map(|h| h.display_id.as_str()).collect();
    display_ids.sort();
    assert_eq!(display_ids, vec!["DevHive", "Hive2"]);
    for h in &hives {
        assert!(
            h.role.is_none(),
            "founder must have role=None; got {:?}",
            h.role
        );
    }
}

/// Network-vs-local differential. `#[ignore]`d because a sweettest agent is
/// authority for its own basis (live-iroh @4 dormancy is e2e-only; covered by
/// humm-tauri's tryorama). Kept as a tripwire: if the assertion ever passes,
/// the harness has changed.
#[ignore = "sweettest agent is authority for its own basis; @4 dormancy is e2e-only"]
#[tokio::test(flavor = "multi_thread")]
async fn network_list_returns_empty_on_dormant_cell() {
    let (conductor, zome) = single_conductor_app().await;
    create_hive(&conductor, &zome, "DormantHive").await;

    let net: Vec<ListedHive> = conductor.call(&zome, "list_my_hives", ()).await;
    let local: Vec<ListedHive> = conductor.call(&zome, "list_my_hives_local", ()).await;

    assert_eq!(
        net.len(),
        0,
        "if this passes, sweettest now reproduces dormancy — unignore the test \
         and document the harness step that made it true"
    );
    assert_eq!(local.len(), 1);
}

/// `mark_migrated_v2` returns `Ok(None)` (not `Err`) when the original entry is
/// unresolvable, so a migration-script per-entry loop steps over markers the
/// dormant cell cannot honour without aborting the bundle.
#[tokio::test(flavor = "multi_thread")]
async fn mark_migrated_v2_returns_none_on_unresolvable_original() {
    let (conductor, zome) = single_conductor_app().await;

    let input = MarkMigratedV2Input {
        original_action_hash: ActionHash::from_raw_36(vec![0xdb; 36]),
        marker: dummy_v2_marker(),
    };
    let result: Result<Option<IgnoredAny>, _> = conductor
        .call_fallible(&zome, "mark_migrated_v2", input)
        .await;

    assert!(
        result.is_ok(),
        "mark_migrated_v2 must NOT Err on an unresolvable original; got {result:?}"
    );
    assert!(
        result.unwrap().is_none(),
        "mark_migrated_v2 must return Ok(None) (skip courtesy marker)"
    );
}

/// Joiner sees a granted hive via the local-store branch
/// (`GetStrategy::Local` + `GetOptions::local`) once gossip has integrated
/// the Inbox link + membership entry into their local DHT store.
#[tokio::test(flavor = "multi_thread")]
async fn joiner_local_lists_granted_membership() {
    holochain_trace::test_run();
    let dna = SweetDnaFile::from_bundle(&dna_path())
        .await
        .expect("load humm_earth_core.dna");

    let mut conductors = SweetConductorBatch::from_standard_config_rendezvous(2).await;
    let apps = conductors
        .setup_app("test-app", &[("humm_earth_core".into(), dna)])
        .await
        .unwrap();
    let ((alice,), (bob,)): ((SweetCell,), (SweetCell,)) = apps.into_tuples();
    let bob_agent: AgentPubKey = bob.agent_pubkey().clone();

    await_consistency(30, [&alice, &bob]).await.unwrap();

    let genesis_hash = {
        let response: GenesisResponse = conductors[0]
            .call(
                &alice.zome("content"),
                "create_hive_genesis",
                CreateHiveGenesisInput {
                    display_id: "joiner-local-hive".to_string(),
                },
            )
            .await;
        response.hash
    };

    let _membership: IgnoredAny = conductors[0]
        .call(
            &alice.zome("content"),
            "create_hive_membership",
            CreateHiveMembershipInput {
                hive_genesis_hash: genesis_hash,
                for_agent: bob_agent,
                role: "Reader".to_string(),
                grantor_membership_hash: None,
                expiry: None,
            },
        )
        .await;

    await_consistency(60, [&alice, &bob]).await.unwrap();

    let bob_hives: Vec<ListedHive> = conductors[1]
        .call(&bob.zome("content"), "list_my_hives_local", ())
        .await;
    assert_eq!(
        bob_hives.len(),
        1,
        "joiner must see the granted hive via list_my_hives_local; got {bob_hives:?}"
    );
    assert_eq!(bob_hives[0].display_id, "joiner-local-hive");
    assert_eq!(bob_hives[0].role.as_deref(), Some("Reader"));

    let alice_hives: Vec<ListedHive> = conductors[0]
        .call(&alice.zome("content"), "list_my_hives_local", ())
        .await;
    assert!(
        alice_hives
            .iter()
            .any(|h| h.display_id == "joiner-local-hive" && h.role.is_none()),
        "founder still lists her own hive (role=None); got {alice_hives:?}"
    );
}
