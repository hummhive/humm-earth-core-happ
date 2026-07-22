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
