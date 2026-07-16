//! Conductor behavior proof for the pass-6-pinned-hosts coordinator
//! generation: bounded source-cursor paging, exact-own lookup,
//! `latest_action_micros` recency, and the BlobPinSignal dispatch path —
//! on a real in-process Holochain 0.6.1 conductor against the HELD
//! pass-6 DNA.

mod support;

use std::time::Duration;

use holo_hash::ActionHash;
use holochain::sweettest::{await_consistency_s, SweetConductor, SweetZome};
use holochain_types::prelude::{Signal, UnsafeBytes};
use holochain_zome_types::prelude::ExternIO;
use support::{
    create_hive, create_open_write_content, my_content, setup_cells, single_conductor_cell_app,
    wait_for_count_links_by_hive_to, wait_for_own_content_id_count, AclSpec, AuthorLinkPageInput,
    BlobPinHint, BlobPinSignal, BoundedLinkPage, ContentRecord, DynamicLinkPageInput,
    EncryptedContent, EncryptedContentHeader, HiveLinkPageInput, ListByHiveInput,
    SendBlobPinSignalInput, UpdateEncryptedContentInput,
};

const BLOB_PROVIDER_CONTENT_TYPE: &str = "hummhive-core-blob-provider-v1";

async fn hive_page(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: ActionHash,
    content_type: &str,
    since_ts: Option<i64>,
    limit: Option<usize>,
    source_after_action_hash: Option<String>,
) -> BoundedLinkPage {
    conductor
        .call(
            zome,
            "list_by_hive_link_page",
            HiveLinkPageInput {
                hive_genesis_hash,
                content_type: content_type.to_string(),
                since_ts,
                limit,
                source_after_action_hash,
            },
        )
        .await
}

fn cursor_of(page: &BoundedLinkPage) -> (Option<i64>, Option<String>) {
    let last = page
        .source_positions
        .last()
        .expect("truncated page carries at least one source position");
    (Some(last.timestamp_micros), Some(last.action_hash.clone()))
}

fn assert_page_invariants(page: &BoundedLinkPage) {
    assert_eq!(
        page.source_count,
        page.source_positions.len(),
        "source_count must equal source_positions length"
    );
    assert!(
        page.records.len() <= page.source_count,
        "records can only shrink relative to source positions"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hive_page_walks_multiple_pages_without_dupes_or_skips() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "pinned-hosts-hive").await;

    let mut created: Vec<String> = Vec::new();
    for i in 0..7 {
        created.push(
            create_open_write_content(
                &conductor,
                &zome,
                hive.clone(),
                BLOB_PROVIDER_CONTENT_TYPE,
                &format!("provider-{i}"),
                None,
            )
            .await,
        );
    }
    await_consistency_s(30, [&cell]).await.unwrap();
    wait_for_count_links_by_hive_to(&conductor, &zome, &hive, BLOB_PROVIDER_CONTENT_TYPE, 7).await;

    let page1 = hive_page(
        &conductor,
        &zome,
        hive.clone(),
        BLOB_PROVIDER_CONTENT_TYPE,
        None,
        Some(3),
        None,
    )
    .await;
    assert_page_invariants(&page1);
    assert_eq!(page1.records.len(), 3);
    assert!(page1.truncated);

    let (since2, after2) = cursor_of(&page1);
    let page2 = hive_page(
        &conductor,
        &zome,
        hive.clone(),
        BLOB_PROVIDER_CONTENT_TYPE,
        since2,
        Some(3),
        after2,
    )
    .await;
    assert_page_invariants(&page2);
    assert_eq!(page2.records.len(), 3);
    assert!(page2.truncated);

    let (since3, after3) = cursor_of(&page2);
    let page3 = hive_page(
        &conductor,
        &zome,
        hive,
        BLOB_PROVIDER_CONTENT_TYPE,
        since3,
        Some(3),
        after3,
    )
    .await;
    assert_page_invariants(&page3);
    assert_eq!(page3.records.len(), 1);
    assert!(!page3.truncated);

    let mut walked: Vec<String> = [&page1, &page2, &page3]
        .into_iter()
        .flat_map(|page| page.records.iter().map(|record| record.hash.clone()))
        .collect();
    let walked_count = walked.len();
    walked.sort();
    walked.dedup();
    assert_eq!(walked.len(), walked_count, "no duplicates across pages");

    let mut expected = created;
    expected.sort();
    assert_eq!(walked, expected, "walk must cover exactly the 7 entries");
}

