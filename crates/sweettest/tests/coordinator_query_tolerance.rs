//! Behavior proof for the pass-4-query-tolerance coordinator fix, on a
//! real in-process holochain 0.6.1 conductor (Sweettest). In-repo
//! tryorama cannot boot on the flake's hc (the quic->webrtc CLI
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

mod support;

use holo_hash::ActionHash;
use holochain::sweettest::await_consistency_s;
use serde::de::IgnoredAny;
use support::{
    setup_cells, single_conductor_app, CreateHiveGenesisInput, CreateHiveMembershipInput,
    GenesisResponse, ListedHive,
};

#[tokio::test(flavor = "multi_thread")]
async fn get_many_encrypted_content_tolerates_a_missing_target() {
    holochain_trace::test_run();

    let (conductor, zome) = single_conductor_app().await;

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

    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let bob_agent = bob.agent_pubkey().clone();

    await_consistency_s(30, [alice, bob]).await.unwrap();

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
                grantor_owner_accept_hash: None,
            },
        )
        .await;

    await_consistency_s(60, [alice, bob]).await.unwrap();

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
