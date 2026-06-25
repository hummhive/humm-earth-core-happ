#![allow(dead_code)]

use std::path::Path;

use holo_hash::{ActionHash, AgentPubKey};
use holochain::prelude::DnaFile;
use holochain::sweettest::{SweetCell, SweetConductor, SweetConductorBatch, SweetDnaFile, SweetZome};
use serde::{Deserialize, Serialize};

/// Expected DNA hash for the pass-6 dry-refactor bundle this suite must run
/// against.
///
/// Stale workdir bundles silently mask coordinator and integrity behavior; this
/// hash gate keeps conductor tests on the intended DNA generation.
pub const EXPECTED_DNA_HASH: &str =
    "uhC0kOQX5rU8yL6CIEWAfGu1G5TaNsgMcS7yp-D0fV2eG1-2bA7iJ";

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
    assert_eq!(
        actual, EXPECTED_DNA_HASH,
        "Stale workdir/humm_earth_core.dna — loaded DNA hash {actual} but expected pass-6 dry-refactor {EXPECTED_DNA_HASH}. \
         Rebuild: `nix develop --command bash -c 'npm run build:zomes && hc dna pack dnas/humm_earth_core/workdir'`."
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