/// A genuine position-without-record row is not constructible via public
/// externs (the delete sweep removes the source link with the entry;
/// only gossip lag produces it live) — so this pins the reachable half
/// of the poison-row contract: a deleted entry drops from BOTH records
/// and source positions, and the page stays a terminal, cursorable page.
#[tokio::test(flavor = "multi_thread")]
async fn deleted_entry_drops_from_page_records_and_positions() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "poison-row-hive").await;
    let content_type = "pinned-hosts-poison-type";

    let mut created: Vec<String> = Vec::new();
    for i in 0..3 {
        created.push(
            create_open_write_content(
                &conductor,
                &zome,
                hive.clone(),
                content_type,
                &format!("poison-{i}"),
                None,
            )
            .await,
        );
    }
    await_consistency_s(30, [&cell]).await.unwrap();
    wait_for_count_links_by_hive_to(&conductor, &zome, &hive, content_type, 3).await;

    let middle = ActionHash::try_from(created[1].as_str()).expect("hash parses");
    let _deleted: ActionHash = conductor
        .call(&zome, "delete_encrypted_content", middle.clone())
        .await;
    await_consistency_s(30, [&cell]).await.unwrap();
    wait_for_count_links_by_hive_to(&conductor, &zome, &hive, content_type, 2).await;
    wait_until_tombstoned(&conductor, &zome, middle).await;

    let page = hive_page(&conductor, &zome, hive, content_type, None, Some(10), None).await;
    assert_page_invariants(&page);
    assert!(!page.truncated, "single page must be terminal");
    assert_eq!(
        page.source_count, 2,
        "swept link must leave exactly two source rows"
    );
    let surviving: Vec<&String> = page.records.iter().map(|record| &record.hash).collect();
    assert!(
        !surviving.contains(&&created[1]),
        "tombstoned entry must not resolve into records"
    );
    assert!(
        surviving.contains(&&created[0]) && surviving.contains(&&created[2]),
        "live entries must survive the deleted row"
    );
}

async fn wait_until_tombstoned(conductor: &SweetConductor, zome: &SweetZome, hash: ActionHash) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        let resolved: Result<ContentRecord, _> = conductor
            .call_fallible(zome, "get_encrypted_content", hash.clone())
            .await;
        if let Err(err) = resolved {
            // Two valid tombstone classes: the cascade hides the record by
            // action hash ("no Record found…") or the entry decodes dead
            // ("Could not find…") — either proves the delete integrated.
            let message = format!("{err:?}");
            assert!(
                message.contains("no Record found at given hash")
                    || message.contains("Could not find the EncryptedContent"),
                "unexpected error class while waiting for tombstone: {message}"
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("entry {hash} still resolves 30s after delete");
}

#[tokio::test(flavor = "multi_thread")]
async fn dynamic_page_mirrors_hive_page_scoping() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "dynamic-page-hive").await;
    let content_type = "pinned-hosts-dynamic-type";

    let e1_first = create_open_write_content(
        &conductor,
        &zome,
        hive.clone(),
        content_type,
        "dyn-1",
        Some(vec!["e1".to_string()]),
    )
    .await;
    let e1_second = create_open_write_content(
        &conductor,
        &zome,
        hive.clone(),
        content_type,
        "dyn-2",
        Some(vec!["e1".to_string()]),
    )
    .await;
    let e2_only = create_open_write_content(
        &conductor,
        &zome,
        hive.clone(),
        content_type,
        "dyn-3",
        Some(vec!["e2".to_string()]),
    )
    .await;
    await_consistency_s(30, [&cell]).await.unwrap();
    wait_for_count_links_by_hive_to(&conductor, &zome, &hive, content_type, 3).await;

    let mut walked: Vec<String> = Vec::new();
    let mut since_ts = None;
    let mut after = None;
    for _ in 0..3 {
        let page: BoundedLinkPage = conductor
            .call(
                &zome,
                "list_by_dynamic_link_page",
                DynamicLinkPageInput {
                    hive_genesis_hash: hive.clone(),
                    content_type: content_type.to_string(),
                    dynamic_link: "e1".to_string(),
                    since_ts,
                    limit: Some(1),
                    source_after_action_hash: after.clone(),
                },
            )
            .await;
        assert_page_invariants(&page);
        walked.extend(page.records.iter().map(|record| record.hash.clone()));
        if !page.truncated {
            break;
        }
        (since_ts, after) = cursor_of(&page);
    }

    let mut expected = vec![e1_first, e1_second];
    expected.sort();
    walked.sort();
    assert_eq!(walked, expected, "e1 pages exactly its two entries");
    assert!(!walked.contains(&e2_only), "e2 entry must never appear");
}

