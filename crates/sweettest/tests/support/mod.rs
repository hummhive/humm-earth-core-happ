#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use holo_hash::{ActionHash, AgentPubKey};
use holochain::prelude::DnaFile;
use holochain::sweettest::{
    SweetCell, SweetConductor, SweetConductorBatch, SweetDnaFile, SweetZome,
};
use holochain_types::prelude::{SerializedBytes, UnsafeBytes};
use serde::{Deserialize, Serialize};

/// Expected DNA hash for the pass-7 scratch bundle this suite must run
/// against (moves at every integrity-touching pass-7 milestone).
///
/// Stale workdir bundles silently mask coordinator and integrity behavior; this
/// hash gate keeps conductor tests on the intended DNA generation. Read at
/// RUNTIME from `fixtures/expected-dna-hash.txt`: every test binary textually
/// includes this module (`mod support;`), so a source-constant re-pin would
/// recompile + relink all of them; a data-file re-pin relinks none.
pub fn expected_dna_hash() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/expected-dna-hash.txt");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .trim()
        .to_string()
}

/// Absolute path to the pre-built DNA, resolved from this crate's manifest dir
/// so integration tests are cwd-independent.
pub fn dna_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dnas/humm_earth_core/workdir/humm_earth_core.dna")
}

pub async fn load_dna() -> DnaFile {
    let dna = SweetDnaFile::from_bundle(&dna_path()).await.expect(
        "load humm_earth_core.dna (build: npm run build:zomes && hc dna pack dnas/humm_earth_core/workdir)",
    );
    let actual = dna.dna_hash().to_string();
    let expected = expected_dna_hash();
    assert_eq!(
        actual, expected,
        "Stale workdir/humm_earth_core.dna — loaded DNA hash {actual} but expected the pass-7 scratch pin {expected}. \
         Rebuild: `nix develop --command bash -c 'npm run build:zomes && hc dna pack dnas/humm_earth_core/workdir'`."
    );
    dna
}

/// Expected DNA hash of the vendored pass-6 fixture (the lineage
/// cross-test's prior generation).
pub const PASS_6_EXPECTED_DNA_HASH: &str = "uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz";

/// Absolute path to the vendored pass-6 DNA fixture, resolved from this
/// crate's manifest dir so integration tests are cwd-independent.
pub fn pass_6_dna_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/pass-6-service-meter.dna")
}

pub async fn load_pass_6_dna() -> DnaFile {
    let dna = SweetDnaFile::from_bundle(&pass_6_dna_path())
        .await
        .expect("load fixtures/pass-6-service-meter.dna (vendored at M0 from the v3.3.0 workdir build)");
    let actual = dna.dna_hash().to_string();
    assert_eq!(
        actual, PASS_6_EXPECTED_DNA_HASH,
        "Corrupt pass-6 fixture — loaded DNA hash {actual} but expected the held pass-6 generation {PASS_6_EXPECTED_DNA_HASH}. \
         Re-vendor: `cp dnas/humm_earth_core/workdir/humm_earth_core.dna crates/sweettest/fixtures/pass-6-service-meter.dna` from a pristine v3.3.0 build."
    );
    dna
}

pub async fn setup_cells(agents: usize) -> (SweetConductorBatch, Vec<SweetCell>) {
    let dna = load_dna().await;
    let mut conductors = SweetConductorBatch::from_standard_config_rendezvous(agents).await;
    let apps = conductors
        .setup_app("test-app", &[("humm_earth_core".into(), dna)])
        .await
        .unwrap();
    let cells = apps.cells_flattened();
    (conductors, cells)
}

pub async fn single_conductor_cell_app() -> (SweetConductor, SweetCell, SweetZome) {
    holochain_trace::test_run();
    let dna = load_dna().await;
    let mut conductor = SweetConductor::from_standard_config().await;
    let app = conductor
        .setup_app("test-app", &[("humm_earth_core".into(), dna)])
        .await
        .unwrap();
    let (cell,): (SweetCell,) = app.into_tuple();
    let zome = cell.zome("content");
    (conductor, cell, zome)
}

