//! Conductor behavior proof for the pass-6-idempotent-writes coordinator
//! generation: the find-or-create family, hiveless-content remediation,
//! optional-hive `fetch_pair_ss_with_hive_check`, HiveGenesis migration
//! markers, and `content_summary_many` — on a real in-process Holochain
//! 0.6.1 conductor against the HELD pass-6 DNA.

mod support;

use std::time::Duration;

use holo_hash::{ActionHash, AgentPubKey};
use holochain::sweettest::{await_consistency_s, SweetConductor, SweetZome};
use holochain_types::prelude::UnsafeBytes;
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use support::{
    create_hive, create_open_write_content, owner_only_acl, setup_cells, single_conductor_cell_app,
    wait_for_count_links_by_hive_to, wait_for_own_content_id_count, AclSpec,
    CreateEncryptedContentInput, CreateResponse, GenesisResponse, MembershipResponse,
};

const SS_CONTENT_TYPE: &str = "hummhive-elemental-secrets-v1";
const POLL_ATTEMPTS: usize = 600;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// --- Wire mirrors (field names copied verbatim from the coordinator) --------

#[derive(Debug, Deserialize)]
struct FindOrCreateContentResponse {
    response: CreateResponse,
    was_created: bool,
}

#[derive(Debug, Deserialize)]
struct FindOrCreateGenesisResponse {
    response: GenesisResponse,
    was_created: bool,
}

#[derive(Debug, Deserialize)]
struct FindOrCreateMembershipResponse {
    response: MembershipResponse,
    was_created: bool,
}

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
struct GetLatestGroupMembershipInput {
    agent: AgentPubKey,
    group_genesis_hash: ActionHash,
}

#[derive(Debug, Serialize)]
struct RemediateHivelessItem {
    original_action_hash: ActionHash,
    corrected: CreateEncryptedContentInput,
}

#[derive(Debug, Serialize)]
struct RemediateHivelessInput {
    items: Vec<RemediateHivelessItem>,
}

