pub mod encrypted_content;
pub mod group;
pub mod hive;
pub mod inbox;
pub mod linking;

use content_integrity::*;
use hdk::prelude::*;
pub use linking::*;
use std::collections::HashSet;

use encrypted_content::signals::{DmRemoteSignal, EncryptedContentSignal};

/// Register every coordinator-callable extern as an `Unrestricted` cap
/// grant. Called from `init` (which is `#[hdk_extern]`); the conductor
/// re-runs `init` on coordinator hot-swap, so newly-registered functions
/// pick up grants automatically without a user wipe.
///
/// Two classes of extern are excluded from the granted set:
///
/// 1. **Source-chain mutators** — every `create_*` / `update_*` /
///    `delete_*` extern writes the calling agent's source chain.
///    Granting them `Unrestricted` would let any peer pollute another
///    agent's chain. The local UI reaches them through the conductor's
///    AppWebsocket auth without needing a cap grant.
/// 2. **Source-chain readers that surface private data** —
///    `get_messages_since` and `get_last_probe` both call `query(...)`
///    against the caller's local chain. A remote cap-call would
///    enumerate or leak chain-private contents (action hashes,
///    `DmProbeLog` cursors). Local UI only.
///
/// The sender-side ephemeral signal externs (`send_dm_delete_request`
/// and the three `send_dm_call_*` variants) are excluded for a third
/// reason: each issues `send_remote_signal` to a caller-chosen
/// recipient. Granting them remotely would let any peer use the local
/// agent as a signal reflector to a third party — both an
/// amplification DoS and a spoof-by-proxy vector that subverts the
/// `from_agent` provenance guarantee enforced by `recv_remote_signal`.
///
/// `recv_remote_signal` itself MUST stay granted — the conductor
/// invokes it on every recipient of a `send_remote_signal` and the cap
/// check applies even to that conductor-internal call (per hdk source:
/// "This requirement will likely be removed in the future").
pub fn set_cap_tokens() -> ExternResult<()> {
    let zome = zome_info()?.name;
    let mut fns = HashSet::new();

    // Read surface — every query extern over public DHT data.
    fns.insert((zome.clone(), "get_encrypted_content".into()));
    fns.insert((zome.clone(), "get_many_encrypted_content".into()));
    fns.insert((
        zome.clone(),
        "get_encrypted_content_by_time_and_author".into(),
    ));
    fns.insert((zome.clone(), "list_by_dynamic_link".into()));
    fns.insert((zome.clone(), "list_by_hive_link".into()));
    fns.insert((zome.clone(), "get_by_content_id_link".into()));
    fns.insert((zome.clone(), "list_by_acl_link".into()));
    fns.insert((zome.clone(), "list_by_author".into()));
    fns.insert((zome.clone(), "count_links_by_hive".into()));
    fns.insert((zome.clone(), "fetch_pair_ss_with_hive_check".into()));

    // Migration-marker readers (V1 + V2). Read already-public DHT data
    // (walks an entry's update chain; same data any peer could fetch
    // via `get_details` directly). The write-side externs
    // (`mark_migrated`, `mark_migrated_v2`) are intentionally NOT
    // granted by Rule 1 above. Both readers apply their own
    // author-binding filter — only updates by the original entry's
    // author count as valid markers; see `get_migration_marker`'s
    // doc-comment — so they do not rely on the cap surface for forge
    // resistance.
    fns.insert((zome.clone(), "get_migration_marker".into()));
    fns.insert((zome.clone(), "get_migration_marker_v2".into()));

    // Hive-membership read externs. Surface DHT-public data
    // (`HiveGenesis` and `HiveMembership` entries are public; the link
    // space is public). The write counterparts
    // `create_hive_genesis` / `create_hive_membership` are excluded by
    // Rule 1.
    fns.insert((zome.clone(), "get_latest_membership".into()));
    fns.insert((zome.clone(), "list_my_hives".into()));

    // Group-authority read externs (pass-3). Same rationale as the
    // hive read surface above: GroupGenesis and GroupMembership entries
    // are public DHT data and the corresponding link space is public.
    // The write counterparts (create_group_genesis,
    // create_group_membership, revoke_group_membership) are excluded by
    // Rule 1 — granting them Unrestricted would let any peer pollute
    // another agent's chain with a forged source-chain action.
    fns.insert((zome.clone(), "get_latest_group_membership".into()));
    fns.insert((zome.clone(), "list_group_members".into()));
    fns.insert((zome.clone(), "list_my_groups".into()));
    fns.insert((zome.clone(), "list_groups_in_hive".into()));
    fns.insert((zome.clone(), "get_group_genesis".into()));

    // Inbox read externs. `probe_inbox` walks the PUBLIC DHT link
    // space keyed off the receiving agent's own pubkey
    // (`agent_info().agent_initial_pubkey`); a remote cap-call resolves
    // the receiving agent to the LOCAL conductor's pubkey, making the
    // grant structurally inert from a remote peer's vantage point.
    //
    // `get_last_probe` is intentionally NOT granted: it `query(...)`s
    // the LOCAL source chain for the private `DmProbeLog` entry, and a
    // remote cap-call would leak the private read-receipt cursor
    // (`probed_at_microseconds` + `last_processed_inbox_link_hash`).
    // Matches the `get_messages_since` treatment (Rule 2 above).
    fns.insert((zome.clone(), "probe_inbox".into()));

    // `recv_remote_signal` — required by the HDK cap check; see the
    // doc-comment opener for the rationale.
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

/// Multi-signal dispatcher: try-decode an incoming remote signal
/// against each known wire shape in priority order.
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
/// **Anti-spoof guarantee** (preserved from the original single-signal
/// path). Whatever
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
    match LinkTypes::from_type(create_link.zome_index, create_link.link_type) {
        Ok(Some(link_type)) => {
            emit_signal(Signal::LinkCreated { action, link_type })?;
        }
        // Expected: link belongs to a different zome's link-type registry —
        // not a signal this zome should fan out.
        Ok(None) => {}
        Err(err) => {
            warn!("signal_link_created: LinkTypes::from_type failed; signal skipped: {err:?}");
        }
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
    match LinkTypes::from_type(create_link.zome_index, create_link.link_type) {
        Ok(Some(link_type)) => {
            emit_signal(Signal::LinkDeleted { action, link_type })?;
        }
        // Expected: link belongs to a different zome's link-type registry —
        // not a signal this zome should fan out.
        Ok(None) => {}
        Err(err) => {
            warn!("signal_link_deleted: LinkTypes::from_type failed; signal skipped: {err:?}");
        }
    }
    Ok(())
}

fn signal_entry_created(action: SignedActionHashed) -> ExternResult<()> {
    match get_entry_for_action(&action.hashed.hash) {
        Ok(Some(app_entry)) => {
            emit_signal(Signal::EntryCreated { action, app_entry })?;
        }
        // Expected: action carries no app entry, or entry is shaped for a
        // different zome / not yet retrievable — caller's polling fallback
        // covers any post-commit DHT gossip lag.
        Ok(None) => {}
        Err(err) => {
            warn!("signal_entry_created: get_entry_for_action failed; signal skipped: {err:?}");
        }
    }
    Ok(())
}

fn signal_entry_updated(action: SignedActionHashed, update: Update) -> ExternResult<()> {
    let app_entry = match get_entry_for_action(&action.hashed.hash) {
        Ok(Some(entry)) => entry,
        // Expected: same shape as `signal_entry_created`'s Ok(None) arm.
        Ok(None) => return Ok(()),
        Err(err) => {
            warn!(
                "signal_entry_updated: get_entry_for_action(new) failed; signal skipped: {err:?}"
            );
            return Ok(());
        }
    };
    let original_app_entry = match get_entry_for_action(&update.original_action_address) {
        Ok(Some(entry)) => entry,
        // Expected: original entry not retrievable (deleted, shaped for a
        // different zome). Skip the signal rather than fire a partial one.
        Ok(None) => return Ok(()),
        Err(err) => {
            warn!(
                "signal_entry_updated: get_entry_for_action(original) failed; signal skipped: {err:?}"
            );
            return Ok(());
        }
    };
    emit_signal(Signal::EntryUpdated {
        action,
        app_entry,
        original_app_entry,
    })?;
    Ok(())
}

fn signal_entry_deleted(action: SignedActionHashed, delete: Delete) -> ExternResult<()> {
    match get_entry_for_action(&delete.deletes_address) {
        Ok(Some(original_app_entry)) => {
            emit_signal(Signal::EntryDeleted {
                action,
                original_app_entry,
            })?;
        }
        // Expected: original entry not retrievable (already deleted, shaped
        // for a different zome). Skip the signal.
        Ok(None) => {}
        Err(err) => {
            warn!("signal_entry_deleted: get_entry_for_action failed; signal skipped: {err:?}");
        }
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