pub async fn single_conductor_app() -> (SweetConductor, SweetZome) {
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    (conductor, zome)
}

/// `list_my_hives` row subset; serde ignores fields these tests do not assert.
#[derive(Debug, Deserialize)]
pub struct ListedHive {
    pub display_id: String,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GenesisResponse {
    pub hash: ActionHash,
}

#[derive(Debug, Serialize)]
pub struct CreateHiveGenesisInput {
    pub display_id: String,
}

/// Mirror of the coordinator `CreateHiveMembershipInput`.
#[derive(Debug, Serialize)]
pub struct CreateHiveMembershipInput {
    pub hive_genesis_hash: ActionHash,
    pub for_agent: AgentPubKey,
    pub role: String,
    pub grantor_membership_hash: Option<ActionHash>,
    pub expiry: Option<i64>,
    pub grantor_owner_accept_hash: Option<ActionHash>,
}

#[derive(Debug, Deserialize)]
pub struct MembershipResponse {
    pub hash: ActionHash,
}

pub async fn create_hive(
    conductor: &SweetConductor,
    zome: &SweetZome,
    display_id: &str,
) -> ActionHash {
    let response: GenesisResponse = conductor
        .call(
            zome,
            "create_hive_genesis",
            CreateHiveGenesisInput {
                display_id: display_id.to_string(),
            },
        )
        .await;
    response.hash
}

pub async fn grant_hive_membership(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: ActionHash,
    for_agent: AgentPubKey,
    role: &str,
    grantor_membership_hash: Option<ActionHash>,
    expiry: Option<i64>,
    grantor_owner_accept_hash: Option<ActionHash>,
) -> ActionHash {
    let response: MembershipResponse = conductor
        .call(
            zome,
            "create_hive_membership",
            CreateHiveMembershipInput {
                hive_genesis_hash,
                for_agent,
                role: role.to_string(),
                grantor_membership_hash,
                expiry,
                grantor_owner_accept_hash,
            },
        )
        .await;
    response.hash
}

/// Mirror of the coordinator `DeleteContentResponse`.
#[derive(Debug, Deserialize)]
pub struct DeleteContentResponse {
    pub was_deleted: bool,
    #[serde(default)]
    pub delete_action_hash: Option<ActionHash>,
}

/// Mirror of the coordinator `CreateGroupGenesisInput`.
#[derive(Debug, Serialize)]
pub struct CreateGroupGenesisInput {
    pub hive_genesis_hash: ActionHash,
    pub display_id: String,
    pub hive_wide_role: Option<String>,
    pub creator_hive_membership_hash: Option<ActionHash>,
}

/// Mirror of the coordinator `CreateGroupMembershipInput`.
#[derive(Debug, Serialize)]
pub struct CreateGroupMembershipInput {
    pub group_genesis_hash: ActionHash,
    pub for_agent: AgentPubKey,
    pub role: String,
    pub grantor_membership_hash: Option<ActionHash>,
    pub grantor_hive_membership_hash: Option<ActionHash>,
    pub expiry: Option<i64>,
}

/// Mirror of the coordinator `FindOrCreateGroupGenesisResponse` subset.
#[derive(Debug, Deserialize)]
pub struct FindOrCreateGenesisResponse {
    pub response: GenesisResponse,
    pub was_created: bool,
}

const GROUP_VISIBILITY_POLL_ATTEMPTS: usize = 200;
const GROUP_VISIBILITY_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Deserialize)]
struct ListedGroupRow {
    group_genesis_hash: ActionHash,
}