#[derive(Debug, Deserialize)]
struct RemediationOutcome {
    original_hash: String,
    status: String,
    new_hash: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
struct FetchPairInput {
    author: String,
    active_hive_genesis_hash: Option<ActionHash>,
    content_type: String,
    group_id: String,
}

/// `EncryptedContentResponse` subset for list results.
#[derive(Debug, Deserialize)]
struct ListedRecord {
    hash: String,
}

#[derive(Debug, Serialize, Clone)]
struct ContentSummaryInput {
    hive_genesis_hash: ActionHash,
    content_types: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ContentTypeSummary {
    content_type: String,
    count: usize,
}

#[derive(Debug, Deserialize)]
struct HiveContentSummary {
    hive_genesis_hash: ActionHash,
    summaries: Vec<ContentTypeSummary>,
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

fn hive_identity_marker(new_dna: &str) -> MigrationMarkerV2 {
    MigrationMarkerV2 {
        schema_tag: "humm-earth-core-happ/migration-marker".to_string(),
        schema_version: 2,
        new_dna_hash_base64: new_dna.to_string(),
        new_action_hash_base64: "uhCkkNEWGENESIS".to_string(),
        new_app_id: "humm-earth-core@next".to_string(),
        migrated_at_microseconds: 1_700_000_000_000_000,
        new_hive_genesis_hash_base64: Some("uhCkkNEWGENESIS".to_string()),
        new_hive_genesis_display_id: Some("marker-hive".to_string()),
    }
}

#[derive(Debug, Deserialize)]
enum MigrationMarker {
    V1(IgnoredAny),
    V2(MigrationMarkerV2Read),
}

#[derive(Debug, Deserialize)]
struct MigrationMarkerV2Read {
    schema_tag: String,
    schema_version: u32,
    new_dna_hash_base64: String,
    new_hive_genesis_hash_base64: Option<String>,
}

// --- Fixtures & polls ---------------------------------------------------------

fn content_input(
    hive: Option<ActionHash>,
    author: &AgentPubKey,
    content_type: &str,
    id: &str,
    bytes: Vec<u8>,
    dynamic_links: Option<Vec<String>>,
) -> CreateEncryptedContentInput {
    CreateEncryptedContentInput {
        id: id.to_string(),
        display_hive_id: "sweettest-hive".to_string(),
        content_type: content_type.to_string(),
        revision_author_signing_public_key: author.to_string(),
        bytes: UnsafeBytes::from(bytes).into(),
        acl_spec: AclSpec::OpenWrite {
            target_hive_genesis_hash: hive,
        },
        public_key_acl: owner_only_acl(&author.to_string()),
        dynamic_links,
    }
}

async fn fetch_pair(
    conductor: &SweetConductor,
    zome: &SweetZome,
    author: &AgentPubKey,
    active_hive: Option<ActionHash>,
    group_id: &str,
) -> Vec<ListedRecord> {
    conductor
        .call(
            zome,
            "fetch_pair_ss_with_hive_check",
            FetchPairInput {
                author: author.to_string(),
                active_hive_genesis_hash: active_hive,
                content_type: SS_CONTENT_TYPE.to_string(),
                group_id: group_id.to_string(),
            },
        )
        .await
}

async fn wait_for_fetch_pair_count(
    conductor: &SweetConductor,
    zome: &SweetZome,
    author: &AgentPubKey,
    active_hive: Option<ActionHash>,
    group_id: &str,
    expected: usize,
) -> Vec<ListedRecord> {
    for _ in 0..POLL_ATTEMPTS {
        let found = fetch_pair(conductor, zome, author, active_hive.clone(), group_id).await;
        if found.len() == expected {
            return found;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    panic!("fetch_pair_ss_with_hive_check never reached {expected} records for {group_id}");
}

// --- Tests ---------------------------------------------------------------------

/// Same input twice ⇒ one entry; the second call finds instead of
/// creating, and a diverging retry (same id, new bytes) still returns
/// the original (find wins).
#[tokio::test(flavor = "multi_thread")]
async fn find_or_create_content_is_idempotent() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().clone();
    let hive = create_hive(&conductor, &zome, "idem-hive").await;

    let input = || {
        content_input(
            Some(hive.clone()),
            &author,
            "sweettest-idem",
            "resume-1",
            vec![42u8; 8],
            None,
        )
    };

    let first: FindOrCreateContentResponse = conductor
        .call(&zome, "find_or_create_encrypted_content", input())
        .await;
    assert!(first.was_created, "fresh id must create");

    wait_for_own_content_id_count(&conductor, &zome, hive.clone(), "resume-1", 1).await;

    let second: FindOrCreateContentResponse = conductor
        .call(&zome, "find_or_create_encrypted_content", input())
        .await;
    assert!(!second.was_created, "identical retry must find");
    assert_eq!(second.response.hash, first.response.hash);

    let diverged: FindOrCreateContentResponse = conductor
        .call(
            &zome,
            "find_or_create_encrypted_content",
            content_input(
                Some(hive.clone()),
                &author,
                "sweettest-idem",
                "resume-1",
                vec![7u8; 8],
                None,
            ),
        )
        .await;
    assert!(!diverged.was_created, "find wins over diverging content");
    assert_eq!(diverged.response.hash, first.response.hash);
}

#[tokio::test(flavor = "multi_thread")]
async fn find_or_create_content_requires_hive_context() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().clone();

    let rejected: Result<FindOrCreateContentResponse, _> = conductor
        .call_fallible(
            &zome,
            "find_or_create_encrypted_content",
            content_input(None, &author, "sweettest-idem", "hiveless-1", vec![1], None),
        )
        .await;
    let err = format!(
        "{:?}",
        rejected.expect_err("hiveless input must be rejected")
    );
    assert!(
        err.contains("requires a hive-scoped acl_spec"),
        "unexpected error: {err}"
    );
}

/// Genesis: role-keyed singleton for system role groups. Membership:
/// keyed on (group, agent, role); a role change is a new grant.
#[tokio::test(flavor = "multi_thread")]
async fn find_or_create_group_genesis_and_membership_idempotent() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_zome = alice.zome("content");
    let bob_pubkey = bob.agent_pubkey().clone();
    await_consistency_s(30, [alice, bob]).await.unwrap();

    let hive = create_hive(&conductors[0], &alice_zome, "group-idem-hive").await;