#[tokio::test(flavor = "multi_thread")]
async fn exact_own_lookup_excludes_foreign_collisions_and_scopes_by_hive() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_zome = alice.zome("content");
    let bob_zome = bob.zome("content");
    let content_type = "pinned-hosts-own-type";

    await_consistency_s(30, [alice, bob]).await.unwrap();

    let hive_h = create_hive(&conductors[0], &alice_zome, "own-hive-h").await;
    let hive_h2 = create_hive(&conductors[0], &alice_zome, "own-hive-h2").await;
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let alice_h_first = create_open_write_content(
        &conductors[0],
        &alice_zome,
        hive_h.clone(),
        content_type,
        "blob-x",
        None,
    )
    .await;
    let alice_h_second = create_open_write_content(
        &conductors[0],
        &alice_zome,
        hive_h.clone(),
        content_type,
        "blob-x",
        None,
    )
    .await;
    let alice_h2_only = create_open_write_content(
        &conductors[0],
        &alice_zome,
        hive_h2.clone(),
        content_type,
        "blob-x",
        None,
    )
    .await;
    let bob_h_only = create_open_write_content(
        &conductors[1],
        &bob_zome,
        hive_h.clone(),
        content_type,
        "blob-x",
        None,
    )
    .await;
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let alice_own =
        wait_for_own_content_id_count(&conductors[0], &alice_zome, hive_h.clone(), "blob-x", 2)
            .await;
    assert!(!alice_own.truncated);
    let mut alice_hashes: Vec<String> = alice_own
        .records
        .iter()
        .map(|record| record.hash.clone())
        .collect();
    alice_hashes.sort();
    let mut expected = vec![alice_h_first, alice_h_second];
    expected.sort();
    assert_eq!(
        alice_hashes, expected,
        "Alice sees exactly her two H-scoped duplicate roots — Bob's collision and her H2 root are invisible"
    );
    assert!(!alice_hashes.contains(&alice_h2_only));

    let bob_own =
        wait_for_own_content_id_count(&conductors[1], &bob_zome, hive_h.clone(), "blob-x", 1).await;
    assert!(!bob_own.truncated);
    let bob_hashes: Vec<&String> = bob_own.records.iter().map(|record| &record.hash).collect();
    assert_eq!(bob_hashes, vec![&bob_h_only], "Bob sees exactly his one");

    let empty = my_content(&conductors[0], &alice_zome, hive_h, "blob-y").await;
    assert!(
        empty.records.is_empty(),
        "no-match is an empty result, not an error"
    );
    assert!(!empty.truncated);
}


