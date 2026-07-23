//! Conductor proofs for the coordinator batch/local read paths. M17 seeds the
//! `list_my_groups` per-grant multiplicity guard for the P5 GroupGenesis cache;
//! M19/M20 grow this binary with the batch-extern proofs.

mod support;

use holochain::sweettest::await_consistency_s;
use holo_hash::ActionHash;
use serde::Deserialize;
use support::{
    create_hive, setup_cells, CreateGroupGenesisInput, CreateGroupMembershipInput, GenesisResponse,
    MembershipResponse,
};
use holo_hash::AgentPubKey;
use holochain::sweettest::{SweetConductor, SweetZome};
use holochain_zome_types::prelude::Timestamp;
use serde::Serialize;
use support::{
    create_open_write_content, single_conductor_cell_app, wait_for_count_links_by_hive_to,
    wait_for_own_content_id_count,
};

#[derive(Debug, Deserialize)]
struct ListedGroupRow {
    group_genesis_hash: ActionHash,
    role: Option<String>,
}

/// The P5 genesis cache memoizes the GroupGenesis FETCH, never the response
/// rows: two grants for one group to one agent must surface as two rows.
#[tokio::test(flavor = "multi_thread")]
async fn list_my_groups_returns_one_row_per_grant() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let bob_pubkey = bob.agent_pubkey().clone();
    await_consistency_s(30, [alice, bob]).await.unwrap();

    let hive = create_hive(&conductors[0], &alice.zome("content"), "per-grant-hive").await;

    let group: GenesisResponse = conductors[0]
        .call(
            &alice.zome("content"),
            "create_group_genesis",
            CreateGroupGenesisInput {
                hive_genesis_hash: hive.clone(),
                display_id: "per-grant-group".to_string(),
                hive_wide_role: None,
                creator_hive_membership_hash: None,
            },
        )
        .await;

    for role in ["Reader", "Writer"] {
        let _: MembershipResponse = conductors[0]
            .call(
                &alice.zome("content"),
                "create_group_membership",
                CreateGroupMembershipInput {
                    group_genesis_hash: group.hash.clone(),
                    for_agent: bob_pubkey.clone(),
                    role: role.to_string(),
                    grantor_membership_hash: None,
                    grantor_hive_membership_hash: None,
                    expiry: None,
                },
            )
            .await;
    }

    await_consistency_s(60, [alice, bob]).await.unwrap();

    let rows: Vec<ListedGroupRow> = conductors[1]
        .call(&bob.zome("content"), "list_my_groups", ())
        .await;

    let for_group: Vec<&ListedGroupRow> = rows
        .iter()
        .filter(|row| row.group_genesis_hash == group.hash)
        .collect();
    assert_eq!(
        for_group.len(),
        2,
        "two grants for one group must yield two rows (cache must not dedup): {rows:?}"
    );
    assert!(
        for_group.iter().all(|row| row.role.is_some()),
        "each membership row must carry a role: {rows:?}"
    );
}

const DYNAMIC_LINKS_BATCH_MAX: usize = 64;
const HIVE_LINKS_BATCH_MAX: usize = 32;
const CONTENT_ID_BATCH_MAX: usize = 64;
const AUTHOR_BATCH_MAX: usize = 64;
const DYNAMIC_LINKS_BATCH_REJECT: &str = "dynamic_links batch accepts at most 64 labels";
const HIVE_LINKS_BATCH_REJECT: &str = "hive-link batch accepts at most 32 requests";
const CONTENT_ID_BATCH_REJECT: &str = "content-id batch accepts at most 64 lookups";
const AUTHOR_BATCH_REJECT: &str = "author batch accepts at most 64 lookups";
const BATCH_BUDGET_REJECT: &str =
    "batch total requested records exceed the 4096 budget";

#[derive(Debug, Deserialize)]
struct RecordMirror {
    hash: String,
}

#[derive(Debug, Serialize)]
struct DynamicLinksBatchInput {
    hive_genesis_hash: ActionHash,
    content_type: String,
    dynamic_links: Vec<String>,
    since_ts: Option<Timestamp>,
    limit: Option<usize>,
    include_liveness: bool,
}