    let genesis_input = || CreateGroupGenesisInput {
        hive_genesis_hash: hive.clone(),
        display_id: "admin-group".to_string(),
        hive_wide_role: Some("Admin".to_string()),
        creator_hive_membership_hash: None,
    };
    let first: FindOrCreateGenesisResponse = conductors[0]
        .call(&alice_zome, "find_or_create_group_genesis", genesis_input())
        .await;
    assert!(first.was_created);

    wait_for_group_visible(&conductors[0], &alice_zome, &hive, &first.response.hash).await;

    let second: FindOrCreateGenesisResponse = conductors[0]
        .call(&alice_zome, "find_or_create_group_genesis", genesis_input())
        .await;
    assert!(!second.was_created, "same role group must be found");
    assert_eq!(second.response.hash, first.response.hash);

    let group = first.response.hash;
    let membership_input = |role: &str| CreateGroupMembershipInput {
        group_genesis_hash: group.clone(),
        for_agent: bob_pubkey.clone(),
        role: role.to_string(),
        grantor_membership_hash: None,
        grantor_hive_membership_hash: None,
        expiry: None,
    };
    let granted: FindOrCreateMembershipResponse = conductors[0]
        .call(
            &alice_zome,
            "find_or_create_group_membership",
            membership_input("Reader"),
        )
        .await;
    assert!(granted.was_created);

    wait_for_membership_visible(&conductors[0], &alice_zome, &group, &bob_pubkey).await;

    let regranted: FindOrCreateMembershipResponse = conductors[0]
        .call(
            &alice_zome,
            "find_or_create_group_membership",
            membership_input("Reader"),
        )
        .await;
    assert!(!regranted.was_created, "same-role regrant must be found");
    assert_eq!(regranted.response.hash, granted.response.hash);

