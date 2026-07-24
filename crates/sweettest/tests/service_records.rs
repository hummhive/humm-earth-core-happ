//! Conductor behavior proof for pass-6 service-meter and node-spec records.

mod support;

use std::collections::BTreeMap;
use std::future::Future;
use std::time::{Duration, Instant};

use holo_hash::ActionHash;
use holochain::sweettest::{await_consistency_s, SweetConductor, SweetZome};
use support::{
    create_hive, decode_content_payload, owner_only_acl, public_reader_acl, setup_cells,
    single_conductor_cell_app, DynamicLinkPageInput, EncryptedContentPayloadResponse,
    ListByHiveInput, NodeSpecAttestation, NodeSpecSnapshot, PublishNodeSpecInput,
    ServiceMeterSnapshot, ServiceRecordPage, UpsertContentResponse, UpsertServiceMeterInput,
};

const METER_CONTENT_TYPE: &str = "hummhive-core-service-meter-v1";
const METER_SCHEMA: &str = "hummhive-service-meter/1";
const NODE_SPEC_CONTENT_TYPE: &str = "hummhive-core-node-spec-v1";
const NODE_SPEC_SCHEMA: &str = "hummhive-node-spec/1";
const TEST_PERIOD: &str = "2026-07-17";
const TEST_DECLARED_AT_MICROS: i64 = 1_752_700_000_000_000;
const ZERO_SIGNATURE_B64: &str =
    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==";

fn string_map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

fn meter_input(
    hive_genesis_hash: &ActionHash,
    author: &str,
    period: &str,
    counters: BTreeMap<String, String>,
) -> UpsertServiceMeterInput {
    UpsertServiceMeterInput {
        hive_genesis_hash: hive_genesis_hash.clone(),
        period: period.to_string(),
        counters,
        display_hive_id: "sweettest-service-hive".to_string(),
        revision_author_signing_public_key: author.to_string(),
        public_key_acl: public_reader_acl(author),
    }
}

fn node_spec_input(
    hive_genesis_hash: &ActionHash,
    author: &str,
    spec: BTreeMap<String, String>,
    app_attestation: Option<NodeSpecAttestation>,
) -> PublishNodeSpecInput {
    PublishNodeSpecInput {
        hive_genesis_hash: hive_genesis_hash.clone(),
        spec,
        declared_at_micros: TEST_DECLARED_AT_MICROS,
        app_attestation,
        display_hive_id: "sweettest-service-hive".to_string(),
        revision_author_signing_public_key: author.to_string(),
        public_key_acl: owner_only_acl(author),
    }
}

async fn meter_page(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: &ActionHash,
    period: &str,
) -> ServiceRecordPage {
    conductor
        .call(
            zome,
            "list_by_dynamic_link_page",
            DynamicLinkPageInput {
                hive_genesis_hash: hive_genesis_hash.clone(),
                content_type: METER_CONTENT_TYPE.to_string(),
                dynamic_link: period.to_string(),
                since_ts: None,
                limit: None,
                source_after_action_hash: None,
            },
        )
        .await
}

