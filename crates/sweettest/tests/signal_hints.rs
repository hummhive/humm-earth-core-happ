mod support;

use std::time::Duration;

use holo_hash::{ActionHash, AgentPubKey};
use holochain::sweettest::{await_consistency_s, SweetCell, SweetConductor};
use holochain_types::prelude::{SerializedBytes, Signal, UnsafeBytes};
use holochain_zome_types::prelude::ExternIO;
use serde::{Deserialize, Serialize};
use support::{
    create_hive, setup_cells, Acl, AclBucket, AclByGroupGenesis, AclSpec,
    CreateEncryptedContentInput, CreateGroupGenesisInput, CreateGroupMembershipInput,
    CreateResponse, GenesisResponse, MembershipResponse, RecipientWitness,
};
use tokio::sync::broadcast::Receiver;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EncryptedContentHint {
    action_type: String,
    hash: String,
    original_hash: String,
    #[serde(default)]
    from_agent: Option<AgentPubKey>,
}

#[derive(Debug, Deserialize)]
struct FullContentSignal {
    action_type: String,
    data: FullContentSignalData,
    #[serde(default)]
    from_agent: Option<AgentPubKey>,
}

#[derive(Debug, Deserialize)]
struct FullContentSignalData {
    hash: String,
    original_hash: String,
    encrypted_content: FullEncryptedContent,
}

#[derive(Debug, Deserialize)]
struct FullEncryptedContent {
    bytes: SerializedBytes,
}

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

async fn receive_content_hint(
    signals: &mut Receiver<Signal>,
) -> (EncryptedContentHint, ExternIO) {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let signal = signals.recv().await.expect("signal channel stays open");
            let Signal::App {
                signal: app_signal, ..
            } = signal
            else {
                continue;
            };
            let encoded = app_signal.into_inner();
            if let Ok(hint) = encoded.clone().decode::<EncryptedContentHint>() {
                return (hint, encoded);
            }
        }
    })
    .await
    .expect("Bob must receive the content hint within 60s")
}

async fn receive_full_content_signal(signals: &mut Receiver<Signal>) -> FullContentSignal {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let signal = signals.recv().await.expect("signal channel stays open");
            let Signal::App {
                signal: app_signal, ..
            } = signal
            else {
                continue;
            };
            if let Ok(full) = app_signal.into_inner().decode::<FullContentSignal>() {
                return full;
            }
        }
    })
    .await
    .expect("Alice must receive the full local content signal within 60s")
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

async fn create_reader_membership(
    conductor: &SweetConductor,
    alice: &SweetCell,
    hive_genesis_hash: &ActionHash,
    bob: &AgentPubKey,
) -> (ActionHash, ActionHash) {
    let group: GenesisResponse = conductor
        .call(
            &alice.zome("content"),
            "create_group_genesis",
            CreateGroupGenesisInput {
                hive_genesis_hash: hive_genesis_hash.clone(),
                display_id: "signal-reader-group".to_string(),
                hive_wide_role: None,
                creator_hive_membership_hash: None,
            },
        )
        .await;
    let membership: MembershipResponse = conductor
        .call(
            &alice.zome("content"),
            "create_group_membership",
            CreateGroupMembershipInput {
                group_genesis_hash: group.hash.clone(),
                for_agent: bob.clone(),
                role: "Reader".to_string(),
                grantor_membership_hash: None,
                grantor_hive_membership_hash: None,
                expiry: None,
            },
        )
        .await;
    (group.hash, membership.hash)
}

