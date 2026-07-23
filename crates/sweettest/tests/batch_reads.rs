//! Conductor proofs for the coordinator batch/local read paths. M17 seeds the
//! `list_my_groups` per-grant multiplicity guard for the P5 GroupGenesis cache;
//! M19/M20 grow this binary with the batch-extern proofs.

mod support;

use holochain::sweettest::await_consistency_s;
use holo_hash::ActionHash;
use serde::Deserialize;
use support::{
    create_hive, grant_hive_membership, setup_cells, CreateGroupGenesisInput,
    CreateGroupMembershipInput, GenesisResponse, MembershipResponse,
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
const MEMBERSHIP_BATCH_MAX: usize = 64;
const GROUP_MEMBERS_BATCH_MAX: usize = 64;
const MEMBERSHIP_BATCH_REJECT: &str = "membership batch accepts at most 64 hives";
const GROUP_MEMBERS_BATCH_REJECT: &str =
    "group-members batch accepts at most 64 groups";

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

#[derive(Debug, Serialize)]
struct LatestMembershipsLocalManyInput {
    hive_genesis_hashes: Vec<ActionHash>,
}

#[derive(Debug, Serialize)]
struct GetLatestMembershipInputMirror {
    agent: AgentPubKey,
    hive_genesis_hash: ActionHash,
}

#[derive(Debug, Deserialize)]
struct MembershipMirror {
    hash: ActionHash,
}

#[derive(Debug, Deserialize)]
struct MembershipBatchBucket {
    hive_genesis_hash: ActionHash,
    #[serde(default)]
    membership: Option<MembershipMirror>,
}

#[derive(Debug, Deserialize)]
struct MemberMirror {
    hash: ActionHash,
}

#[derive(Debug, Deserialize)]
struct GroupMembersBatchBucket {
    group_genesis_hash: ActionHash,
    members: Vec<MemberMirror>,
}

#[derive(Debug, Serialize)]
struct HiveLinkPageInputMirror {
    hive_genesis_hash: ActionHash,
    content_type: String,
    since_ts: Option<Timestamp>,
    limit: Option<usize>,
    source_after_action_hash: Option<String>,
    include_liveness: bool,
}

#[derive(Debug, Deserialize)]
struct BoundedLinkPageMirror {
    records: Vec<RecordMirror>,
    truncated: bool,
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

async fn create_group(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: &ActionHash,
    display_id: &str,
) -> ActionHash {
    let response: GenesisResponse = conductor
        .call(
            zome,
            "create_group_genesis",
            CreateGroupGenesisInput {
                hive_genesis_hash: hive_genesis_hash.clone(),
                display_id: display_id.to_string(),
                hive_wide_role: None,
                creator_hive_membership_hash: None,
            },
        )
        .await;
    response.hash
}

async fn grant_group_membership(
    conductor: &SweetConductor,
    zome: &SweetZome,
    group_genesis_hash: ActionHash,
    for_agent: AgentPubKey,
    role: &str,
    expiry: Option<i64>,
) -> ActionHash {
    let response: MembershipResponse = conductor
        .call(
            zome,
            "create_group_membership",
            CreateGroupMembershipInput {
                group_genesis_hash,
                for_agent,
                role: role.to_string(),
                grantor_membership_hash: None,
                grantor_hive_membership_hash: None,
                expiry,
            },
        )
        .await;
    response.hash
}

async fn grant_hive_role(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: ActionHash,
    for_agent: AgentPubKey,
    role: &str,
    expiry: Option<i64>,
) -> ActionHash {
    grant_hive_membership(
        conductor,
        zome,
        hive_genesis_hash,
        for_agent,
        role,
        None,
        expiry,
        None,
    )
    .await
}

async fn create_membership_batch_fixture(
    conductor: &SweetConductor,
    zome: &SweetZome,
    bob: &AgentPubKey,
) -> (ActionHash, ActionHash, ActionHash, ActionHash) {
    let hive_a = create_hive(conductor, zome, "membership-batch-a").await;
    let hive_b = create_hive(conductor, zome, "membership-batch-b").await;
    let hive_c = create_hive(conductor, zome, "membership-batch-c").await;

    let _reader_hash =
        grant_hive_role(conductor, zome, hive_a.clone(), bob.clone(), "Reader", None).await;
    let writer_hash =
        grant_hive_role(conductor, zome, hive_a.clone(), bob.clone(), "Writer", None).await;
    let _expired_hash =
        grant_hive_role(conductor, zome, hive_b.clone(), bob.clone(), "Reader", Some(1)).await;

    (hive_a, hive_b, hive_c, writer_hash)
}

async fn latest_memberships_local_many(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hashes: Vec<ActionHash>,
) -> Vec<MembershipBatchBucket> {
    conductor
        .call(
            zome,
            "get_latest_memberships_local_many",
            LatestMembershipsLocalManyInput {
                hive_genesis_hashes,
            },
        )
        .await
}

async fn latest_membership_local(
    conductor: &SweetConductor,
    zome: &SweetZome,
    agent: &AgentPubKey,
    hive_genesis_hash: ActionHash,
) -> Option<MembershipMirror> {
    conductor
        .call(
            zome,
            "get_latest_membership_local",
            GetLatestMembershipInputMirror {
                agent: agent.clone(),
                hive_genesis_hash,
            },
        )
        .await
}

fn assert_membership_parity(
    bucket: &MembershipBatchBucket,
    expected_hive: &ActionHash,
    singleton: &Option<MembershipMirror>,
) {
    assert_eq!(&bucket.hive_genesis_hash, expected_hive);
    assert_eq!(
        bucket.membership.as_ref().map(|membership| &membership.hash),
        singleton.as_ref().map(|membership| &membership.hash)
    );
}

async fn hive_link_page(
    conductor: &SweetConductor,
    zome: &SweetZome,
    function_name: &str,
    hive_genesis_hash: ActionHash,
    content_type: &str,
    limit: usize,
) -> BoundedLinkPageMirror {
    conductor
        .call(
            zome,
            function_name,
            HiveLinkPageInputMirror {
                hive_genesis_hash,
                content_type: content_type.to_string(),
                since_ts: None,
                limit: Some(limit),
                source_after_action_hash: None,
                include_liveness: false,
            },
        )
        .await
}

fn sorted_record_hashes(page: &BoundedLinkPageMirror) -> Vec<String> {
    let mut hashes: Vec<String> = page
        .records
        .iter()
        .map(|record| record.hash.clone())
        .collect();
    hashes.sort();
    hashes
}

async fn create_local_group_fixture(
    conductor: &SweetConductor,
    zome: &SweetZome,
    bob: &AgentPubKey,
) -> (ActionHash, ActionHash) {
    let hive = create_hive(conductor, zome, "local-groups-hive").await;
    let granted_group = create_group(conductor, zome, &hive, "local-granted-group").await;
    let expired_group = create_group(conductor, zome, &hive, "local-expired-group").await;

    let _active_hash = grant_group_membership(
        conductor,
        zome,
        granted_group.clone(),
        bob.clone(),
        "Reader",
        None,
    )
    .await;
    let _expired_hash = grant_group_membership(
        conductor,
        zome,
        expired_group.clone(),
        bob.clone(),
        "Reader",
        Some(1),
    )
    .await;

    (granted_group, expired_group)
}

async fn create_hive_link_page_fixture(
    conductor: &SweetConductor,
    zome: &SweetZome,
    content_type: &str,
    other_content_type: &str,
) -> (ActionHash, Vec<String>, String) {
    let hive = create_hive(conductor, zome, "local-page-hive").await;
    let created = create_contents(
        conductor,
        zome,
        &hive,
        content_type,
        &["local-page-1", "local-page-2", "local-page-3"],
        None,
    )
    .await;
    let mut other =
        create_contents(conductor, zome, &hive, other_content_type, &["other-type-1"], None)
            .await;
    let other_hash = other.pop().expect("one different-type fixture record");
    (hive, created, other_hash)
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

#[tokio::test(flavor = "multi_thread")]
async fn local_membership_batch_matches_singleton_per_hive() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_zome = alice.zome("content");
    let bob_zome = bob.zome("content");
    let bob_pubkey = bob.agent_pubkey().clone();
    await_consistency_s(30, [alice, bob]).await.unwrap();

    let (hive_a, hive_b, hive_c, writer_hash) =
        create_membership_batch_fixture(&conductors[0], &alice_zome, &bob_pubkey).await;
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let buckets = latest_memberships_local_many(
        &conductors[1],
        &bob_zome,
        vec![hive_a.clone(), hive_b.clone(), hive_a.clone(), hive_c.clone()],
    )
    .await;
    assert_eq!(buckets.len(), 4);

    let singleton_a =
        latest_membership_local(&conductors[1], &bob_zome, &bob_pubkey, hive_a.clone()).await;
    let singleton_b =
        latest_membership_local(&conductors[1], &bob_zome, &bob_pubkey, hive_b.clone()).await;
    let singleton_c =
        latest_membership_local(&conductors[1], &bob_zome, &bob_pubkey, hive_c.clone()).await;
    assert_eq!(
        singleton_a.as_ref().map(|membership| &membership.hash),
        Some(&writer_hash)
    );
    assert!(singleton_b.is_none(), "the expired hive-B grant must be filtered");
    assert!(singleton_c.is_none(), "an ungranted hive must remain absent");

    assert_membership_parity(&buckets[0], &hive_a, &singleton_a);
    assert_membership_parity(&buckets[1], &hive_b, &singleton_b);
    assert_membership_parity(&buckets[2], &hive_a, &singleton_a);
    assert_membership_parity(&buckets[3], &hive_c, &singleton_c);
}

#[tokio::test(flavor = "multi_thread")]
async fn local_membership_batch_rejects_over_64_hives() {
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "membership-bound-hive").await;
    let rejected: Result<Vec<MembershipBatchBucket>, _> = conductor
        .call_fallible(
            &zome,
            "get_latest_memberships_local_many",
            LatestMembershipsLocalManyInput {
                hive_genesis_hashes: vec![hive; MEMBERSHIP_BATCH_MAX + 1],
            },
        )
        .await;
    assert_reject_contains(rejected, MEMBERSHIP_BATCH_REJECT);
}

#[tokio::test(flavor = "multi_thread")]
async fn group_members_many_returns_complete_rosters_in_order() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_zome = alice.zome("content");
    let bob_pubkey = bob.agent_pubkey().clone();
    await_consistency_s(30, [alice, bob]).await.unwrap();

    let hive = create_hive(&conductors[0], &alice_zome, "group-members-batch-hive").await;
    let group1 = create_group(&conductors[0], &alice_zome, &hive, "roster-group-1").await;
    let group2 = create_group(&conductors[0], &alice_zome, &hive, "roster-group-2").await;
    let _reader_hash = grant_group_membership(
        &conductors[0],
        &alice_zome,
        group1.clone(),
        bob_pubkey.clone(),
        "Reader",
        None,
    )
    .await;
    let writer_hash = grant_group_membership(
        &conductors[0],
        &alice_zome,
        group1.clone(),
        bob_pubkey,
        "Writer",
        None,
    )
    .await;
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let buckets: Vec<GroupMembersBatchBucket> = conductors[0]
        .call(
            &alice_zome,
            "list_group_members_many",
            vec![group1.clone(), group2.clone()],
        )
        .await;
    assert_eq!(
        buckets
            .iter()
            .map(|bucket| bucket.group_genesis_hash.clone())
            .collect::<Vec<_>>(),
        vec![group1, group2]
    );
    assert_eq!(buckets[0].members.len(), 1);
    assert_eq!(buckets[0].members[0].hash, writer_hash);
    assert!(buckets[1].members.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn group_members_many_rejects_over_64_groups() {
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "group-members-bound-hive").await;
    let group = create_group(&conductor, &zome, &hive, "group-members-bound-group").await;
    let rejected: Result<Vec<GroupMembersBatchBucket>, _> = conductor
        .call_fallible(
            &zome,
            "list_group_members_many",
            vec![group; GROUP_MEMBERS_BATCH_MAX + 1],
        )
        .await;
    assert_reject_contains(rejected, GROUP_MEMBERS_BATCH_REJECT);
}

#[tokio::test(flavor = "multi_thread")]
async fn list_my_groups_local_lists_founded_and_granted() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_zome = alice.zome("content");
    let bob_zome = bob.zome("content");
    let bob_pubkey = bob.agent_pubkey().clone();
    await_consistency_s(30, [alice, bob]).await.unwrap();

    let (granted_group, expired_group) =
        create_local_group_fixture(&conductors[0], &alice_zome, &bob_pubkey).await;
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let alice_rows: Vec<ListedGroupRow> = conductors[0]
        .call(&alice_zome, "list_my_groups_local", ())
        .await;
    let bob_rows: Vec<ListedGroupRow> = conductors[1]
        .call(&bob_zome, "list_my_groups_local", ())
        .await;

    let founded = alice_rows
        .iter()
        .find(|row| row.group_genesis_hash == granted_group)
        .expect("alice's locally founded group");
    assert!(founded.role.is_none());
    let granted = bob_rows
        .iter()
        .find(|row| row.group_genesis_hash == granted_group)
        .expect("bob's locally integrated group grant");
    assert_eq!(granted.role.as_deref(), Some("Reader"));
    assert!(
        bob_rows
            .iter()
            .all(|row| row.group_genesis_hash != expired_group),
        "an expired grant must not appear in the local group listing: {bob_rows:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hive_link_local_page_returns_own_content() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let content_type = "local-page-type";
    let other_content_type = "local-page-other-type";
    let (hive, mut expected_hashes, other_hash) =
        create_hive_link_page_fixture(&conductor, &zome, content_type, other_content_type).await;
    await_consistency_s(30, [&cell]).await.unwrap();
    wait_for_count_links_by_hive_to(&conductor, &zome, &hive, content_type, 3).await;
    wait_for_count_links_by_hive_to(&conductor, &zome, &hive, other_content_type, 1).await;

    let bounded =
        hive_link_page(&conductor, &zome, "list_by_hive_link_local_page", hive.clone(), content_type, 2)
            .await;
    assert_eq!(bounded.records.len(), 2);
    assert!(bounded.truncated);

    let complete =
        hive_link_page(&conductor, &zome, "list_by_hive_link_local_page", hive.clone(), content_type, 5)
            .await;
    assert_eq!(complete.records.len(), 3);
    assert!(!complete.truncated);

    let network =
        hive_link_page(&conductor, &zome, "list_by_hive_link_page", hive, content_type, 5).await;
    assert_eq!(network.records.len(), 3);
    assert!(!network.truncated);

    expected_hashes.sort();
    let local_hashes = sorted_record_hashes(&complete);
    let network_hashes = sorted_record_hashes(&network);
    assert_eq!(local_hashes, expected_hashes);
    assert_eq!(network_hashes, expected_hashes);
    assert!(!local_hashes.contains(&other_hash));
}