#[derive(Debug, Deserialize)]
struct DynamicLinkBatchBucket {
    dynamic_link: String,
    records: Vec<RecordMirror>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct HiveLinkBatchRequest {
    content_type: String,
    since_ts: Option<Timestamp>,
    limit: Option<usize>,
    include_liveness: bool,
}

#[derive(Debug, Serialize)]
struct HiveLinksBatchInput {
    hive_genesis_hash: ActionHash,
    requests: Vec<HiveLinkBatchRequest>,
}

#[derive(Debug, Deserialize)]
struct HiveLinkBatchBucket {
    content_type: String,
    records: Vec<RecordMirror>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct ContentIdLookupInput {
    hive_genesis_hash: ActionHash,
    content_id: String,
}

#[derive(Debug, Deserialize)]
struct ContentIdBatchResult {
    hive_genesis_hash: ActionHash,
    content_id: String,
    #[serde(default)]
    record: Option<RecordMirror>,
}

#[derive(Debug, Serialize)]
struct AuthorContentLookupInput {
    author: AgentPubKey,
    content_type: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AuthorBatchBucket {
    author: AgentPubKey,
    records: Vec<RecordMirror>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct ContentIdExistsInput {
    hive_genesis_hash: ActionHash,
    content_id: String,
}

fn assert_reject_contains<T, E>(result: Result<T, E>, expected: &str)
where
    T: std::fmt::Debug,
    E: std::fmt::Debug,
{
    let error = format!(
        "{:?}",
        result.expect_err("an over-cap batch must be rejected")
    );
    assert!(error.contains(expected), "unexpected error: {error}");
}

async fn create_contents(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: &ActionHash,
    content_type: &str,
    content_ids: &[&str],
    dynamic_link: Option<&str>,
) -> Vec<String> {
    let mut hashes = Vec::with_capacity(content_ids.len());
    for content_id in content_ids {
        hashes.push(
            create_open_write_content(
                conductor,
                zome,
                hive_genesis_hash.clone(),
                content_type,
                content_id,
                dynamic_link.map(|label| vec![label.to_string()]),
            )
            .await,
        );
    }
    hashes
}

async fn list_dynamic_link_buckets(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: ActionHash,
    content_type: &str,
    dynamic_links: &[&str],
    limit: usize,
) -> Vec<DynamicLinkBatchBucket> {
    conductor
        .call(
            zome,
            "list_encrypted_content_by_dynamic_links",
            DynamicLinksBatchInput {
                hive_genesis_hash,
                content_type: content_type.to_string(),
                dynamic_links: dynamic_links
                    .iter()
                    .map(|label| (*label).to_string())
                    .collect(),
                since_ts: None,
                limit: Some(limit),
                include_liveness: false,
            },
        )
        .await
}

fn hive_link_request(content_type: impl Into<String>) -> HiveLinkBatchRequest {
    HiveLinkBatchRequest {
        content_type: content_type.into(),
        since_ts: None,
        limit: None,
        include_liveness: false,
    }
}

fn content_id_lookup(
    hive_genesis_hash: ActionHash,
    content_id: impl Into<String>,
) -> ContentIdLookupInput {
    ContentIdLookupInput {
        hive_genesis_hash,
        content_id: content_id.into(),
    }
}

fn author_lookup(
    author: AgentPubKey,
    content_type: impl Into<String>,
) -> AuthorContentLookupInput {
    AuthorContentLookupInput {
        author,
        content_type: content_type.into(),
        limit: None,
    }
}

async fn probe_content_id_exists(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: ActionHash,
    content_id: &str,
) -> bool {
    conductor
        .call(
            zome,
            "content_id_exists",
            ContentIdExistsInput {
                hive_genesis_hash,
                content_id: content_id.to_string(),
            },
        )
        .await
}

fn assert_bounded_dynamic_buckets(
    buckets: &[DynamicLinkBatchBucket],
    populated_label: &str,
    empty_label: &str,
) {
    let labels: Vec<&str> = buckets
        .iter()
        .map(|bucket| bucket.dynamic_link.as_str())
        .collect();
    assert_eq!(labels, vec![populated_label, empty_label]);
    assert_eq!(buckets[0].records.len(), 2);
    assert!(buckets[0].truncated);
    assert!(buckets[1].records.is_empty());
    assert!(!buckets[1].truncated);
}

#[tokio::test(flavor = "multi_thread")]
async fn dynamic_links_batch_first_page_bounds_and_buckets() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "dynamic-batch-hive").await;
    let content_type = "batch-dynamic-type";
    let populated_label = "shared-label";
    let empty_label = "empty-label";
    let mut created = create_contents(
        &conductor,
        &zome,
        &hive,
        content_type,
        &["dynamic-1", "dynamic-2", "dynamic-3"],
        Some(populated_label),
    )
    .await;
    await_consistency_s(30, [&cell]).await.unwrap();
    wait_for_count_links_by_hive_to(&conductor, &zome, &hive, content_type, 3).await;

