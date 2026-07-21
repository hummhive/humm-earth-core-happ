//! Conductor proof for pass-7 M10 coordinator riders: idempotent
//! `delete_encrypted_content`, `list_by_acl_link` liveness parity, and
//! the paged `probe_inbox_page` (composite source cursor).

mod support;

use std::time::Duration;

use holo_hash::{ActionHash, AgentPubKey};
use holochain::sweettest::{SweetConductor, SweetZome};
use holochain_types::prelude::UnsafeBytes;
use serde::{Deserialize, Serialize};
use support::{
    create_hive, create_open_write_content, single_conductor_cell_app, Acl, AclByGroupGenesis,
    AclSpec, CreateEncryptedContentInput, CreateGroupGenesisInput, CreateResponse,
    DeleteContentResponse, GenesisResponse, SourcePosition,
};

const POLL_ATTEMPTS: usize = 200;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Serialize)]
struct ListByAclInput {
    hive_genesis_hash: ActionHash,
    content_type: String,
    acl_role: String,
    entity_id: String,
    include_liveness: bool,
}

#[derive(Debug, Deserialize)]
struct LivenessRecord {
    hash: String,
    #[serde(default)]
    tombstoned: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SendToInboxInput {
    recipient: AgentPubKey,
    target: ActionHash,
    event: String,
}

#[derive(Debug, Serialize)]
struct ProbeInboxPageInput {
    event_filter: Option<String>,
    since_ts: Option<i64>,
    limit: Option<usize>,
    source_after_action_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InboxItemRow {
    link_action_hash: ActionHash,
    event: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InboxPage {
    items: Vec<InboxItemRow>,
    source_count: usize,
    source_positions: Vec<SourcePosition>,
    truncated: bool,
}

async fn delete_content(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hash: &ActionHash,
) -> DeleteContentResponse {
    conductor
        .call(zome, "delete_encrypted_content", hash.clone())
        .await
}

/// HiveGroup content is the ONLY AclSpec that writes `HummContent*` links;
/// its ACL is keyed by GroupGenesis action hashes (`AclByGroupGenesis`).
/// Founds a custom group and commits one owner-only record into it.
async fn create_hive_group_content(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive: &ActionHash,
) -> (String, ActionHash) {
    let author = zome.cell_id().agent_pubkey().to_string();
    let group: GenesisResponse = conductor
        .call(
            zome,
            "create_group_genesis",
            CreateGroupGenesisInput {
                hive_genesis_hash: hive.clone(),
                display_id: "acl-liveness-group".to_string(),
                hive_wide_role: None,
                creator_hive_membership_hash: None,
            },
        )
        .await;
    let created: CreateResponse = conductor
        .call(
            zome,
            "create_encrypted_content",
            CreateEncryptedContentInput {
                id: "acl-1".to_string(),
                display_hive_id: "sweettest-hive".to_string(),
                content_type: "post".to_string(),
                revision_author_signing_public_key: author,
                bytes: UnsafeBytes::from(vec![9u8, 9]).into(),
                acl_spec: AclSpec::HiveGroup {
                    hive_genesis_hash: hive.clone(),
                    author_membership_hash: None,
                    group_acl: AclByGroupGenesis {
                        owner: group.hash.clone(),
                        admin: vec![],
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
            },
        )
        .await;
    (created.hash, group.hash)
}

async fn acl_owner_list(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive: &ActionHash,
    entity_id: &ActionHash,
    include_liveness: bool,
) -> Vec<LivenessRecord> {
    conductor
        .call(
            zome,
            "list_by_acl_link",
            ListByAclInput {
                hive_genesis_hash: hive.clone(),
                content_type: "post".to_string(),
                acl_role: "Owner".to_string(),
                entity_id: entity_id.to_string(),
                include_liveness,
            },
        )
        .await
}

async fn inbox_page(
    conductor: &SweetConductor,
    zome: &SweetZome,
    event_filter: Option<&str>,
    since_ts: Option<i64>,
    limit: Option<usize>,
    source_after_action_hash: Option<String>,
) -> InboxPage {
    conductor
        .call(
            zome,
            "probe_inbox_page",
            ProbeInboxPageInput {
                event_filter: event_filter.map(str::to_string),
                since_ts,
                limit,
                source_after_action_hash,
            },
        )
        .await
}

/// Seed `n` distinct contents and one self-Inbox DmCreate pointer at each.
async fn seed_dm_items(conductor: &SweetConductor, zome: &SweetZome, hive: &ActionHash, n: usize) {
    let me = zome.cell_id().agent_pubkey().clone();
    for i in 0..n {
        let created =
            create_open_write_content(conductor, zome, hive.clone(), "post", &format!("dm-{i}"), None)
                .await;
        let _: ActionHash = conductor
            .call(
                zome,
                "send_to_inbox",
                SendToInboxInput {
                    recipient: me.clone(),
                    target: ActionHash::try_from(created.as_str()).expect("hash parses"),
                    event: "DmCreate".to_string(),
                },
            )
            .await;
    }
}

fn last_cursor(page: &InboxPage) -> (Option<i64>, Option<String>) {
    let last = page
        .source_positions
        .last()
        .expect("page carries at least one source position");
    (Some(last.timestamp_micros), Some(last.action_hash.clone()))
}

#[tokio::test(flavor = "multi_thread")]
async fn double_delete_is_an_idempotent_noop() {
    holochain_trace::test_run();
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "double-delete-hive").await;
    let created = create_open_write_content(&conductor, &zome, hive.clone(), "post", "dd-1", None)
        .await;
    let hash = ActionHash::try_from(created.as_str()).expect("hash parses");

    let first = delete_content(&conductor, &zome, &hash).await;
    assert!(first.was_deleted, "existing target must be really deleted");
    assert!(
        first.delete_action_hash.is_some(),
        "real delete must surface its Delete action hash"
    );

    let second = delete_content(&conductor, &zome, &hash).await;
    assert!(
        !second.was_deleted,
        "re-delete of a tombstoned target must be a no-op success"
    );
    assert!(
        second.delete_action_hash.is_none(),
        "no-op delete must carry no action hash"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn acl_list_carries_the_liveness_rider() {
    holochain_trace::test_run();
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "acl-liveness-hive").await;
    let (created, group_hash) = create_hive_group_content(&conductor, &zome, &hive).await;

    let flag_off = poll_acl_len(&conductor, &zome, &hive, &group_hash, false, 1).await;
    assert_eq!(
        flag_off[0].tombstoned, None,
        "flag off must be byte-identical legacy shape (tombstoned absent)"
    );
    let flag_on = poll_acl_len(&conductor, &zome, &hive, &group_hash, true, 1).await;
    assert_eq!(
        flag_on[0].tombstoned,
        Some(false),
        "live root with the rider on must flag Some(false)"
    );
    assert_eq!(flag_on[0].hash, created, "the listed record is the created one");

    let deleted = delete_content(
        &conductor,
        &zome,
        &ActionHash::try_from(created.as_str()).expect("hash parses"),
    )
    .await;
    assert!(deleted.was_deleted);
    poll_acl_len(&conductor, &zome, &hive, &group_hash, true, 0).await;
}

/// Poll until the Owner ACL list returns exactly `expected` records.
async fn poll_acl_len(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive: &ActionHash,
    entity_id: &ActionHash,
    include_liveness: bool,
    expected: usize,
) -> Vec<LivenessRecord> {
    for _ in 0..POLL_ATTEMPTS {
        let records = acl_owner_list(conductor, zome, hive, entity_id, include_liveness).await;
        if records.len() == expected {
            return records;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    panic!("Owner ACL list never reached {expected} records");
}

#[tokio::test(flavor = "multi_thread")]
async fn paged_inbox_cursors_without_overlap_or_skips() {
    holochain_trace::test_run();
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "paged-inbox-hive").await;
    seed_dm_items(&conductor, &zome, &hive, 5).await;

    let mut seen: Vec<ActionHash> = Vec::new();
    let mut cursor: (Option<i64>, Option<String>) = (None, None);
    let mut pages = 0;
    loop {
        let page = inbox_page(
            &conductor,
            &zome,
            Some("DmCreate"),
            cursor.0,
            Some(2),
            cursor.1.clone(),
        )
        .await;
        assert_eq!(page.source_count, page.source_positions.len());
        assert_eq!(page.items.len(), page.source_positions.len());
        for item in &page.items {
            assert!(
                !seen.contains(&item.link_action_hash),
                "exclusive cursor must never re-deliver {:?}",
                item.link_action_hash
            );
            seen.push(item.link_action_hash.clone());
        }
        pages += 1;
        if !page.truncated {
            break;
        }
        assert_eq!(page.items.len(), 2, "truncated page must be exactly limit-sized");
        cursor = last_cursor(&page);
    }
    assert_eq!(seen.len(), 5, "every seeded DmCreate item paged out exactly once");
    assert_eq!(pages, 3, "5 items at limit 2 = 3 pages");
}

#[tokio::test(flavor = "multi_thread")]
async fn paged_inbox_filters_by_event_and_rejects_bad_cursors() {
    holochain_trace::test_run();
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "filtered-inbox-hive").await;
    seed_dm_items(&conductor, &zome, &hive, 2).await;

    let invites = inbox_page(&conductor, &zome, Some("HiveInvite"), None, None, None).await;
    assert_eq!(
        invites.items.len(),
        1,
        "create_hive writes exactly one self HiveInvite pointer"
    );
    assert!(invites
        .items
        .iter()
        .all(|i| i.event.as_deref() == Some("HiveInvite")));

    let unfiltered = inbox_page(&conductor, &zome, None, None, None, None).await;
    assert_eq!(unfiltered.items.len(), 3, "1 HiveInvite + 2 DmCreate");

    let orphan_cursor: Result<InboxPage, _> = conductor
        .call_fallible(
            &zome,
            "probe_inbox_page",
            ProbeInboxPageInput {
                event_filter: None,
                since_ts: None,
                limit: None,
                source_after_action_hash: Some(
                    unfiltered.source_positions[0].action_hash.clone(),
                ),
            },
        )
        .await;
    let err = format!("{:?}", orphan_cursor.expect_err("hash-only cursor must reject"));
    assert!(
        err.contains("source_after_action_hash requires since_ts"),
        "unexpected error: {err}"
    );

    let zero_limit: Result<InboxPage, _> = conductor
        .call_fallible(
            &zome,
            "probe_inbox_page",
            ProbeInboxPageInput {
                event_filter: None,
                since_ts: None,
                limit: Some(0),
                source_after_action_hash: None,
            },
        )
        .await;
    let err = format!("{:?}", zero_limit.expect_err("limit 0 must reject"));
    assert!(err.contains("limit must be >= 1"), "unexpected error: {err}");
}