#[tokio::test(flavor = "multi_thread")]
async fn latest_action_micros_populated_on_get_none_on_create() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = zome.cell_id().agent_pubkey().clone();
    let hive = create_hive(&conductor, &zome, "recency-hive").await;
    let content_type = "pinned-hosts-recency-type";

    let created: support::CreateResponse = conductor
        .call(
            &zome,
            "create_encrypted_content",
            support::CreateEncryptedContentInput {
                id: "recency-1".to_string(),
                display_hive_id: "sweettest-hive".to_string(),
                content_type: content_type.to_string(),
                revision_author_signing_public_key: author.to_string(),
                bytes: UnsafeBytes::from(vec![0u8]).into(),
                acl_spec: AclSpec::OpenWrite {
                    target_hive_genesis_hash: Some(hive.clone()),
                },
                public_key_acl: support::owner_only_acl(&author.to_string()),
                dynamic_links: None,
            },
        )
        .await;
    assert_eq!(
        created.latest_action_micros, None,
        "create response must never fabricate a recency timestamp"
    );
    await_consistency_s(30, [&cell]).await.unwrap();

    let content_ah = ActionHash::try_from(created.hash.as_str()).expect("hash parses");
    let got: ContentRecord = conductor
        .call(&zome, "get_encrypted_content", content_ah.clone())
        .await;
    let t0 = got
        .latest_action_micros
        .expect("get must surface the selected action's timestamp");

    let updated: ContentRecord = conductor
        .call(
            &zome,
            "update_encrypted_content",
            UpdateEncryptedContentInput {
                previous_encrypted_content_hash: content_ah.clone(),
                updated_encrypted_content: EncryptedContent {
                    header: EncryptedContentHeader {
                        id: "recency-1".to_string(),
                        display_hive_id: "sweettest-hive".to_string(),
                        content_type: content_type.to_string(),
                        revision_author_signing_public_key: author.to_string(),
                        acl_spec: AclSpec::OpenWrite {
                            target_hive_genesis_hash: Some(hive.clone()),
                        },
                        public_key_acl: support::owner_only_acl(&author.to_string()),
                    },
                    bytes: UnsafeBytes::from(vec![9u8]).into(),
                },
            },
        )
        .await;
    let t1 = updated
        .latest_action_micros
        .expect("update response routes through get and must carry recency");
    assert!(t1 > t0, "update recency must advance: t0={t0} t1={t1}");
    await_consistency_s(30, [&cell]).await.unwrap();

    let re_got: ContentRecord = conductor
        .call(&zome, "get_encrypted_content", content_ah)
        .await;
    assert_eq!(
        re_got.latest_action_micros,
        Some(t1),
        "get on the original must select the latest update's timestamp"
    );

    let listed: Vec<ContentRecord> = conductor
        .call(
            &zome,
            "list_by_hive_link",
            ListByHiveInput {
                hive_genesis_hash: hive,
                content_type: content_type.to_string(),
                since_ts: None,
                limit: None,
            },
        )
        .await;
    assert!(
        listed
            .iter()
            .all(|record| record.latest_action_micros.is_some()),
        "legacy list records inherit recency via the get path"
    );
}

fn sample_hint(hive: ActionHash, provider_record: ActionHash) -> BlobPinHint {
    BlobPinHint {
        hive_genesis_hash: hive,
        blake3: "b3-hex".to_string(),
        byte_variant: "enc".to_string(),
        provider_record_hash: provider_record,
        expires_at_micros: None,
        from_agent: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn blob_pin_signal_dispatch_accepts_family_and_rejects_junk() {
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "signal-hive").await;

    let signal = BlobPinSignal::Available(sample_hint(hive.clone(), hive));
    let pre_encoded = ExternIO::encode(&signal).expect("encode");
    let _: () = conductor
        .call(&zome, "recv_remote_signal", pre_encoded)
        .await;

    let junk = ExternIO::encode("garbage").expect("encode");
    let rejected: Result<(), _> = conductor
        .call_fallible(&zome, "recv_remote_signal", junk)
        .await;
    let err = format!("{:?}", rejected.expect_err("junk payload must be rejected"));
    assert!(
        err.contains("did not decode as EncryptedContentSignal, DmRemoteSignal, or BlobPinSignal"),
        "fallthrough must name every family; got: {err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn blob_pin_signal_round_trips_between_agents() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_key = alice.agent_pubkey().clone();
    let bob_key = bob.agent_pubkey().clone();
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let mut alice_signals = conductors[0]
        .raw_handle()
        .subscribe_to_app_signals("test-app".to_string());

    let hive = create_hive(&conductors[1], &bob.zome("content"), "signal-fanout-hive").await;
    // Forged sender claim: the recipient must see conductor-attested bob,
    // proving sender-side clear + receiver-side stamp against a live forgery.
    let mut forged_hint = sample_hint(hive.clone(), hive.clone());
    forged_hint.from_agent = Some(alice_key.clone());
    let _: () = conductors[1]
        .call(
            &bob.zome("content"),
            "send_blob_pin_signal",
            SendBlobPinSignalInput {
                signal: BlobPinSignal::TakeNow(forged_hint),
                recipients: vec![alice_key],
            },
        )
        .await;

    let received = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let signal = alice_signals
                .recv()
                .await
                .expect("signal channel stays open");
            let Signal::App {
                signal: app_signal, ..
            } = signal
            else {
                continue;
            };
            if let Ok(pin) = app_signal.into_inner().decode::<BlobPinSignal>() {
                return pin;
            }
        }
    })
    .await
    .expect("Alice must receive the blob-pin signal within 60s");

    let BlobPinSignal::TakeNow(hint) = received else {
        panic!("variant must survive the wire");
    };
    assert_eq!(
        hint.from_agent.as_ref(),
        Some(&bob_key),
        "receiver-side dispatcher must stamp conductor-attested sender provenance"
    );
    assert_eq!(hint.hive_genesis_hash, hive);
}

