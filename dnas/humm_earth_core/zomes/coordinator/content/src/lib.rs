pub mod encrypted_content;
pub mod linking;

use content_integrity::*;
use hdk::prelude::*;
use std::collections::HashSet;
pub use linking::*;

pub fn set_cap_tokens() -> ExternResult<()> {
    let mut fns = HashSet::new();
    fns.insert((zome_info()?.name, "get_encrypted_content".into()));
    fns.insert((zome_info()?.name, "get_many_encrypted_conten".into()));
    fns.insert((
        zome_info()?.name,
        "get_encrypted_content_by_time_and_author".into(),
    ));
    fns.insert((zome_info()?.name, "list_by_dynamic_link".into()));
    fns.insert((zome_info()?.name, "list_by_hive_link".into()));
    fns.insert((zome_info()?.name, "get_by_content_id_link".into()));
    fns.insert((zome_info()?.name, "list_by_acl_link".into()));
    fns.insert((zome_info()?.name, "list_by_author".into()));
    // `recv_remote_signal` is invoked by the conductor on every agent
    // listed in `send_remote_signal`'s recipient list. The HDK requires
    // this cap to be granted explicitly by the receiver zome — see
    // `hdk::p2p::send_remote_signal` impl comment: "This requirement
    // will likely be removed in the future". Until then, granting
    // Unrestricted access is the standard pattern (the function only
    // re-emits the incoming signal locally; it doesn't expose any
    // state). This addition is purely additive — older clients that
    // don't call `send_remote_signal` are unaffected.
    fns.insert((zome_info()?.name, "recv_remote_signal".into()));

    let functions = GrantedFunctions::Listed(fns);
    create_cap_grant(CapGrantEntry {
        tag: "".into(),
        access: CapAccess::Unrestricted,
        functions,
    })?;
    Ok(())
}

#[hdk_extern]
pub fn init(_: ()) -> ExternResult<InitCallbackResult> {
    set_cap_tokens()?;
    Ok(InitCallbackResult::Pass)
}

