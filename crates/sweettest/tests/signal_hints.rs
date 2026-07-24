mod support;

use std::time::Duration;

use holo_hash::{ActionHash, AgentPubKey};
use holochain::sweettest::await_consistency_s;
use holochain_types::prelude::Signal;
use holochain_zome_types::prelude::ExternIO;
use serde::{Deserialize, Serialize};
use support::{create_hive, setup_cells};
use tokio::sync::broadcast::Receiver;

#[derive(Debug, Deserialize)]
struct OwnerHandoffOfferHint {
    offer_hash: ActionHash,
    hive_genesis_hash: ActionHash,
    #[serde(default)]
    from_agent: Option<AgentPubKey>,
}

#[derive(Debug, Serialize)]
struct InitiateOwnerHandoffInput {
    hive_genesis_hash: ActionHash,
    to_agent: AgentPubKey,
    offerer_owner_accept_hash: Option<ActionHash>,
}

async fn receive_owner_handoff_hint(signals: &mut Receiver<Signal>) -> OwnerHandoffOfferHint {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let signal = signals.recv().await.expect("signal channel stays open");
            let Signal::App {
                signal: app_signal, ..
            } = signal
            else {
                continue;
            };
            if let Ok(hint) = app_signal.into_inner().decode::<OwnerHandoffOfferHint>() {
                return hint;
            }
        }
    })
    .await
    .expect("Bob must receive the owner-handoff offer hint within 60s")
}

#[tokio::test(flavor = "multi_thread")]
async fn owner_handoff_offer_hint_delivers_to_recipient() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_key = alice.agent_pubkey().clone();
    let bob_key = bob.agent_pubkey().clone();
    await_consistency_s(30, [alice, bob]).await.unwrap();

    let hive = create_hive(&conductors[0], &alice.zome("content"), "handoff-hint-hive").await;
    await_consistency_s(60, [alice, bob]).await.unwrap();
    let mut bob_signals = conductors[1]
        .raw_handle()
        .subscribe_to_app_signals("test-app".to_string());
    let offer_hash: ActionHash = conductors[0]
        .call(
            &alice.zome("content"),
            "initiate_owner_handoff",
            InitiateOwnerHandoffInput {
                hive_genesis_hash: hive.clone(),
                to_agent: bob_key,
                offerer_owner_accept_hash: None,
            },
        )
        .await;

    let hint = receive_owner_handoff_hint(&mut bob_signals).await;
    assert_eq!(hint.offer_hash, offer_hash);
    assert_eq!(hint.hive_genesis_hash, hive);
    assert_eq!(hint.from_agent.as_ref(), Some(&alice_key));
}

#[derive(Debug, Serialize)]
struct ForgedOwnerHandoffOfferHint {
    offer_hash: ActionHash,
    hive_genesis_hash: ActionHash,
    from_agent: Option<AgentPubKey>,
}

/// recv_remote_signal MUST overwrite any sender-supplied `from_agent` with the
/// real call provenance, so a spoofed origin cannot survive.
#[tokio::test(flavor = "multi_thread")]
async fn recv_overwrites_forged_provenance() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_key = alice.agent_pubkey().clone();
    let bob_key = bob.agent_pubkey().clone();
    await_consistency_s(30, [alice, bob]).await.unwrap();

    let forged_hash = create_hive(
        &conductors[1],
        &bob.zome("content"),
        "forged-handoff-hint-hive",
    )
    .await;
    let mut bob_signals = conductors[1]
        .raw_handle()
        .subscribe_to_app_signals("test-app".to_string());

    // Bob feeds his OWN recv_remote_signal a hint that lies about its origin.
    let forged = ForgedOwnerHandoffOfferHint {
        offer_hash: forged_hash.clone(),
        hive_genesis_hash: forged_hash,
        from_agent: Some(alice_key.clone()),
    };
    let payload = ExternIO::encode(&forged).expect("encode forged hint");
    let _: () = conductors[1]
        .call(&bob.zome("content"), "recv_remote_signal", payload)
        .await;

    let received = receive_owner_handoff_hint(&mut bob_signals).await;
    assert_eq!(
        received.from_agent.as_ref(),
        Some(&bob_key),
        "recv must stamp the real provenance (the caller), overwriting the forge"
    );
    assert_ne!(
        received.from_agent.as_ref(),
        Some(&alice_key),
        "forged from_agent must NOT survive recv_remote_signal"
    );
}