#[tokio::test(flavor = "multi_thread")]
async fn author_page_scopes_to_author_and_pages() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_zome = alice.zome("content");
    let bob_zome = bob.zome("content");
    let content_type = "pinned-hosts-author-type";

    await_consistency_s(30, [alice, bob]).await.unwrap();
    let hive = create_hive(&conductors[0], &alice_zome, "author-page-hive").await;
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let mut alice_created: Vec<String> = Vec::new();
    for i in 0..3 {
        alice_created.push(
            create_open_write_content(
                &conductors[0],
                &alice_zome,
                hive.clone(),
                content_type,
                &format!("author-a{i}"),
                None,
            )
            .await,
        );
    }
    let bob_created = create_open_write_content(
        &conductors[1],
        &bob_zome,
        hive,
        content_type,
        "author-b",
        None,
    )
    .await;
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let author = alice.agent_pubkey().to_string();
    let page1 = author_page(
        &conductors[0],
        &alice_zome,
        &author,
        content_type,
        None,
        None,
    )
    .await;
    assert_page_invariants(&page1);
    assert_eq!(page1.records.len(), 2);
    assert!(page1.truncated);

    let (since2, after2) = cursor_of(&page1);
    let page2 = author_page(
        &conductors[0],
        &alice_zome,
        &author,
        content_type,
        since2,
        after2,
    )
    .await;
    assert_page_invariants(&page2);
    assert_eq!(page2.records.len(), 1);
    assert!(!page2.truncated);

    let mut walked: Vec<String> = page1
        .records
        .iter()
        .chain(page2.records.iter())
        .map(|record| record.hash.clone())
        .collect();
    walked.sort();
    alice_created.sort();
    assert_eq!(
        walked, alice_created,
        "author pages cover exactly Alice's 3"
    );
    assert!(
        !walked.contains(&bob_created),
        "Bob's entry must never appear"
    );
}

async fn author_page(
    conductor: &SweetConductor,
    zome: &SweetZome,
    author: &str,
    content_type: &str,
    since_ts: Option<i64>,
    source_after_action_hash: Option<String>,
) -> BoundedLinkPage {
    conductor
        .call(
            zome,
            "list_by_author_page",
            AuthorLinkPageInput {
                author: author.to_string(),
                content_type: content_type.to_string(),
                since_ts,
                limit: Some(2),
                source_after_action_hash,
            },
        )
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn legacy_hive_link_since_ts_limit_watermark_sweep() {
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "watermark-hive").await;
    let content_type = "pinned-hosts-watermark-type";

    let mut created: Vec<String> = Vec::new();
    for i in 0..5 {
        created.push(
            create_open_write_content(
                &conductor,
                &zome,
                hive.clone(),
                content_type,
                &format!("sweep-{i}"),
                None,
            )
            .await,
        );
    }
    await_consistency_s(30, [&cell]).await.unwrap();
    wait_for_count_links_by_hive_to(&conductor, &zome, &hive, content_type, 5).await;

    let mut union: Vec<String> = Vec::new();
    let mut since_ts: Option<i64> = None;
    for _round in 0..10 {
        let batch: Vec<ContentRecord> = conductor
            .call(
                &zome,
                "list_by_hive_link",
                ListByHiveInput {
                    hive_genesis_hash: hive.clone(),
                    content_type: content_type.to_string(),
                    since_ts,
                    limit: Some(2),
                },
            )
            .await;
        if batch.is_empty() {
            break;
        }

        let timestamps: Vec<i64> = batch
            .iter()
            .map(|record| {
                record
                    .latest_action_micros
                    .expect("hive-listed records carry recency")
            })
            .collect();
        assert!(
            timestamps.windows(2).all(|pair| pair[0] <= pair[1]),
            "each sweep round must return oldest-first: {timestamps:?}"
        );

        let before = union.len();
        for record in &batch {
            if !union.contains(&record.hash) {
                union.push(record.hash.clone());
            }
        }
        since_ts = timestamps.last().copied();
        if union.len() == 5 && union.len() == before {
            break;
        }
    }

    let mut swept = union.clone();
    swept.sort();
    created.sort();
    assert_eq!(
        swept, created,
        "watermark sweep must cover all 5 entries with no loss (boundary dupes deduped by hash)"
    );
}
