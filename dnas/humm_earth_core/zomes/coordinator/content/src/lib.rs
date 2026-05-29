pub mod encrypted_content;
pub mod linking;

use content_integrity::*;
use hdk::prelude::*;
use std::collections::HashSet;
pub use linking::*;

use encrypted_content::signals::{DmRemoteSignal, EncryptedContentSignal};

/// Register every coordinator-callable extern as an `Unrestricted` cap
/// grant. Called from `init` (which is `#[hdk_extern]`); the conductor
/// re-runs `init` on coordinator hot-swap, so newly-registered functions
/// pick up grants automatically without a user wipe.
///
/// **C5** fixes:
/// - Typo: `get_many_encrypted_conten` → `get_many_encrypted_content`.
///   Pre-fix, cross-agent callers calling the externally-correct name
///   silently failed at the cap check; this affected any RPC pattern
///   relying on remote calls to that function.
/// - Adds grants for query externs newly introduced this pass:
///   `count_links_by_hive` (C3) and `fetch_pair_ss_with_hive_check`
///   (C4). Both are read-only queries over already-public DHT data, so
///   `Unrestricted` matches the pattern of every other `list_by_*` /
///   `get_*` extern.
///
/// `recv_remote_signal` MUST stay granted — the conductor invokes it on
/// every recipient of a `send_remote_signal` and the cap check applies
/// even to that conductor-internal call (per hdk source: "This
/// requirement will likely be removed in the future").
///
/// **NOT GRANTED** (deliberate — security-reviewer SEC-2):
///
/// - `send_dm_delete_request` (C6) and the three `send_dm_call_*` (C7)
///   externs are SENDER-side: each calls `send_remote_signal` to a
///   caller-chosen recipient. Granting them `Unrestricted` would let
///   any peer call_remote them on MY cell to use my agent as a signal
///   reflector to a third party — both an amplification DoS and a
///   spoof-by-proxy vector that subverts the C1 `from_agent` guarantee
///   (the third party would cryptographically attribute the signal to
///   ME, with attacker-chosen payload). Local-UI callers do NOT need
///   the grant — they reach the extern through the conductor's
///   AppWebsocket auth (see the unchanged `create_encrypted_content` /
///   `update_encrypted_content` / `delete_encrypted_content` precedent,
///   which are intentionally not in `set_cap_tokens` yet work fine
///   from humm-tauri's local UI).
/// - `get_messages_since` queries the LOCAL source chain; granting it
///   would let any peer enumerate every action hash committed to my
///   chain. Stays ungranted by design — do not add it here.
/// - `create_encrypted_content` / `update_encrypted_content` /
///   `delete_encrypted_content` mutate this agent's source chain;
///   stays ungranted (only the author should write to their own chain).
pub fn set_cap_tokens() -> ExternResult<()> {
    let zome = zome_info()?.name;
    let mut fns = HashSet::new();

    // CRUD + read externs.
    fns.insert((zome.clone(), "get_encrypted_content".into()));
    fns.insert((zome.clone(), "get_many_encrypted_content".into())); // C5 typo fix
    fns.insert((zome.clone(), "get_encrypted_content_by_time_and_author".into()));
    fns.insert((zome.clone(), "list_by_dynamic_link".into()));
    fns.insert((zome.clone(), "list_by_hive_link".into()));
    fns.insert((zome.clone(), "get_by_content_id_link".into()));
    fns.insert((zome.clone(), "list_by_acl_link".into()));
    fns.insert((zome.clone(), "list_by_author".into()));

    // C3 — new count extern.
    fns.insert((zome.clone(), "count_links_by_hive".into()));

    // C4 — new intersection-fetch extern.
    fns.insert((zome.clone(), "fetch_pair_ss_with_hive_check".into()));


    // `recv_remote_signal` is invoked by the conductor on every agent
    // listed in `send_remote_signal`'s recipient list. The HDK requires
    // this cap to be granted explicitly by the receiver zome — see
    // `hdk::p2p::send_remote_signal` impl comment: "This requirement
    // will likely be removed in the future". Until then, granting
    // Unrestricted access is the standard pattern (the function only
    // re-emits the incoming signal locally; it doesn't expose any
    // state). This addition is purely additive — older clients that
    // don't call `send_remote_signal` are unaffected.
    fns.insert((zome.clone(), "recv_remote_signal".into()));

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

/// **C7b** — multi-signal dispatcher.
///
/// Holochain permits exactly one `recv_remote_signal` extern per zome,
/// so adding new signal families (DM delete-request, WebRTC signalling)
/// without breaking the shipped `EncryptedContentSignal` wire path
/// requires this single entry point to try-decode the incoming
/// `ExternIO` against each known shape in priority order.
///
/// **Ordering matters.** `EncryptedContentSignal` is tried FIRST because
/// it is the established, shipped payload — every running humm-tauri
/// today emits and expects this shape. `DmRemoteSignal` (the C6/C7
/// envelope) is tried second.
///
/// **Why the try-decode is safe (vs. structural ambiguity).**
/// - `EncryptedContentSignal` requires `action_type` (a small enum tag)
///   and `data` (a full `EncryptedContentResponse` map with `hash` etc).
/// - `DmRemoteSignal` is `#[serde(tag = "kind")]` with variants
///   `DmDeleteRequest { thread_id, target_action_hash, … }` and
///   `DmCall(DmCallSignal)`. Each variant requires the `kind`
///   discriminator plus its own variant fields.
/// - The two shapes share no required field name (the
///   `action_type` / `kind` discriminators differ, and the inner
///   payload shapes are structurally disjoint), so neither can
///   structurally decode as the other under msgpack. The host-side
///   serde round-trip unit tests in
///   `encrypted_content::signals::tests` empirically pin this property.
///
/// **Anti-spoof guarantee (C1, preserved through C7b refactor).** Whatever
/// the wire payload claimed about `from_agent` is overwritten with
/// `call_info()?.provenance` — the lair-attested AgentPubKey of the
/// caller. A peer cannot impersonate another peer in the emitted
/// signal even if they hand-craft the payload.
///
/// THREAT MODEL — read before consuming signals in a sidecar:
///
/// The cap grant on this function is `Unrestricted`, so any peer on
/// the network can call it with any decode-able payload. Beyond the
/// `from_agent` stamping, the payload body is attacker-controlled —
/// fields like `hash`, `original_hash`, the inner `encrypted_content`,
/// the `target_action_hash`, the SDP blob — are NOT authenticated by
/// the act of receiving a remote signal. Specifically:
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
pub fn recv_remote_signal(signal: ExternIO) -> ExternResult<()> {
    let caller_agent = call_info()?.provenance;

    // 1. Try the established / shipped wire shape FIRST. Byte-for-byte
    //    compatible with the C1 path — pre-existing cross-host DM
    //    senders see identical receiver behaviour.
    if let Ok(mut payload) = signal.decode::<EncryptedContentSignal>() {
        info!(
            "recv_remote_signal[EncryptedContentSignal]: action_type={:?} hash={} from_agent={}",
            payload.action_type, payload.data.hash, caller_agent,
        );
        payload.from_agent = Some(caller_agent);
        return emit_signal(payload);
    }

    // 2. Try the new C6/C7 envelope.
    if let Ok(mut payload) = signal.decode::<DmRemoteSignal>() {
        info!(
            "recv_remote_signal[DmRemoteSignal]: variant={} from_agent={}",
            match &payload {
                DmRemoteSignal::DmDeleteRequest(_) => "DmDeleteRequest",
                DmRemoteSignal::DmCall(_) => "DmCall",
            },
            caller_agent,
        );
        payload.stamp_from_agent(caller_agent);
        return emit_signal(payload);
    }

    // 3. Unknown payload shape — explicitly error so misrouted or
    //    malformed signals are visible in conductor logs rather than
    //    silently dropped. The cap grant is open so a misbehaving peer
    //    can absolutely send garbage; this is the audit trail.
    Err(wasm_error!(WasmErrorInner::Guest(
        "recv_remote_signal: payload did not decode as EncryptedContentSignal or DmRemoteSignal"
            .into()
    )))
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

/// Dispatch a single committed action to the local signal channel. Per the
/// repo's <60-line/fn guidance, the per-variant emission lives in dedicated
/// helpers below — keeps `signal_action` itself a one-screen-tall switch.
fn signal_action(action: SignedActionHashed) -> ExternResult<()> {
    match action.hashed.content.clone() {
        Action::CreateLink(create_link) => signal_link_created(action, create_link),
        Action::DeleteLink(delete_link) => signal_link_deleted(action, delete_link),
        Action::Create(_) => signal_entry_created(action),
        Action::Update(update) => signal_entry_updated(action, update),
        Action::Delete(delete) => signal_entry_deleted(action, delete),
        _ => Ok(()),
    }
}

fn signal_link_created(action: SignedActionHashed, create_link: CreateLink) -> ExternResult<()> {
    if let Ok(Some(link_type)) =
        LinkTypes::from_type(create_link.zome_index, create_link.link_type)
    {
        emit_signal(Signal::LinkCreated { action, link_type })?;
    }
    Ok(())
}

fn signal_link_deleted(action: SignedActionHashed, delete_link: DeleteLink) -> ExternResult<()> {
    let record = get(
        delete_link.link_add_address.clone(),
        GetOptions {
            strategy: GetStrategy::Network,
        },
    )?
    .ok_or(wasm_error!(WasmErrorInner::Guest(
        "Failed to fetch CreateLink action".to_string()
    )))?;
    let Action::CreateLink(create_link) = record.action() else {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "Create Link should exist".to_string()
        )));
    };
    if let Ok(Some(link_type)) =
        LinkTypes::from_type(create_link.zome_index, create_link.link_type)
    {
        emit_signal(Signal::LinkDeleted { action, link_type })?;
    }
    Ok(())
}

fn signal_entry_created(action: SignedActionHashed) -> ExternResult<()> {
    if let Ok(Some(app_entry)) = get_entry_for_action(&action.hashed.hash) {
        emit_signal(Signal::EntryCreated { action, app_entry })?;
    }
    Ok(())
}

fn signal_entry_updated(action: SignedActionHashed, update: Update) -> ExternResult<()> {
    if let Ok(Some(app_entry)) = get_entry_for_action(&action.hashed.hash) {
        if let Ok(Some(original_app_entry)) = get_entry_for_action(&update.original_action_address)
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

fn signal_entry_deleted(action: SignedActionHashed, delete: Delete) -> ExternResult<()> {
    if let Ok(Some(original_app_entry)) = get_entry_for_action(&delete.deletes_address) {
        emit_signal(Signal::EntryDeleted {
            action,
            original_app_entry,
        })?;
    }
    Ok(())
}

fn get_entry_for_action(action_hash: &ActionHash) -> ExternResult<Option<EntryTypes>> {
    let record = match get_details(
        action_hash.clone(),
        GetOptions {
            strategy: GetStrategy::Network,
        },
    )? {
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
