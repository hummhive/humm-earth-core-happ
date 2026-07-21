//! Conductor proof for pass-7 M11: `role_key_closure` returns the
//! dominated role set with each role's canonical system-role
//! `GroupGenesis` identity (`None` for a role group that does not exist).

mod support;

use std::time::Duration;

use holo_hash::ActionHash;
use holochain::sweettest::{SweetConductor, SweetZome};
use serde::{Deserialize, Serialize};
use support::{create_hive, single_conductor_cell_app, CreateGroupGenesisInput, GenesisResponse};

const POLL_ATTEMPTS: usize = 200;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Serialize)]
struct RoleKeyClosureInput {
    hive_genesis_hash: ActionHash,
    granted_role: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct RoleClosureEntry {
    role: String,
    #[serde(default)]
    group_genesis_hash: Option<ActionHash>,
}

#[derive(Debug, Deserialize)]
struct RoleKeyClosure {
    entries: Vec<RoleClosureEntry>,
}

async fn create_system_role_group(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive: &ActionHash,
    role: &str,
) -> ActionHash {
    let response: GenesisResponse = conductor
        .call(
            zome,
            "create_group_genesis",
            CreateGroupGenesisInput {
                hive_genesis_hash: hive.clone(),
                display_id: format!("squuid-{role}"),
                hive_wide_role: Some(role.to_string()),
                creator_hive_membership_hash: None,
            },
        )
        .await;
    response.hash
}

async fn closure_for(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive: &ActionHash,
    granted_role: &str,
) -> RoleKeyClosure {
    conductor
        .call(
            zome,
            "role_key_closure",
            RoleKeyClosureInput {
                hive_genesis_hash: hive.clone(),
                granted_role: granted_role.to_string(),
            },
        )
        .await
}

/// Poll until the closure for `granted_role` resolves `expected_some`
/// role groups (HiveToGroups links integrate on the cascade's cadence).
async fn poll_closure_resolved(
    conductor: &SweetConductor,
    zome: &SweetZome,
    hive: &ActionHash,
    granted_role: &str,
    expected_some: usize,
) -> RoleKeyClosure {
    for _ in 0..POLL_ATTEMPTS {
        let closure = closure_for(conductor, zome, hive, granted_role).await;
        let resolved = closure
            .entries
            .iter()
            .filter(|e| e.group_genesis_hash.is_some())
            .count();
        if resolved == expected_some {
            return closure;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    panic!("closure for {granted_role} never resolved {expected_some} role groups");
}

#[tokio::test(flavor = "multi_thread")]
async fn closure_pairs_each_dominated_role_with_its_genesis() {
    holochain_trace::test_run();
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "closure-hive").await;
    let admin_group = create_system_role_group(&conductor, &zome, &hive, "Admin").await;
    let writer_group = create_system_role_group(&conductor, &zome, &hive, "Writer").await;
    let reader_group = create_system_role_group(&conductor, &zome, &hive, "Reader").await;

    let admin_closure = poll_closure_resolved(&conductor, &zome, &hive, "Admin", 3).await;
    assert_eq!(
        admin_closure.entries,
        vec![
            RoleClosureEntry {
                role: "Admin".into(),
                group_genesis_hash: Some(admin_group),
            },
            RoleClosureEntry {
                role: "Writer".into(),
                group_genesis_hash: Some(writer_group),
            },
            RoleClosureEntry {
                role: "Reader".into(),
                group_genesis_hash: Some(reader_group.clone()),
            },
        ],
        "Admin closure = [Admin, Writer, Reader] highest-first, each with its genesis"
    );

    let reader_closure = closure_for(&conductor, &zome, &hive, "Reader").await;
    assert_eq!(
        reader_closure.entries,
        vec![RoleClosureEntry {
            role: "Reader".into(),
            group_genesis_hash: Some(reader_group),
        }],
        "Reader dominates only itself"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn closure_marks_missing_role_groups_none() {
    holochain_trace::test_run();
    let (conductor, _cell, zome) = single_conductor_cell_app().await;
    let hive = create_hive(&conductor, &zome, "sparse-closure-hive").await;
    let admin_group = create_system_role_group(&conductor, &zome, &hive, "Admin").await;
    let reader_group = create_system_role_group(&conductor, &zome, &hive, "Reader").await;

    let closure = poll_closure_resolved(&conductor, &zome, &hive, "Owner", 2).await;
    assert_eq!(
        closure.entries,
        vec![
            RoleClosureEntry {
                role: "Owner".into(),
                group_genesis_hash: None,
            },
            RoleClosureEntry {
                role: "Admin".into(),
                group_genesis_hash: Some(admin_group),
            },
            RoleClosureEntry {
                role: "Writer".into(),
                group_genesis_hash: None,
            },
            RoleClosureEntry {
                role: "Reader".into(),
                group_genesis_hash: Some(reader_group),
            },
        ],
        "missing role groups surface as None, present ones resolve"
    );
}