fn reader_content_input(
    hive_genesis_hash: ActionHash,
    group_genesis_hash: ActionHash,
    membership_hash: ActionHash,
    alice: &AgentPubKey,
    bob: &AgentPubKey,
) -> CreateEncryptedContentInput {
    CreateEncryptedContentInput {
        id: "signal-hint-content".to_string(),
        display_hive_id: "signal-hint-hive".to_string(),
        content_type: "sweettest-signal-hint".to_string(),
        revision_author_signing_public_key: alice.to_string(),
        bytes: UnsafeBytes::from(vec![1_u8, 2, 3]).into(),
        acl_spec: AclSpec::HiveGroup {
            hive_genesis_hash,
            author_membership_hash: None,
            group_acl: AclByGroupGenesis {
                owner: group_genesis_hash,
                admin: vec![],
                writer: vec![],
                reader: vec![],
            },
            author_group_membership_hash: None,
            recipient_witnesses: vec![RecipientWitness {
                pubkey: bob.clone(),
                bucket: AclBucket::Reader,
                membership_hash,
            }],
        },
        public_key_acl: Acl {
            owner: String::new(),
            admin: vec![],
            writer: vec![],
            reader: vec![bob.to_string()],
        },
        dynamic_links: None,
    }
}

async fn prepare_reader_content(
    conductor: &SweetConductor,
    alice: &SweetCell,
    bob: &SweetCell,
) -> CreateEncryptedContentInput {
    let hive = create_hive(conductor, &alice.zome("content"), "signal-hint-hive").await;
    let (group, membership) =
        create_reader_membership(conductor, alice, &hive, bob.agent_pubkey()).await;
    await_consistency_s(60, [alice, bob]).await.unwrap();
    reader_content_input(
        hive,
        group,
        membership,
        alice.agent_pubkey(),
        bob.agent_pubkey(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn remote_reader_receives_hint_without_ciphertext() {
    holochain_trace::test_run();
    let (conductors, cells) = setup_cells(2).await;
    let (alice, bob) = (&cells[0], &cells[1]);
    let alice_key = alice.agent_pubkey().clone();
    await_consistency_s(30, [alice, bob]).await.unwrap();
    let input = prepare_reader_content(&conductors[0], alice, bob).await;

    let mut alice_signals = conductors[0]
        .raw_handle()
        .subscribe_to_app_signals("test-app".to_string());
    let mut bob_signals = conductors[1]
        .raw_handle()
        .subscribe_to_app_signals("test-app".to_string());
    let created: CreateResponse = conductors[0]
        .call(&alice.zome("content"), "create_encrypted_content", input)
        .await;

    let (remote_hint, remote_wire) = receive_content_hint(&mut bob_signals).await;
    assert_eq!(remote_hint.action_type, "Create");
    assert_eq!(remote_hint.hash, created.hash);
    assert_eq!(remote_hint.original_hash, created.hash);
    assert_eq!(remote_hint.from_agent.as_ref(), Some(&alice_key));
    assert!(
        remote_wire.decode::<FullContentSignal>().is_err(),
        "reader-side hint wire must not contain the full payload or ciphertext"
    );

    let local_full = receive_full_content_signal(&mut alice_signals).await;
    assert_eq!(local_full.action_type, "Create");
    assert_eq!(local_full.data.hash, created.hash);
    assert_eq!(local_full.data.original_hash, created.hash);
    assert_eq!(
        local_full.data.encrypted_content.bytes.bytes(),
        &[1_u8, 2, 3]
    );
    assert!(local_full.from_agent.is_none(), "local signal is unstamped");
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
struct ForgedContentHint {
    action_type: String,
    hash: String,
    original_hash: String,
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

    let mut bob_signals = conductors[1]
        .raw_handle()
        .subscribe_to_app_signals("test-app".to_string());

    // Bob feeds his OWN recv_remote_signal a hint that lies about its origin.
    let forged = ForgedContentHint {
        action_type: "Create".to_string(),
        hash: "deadbeef".to_string(),
        original_hash: "deadbeef".to_string(),
        from_agent: Some(alice_key.clone()),
    };
    let payload = ExternIO::encode(&forged).expect("encode forged hint");
    let _: () = conductors[1]
        .call(&bob.zome("content"), "recv_remote_signal", payload)
        .await;

    let (received, _) = receive_content_hint(&mut bob_signals).await;
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