    let control = list_dynamic_link_buckets(
        &conductor,
        &zome,
        hive.clone(),
        content_type,
        &[populated_label],
        5,
    )
    .await;
    assert_eq!(control.len(), 1);
    assert_eq!(control[0].dynamic_link, populated_label);
    assert_eq!(control[0].records.len(), 3);
    assert!(!control[0].truncated);
    let mut control_hashes: Vec<String> =
        control[0].records.iter().map(|record| record.hash.clone()).collect();
    control_hashes.sort();
    created.sort();
    assert_eq!(control_hashes, created);

    let bounded = list_dynamic_link_buckets(
        &conductor,
        &zome,
        hive,
        content_type,
        &[populated_label, empty_label],
        2,
    )
    .await;
    assert_bounded_dynamic_buckets(&bounded, populated_label, empty_label);
}

#[tokio::test(flavor = "multi_thread")]
async fn dynamic_links_batch_rejects_over_64_labels() {
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "dynamic-bound-hive").await;
    let dynamic_links = (0..=DYNAMIC_LINKS_BATCH_MAX)
        .map(|index| format!("label-{index}"))
        .collect();
    let rejected: Result<Vec<DynamicLinkBatchBucket>, _> = conductor
        .call_fallible(
            &zome,
            "list_encrypted_content_by_dynamic_links",
            DynamicLinksBatchInput {
                hive_genesis_hash: hive,
                content_type: "dynamic-bound-type".to_string(),
                dynamic_links,
                since_ts: None,
                limit: None,
                include_liveness: false,
            },
        )
        .await;
    assert_reject_contains(rejected, DYNAMIC_LINKS_BATCH_REJECT);
}

#[tokio::test(flavor = "multi_thread")]
async fn hive_links_many_buckets_in_request_order() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "hive-links-batch-hive").await;
    let first_type = "hive-batch-first";
    let second_type = "hive-batch-second";
    let empty_type = "hive-batch-empty";
    let mut first_hashes =
        create_contents(&conductor, &zome, &hive, first_type, &["first-1", "first-2"], None)
            .await;
    let second_hashes =
        create_contents(&conductor, &zome, &hive, second_type, &["second-1"], None).await;
    await_consistency_s(30, [&cell]).await.unwrap();
    wait_for_count_links_by_hive_to(&conductor, &zome, &hive, first_type, 2).await;
    wait_for_count_links_by_hive_to(&conductor, &zome, &hive, second_type, 1).await;

    let buckets: Vec<HiveLinkBatchBucket> = conductor
        .call(
            &zome,
            "list_by_hive_links_many",
            HiveLinksBatchInput {
                hive_genesis_hash: hive,
                requests: vec![
                    hive_link_request(second_type),
                    hive_link_request(empty_type),
                    hive_link_request(first_type),
                ],
            },
        )
        .await;
    let types: Vec<&str> = buckets
        .iter()
        .map(|bucket| bucket.content_type.as_str())
        .collect();
    assert_eq!(types, vec![second_type, empty_type, first_type]);
    assert_eq!(
        buckets.iter().map(|bucket| bucket.records.len()).collect::<Vec<_>>(),
        vec![1, 0, 2]
    );
    assert!(buckets.iter().all(|bucket| !bucket.truncated));
    assert_eq!(&buckets[0].records[0].hash, &second_hashes[0]);
    let mut returned_first: Vec<String> =
        buckets[2].records.iter().map(|record| record.hash.clone()).collect();
    returned_first.sort();
    first_hashes.sort();
    assert_eq!(returned_first, first_hashes);
}

