//! Conductor proof for the pass-7 durable `HiveMembershipIndex`: hive
//! discovery survives a full Inbox sweep, founders appear via the
//! self-index, and the index link is author-only-deletable.

mod support;

use std::time::Duration;

use holo_hash::ActionHash;
use holochain::sweettest::{await_consistency_s, SweetConductor, SweetZome};
use holochain_types::prelude::Signal;
use serde::{Deserialize, Serialize};
use support::{create_hive, grant_hive_membership, setup_cells, ListedHive};

#[derive(Debug, Serialize)]
struct ProbeInboxInput {
    event_filter: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InboxItem {
    link_action_hash: ActionHash,
}

/// Coordinator `Signal` subset: only the `LinkCreated` arm, decoded per
/// signal; every other variant fails the decode and is skipped.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum LinkSignal {
    LinkCreated {
        action: SignedActionMirror,
        link_type: String,
    },
}

#[derive(Debug, Deserialize)]
struct SignedActionMirror {
    hashed: HashedActionMirror,
}

#[derive(Debug, Deserialize)]
struct HashedActionMirror {
    hash: ActionHash,
}

async fn list_my_hives(conductor: &SweetConductor, zome: &SweetZome) -> Vec<ListedHive> {
    conductor.call(zome, "list_my_hives", ()).await
}

/// Consume every inbox item the agent currently sees; returns how many
/// were swept.
async fn sweep_inbox(conductor: &SweetConductor, zome: &SweetZome) -> usize {
    let items: Vec<InboxItem> = conductor
        .call(zome, "probe_inbox", ProbeInboxInput { event_filter: None })
        .await;
    for item in &items {
        let _: ActionHash = conductor
            .call(zome, "consume_inbox_item", item.link_action_hash.clone())
            .await;
    }
    items.len()
}

async fn first_index_link_hash(
    signals: &mut tokio::sync::broadcast::Receiver<Signal>,
) -> ActionHash {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let signal = signals.recv().await.expect("signal channel stays open");
            let Signal::App {
                signal: app_signal, ..
            } = signal
            else {
                continue;
            };
            let Ok(LinkSignal::LinkCreated { action, link_type }) =
                app_signal.into_inner().decode::<LinkSignal>()
            else {
                continue;
            };
            if link_type == "HiveMembershipIndex" {
                return action.hashed.hash;
            }
        }
    })
    .await
    .expect("HiveMembershipIndex LinkCreated signal within 60s")
}

#[tokio::test(flavor = "multi_thread")]
async fn hive_discovery_survives_full_inbox_sweep() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_zome = alice.zome("content");
    let bob_zome = bob.zome("content");
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let hive = create_hive(&conductors[0], &alice_zome, "sweep-proof-hive").await;
    grant_hive_membership(
        &conductors[0],
        &alice_zome,
        hive.clone(),
        bob.agent_pubkey().clone(),
        "Writer",
        None,
        None,
        None,
    )
    .await;
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let before = list_my_hives(&conductors[1], &bob_zome).await;
    assert!(
        before
            .iter()
            .any(|h| h.display_id == "sweep-proof-hive" && h.role.as_deref() == Some("Writer")),
        "granted hive must be discoverable via the durable index; got {before:?}"
    );

    let swept = sweep_inbox(&conductors[1], &bob_zome).await;
    assert!(swept > 0, "sweep must consume the HiveInvite inbox item");
    let leftover: Vec<InboxItem> = conductors[1]
        .call(&bob_zome, "probe_inbox", ProbeInboxInput { event_filter: None })
        .await;
    assert!(leftover.is_empty(), "sweep must leave the inbox empty");

    let after = list_my_hives(&conductors[1], &bob_zome).await;
    assert!(
        after
            .iter()
            .any(|h| h.display_id == "sweep-proof-hive" && h.role.as_deref() == Some("Writer")),
        "hive discovery must survive the inbox sweep (durable index); got {after:?}"
    );

    let founder_view = list_my_hives(&conductors[0], &alice_zome).await;
    assert!(
        founder_view
            .iter()
            .any(|h| h.display_id == "sweep-proof-hive" && h.role.is_none()),
        "founder must see the hive via the self-index; got {founder_view:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn membership_index_link_rejects_foreign_delete() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_zome = alice.zome("content");
    let bob_zome = bob.zome("content");
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let mut alice_signals = conductors[0]
        .raw_handle()
        .subscribe_to_app_signals("test-app".to_string());
    let _hive = create_hive(&conductors[0], &alice_zome, "foreign-delete-hive").await;
    let index_link_hash = first_index_link_hash(&mut alice_signals).await;
    await_consistency_s(60, [alice, bob]).await.unwrap();

    let rejected: Result<ActionHash, _> = conductors[1]
        .call_fallible(&bob_zome, "consume_inbox_item", index_link_hash)
        .await;
    let err = format!(
        "{:?}",
        rejected.expect_err("non-author index-link delete must fail validation")
    );
    assert!(
        err.contains("HiveMembershipIndex link may only be deleted by its author"),
        "delete must be rejected by the author-only validator; got: {err}"
    );
}
