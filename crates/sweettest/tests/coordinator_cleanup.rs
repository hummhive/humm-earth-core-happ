//! Behavior proof for the pass-4 coordinator cleanup fixes on a real
//! in-process holochain 0.6.0 conductor (Sweettest):
//!   1. `delete_encrypted_content` now sweeps the entry's discovery links
//!      (the old `// TODO: delete links` gap). After a self-delete,
//!      `list_by_hive_link` / `list_by_dynamic_link` / `count_links_by_hive`
//!      all drop to empty/0 instead of dangling to a tombstoned target
//!      (the C3 over-count).
//!   2. `get_messages_since(0)` replays the full local chain — the corrected
//!      doc contract (the old comment falsely claimed `u32::MAX` wrapped to 0;
//!      the real full-replay sentinel is `since_seq = 0`).
//!
//! In-repo tryorama cannot boot on the flake's hc 0.6.0 (the quic->webrtc
//! CLI rename), so Sweettest is the in-tree conductor path. Loads the
//! pre-built DNA bundle (carries the fixed coordinator).

use std::path::Path;

use holo_hash::ActionHash;
use holochain::sweettest::{await_consistency_s, SweetCell, SweetConductor, SweetDnaFile};
use holochain_types::prelude::{SerializedBytes, UnsafeBytes};
use serde::de::IgnoredAny;
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
struct Acl {
    owner: String,
    admin: Vec<String>,
    writer: Vec<String>,
    reader: Vec<String>,
}

/// Mirror of the coordinator `AclSpec` — only `OpenWrite` is needed: it
/// requires no hive membership (just a real target hive) yet still creates
/// the full hive-scoped link bundle (hive-shape Hive + HummContentId +
/// Dynamic), giving the delete sweep links to clean up. Externally-tagged,
/// matching the coordinator enum's default serde shape.
#[derive(Debug, Serialize)]
enum AclSpec {
    OpenWrite {
        target_hive_genesis_hash: Option<ActionHash>,
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

#[derive(Debug, Serialize)]
struct ListByHiveInput {
    hive_genesis_hash: ActionHash,
    content_type: String,
    since_ts: Option<i64>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ListByDynamicLinkInput {
    hive_genesis_hash: ActionHash,
    content_type: String,
    dynamic_link: String,
}

#[derive(Debug, Serialize)]
struct CountByHiveInput {
    hive_genesis_hash: ActionHash,
    content_type: String,
    since_ts: Option<i64>,
}

#[derive(Debug, Serialize)]
struct GetMessagesSinceInput {
    since_seq: u32,
}

async fn setup() -> (SweetConductor, SweetCell) {
    let dna = SweetDnaFile::from_bundle(&dna_path())
        .await
        .expect("load humm_earth_core.dna (build: npm run build:zomes && hc app pack workdir --recursive)");
    let mut conductor = SweetConductor::from_standard_config().await;
    let app = conductor
        .setup_app("test-app", &[("humm_earth_core".into(), dna)])
        .await
        .unwrap();
    let (cell,): (SweetCell,) = app.into_tuple();
    (conductor, cell)
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_encrypted_content_cleans_up_discovery_links() {
    holochain_trace::test_run();
    let (conductor, cell) = setup().await;
    let zome = cell.zome("content");
    let author = cell.agent_pubkey().clone();

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
    let created: CreateResponse = conductor
        .call(
            &zome,
            "create_encrypted_content",
            CreateEncryptedContentInput {
                id: "cleanup-1".to_string(),
                display_hive_id: "sweettest-cleanup-hive".to_string(),
                content_type: content_type.clone(),
                revision_author_signing_public_key: author.to_string(),
                bytes: UnsafeBytes::from(vec![0u8]).into(),
                acl_spec: AclSpec::OpenWrite {
                    target_hive_genesis_hash: Some(genesis.hash.clone()),
                },
                public_key_acl: Acl {
                    owner: author.to_string(),
                    admin: vec![],
                    writer: vec![],
                    reader: vec![],
                },
                dynamic_links: Some(vec![label.clone()]),
            },
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
    let content_ah = ActionHash::try_from(created.hash.as_str())
        .expect("create response hash parses as an ActionHash");
    let _deleted: ActionHash = conductor
        .call(&zome, "delete_encrypted_content", content_ah)
        .await;

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

    let count_after: usize = conductor
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
    assert_eq!(
        count_after, 0,
        "hive link count must drop to 0 (the C3 over-count fix)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn get_messages_since_zero_replays_full_chain() {
    holochain_trace::test_run();
    let (conductor, cell) = setup().await;
    let zome = cell.zome("content");

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