#[tokio::test(flavor = "multi_thread")]
async fn hive_links_many_rejects_over_32_requests() {
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "hive-links-bound-hive").await;
    let requests = (0..=HIVE_LINKS_BATCH_MAX)
        .map(|index| hive_link_request(format!("type-{index}")))
        .collect();
    let rejected: Result<Vec<HiveLinkBatchBucket>, _> = conductor
        .call_fallible(
            &zome,
            "list_by_hive_links_many",
            HiveLinksBatchInput {
                hive_genesis_hash: hive,
                requests,
            },
        )
        .await;
    assert_reject_contains(rejected, HIVE_LINKS_BATCH_REJECT);
}

#[tokio::test(flavor = "multi_thread")]
async fn content_id_batch_keeps_row_per_lookup_incl_missing() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "content-id-batch-hive").await;
    let content_type = "content-id-batch-type";
    let existing_id = "known-content-id";
    let missing_id = "missing-content-id";
    let created =
        create_contents(&conductor, &zome, &hive, content_type, &[existing_id], None).await;
    await_consistency_s(30, [&cell]).await.unwrap();
    wait_for_own_content_id_count(&conductor, &zome, hive.clone(), existing_id, 1).await;

    let rows: Vec<ContentIdBatchResult> = conductor
        .call(
            &zome,
            "get_many_by_content_id_link",
            vec![
                content_id_lookup(hive.clone(), existing_id),
                content_id_lookup(hive.clone(), missing_id),
            ],
        )
        .await;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .map(|row| row.content_id.as_str())
            .collect::<Vec<_>>(),
        vec![existing_id, missing_id]
    );
    assert!(
        rows.iter()
            .all(|row| &row.hive_genesis_hash == &hive)
    );
    assert_eq!(
        rows[0]
            .record
            .as_ref()
            .map(|record| record.hash.as_str()),
        Some(created[0].as_str())
    );
    assert!(rows[1].record.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn content_id_batch_rejects_over_64_lookups() {
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "content-id-bound-hive").await;
    let lookups: Vec<_> = (0..=CONTENT_ID_BATCH_MAX)
        .map(|index| content_id_lookup(hive.clone(), format!("content-{index}")))
        .collect();
    let rejected: Result<Vec<ContentIdBatchResult>, _> = conductor
        .call_fallible(&zome, "get_many_by_content_id_link", lookups)
        .await;
    assert_reject_contains(rejected, CONTENT_ID_BATCH_REJECT);
}

#[tokio::test(flavor = "multi_thread")]
async fn author_batch_buckets_by_author() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_zome = alice.zome("content");
    let bob_zome = bob.zome("content");
    let content_type = "author-batch-type";
    await_consistency_s(30, [alice, bob]).await.unwrap();
    let hive = create_hive(&conductors[0], &alice_zome, "author-batch-hive").await;
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let alice_hashes =
        create_contents(&conductors[0], &alice_zome, &hive, content_type, &["alice-1"], None)
            .await;
    let bob_hashes =
        create_contents(&conductors[1], &bob_zome, &hive, content_type, &["bob-1"], None).await;
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let buckets: Vec<AuthorBatchBucket> = conductors[0]
        .call(
            &alice_zome,
            "list_by_author_many",
            vec![
                author_lookup(bob.agent_pubkey().clone(), content_type),
                author_lookup(alice.agent_pubkey().clone(), content_type),
            ],
        )
        .await;
    assert_eq!(
        buckets
            .iter()
            .map(|bucket| bucket.author.clone())
            .collect::<Vec<_>>(),
        vec![bob.agent_pubkey().clone(), alice.agent_pubkey().clone()]
    );
    assert_eq!(
        buckets.iter().map(|bucket| bucket.records.len()).collect::<Vec<_>>(),
        vec![1, 1]
    );
    assert!(buckets.iter().all(|bucket| !bucket.truncated));
    assert_eq!(&buckets[0].records[0].hash, &bob_hashes[0]);
    assert_eq!(&buckets[1].records[0].hash, &alice_hashes[0]);
}