    let role_change: FindOrCreateMembershipResponse = conductors[0]
        .call(
            &alice_zome,
            "find_or_create_group_membership",
            membership_input("Writer"),
        )
        .await;
    assert!(
        role_change.was_created,
        "a different role is a legitimate new grant"
    );
}

#[derive(Debug, Deserialize)]
struct ListedGroup {
    group_genesis_hash: ActionHash,
}

async fn wait_for_group_visible(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive: &ActionHash,
    group: &ActionHash,
) {
    for _ in 0..POLL_ATTEMPTS {
        let groups: Vec<ListedGroup> = conductor
            .call(zome, "list_groups_in_hive", hive.clone())
            .await;
        if groups.iter().any(|g| &g.group_genesis_hash == group) {
            return;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    panic!("group {group} never became visible in hive listing");
}

async fn wait_for_membership_visible(
    conductor: &SweetConductor,
    zome: &SweetZome,
    group: &ActionHash,
    agent: &AgentPubKey,
) {
    for _ in 0..POLL_ATTEMPTS {
        let latest: Option<MembershipResponse> = conductor
            .call(
                zome,
                "get_latest_group_membership",
                GetLatestGroupMembershipInput {
                    agent: agent.clone(),
                    group_genesis_hash: group.clone(),
                },
            )
            .await;
        if latest.is_some() {
            return;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    panic!("membership for {agent} in {group} never became visible");
}

/// THE 02_A C4 proof: a legacy hiveless entry is invisible to
/// `fetch_pair_ss_with_hive_check` until remediation recreates it with
/// the full discovery-link bundle; the original is tombstoned and a
/// re-run reports `skipped_already_remediated`.
#[tokio::test(flavor = "multi_thread")]
async fn remediate_hiveless_recreates_deletes_and_skips() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().clone();
    let hive = create_hive(&conductor, &zome, "remediation-hive").await;

    let legacy: CreateResponse = conductor
        .call(
            &zome,
            "create_encrypted_content",
            content_input(None, &author, SS_CONTENT_TYPE, "ss-1", vec![5u8; 16], None),
        )
        .await;
    let legacy_hash =
        ActionHash::try_from(legacy.hash.as_str()).expect("create hash parses as ActionHash");

    let hiveless = wait_for_hiveless_count(&conductor, &zome, 1).await;
    assert_eq!(hiveless[0].hash, legacy.hash);

    let invisible = fetch_pair(&conductor, &zome, &author, Some(hive.clone()), "group-1").await;
    assert!(
        invisible.is_empty(),
        "hiveless legacy entry must be invisible to the C4 intersection"
    );

    let corrected = || {
        content_input(
            Some(hive.clone()),
            &author,
            SS_CONTENT_TYPE,
            "ss-1",
            vec![5u8; 16],
            Some(vec!["group-1".to_string()]),
        )
    };
    let outcomes: Vec<RemediationOutcome> = conductor
        .call(
            &zome,
            "remediate_hiveless_content",
            RemediateHivelessInput {
                items: vec![RemediateHivelessItem {
                    original_action_hash: legacy_hash.clone(),
                    corrected: corrected(),
                }],
            },
        )
        .await;
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0].status, "recreated",
        "unexpected outcome: {outcomes:?}"
    );
    assert_eq!(outcomes[0].original_hash, legacy.hash);
    let new_hash = outcomes[0]
        .new_hash
        .clone()
        .expect("recreated outcome carries the new hash");
    assert_ne!(new_hash, legacy.hash);

    let now_visible =
        wait_for_fetch_pair_count(&conductor, &zome, &author, Some(hive.clone()), "group-1", 1)
            .await;
    assert_eq!(now_visible[0].hash, new_hash);

    wait_for_hiveless_count(&conductor, &zome, 0).await;
    wait_for_own_content_id_count(&conductor, &zome, hive.clone(), "ss-1", 1).await;

    let rerun: Vec<RemediationOutcome> = conductor
        .call(
            &zome,
            "remediate_hiveless_content",
            RemediateHivelessInput {
                items: vec![
                    RemediateHivelessItem {
                        original_action_hash: ActionHash::from_raw_36(vec![9u8; 36]),
                        corrected: content_input(
                            None,
                            &author,
                            SS_CONTENT_TYPE,
                            "ss-bad",
                            vec![6u8; 4],
                            None,
                        ),
                    },
                    RemediateHivelessItem {
                        original_action_hash: legacy_hash.clone(),
                        corrected: corrected(),
                    },
                ],
            },
        )
        .await;
    assert_eq!(rerun.len(), 2, "one outcome per item, order-preserving");
    assert_eq!(rerun[0].status, "failed");
    assert!(
        rerun[0]
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("lacks hive context"),
        "unexpected detail: {:?}",
        rerun[0].detail
    );
    assert_eq!(rerun[1].status, "skipped_already_remediated");
    assert_eq!(rerun[1].new_hash.as_deref(), Some(new_hash.as_str()));
}

async fn wait_for_hiveless_count(
    conductor: &SweetConductor,
    zome: &SweetZome,
    expected: usize,
) -> Vec<ListedRecord> {
    for _ in 0..POLL_ATTEMPTS {
        let listed: Vec<ListedRecord> = conductor
            .call(
                zome,
                "list_my_hiveless_content",
                SS_CONTENT_TYPE.to_string(),
            )
            .await;
        if listed.len() == expected {
            return listed;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    panic!("list_my_hiveless_content never reached {expected} records");
}

/// `None` = bounded union over the callee's own hives; `Some` keeps
/// exact single-hive scoping.
#[tokio::test(flavor = "multi_thread")]
async fn fetch_pair_none_hive_unions_across_hives() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().clone();
    let hive1 = create_hive(&conductor, &zome, "union-hive-1").await;
    let hive2 = create_hive(&conductor, &zome, "union-hive-2").await;

    let e1 = create_open_write_content(
        &conductor,
        &zome,
        hive1.clone(),
        SS_CONTENT_TYPE,
        "pair-1",
        Some(vec!["g1".to_string()]),
    )
    .await;
    let e2 = create_open_write_content(
        &conductor,
        &zome,
        hive2.clone(),
        SS_CONTENT_TYPE,
        "pair-2",
        Some(vec!["g2".to_string()]),
    )
    .await;

    wait_for_fetch_pair_count(&conductor, &zome, &author, Some(hive1.clone()), "g1", 1).await;
    wait_for_fetch_pair_count(&conductor, &zome, &author, Some(hive2.clone()), "g2", 1).await;

    let union_g1 = fetch_pair(&conductor, &zome, &author, None, "g1").await;
    assert_eq!(union_g1.len(), 1);
    assert_eq!(union_g1[0].hash, e1);

    let union_g2 = fetch_pair(&conductor, &zome, &author, None, "g2").await;
    assert_eq!(union_g2.len(), 1);
    assert_eq!(union_g2[0].hash, e2);

    let cross_scoped = fetch_pair(&conductor, &zome, &author, Some(hive1), "g2").await;
    assert!(
        cross_scoped.is_empty(),
        "Some(hive) must keep exact single-hive scoping"
    );
}

/// Founder marks a HiveGenesis migrated via a create-based marker;
/// re-marking updates the single marker entry; non-founders are
/// rejected; the V1 reader never sees hive markers.
#[tokio::test(flavor = "multi_thread")]
async fn hive_genesis_marker_roundtrip() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_zome = alice.zome("content");
    let bob_zome = bob.zome("content");
    await_consistency_s(30, [alice, bob]).await.unwrap();

    let hive = create_hive(&conductors[0], &alice_zome, "marker-hive").await;

    let marked: Option<CreateResponse> = conductors[0]
        .call(
            &alice_zome,
            "mark_migrated_v2",
            MarkMigratedV2Input {
                original_action_hash: hive.clone(),
                marker: hive_identity_marker("uhC0kNEWDNA-A"),
            },
        )
        .await;
    marked.expect("founder mark must write a marker");

    let first_read =
        wait_for_hive_marker(&conductors[0], &alice_zome, &hive, "uhC0kNEWDNA-A").await;
    assert_eq!(first_read.schema_version, 2);
    assert_eq!(
        first_read.schema_tag,
        "humm-earth-core-happ/migration-marker"
    );
    assert_eq!(
        first_read.new_hive_genesis_hash_base64.as_deref(),
        Some("uhCkkNEWGENESIS")
    );

    wait_for_own_content_id_count(
        &conductors[0],
        &alice_zome,
        hive.clone(),
        "hive-migration-marker-v2",
        1,
    )
    .await;

    let remarked: Option<CreateResponse> = conductors[0]
        .call(
            &alice_zome,
            "mark_migrated_v2",
            MarkMigratedV2Input {
                original_action_hash: hive.clone(),
                marker: hive_identity_marker("uhC0kNEWDNA-B"),
            },
        )
        .await;
    remarked.expect("re-mark must update the marker");

    wait_for_hive_marker(&conductors[0], &alice_zome, &hive, "uhC0kNEWDNA-B").await;
    let own = wait_for_own_content_id_count(
        &conductors[0],
        &alice_zome,
        hive.clone(),
        "hive-migration-marker-v2",
        1,
    )
    .await;
    assert_eq!(
        own.records.len(),
        1,
        "re-mark must update the one marker entry, not add a second"
    );

    let v1_read: Option<IgnoredAny> = conductors[0]
        .call(&alice_zome, "get_migration_marker", hive.clone())
        .await;
    assert!(
        v1_read.is_none(),
        "V1 reader must structurally never see hive markers"
    );

    let group: GenesisResponse = conductors[0]
        .call(
            &alice_zome,
            "create_group_genesis",
            CreateGroupGenesisInput {
                hive_genesis_hash: hive.clone(),
                display_id: "not-a-hive".to_string(),
                hive_wide_role: None,
                creator_hive_membership_hash: None,
            },
        )
        .await;
    let group_marked: Result<Option<CreateResponse>, _> = conductors[0]
        .call_fallible(
            &alice_zome,
            "mark_migrated_v2",
            MarkMigratedV2Input {
                original_action_hash: group.hash,
                marker: hive_identity_marker("uhC0kNEWDNA-GROUP"),
            },
        )
        .await;
    let err = format!(
        "{:?}",
        group_marked.expect_err(
            "a GroupGenesis original must hit the explicit-Err arm, not the hive-marker branch"
        )
    );
    assert!(
        err.contains("must be an EncryptedContent or HiveGenesis entry"),
        "unexpected error: {err}"
    );

    assert_non_founder_mark_rejected(&conductors[1], &bob_zome, &hive).await;
}

async fn wait_for_hive_marker(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive: &ActionHash,
    expected_dna: &str,
) -> MigrationMarkerV2Read {
    for _ in 0..POLL_ATTEMPTS {
        let read: Option<MigrationMarker> = conductor
            .call(zome, "get_migration_marker_v2", hive.clone())
            .await;
        match read {
            Some(MigrationMarker::V2(v2)) if v2.new_dna_hash_base64 == expected_dna => {
                return v2;
            }
            Some(MigrationMarker::V1(_)) => panic!("hive marker must decode as V2"),
            _ => tokio::time::sleep(POLL_INTERVAL).await,
        }
    }
    panic!("hive marker for {expected_dna} never became readable");
}

/// A dormant-looking `Ok(None)` means bob's cell has not resolved the
/// foreign genesis yet — retry until the author gate actually fires.
async fn assert_non_founder_mark_rejected(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive: &ActionHash,
) {
    for _ in 0..POLL_ATTEMPTS {
        let attempt: Result<Option<CreateResponse>, _> = conductor
            .call_fallible(
                zome,
                "mark_migrated_v2",
                MarkMigratedV2Input {
                    original_action_hash: hive.clone(),
                    marker: hive_identity_marker("uhC0kNEWDNA-EVIL"),
                },
            )
            .await;
        match attempt {
            Err(err) => {
                let err = format!("{err:?}");
                assert!(
                    err.contains("only the hive founder"),
                    "unexpected rejection: {err}"
                );
                return;
            }
            Ok(Some(_)) => panic!("non-founder must never write a hive marker"),
            Ok(None) => tokio::time::sleep(POLL_INTERVAL).await,
        }
    }
    panic!("non-founder mark never resolved the foreign genesis");
}

/// Batch equals the per-hive singles, order-preserving; 33 hives is
/// over the cap.
#[tokio::test(flavor = "multi_thread")]
async fn content_summary_many_matches_singles() {
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive1 = create_hive(&conductor, &zome, "summary-hive-1").await;
    let hive2 = create_hive(&conductor, &zome, "summary-hive-2").await;

    create_open_write_content(&conductor, &zome, hive1.clone(), "ct-a", "s1", None).await;
    create_open_write_content(&conductor, &zome, hive1.clone(), "ct-a", "s2", None).await;
    create_open_write_content(&conductor, &zome, hive2.clone(), "ct-a", "s3", None).await;
    wait_for_count_links_by_hive_to(&conductor, &zome, &hive1, "ct-a", 2).await;
    wait_for_count_links_by_hive_to(&conductor, &zome, &hive2, "ct-a", 1).await;

    let summary_input = |hive: &ActionHash| ContentSummaryInput {
        hive_genesis_hash: hive.clone(),
        content_types: vec!["ct-a".to_string()],
    };
    let single1: Vec<ContentTypeSummary> = conductor
        .call(&zome, "content_summary", summary_input(&hive1))
        .await;
    let single2: Vec<ContentTypeSummary> = conductor
        .call(&zome, "content_summary", summary_input(&hive2))
        .await;

    let batched: Vec<HiveContentSummary> = conductor
        .call(
            &zome,
            "content_summary_many",
            vec![summary_input(&hive1), summary_input(&hive2)],
        )
        .await;
    assert_eq!(batched.len(), 2);
    assert_eq!(batched[0].hive_genesis_hash, hive1);
    assert_eq!(batched[1].hive_genesis_hash, hive2);
    assert_eq!(batched[0].summaries, single1);
    assert_eq!(batched[1].summaries, single2);
    assert_eq!(batched[0].summaries[0].count, 2);
    assert_eq!(batched[1].summaries[0].count, 1);

    let over_cap: Vec<ContentSummaryInput> = (0..33).map(|_| summary_input(&hive1)).collect();
    let rejected: Result<Vec<HiveContentSummary>, _> = conductor
        .call_fallible(&zome, "content_summary_many", over_cap)
        .await;
    let err = format!("{:?}", rejected.expect_err("33 hives must be rejected"));
    assert!(err.contains("at most 32 hives"), "unexpected error: {err}");
}