async fn wait_for_single_snapshot<T, F, Fut>(
    fetch: F,
    expected: &T,
    what: &str,
) -> Vec<EncryptedContentPayloadResponse>
where
    T: serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    F: Fn() -> Fut,
    Fut: Future<Output = Vec<EncryptedContentPayloadResponse>>,
{
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let latest = fetch().await;
        if latest.len() == 1 {
            let snapshot: T = decode_content_payload(&latest[0]);
            if snapshot == *expected {
                return latest;
            }
        }
        if Instant::now() >= deadline {
            panic!("{what} did not resolve one expected snapshot within 30s: {latest:?}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_for_meter_page(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: &ActionHash,
    expected: &ServiceMeterSnapshot,
) -> Vec<EncryptedContentPayloadResponse> {
    wait_for_single_snapshot(
        || async {
            meter_page(conductor, zome, hive_genesis_hash, &expected.period)
                .await
                .records
        },
        expected,
        "meter page",
    )
    .await
}

async fn node_spec_records(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: &ActionHash,
) -> Vec<EncryptedContentPayloadResponse> {
    conductor
        .call(
            zome,
            "list_by_hive_link",
            ListByHiveInput {
                hive_genesis_hash: hive_genesis_hash.clone(),
                content_type: NODE_SPEC_CONTENT_TYPE.to_string(),
                since_ts: None,
                limit: None,
            },
        )
        .await
}

async fn wait_for_node_spec_records(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: &ActionHash,
    expected: &NodeSpecSnapshot,
) -> Vec<EncryptedContentPayloadResponse> {
    wait_for_single_snapshot(
        || async { node_spec_records(conductor, zome, hive_genesis_hash).await },
        expected,
        "node-spec list",
    )
    .await
}

async fn assert_call_rejected<I>(
    conductor: &SweetConductor,
    zome: &SweetZome,
    fn_name: &str,
    input: I,
    expected_literal: &str,
    behavior: &str,
) where
    I: serde::Serialize + std::fmt::Debug,
{
    let rejected: Result<UpsertContentResponse, _> =
        conductor.call_fallible(zome, fn_name, input).await;
    let err = format!("{:?}", rejected.expect_err(behavior));
    assert!(
        err.contains(expected_literal),
        "{behavior}: expected rejection containing {expected_literal:?}, got {err}"
    );
}

async fn assert_meter_rejected(
    conductor: &SweetConductor,
    zome: &SweetZome,
    input: UpsertServiceMeterInput,
    expected_literal: &str,
    behavior: &str,
) {
    assert_call_rejected(
        conductor,
        zome,
        "upsert_service_meter",
        input,
        expected_literal,
        behavior,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn meter_first_upsert_creates_and_lists_on_period_page() {
    holochain_trace::test_run();
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().to_string();
    let hive = create_hive(&conductor, &zome, "meter-first-hive").await;
    let expected = ServiceMeterSnapshot {
        schema: METER_SCHEMA.to_string(),
        period: TEST_PERIOD.to_string(),
        counters: string_map(&[("bytes_served", "100"), ("gets", "7")]),
    };

    let created: UpsertContentResponse = conductor
        .call(
            &zome,
            "upsert_service_meter",
            meter_input(
                &hive,
                &author,
                TEST_PERIOD,
                string_map(&[("bytes_served", "100"), ("gets", "007")]),
            ),
        )
        .await;

    assert_eq!(
        (created.was_created, created.was_updated),
        (true, false),
        "first meter upsert must create without reporting an update"
    );
    let records = wait_for_meter_page(&conductor, &zome, &hive, &expected).await;
    assert_eq!(records.len(), 1, "period page must contain one meter");
    assert_eq!(records[0].hash, created.response.hash);
    let snapshot: ServiceMeterSnapshot = decode_content_payload(&records[0]);
    assert_eq!(
        snapshot, expected,
        "stored meter must canonicalize counters"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn meter_identical_reupsert_is_noop_single_record() {
    holochain_trace::test_run();
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().to_string();
    let hive = create_hive(&conductor, &zome, "meter-noop-hive").await;
    let expected = ServiceMeterSnapshot {
        schema: METER_SCHEMA.to_string(),
        period: TEST_PERIOD.to_string(),
        counters: string_map(&[("bytes_served", "100"), ("gets", "7")]),
    };
    let created: UpsertContentResponse = conductor
        .call(
            &zome,
            "upsert_service_meter",
            meter_input(&hive, &author, TEST_PERIOD, expected.counters.clone()),
        )
        .await;
    assert!(created.was_created, "fixture meter must be created");
    wait_for_meter_page(&conductor, &zome, &hive, &expected).await;

    let retried: UpsertContentResponse = conductor
        .call(
            &zome,
            "upsert_service_meter",
            meter_input(&hive, &author, TEST_PERIOD, expected.counters.clone()),
        )
        .await;

    assert_eq!(
        (retried.was_created, retried.was_updated),
        (false, false),
        "identical retry must not write"
    );
    let records = wait_for_meter_page(&conductor, &zome, &hive, &expected).await;
    assert_eq!(records.len(), 1, "identical retry must keep one meter");
    let snapshot: ServiceMeterSnapshot = decode_content_payload(&records[0]);
    assert_eq!(snapshot, expected, "identical retry must preserve counters");
}

#[tokio::test(flavor = "multi_thread")]
async fn meter_merge_takes_max_over_union() {
    holochain_trace::test_run();
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().to_string();
    let hive = create_hive(&conductor, &zome, "meter-merge-hive").await;
    let initial = ServiceMeterSnapshot {
        schema: METER_SCHEMA.to_string(),
        period: TEST_PERIOD.to_string(),
        counters: string_map(&[("bytes_served", "100"), ("gets", "7")]),
    };
    let created: UpsertContentResponse = conductor
        .call(
            &zome,
            "upsert_service_meter",
            meter_input(&hive, &author, TEST_PERIOD, initial.counters.clone()),
        )
        .await;
    assert!(created.was_created, "fixture meter must be created");
    wait_for_meter_page(&conductor, &zome, &hive, &initial).await;

    let updated: UpsertContentResponse = conductor
        .call(
            &zome,
            "upsert_service_meter",
            meter_input(
                &hive,
                &author,
                TEST_PERIOD,
                string_map(&[("bytes_served", "90"), ("gets", "5"), ("puts", "2")]),
            ),
        )
        .await;

    assert_eq!(
        (updated.was_created, updated.was_updated),
        (false, true),
        "higher-or-new dimensions must update the existing meter"
    );
    let expected = ServiceMeterSnapshot {
        schema: METER_SCHEMA.to_string(),
        period: TEST_PERIOD.to_string(),
        counters: string_map(&[("bytes_served", "100"), ("gets", "7"), ("puts", "2")]),
    };
    let records = wait_for_meter_page(&conductor, &zome, &hive, &expected).await;
    assert_eq!(records.len(), 1, "merge must keep one meter");
    let snapshot: ServiceMeterSnapshot = decode_content_payload(&records[0]);
    assert_eq!(snapshot, expected, "meters must take max over the union");
}

#[tokio::test(flavor = "multi_thread")]
async fn meter_public_cross_agent_read() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_zome = alice.zome("content");
    let bob_zome = bob.zome("content");
    await_consistency_s(30, [alice, bob]).await.unwrap();
    let hive = create_hive(&conductors[0], &alice_zome, "meter-public-hive").await;
    let expected = ServiceMeterSnapshot {
        schema: METER_SCHEMA.to_string(),
        period: TEST_PERIOD.to_string(),
        counters: string_map(&[("bytes_served", "100"), ("gets", "7")]),
    };

    let created: UpsertContentResponse = conductors[0]
        .call(
            &alice_zome,
            "upsert_service_meter",
            meter_input(
                &hive,
                &alice.agent_pubkey().to_string(),
                TEST_PERIOD,
                expected.counters.clone(),
            ),
        )
        .await;
    assert!(created.was_created, "alice's public meter must be created");
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let records = wait_for_meter_page(&conductors[1], &bob_zome, &hive, &expected).await;
    assert_eq!(records.len(), 1, "bob must see alice's public meter");
    let snapshot: ServiceMeterSnapshot = decode_content_payload(&records[0]);
    assert_eq!(
        snapshot, expected,
        "bob must decode alice's public counters"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn meter_rejects_are_exact() {
    holochain_trace::test_run();
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().to_string();
    let hive = create_hive(&conductor, &zome, "meter-reject-hive").await;

    assert_meter_rejected(
        &conductor,
        &zome,
        meter_input(&hive, &author, "2026-7-17", string_map(&[("gets", "1")])),
        "service meter period must be YYYY-MM-DD",
        "non-padded period must be rejected",
    )
    .await;
    let too_many = (0..17)
        .map(|index| (format!("counter_{index}"), "1".to_string()))
        .collect();
    assert_meter_rejected(
        &conductor,
        &zome,
        meter_input(&hive, &author, TEST_PERIOD, too_many),
        "service meter accepts at most 16 counter dimensions",
        "seventeen dimensions must be rejected",
    )
    .await;
    assert_meter_rejected(
        &conductor,
        &zome,
        meter_input(&hive, &author, TEST_PERIOD, string_map(&[("gets", "1.5")])),
        "service meter counters must be canonical u128 decimal strings",
        "fractional counters must be rejected",
    )
    .await;
}

fn stored_readers(upsert: &UpsertContentResponse) -> &[String] {
    &upsert
        .response
        .encrypted_content
        .header
        .public_key_acl
        .reader
}

#[tokio::test(flavor = "multi_thread")]
async fn meter_acl_upgrade_is_a_real_update() {
    holochain_trace::test_run();
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().to_string();
    let hive = create_hive(&conductor, &zome, "meter-acl-hive").await;
    let counters = string_map(&[("bytes_served", "100")]);

    let mut private_input = meter_input(&hive, &author, TEST_PERIOD, counters.clone());
    private_input.public_key_acl = owner_only_acl(&author);
    let first: UpsertContentResponse = conductor
        .call(&zome, "upsert_service_meter", private_input)
        .await;
    assert!(first.was_created, "fresh day bucket must create");
    assert!(
        stored_readers(&first).is_empty(),
        "private meter must start with no readers"
    );

    let upgraded: UpsertContentResponse = conductor
        .call(
            &zome,
            "upsert_service_meter",
            meter_input(&hive, &author, TEST_PERIOD, counters.clone()),
        )
        .await;
    assert!(!upgraded.was_created, "same day bucket must not fork");
    assert!(
        upgraded.was_updated,
        "ACL widening alone must be a real update"
    );
    assert_eq!(
        stored_readers(&upgraded),
        ["*".to_string()],
        "stored header must converge to the caller's ACL"
    );

    let settled: UpsertContentResponse = conductor
        .call(
            &zome,
            "upsert_service_meter",
            meter_input(&hive, &author, TEST_PERIOD, counters),
        )
        .await;
    assert!(
        !settled.was_created && !settled.was_updated,
        "converged header and counters must no-op after the update"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn node_spec_publish_lists_and_is_self_reported() {
    holochain_trace::test_run();
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().to_string();
    let hive = create_hive(&conductor, &zome, "node-spec-first-hive").await;
    let expected = NodeSpecSnapshot {
        schema: NODE_SPEC_SCHEMA.to_string(),
        spec: string_map(&[("cpu_cores", "8"), ("ram_gb", "32")]),
        declared_at_micros: TEST_DECLARED_AT_MICROS,
        verified_by_app_key: None,
    };

    let created: UpsertContentResponse = conductor
        .call(
            &zome,
            "publish_node_spec",
            node_spec_input(&hive, &author, expected.spec.clone(), None),
        )
        .await;

    assert_eq!(
        (created.was_created, created.was_updated),
        (true, false),
        "first node-spec publish must create without reporting an update"
    );
    let records = wait_for_node_spec_records(&conductor, &zome, &hive, &expected).await;
    assert_eq!(records.len(), 1, "hive list must contain one node spec");
    assert_eq!(records[0].hash, created.response.hash);
    let snapshot: NodeSpecSnapshot = decode_content_payload(&records[0]);
    assert_eq!(
        snapshot, expected,
        "unattested spec must remain self-reported"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn node_spec_replace_and_noop_semantics() {
    holochain_trace::test_run();
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().to_string();
    let hive = create_hive(&conductor, &zome, "node-spec-replace-hive").await;
    let initial = NodeSpecSnapshot {
        schema: NODE_SPEC_SCHEMA.to_string(),
        spec: string_map(&[("cpu_cores", "8"), ("ram_gb", "32")]),
        declared_at_micros: TEST_DECLARED_AT_MICROS,
        verified_by_app_key: None,
    };
    let created: UpsertContentResponse = conductor
        .call(
            &zome,
            "publish_node_spec",
            node_spec_input(&hive, &author, initial.spec.clone(), None),
        )
        .await;
    assert!(created.was_created, "fixture node spec must be created");
    wait_for_node_spec_records(&conductor, &zome, &hive, &initial).await;

    let retried: UpsertContentResponse = conductor
        .call(
            &zome,
            "publish_node_spec",
            node_spec_input(&hive, &author, initial.spec.clone(), None),
        )
        .await;
    assert_eq!(
        (retried.was_created, retried.was_updated),
        (false, false),
        "identical node-spec retry must not write"
    );
    let replacement = NodeSpecSnapshot {
        schema: NODE_SPEC_SCHEMA.to_string(),
        spec: string_map(&[("cpu_cores", "16")]),
        declared_at_micros: TEST_DECLARED_AT_MICROS,
        verified_by_app_key: None,
    };
    let updated: UpsertContentResponse = conductor
        .call(
            &zome,
            "publish_node_spec",
            node_spec_input(&hive, &author, replacement.spec.clone(), None),
        )
        .await;
    assert_eq!(
        (updated.was_created, updated.was_updated),
        (false, true),
        "changed node spec must update the existing record"
    );

    let records = wait_for_node_spec_records(&conductor, &zome, &hive, &replacement).await;
    assert_eq!(records.len(), 1, "replace must keep one node spec");
    let snapshot: NodeSpecSnapshot = decode_content_payload(&records[0]);
    assert!(
        !snapshot.spec.contains_key("ram_gb"),
        "replace must remove omitted fields"
    );
    assert_eq!(
        snapshot, replacement,
        "node spec must replace rather than merge"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn node_spec_attestation_rejects_foreign_signing_key() {
    holochain_trace::test_run();
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().to_string();
    let hive = create_hive(&conductor, &zome, "node-spec-attestation-hive").await;
    let input = node_spec_input(
        &hive,
        &author,
        string_map(&[("cpu_cores", "8")]),
        Some(NodeSpecAttestation {
            // Valid key shape, absent from the allowlist — must fail the
            // allowlist gate, not the signature gate.
            app_signing_key_b64: author.clone(),
            signature_b64: ZERO_SIGNATURE_B64.to_string(),
        }),
    );

    assert_call_rejected(
        &conductor,
        &zome,
        "publish_node_spec",
        input,
        "unrecognized app signing key",
        "attestation must reject a key absent from the accepted list",
    )
    .await;
}

/// The ACCEPTED production key with a garbage signature must clear the
/// allowlist gate and fail at the SIGNATURE gate (catches an empty/mistyped list).
#[tokio::test(flavor = "multi_thread")]
async fn node_spec_attestation_accepted_key_fails_at_signature_gate() {
    holochain_trace::test_run();
    let (conductor, cell, zome) = single_conductor_cell_app().await;
    let author = cell.agent_pubkey().to_string();
    let hive = create_hive(&conductor, &zome, "node-spec-accepted-key-hive").await;
    let input = node_spec_input(
        &hive,
        &author,
        string_map(&[("cpu_cores", "8")]),
        Some(NodeSpecAttestation {
            app_signing_key_b64: "uhCAkyyOeMalaAEDiWSFPoywDMtLOB5AaisjAhnQ-9m2y81p9xnJC"
                .to_string(),
            signature_b64: ZERO_SIGNATURE_B64.to_string(),
        }),
    );

    assert_call_rejected(
        &conductor,
        &zome,
        "publish_node_spec",
        input,
        "app attestation signature invalid",
        "accepted key with a garbage signature must fail the signature gate, not the allowlist",
    )
    .await;
}