/// Poll until `group` appears in `list_groups_in_hive(hive)` — HiveToGroups
/// links integrate on the cascade's own cadence after commit.
pub async fn wait_for_group_visible(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive: &ActionHash,
    group: &ActionHash,
) {
    for _ in 0..GROUP_VISIBILITY_POLL_ATTEMPTS {
        let groups: Vec<ListedGroupRow> = conductor
            .call(zome, "list_groups_in_hive", hive.clone())
            .await;
        if groups.iter().any(|g| &g.group_genesis_hash == group) {
            return;
        }
        tokio::time::sleep(GROUP_VISIBILITY_POLL_INTERVAL).await;
    }
    panic!("group {group} never became visible in hive {hive}");
}

// --- Encrypted-content wire mirrors (shared by conductor tests) --------------

#[derive(Debug, Serialize)]
pub struct Acl {
    pub owner: String,
    pub admin: Vec<String>,
    pub writer: Vec<String>,
    pub reader: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AclByGroupGenesis {
    pub owner: ActionHash,
    pub admin: Vec<ActionHash>,
    pub writer: Vec<ActionHash>,
    pub reader: Vec<ActionHash>,
}

#[derive(Debug, Serialize)]
pub enum AclBucket {
    Reader,
}

#[derive(Debug, Serialize)]
pub struct RecipientWitness {
    pub pubkey: AgentPubKey,
    pub bucket: AclBucket,
    pub membership_hash: ActionHash,
}

/// Mirror of the coordinator `AclSpec`, externally tagged like the
/// coordinator enum's default serde shape. `OpenWrite` needs no hive
/// membership (just a real target hive) yet still creates the full
/// hive-scoped link bundle; `HiveGroup` carries the witness contract.
#[derive(Debug, Serialize)]
pub enum AclSpec {
    HiveGroup {
        hive_genesis_hash: ActionHash,
        author_membership_hash: Option<ActionHash>,
        group_acl: AclByGroupGenesis,
        author_group_membership_hash: Option<ActionHash>,
        recipient_witnesses: Vec<RecipientWitness>,
    },
    OpenWrite {
        target_hive_genesis_hash: Option<ActionHash>,
    },
}

#[derive(Debug, Serialize)]
pub struct EncryptedContentHeader {
    pub id: String,
    pub display_hive_id: String,
    pub content_type: String,
    pub revision_author_signing_public_key: String,
    pub acl_spec: AclSpec,
    pub public_key_acl: Acl,
}

#[derive(Debug, Serialize)]
pub struct EncryptedContent {
    pub header: EncryptedContentHeader,
    pub bytes: SerializedBytes,
}

#[derive(Debug, Serialize)]
pub struct CreateEncryptedContentInput {
    pub id: String,
    pub display_hive_id: String,
    pub content_type: String,
    pub revision_author_signing_public_key: String,
    pub bytes: SerializedBytes,
    pub acl_spec: AclSpec,
    pub public_key_acl: Acl,
    pub dynamic_links: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct UpdateEncryptedContentInput {
    pub previous_encrypted_content_hash: ActionHash,
    pub updated_encrypted_content: EncryptedContent,
}

/// `EncryptedContentResponse` subset; serde ignores unasserted fields.
#[derive(Debug, Deserialize)]
pub struct CreateResponse {
    pub hash: String,
    #[serde(default)]
    pub latest_action_micros: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ListByHiveInput {
    pub hive_genesis_hash: ActionHash,
    pub content_type: String,
    pub since_ts: Option<i64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct CountByHiveInput {
    pub hive_genesis_hash: ActionHash,
    pub content_type: String,
    pub since_ts: Option<i64>,
}

// --- pass-6-pinned-hosts wire mirrors -----------------------------------------

#[derive(Debug, Serialize)]
pub struct HiveLinkPageInput {
    pub hive_genesis_hash: ActionHash,
    pub content_type: String,
    pub since_ts: Option<i64>,
    pub limit: Option<usize>,
    pub source_after_action_hash: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DynamicLinkPageInput {
    pub hive_genesis_hash: ActionHash,
    pub content_type: String,
    pub dynamic_link: String,
    pub since_ts: Option<i64>,
    pub limit: Option<usize>,
    pub source_after_action_hash: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthorLinkPageInput {
    pub author: String,
    pub content_type: String,
    pub since_ts: Option<i64>,
    pub limit: Option<usize>,
    pub source_after_action_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SourcePosition {
    pub timestamp_micros: i64,
    pub action_hash: String,
}

#[derive(Debug, Deserialize)]
pub struct ContentRecord {
    pub hash: String,
    pub original_hash: String,
    #[serde(default)]
    pub latest_action_micros: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct BoundedLinkPage {
    pub records: Vec<ContentRecord>,
    pub source_count: usize,
    pub source_positions: Vec<SourcePosition>,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct MyContentByIdInput {
    pub hive_genesis_hash: ActionHash,
    pub content_id: String,
}

#[derive(Debug, Deserialize)]
pub struct OwnContentRecords {
    pub records: Vec<ContentRecord>,
    pub truncated: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct BlobPinHint {
    pub hive_genesis_hash: ActionHash,
    pub blake3: String,
    pub byte_variant: String,
    pub provider_record_hash: ActionHash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_micros: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_agent: Option<AgentPubKey>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "pin")]
pub enum BlobPinSignal {
    Available(BlobPinHint),
    TakeNow(BlobPinHint),
}

#[derive(Debug, Serialize)]
pub struct SendBlobPinSignalInput {
    pub signal: BlobPinSignal,
    pub recipients: Vec<AgentPubKey>,
}

// --- pass-6-service-meter wire mirrors ---------------------------------------

#[derive(Debug, Serialize)]
pub struct UpsertServiceMeterInput {
    pub hive_genesis_hash: ActionHash,
    pub period: String,
    pub counters: BTreeMap<String, String>,
    pub display_hive_id: String,
    pub revision_author_signing_public_key: String,
    pub public_key_acl: Acl,
}

#[derive(Debug, Serialize)]
pub struct NodeSpecAttestation {
    pub app_signing_key_b64: String,
    pub signature_b64: String,
}

#[derive(Debug, Serialize)]
pub struct PublishNodeSpecInput {
    pub hive_genesis_hash: ActionHash,
    pub spec: BTreeMap<String, String>,
    pub declared_at_micros: i64,
    pub app_attestation: Option<NodeSpecAttestation>,
    pub display_hive_id: String,
    pub revision_author_signing_public_key: String,
    pub public_key_acl: Acl,
}

/// `Acl` subset carrying only the reader list.
#[derive(Debug, Deserialize)]
pub struct ReaderAcl {
    pub reader: Vec<String>,
}

/// `EncryptedContentHeader` subset for header-convergence assertions.
#[derive(Debug, Deserialize)]
pub struct ServiceRecordHeader {
    pub display_hive_id: String,
    pub public_key_acl: ReaderAcl,
}

/// `EncryptedContent` subset carrying the stored payload and header.
#[derive(Debug, Deserialize)]
pub struct EncryptedContentPayload {
    pub header: ServiceRecordHeader,
    pub bytes: SerializedBytes,
}

/// `EncryptedContentResponse` subset used by service-record assertions.
#[derive(Debug, Deserialize)]
pub struct EncryptedContentPayloadResponse {
    pub hash: String,
    pub encrypted_content: EncryptedContentPayload,
}

#[derive(Debug, Deserialize)]
pub struct UpsertContentResponse {
    pub response: EncryptedContentPayloadResponse,
    pub was_created: bool,
    pub was_updated: bool,
}

/// `BoundedLinkPage` subset; serde ignores paging metadata.
#[derive(Debug, Deserialize)]
pub struct ServiceRecordPage {
    pub records: Vec<EncryptedContentPayloadResponse>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct ServiceMeterSnapshot {
    pub schema: String,
    pub period: String,
    pub counters: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct NodeSpecSnapshot {
    pub schema: String,
    pub spec: BTreeMap<String, String>,
    pub declared_at_micros: i64,
    pub verified_by_app_key: Option<String>,
}

pub fn public_reader_acl(owner: &str) -> Acl {
    let mut acl = owner_only_acl(owner);
    acl.reader.push("*".to_string());
    acl
}

pub fn decode_content_payload<T>(record: &EncryptedContentPayloadResponse) -> T
where
    T: for<'de> Deserialize<'de> + std::fmt::Debug,
{
    holochain_types::prelude::decode(record.encrypted_content.bytes.bytes())
        .expect("decode service record snapshot from msgpack")
}

// --- Shared conductor helpers --------------------------------------------------

pub fn owner_only_acl(owner: &str) -> Acl {
    Acl {
        owner: owner.to_string(),
        admin: vec![],
        writer: vec![],
        reader: vec![],
    }
}

/// Commit one OpenWrite entry targeting `hive_genesis_hash` and return the
/// create-action hash (b64 string form, as the coordinator responds).
/// Bytes are unique per call: byte-identical entries content-address to ONE
/// entry hash, silently collapsing "duplicate root" test fixtures.
pub async fn create_open_write_content(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: ActionHash,
    content_type: &str,
    id: &str,
    dynamic_links: Option<Vec<String>>,
) -> String {
    static UNIQUE_ENTRY_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = UNIQUE_ENTRY_BYTES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let author = zome.cell_id().agent_pubkey().clone();
    let response: CreateResponse = conductor
        .call(
            zome,
            "create_encrypted_content",
            CreateEncryptedContentInput {
                id: id.to_string(),
                display_hive_id: "sweettest-hive".to_string(),
                content_type: content_type.to_string(),
                revision_author_signing_public_key: author.to_string(),
                bytes: UnsafeBytes::from(unique.to_le_bytes().to_vec()).into(),
                acl_spec: AclSpec::OpenWrite {
                    target_hive_genesis_hash: Some(hive_genesis_hash),
                },
                public_key_acl: owner_only_acl(&author.to_string()),
                dynamic_links,
            },
        )
        .await;
    response.hash
}

/// Poll `count_links_by_hive` every 50 ms until it returns `expected`, or
/// fail the test after 30 s. Holochain 0.6.1's cascade integrates
/// self-authored link ops on its own cadence after `await_consistency_s`
/// returns (the barrier only waits for ops already in the DHT-op table),
/// so a bare post-commit count assert flakes. Mirrors the
/// `wait_for_link_count` idiom holochain's own count_links tests use.
pub async fn wait_for_count_links_by_hive_to(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: &ActionHash,
    content_type: &str,
    expected: usize,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut latest: usize = usize::MAX;
    while std::time::Instant::now() < deadline {
        latest = conductor
            .call(
                zome,
                "count_links_by_hive",
                CountByHiveInput {
                    hive_genesis_hash: hive_genesis_hash.clone(),
                    content_type: content_type.to_string(),
                    since_ts: None,
                },
            )
            .await;
        if latest == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "count_links_by_hive did not reach {expected} within 30s of polling (last value {latest})"
    );
}

pub async fn my_content(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: ActionHash,
    content_id: &str,
) -> OwnContentRecords {
    conductor
        .call(
            zome,
            "get_my_content_by_id_link",
            MyContentByIdInput {
                hive_genesis_hash,
                content_id: content_id.to_string(),
            },
        )
        .await
}

/// Poll until the own-lookup sees `expected` records — self-authored link
/// ops integrate on the cascade's own cadence after `await_consistency_s`
/// (same idiom as `wait_for_count_links_by_hive_to`).
pub async fn wait_for_own_content_id_count(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive_genesis_hash: ActionHash,
    content_id: &str,
    expected: usize,
) -> OwnContentRecords {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut latest = my_content(conductor, zome, hive_genesis_hash.clone(), content_id).await;
    while latest.records.len() != expected && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
        latest = my_content(conductor, zome, hive_genesis_hash.clone(), content_id).await;
    }
    assert_eq!(
        latest.records.len(),
        expected,
        "own-lookup did not reach {expected} records within 30s"
    );
    latest
}