/// Relay a remote signal to the local AppWebsocket subscribers.
///
/// When agent A calls `send_remote_signal(payload, [B, C, ...])`, the
/// conductor on each recipient (B, C, ...) invokes THIS function with
/// the payload. We re-emit it locally so the subscribed app (sidecar
/// / smoketest / future client) receives the same shape as the local
/// `emit_signal` path. Same payload, two delivery surfaces.
///
/// THREAT MODEL — read before consuming signals in a sidecar:
///
/// The cap grant on this function is `Unrestricted`, so any peer on
/// the network can call it with any `EncryptedContentSignal` payload.
/// The payload is attacker-controlled — fields like `hash`,
/// `original_hash`, and the inner `encrypted_content` are NOT
/// authenticated by the act of receiving a remote signal. Specifically:
///
///   - A peer can flood the signal queue (DoS against UI/sidecar
///     event loop); conductor-level p2p limits help but don't
///     eliminate this.
///   - A peer can emit signals whose `hash` references a real
///     committed entry but whose `encrypted_content` body is junk,
///     or vice versa.
///   - The signal and the DHT entry are decoupled: signal arrival
///     does NOT prove the entry was actually committed.
///
/// Sidecars MUST treat signal data as a HINT, not authoritative.
/// The legitimate use is "wake up and re-query the DHT for this
/// hash"; reading the signal's `encrypted_content` directly without
/// a follow-up `get_encrypted_content(hash)` is a confusion-attack
/// surface. The polling fallback (sidecar's periodic
/// `list_by_hive_link`) provides the authoritative read.
#[hdk_extern]
pub fn recv_remote_signal(payload: encrypted_content::EncryptedContentSignal) -> ExternResult<()> {
    // `#[hdk_extern]` unwraps the outer `ExternIO` and deserialises
    // into our typed input automatically — taking
    // `EncryptedContentSignal` directly lets the macro handle the
    // wire decode, giving a clean error if the payload shape ever
    // drifts (instead of the silent-corruption that an `ExternIO`
    // arg + manual `.decode()` would produce).
    //
    // Observability: log every arrival at info so cross-host DM
    // debugging has a concrete breadcrumb. Without this line the
    // receiver was completely silent — the only way to confirm
    // delivery was via downstream `emit_signal` side-effects which
    // require an AppWebsocket subscriber already attached. Pinned
    // by `.extraResearch/DM_SECURITY_RECV_REMOTE_SIGNAL_2026-05-21.md`.
    info!(
        "recv_remote_signal: action_type={:?} hash={}",
        payload.action_type, payload.data.hash,
    );
    emit_signal(payload)?;
    Ok(())
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Signal {
    LinkCreated {
        action: SignedActionHashed,
        link_type: LinkTypes,
    },
    LinkDeleted {
        action: SignedActionHashed,
        link_type: LinkTypes,
    },
    EntryCreated {
        action: SignedActionHashed,
        app_entry: EntryTypes,
    },
    EntryUpdated {
        action: SignedActionHashed,
        app_entry: EntryTypes,
        original_app_entry: EntryTypes,
    },
    EntryDeleted {
        action: SignedActionHashed,
        original_app_entry: EntryTypes,
    },
}
#[hdk_extern(infallible)]
pub fn post_commit(committed_actions: Vec<SignedActionHashed>) {
    for action in committed_actions {
        if let Err(err) = signal_action(action) {
            error!("Error signaling new action: {:?}", err);
        }
    }
}
fn signal_action(action: SignedActionHashed) -> ExternResult<()> {
    match action.hashed.content.clone() {
        Action::CreateLink(create_link) => {
            if let Ok(Some(link_type)) =
                LinkTypes::from_type(create_link.zome_index, create_link.link_type)
            {
                emit_signal(Signal::LinkCreated { action, link_type })?;
            }
            Ok(())
        }
        Action::DeleteLink(delete_link) => {
            let record = get(delete_link.link_add_address.clone(), GetOptions { strategy: GetStrategy::Network })?.ok_or(
                wasm_error!(WasmErrorInner::Guest(
                    "Failed to fetch CreateLink action".to_string()
                )),
            )?;
            match record.action() {
                Action::CreateLink(create_link) => {
                    if let Ok(Some(link_type)) =
                        LinkTypes::from_type(create_link.zome_index, create_link.link_type)
                    {
                        emit_signal(Signal::LinkDeleted { action, link_type })?;
                    }
                    Ok(())
                }
                _ => {
                    return Err(wasm_error!(WasmErrorInner::Guest(
                        "Create Link should exist".to_string()
                    )));
                }
            }
        }
        Action::Create(_create) => {
            if let Ok(Some(app_entry)) = get_entry_for_action(&action.hashed.hash) {
                emit_signal(Signal::EntryCreated { action, app_entry })?;
            }
            Ok(())
        }
        Action::Update(update) => {
            if let Ok(Some(app_entry)) = get_entry_for_action(&action.hashed.hash) {
                if let Ok(Some(original_app_entry)) =
                    get_entry_for_action(&update.original_action_address)
                {
                    emit_signal(Signal::EntryUpdated {
                        action,
                        app_entry,
                        original_app_entry,
                    })?;
                }
            }
            Ok(())
        }
        Action::Delete(delete) => {
            if let Ok(Some(original_app_entry)) = get_entry_for_action(&delete.deletes_address) {
                emit_signal(Signal::EntryDeleted {
                    action,
                    original_app_entry,
                })?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
fn get_entry_for_action(action_hash: &ActionHash) -> ExternResult<Option<EntryTypes>> {
    let record = match get_details(action_hash.clone(), GetOptions { strategy: GetStrategy::Network })? {
        Some(Details::Record(record_details)) => record_details.record,
        _ => {
            return Ok(None);
        }
    };
    let entry = match record.entry().as_option() {
        Some(entry) => entry,
        None => {
            return Ok(None);
        }
    };
    let (zome_index, entry_index) = match record.action().entry_type() {
        Some(EntryType::App(AppEntryDef {
            zome_index,
            entry_index,
            ..
        })) => (zome_index, entry_index),
        _ => {
            return Ok(None);
        }
    };
    Ok(EntryTypes::deserialize_from_type(
        zome_index.clone(),
        entry_index.clone(),
        entry,
    )?)
}

/// Input for [`get_messages_since`].
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct GetMessagesSinceInput {
    pub since_seq: u32,
}

/// Return all source-chain records committed after `since_seq` (exclusive),
/// with entries included.
///
/// This queries the LOCAL agent's own source chain only — it is NOT a DHT
/// or cross-agent query. It is intended for the startup cache to detect
/// outgoing messages that were committed after the cache was last written.
///
/// `since_seq = u32::MAX` causes `saturating_add(1)` to wrap to 0, which
/// returns the full chain — this is the intended behaviour for a "full
/// resync" path.
#[hdk_extern]
pub fn get_messages_since(input: GetMessagesSinceInput) -> ExternResult<Vec<Record>> {
    debug!(
        "get_messages_since: querying chain seq_range=[{}, {}]",
        input.since_seq.saturating_add(1),
        u32::MAX,
    );
    let filter = ChainQueryFilter::new()
        .sequence_range(ChainQueryFilterRange::ActionSeqRange(
            input.since_seq.saturating_add(1),
            u32::MAX,
        ))
        .include_entries(true);
    query(filter)
}
