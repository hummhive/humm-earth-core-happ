//! Behavior proof for the pass-4 coordinator cleanup fixes on a real
//! in-process Holochain 0.6.1 conductor (Sweettest):
//!   1. `delete_encrypted_content` now sweeps the entry's discovery links.
//!   2. `get_messages_since(0)` replays the full local chain.
//!
//! Sweettest is the in-tree conductor path for hc 0.6.1/iroh. It loads the
//! pre-built DNA through shared support so stale integrity generations fail
//! before coordinator behavior is asserted.

mod support;

use holo_hash::ActionHash;
use holochain::sweettest::await_consistency_s;
use serde::de::IgnoredAny;
use serde::Serialize;
use support::{
    create_open_write_content, single_conductor_app, single_conductor_cell_app,
    wait_for_count_links_by_hive_to, CountByHiveInput, CreateHiveGenesisInput, GenesisResponse,
    ListByHiveInput,
};

#[derive(Debug, Serialize)]
struct ListByDynamicLinkInput {
    hive_genesis_hash: ActionHash,
    content_type: String,
    dynamic_link: String,
}

#[derive(Debug, Serialize)]
struct GetMessagesSinceInput {
    since_seq: u32,
}

#[derive(Debug, serde::Deserialize)]
struct DeleteResponse {
    was_deleted: bool,
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_encrypted_content_cleans_up_discovery_links() {
    holochain_trace::test_run();
    let (conductor, cell, zome) = single_conductor_cell_app().await;

    // Found a hive so OpenWrite has a real target (the hive's existence is
    // OpenWrite's only validation gate beyond the author-pubkey header match).
    let genesis: GenesisResponse = conductor
        .call(
            &zome,
            "create_hive_genesis",
            CreateHiveGenesisInput {
                display_id: "sweettest-cleanup-hive".to_string(),
            },
        )
        .await;

    let content_type = "sweettest-cleanup-type".to_string();
    let label = "sweep-me".to_string();
    let created_hash = create_open_write_content(
        &conductor,
        &zome,
        genesis.hash.clone(),
        &content_type,
        "cleanup-1",
        Some(vec![label.clone()]),
    )
    .await;

    await_consistency_s(30, [&cell]).await.unwrap();

    // Pre-delete: the hive-shape Hive link, the Dynamic link, and the hive
    // link count all see exactly the one entry.
    let by_hive: Vec<IgnoredAny> = conductor
        .call(
            &zome,
            "list_by_hive_link",
            ListByHiveInput {
                hive_genesis_hash: genesis.hash.clone(),
                content_type: content_type.clone(),
                since_ts: None,
                limit: None,
            },
        )
        .await;
    assert_eq!(
        by_hive.len(),
        1,
        "entry must be discoverable by hive link pre-delete"
    );

    let by_dyn: Vec<IgnoredAny> = conductor
        .call(
            &zome,
            "list_by_dynamic_link",
            ListByDynamicLinkInput {
                hive_genesis_hash: genesis.hash.clone(),
                content_type: content_type.clone(),
                dynamic_link: label.clone(),
            },
        )
        .await;
    assert_eq!(
        by_dyn.len(),
        1,
        "entry must be discoverable by dynamic link pre-delete"
    );

    let count: usize = conductor
        .call(
            &zome,
            "count_links_by_hive",
            CountByHiveInput {
                hive_genesis_hash: genesis.hash.clone(),
                content_type: content_type.clone(),
                since_ts: None,
            },
        )
        .await;
    assert_eq!(count, 1, "hive link count must be 1 pre-delete");

    // Self-delete (author == link author → the discovery-link delete
    // validators pass; the local-chain sweep finds and removes them).
    let content_ah = ActionHash::try_from(created_hash.as_str())
        .expect("create response hash parses as an ActionHash");
    let deleted: DeleteResponse = conductor
        .call(&zome, "delete_encrypted_content", content_ah)
        .await;
    assert!(deleted.was_deleted, "existing target must be really deleted");

    await_consistency_s(30, [&cell]).await.unwrap();

    // Post-delete: every discovery link the author created is swept, so the
    // tombstoned entry no longer dangles in any index.
    let by_hive_after: Vec<IgnoredAny> = conductor
        .call(
            &zome,
            "list_by_hive_link",
            ListByHiveInput {
                hive_genesis_hash: genesis.hash.clone(),
                content_type: content_type.clone(),
                since_ts: None,
                limit: None,
            },
        )
        .await;
    assert_eq!(
        by_hive_after.len(),
        0,
        "hive link must be swept post-delete"
    );

    let by_dyn_after: Vec<IgnoredAny> = conductor
        .call(
            &zome,
            "list_by_dynamic_link",
            ListByDynamicLinkInput {
                hive_genesis_hash: genesis.hash.clone(),
                content_type: content_type.clone(),
                dynamic_link: label.clone(),
            },
        )
        .await;
    assert_eq!(
        by_dyn_after.len(),
        0,
        "dynamic link must be swept post-delete"
    );

    wait_for_count_links_by_hive_to(&conductor, &zome, &genesis.hash, &content_type, 0).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn get_messages_since_zero_replays_full_chain() {
    holochain_trace::test_run();
    let (conductor, zome) = single_conductor_app().await;

    // Baseline: the chain already holds genesis + init + cap-grant actions,
    // so since_seq=0 (range (1, u32::MAX)) is never empty.
    let before: Vec<IgnoredAny> = conductor
        .call(
            &zome,
            "get_messages_since",
            GetMessagesSinceInput { since_seq: 0 },
        )
        .await;
    assert!(
        !before.is_empty(),
        "since_seq=0 replays the full chain, never empty"
    );

    // A fresh commit must appear in the full replay — proving since_seq=0 is
    // the unbounded full-replay sentinel the corrected doc describes (the old
    // comment falsely claimed u32::MAX wrapped to 0; saturating_add does not).
    let _genesis: GenesisResponse = conductor
        .call(
            &zome,
            "create_hive_genesis",
            CreateHiveGenesisInput {
                display_id: "sweettest-resync-hive".to_string(),
            },
        )
        .await;

    let after: Vec<IgnoredAny> = conductor
        .call(
            &zome,
            "get_messages_since",
            GetMessagesSinceInput { since_seq: 0 },
        )
        .await;
    assert!(
        after.len() > before.len(),
        "a fresh commit must appear in the since_seq=0 full replay; before={} after={}",
        before.len(),
        after.len()
    );
}
