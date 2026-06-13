//! Behavior proof for the pass-4-query-tolerance coordinator fix, on a
//! real in-process holochain 0.6.0 conductor (Sweettest). In-repo
//! tryorama cannot boot on the flake's hc 0.6.0 (the quic->webrtc CLI
//! rename), so this is the in-tree, drift-free conductor proof — the
//! same posture the recv-signal-fix bump used (host + BDD), modernised.
//!
//! Loads the pre-built DNA bundle (carries the fixed coordinator) and
//! exercises the two repaired query paths:
//!   1. `get_many_encrypted_content` tolerates an unresolvable target
//!      instead of failing the whole batch (the all-or-nothing wart that
//!      poisoned every list_by_* read — live-confirmed "no Record found
//!      at given hash").
//!   2. `list_my_hives` classifies a mixed-type Inbox (HiveGenesis +
//!      HiveMembership) without `?`-propagating the wrong-type
//!      deserialize — the bug that broke the joiner's list forever.

use std::path::Path;

use holo_hash::{ActionHash, AgentPubKey};
use holochain::sweettest::{
    await_consistency, SweetCell, SweetConductor, SweetConductorBatch, SweetDnaFile,
};
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};

/// Absolute path to the pre-built DNA (resolved from this crate's manifest
/// dir → repo root), so the test is cwd-independent.
fn dna_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dnas/humm_earth_core/workdir/humm_earth_core.dna")
}

/// `list_my_hives` row (subset of the coordinator's `ListedHive`; serde
/// ignores the fields we don't assert on).
#[derive(Debug, Deserialize)]
struct ListedHive {
    display_id: String,
    role: Option<String>,
}

/// `create_hive_genesis` response (subset — we only need the action hash).
#[derive(Debug, Deserialize)]
struct GenesisResponse {
    hash: ActionHash,
}

/// `create_hive_genesis` input.
#[derive(Debug, Serialize)]
struct CreateHiveGenesisInput {
    display_id: String,
}

/// `create_hive_membership` input — mirrors the coordinator extern's
/// `CreateHiveMembershipInput`. `role` is the bare-variant string the
/// `Role` enum (de)serializes as ("Reader").
#[derive(Debug, Serialize)]
struct CreateHiveMembershipInput {
    hive_genesis_hash: ActionHash,
    for_agent: AgentPubKey,
    role: String,
    grantor_membership_hash: Option<ActionHash>,
    expiry: Option<i64>,
}

#[tokio::test(flavor = "multi_thread")]
async fn get_many_encrypted_content_tolerates_a_missing_target() {
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

    // A hash that resolves to no record — the exact gossip-lag / tombstone
    // condition. Pre-fix the all-or-nothing `collect()` propagated the
    // "no Record found at given hash" Err out of the whole batch; post-fix
    // `filter_map(.ok())` drops it and returns the resolvable subset.
    let missing = ActionHash::from_raw_36(vec![0xdb; 36]);

    let result: Result<Vec<IgnoredAny>, _> = conductor
        .call_fallible(&zome, "get_many_encrypted_content", vec![missing])
        .await;

    assert!(
		result.is_ok(),
		"get_many_encrypted_content must NOT fail the batch on an unresolvable target; got {result:?}"
	);
    assert_eq!(
        result.unwrap().len(),
        0,
        "the unresolvable target is dropped, leaving an empty (but Ok) result"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn joiner_lists_hive_without_cross_type_decode_failure() {
    holochain_trace::test_run();

    let dna = SweetDnaFile::from_bundle(&dna_path())
        .await
        .expect("load humm_earth_core.dna");

    // Two conductors over a shared rendezvous so Bob gossips Alice's writes.
    let mut conductors = SweetConductorBatch::from_standard_config_rendezvous(2).await;
    let apps = conductors
        .setup_app("test-app", &[("humm_earth_core".into(), dna)])
        .await
        .unwrap();
    let ((alice,), (bob,)): ((SweetCell,), (SweetCell,)) = apps.into_tuples();
    let bob_agent: AgentPubKey = bob.agent_pubkey().clone();

    await_consistency(30, [&alice, &bob]).await.unwrap();

    // Alice founds a hive (she is the genesis author → implicit Owner).
    let genesis: GenesisResponse = conductors[0]
        .call(
            &alice.zome("content"),
            "create_hive_genesis",
            CreateHiveGenesisInput {
                display_id: "sweettest-join-hive".to_string(),
            },
        )
        .await;

    // Alice grants Bob a Reader membership (null grantor = genesis-author mint).
    let _membership: IgnoredAny = conductors[0]
        .call(
            &alice.zome("content"),
            "create_hive_membership",
            CreateHiveMembershipInput {
                hive_genesis_hash: genesis.hash,
                for_agent: bob_agent,
                role: "Reader".to_string(),
                grantor_membership_hash: None,
                expiry: None,
            },
        )
        .await;

    await_consistency(60, [&alice, &bob]).await.unwrap();

    // Bob's Inbox now holds a HiveInvite → HiveMembership target. Pre-fix,
    // list_my_hives decoded it as HiveGenesis FIRST and `?`-propagated the
    // "missing field display_id" deserialize error, breaking the call.
    let bob_hives: Vec<ListedHive> = conductors[1]
        .call(&bob.zome("content"), "list_my_hives", ())
        .await;

    assert_eq!(
        bob_hives.len(),
        1,
        "joiner must see exactly the joined hive"
    );
    assert_eq!(bob_hives[0].display_id, "sweettest-join-hive");
    assert_eq!(bob_hives[0].role.as_deref(), Some("Reader"));

    // The founder still lists her hive (role: None) after granting — the
    // genesis-target decode in get_latest_membership / list_my_hives no
    // longer throws for an owner either.
    let alice_hives: Vec<ListedHive> = conductors[0]
        .call(&alice.zome("content"), "list_my_hives", ())
        .await;
    assert!(
        alice_hives
            .iter()
            .any(|h| h.display_id == "sweettest-join-hive" && h.role.is_none()),
        "founder lists her own hive with role=None; got {alice_hives:?}"
    );
}