#[tokio::test(flavor = "multi_thread")]
async fn author_batch_rejects_over_64_lookups() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().clone();
    let lookups: Vec<_> = (0..=AUTHOR_BATCH_MAX)
        .map(|index| author_lookup(author.clone(), format!("type-{index}")))
        .collect();
    let rejected: Result<Vec<AuthorBatchBucket>, _> = conductor
        .call_fallible(&zome, "list_by_author_many", lookups)
        .await;
    assert_reject_contains(rejected, AUTHOR_BATCH_REJECT);
}

#[tokio::test(flavor = "multi_thread")]
async fn content_id_exists_true_and_false() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "content-id-exists-hive").await;
    let existing_id = "existing-content-id";
    let missing_id = "absent-content-id";
    let created = create_contents(
        &conductor,
        &zome,
        &hive,
        "content-id-exists-type",
        &[existing_id],
        None,
    )
    .await;
    assert_eq!(created.len(), 1);
    await_consistency_s(30, [&cell]).await.unwrap();
    wait_for_own_content_id_count(&conductor, &zome, hive.clone(), existing_id, 1).await;

    assert!(
        probe_content_id_exists(&conductor, &zome, hive.clone(), existing_id).await
    );
    assert!(!probe_content_id_exists(&conductor, &zome, hive, missing_id).await);
}

#[tokio::test(flavor = "multi_thread")]
async fn dynamic_links_batch_rejects_over_budget() {
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "dynamic-budget-hive").await;
    let dynamic_links = (0..45)
        .map(|index| format!("budget-label-{index}"))
        .collect();
    let rejected: Result<Vec<DynamicLinkBatchBucket>, _> = conductor
        .call_fallible(
            &zome,
            "list_encrypted_content_by_dynamic_links",
            DynamicLinksBatchInput {
                hive_genesis_hash: hive,
                content_type: "dynamic-budget-type".to_string(),
                dynamic_links,
                since_ts: None,
                limit: Some(100),
                include_liveness: false,
            },
        )
        .await;
    assert_reject_contains(rejected, BATCH_BUDGET_REJECT);
}

#[tokio::test(flavor = "multi_thread")]
async fn hive_links_many_rejects_over_budget() {
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "hive-links-budget-hive").await;
    let requests = (0..32)
        .map(|index| HiveLinkBatchRequest {
            content_type: format!("budget-type-{index}"),
            since_ts: None,
            limit: Some(150),
            include_liveness: false,
        })
        .collect();
    let rejected: Result<Vec<HiveLinkBatchBucket>, _> = conductor
        .call_fallible(
            &zome,
            "list_by_hive_links_many",
            HiveLinksBatchInput {
                hive_genesis_hash: hive,
                requests,
            },
        )
        .await;
    assert_reject_contains(rejected, BATCH_BUDGET_REJECT);
}

#[tokio::test(flavor = "multi_thread")]
async fn author_batch_rejects_over_budget() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().clone();
    let lookups: Vec<_> = (0..45)
        .map(|index| AuthorContentLookupInput {
            author: author.clone(),
            content_type: format!("budget-type-{index}"),
            limit: Some(100),
        })
        .collect();
    let rejected: Result<Vec<AuthorBatchBucket>, _> = conductor
        .call_fallible(&zome, "list_by_author_many", lookups)
        .await;
    assert_reject_contains(rejected, BATCH_BUDGET_REJECT);
}
