pub mod encrypted_content;
pub mod group;
pub mod hive;
pub mod inbox;
pub mod invite;
pub mod linking;

use content_integrity::*;
use hdk::prelude::*;
pub use linking::*;
use std::collections::HashSet;

use encrypted_content::signals::{BlobPinSignal, DmRemoteSignal, EncryptedContentSignal};

/// Fetch + decode a DHT entry, tolerating absence AND an undecodable/wrong-type
/// target as `None`. Intentional resilience: owner-resolution and inbox scans
/// must not let one forged or not-yet-propagated link abort the whole read — a
/// hard error here would be a cheap DoS on the governance gate.
pub(crate) fn get_typed_entry<T: TryFrom<SerializedBytes, Error = SerializedBytesError>>(
    action_hash: &ActionHash,
) -> ExternResult<Option<T>> {
    Ok(get(action_hash.clone(), GetOptions::network())?
        .and_then(|record| record.entry().to_app_option::<T>().ok().flatten()))
}

/// Like [`get_typed_entry`] but returns the fetched action's timestamp too and
/// takes an explicit [`GetOptions`] (network vs local store). Same tolerance:
/// absent or wrong-shape targets resolve to `None`.
pub(crate) fn get_typed_entry_with_timestamp<
    T: TryFrom<SerializedBytes, Error = SerializedBytesError>,
>(
    action_hash: &ActionHash,
    options: GetOptions,
) -> ExternResult<Option<(T, Timestamp)>> {
    Ok(get(action_hash.clone(), options)?.and_then(|record| {
        let ts = record.action().timestamp();
        record
            .entry()
            .to_app_option::<T>()
            .ok()
            .flatten()
            .map(|entry| (entry, ts))
    }))
}

/// Delete every `CreateLink` on the caller's OWN source chain whose target is
/// `target`. Self-scoping: a foreign caller authored none of these links, so a
/// cross-author call is a harmless no-op (the link-delete validators require
/// deleter == link author). `EncryptedContentUpdates` (target = revision, not
/// original) is never matched and stays immortal by design.
pub(crate) fn delete_own_links_targeting(target: AnyLinkableHash) -> ExternResult<()> {
    for link_record in query(ChainQueryFilter::new().include_entries(false))? {
        if let Action::CreateLink(create_link) = link_record.action() {
            if create_link.target_address == target {
                delete_link(link_record.action_address().clone(), GetOptions::network())?;
            }
        }
    }
    Ok(())
}

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
/// The sender-side ephemeral signal externs (`send_dm_delete_request`,
/// the three `send_dm_call_*` variants, and `send_blob_pin_signal`)
/// are excluded for a third reason: each issues `send_remote_signal`
/// to a caller-chosen recipient. Granting them remotely would let any
/// peer use the local agent as a signal reflector to a third party —
/// both an amplification DoS and a spoof-by-proxy vector that subverts
/// the `from_agent` provenance guarantee enforced by
/// `recv_remote_signal`.
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
    fns.insert((zome.clone(), "list_by_dynamic_link".into()));
    fns.insert((zome.clone(), "list_by_hive_link".into()));
    fns.insert((zome.clone(), "get_by_content_id_link".into()));
    fns.insert((zome.clone(), "list_by_acl_link".into()));
    fns.insert((zome.clone(), "role_key_closure".into()));
    fns.insert((zome.clone(), "list_by_author".into()));
    fns.insert((zome.clone(), "count_links_by_hive".into()));
    fns.insert((zome.clone(), "fetch_pair_ss_with_hive_check".into()));
    // Pass-7 lineage reverse-lookup over public DHT link space; no author
    // scoping, so it grants like the other public readers above.
    fns.insert((zome.clone(), "resolve_by_prior_generation".into()));

    // Bounded source-cursor page externs (pass-6-pinned-hosts): read-only
    // queries over public DHT link space, same grant class as their
    // legacy twins above. `get_my_content_by_id_link` is intentionally
    // NOT granted — "my" is provenance-derived, and a remote grant would
    // let any peer enumerate the callee's own records (same treatment as
    // `get_messages_since`).
    fns.insert((zome.clone(), "list_by_hive_link_page".into()));
    fns.insert((zome.clone(), "list_by_dynamic_link_page".into()));
    fns.insert((zome.clone(), "list_by_author_page".into()));

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
    // Dormancy-proof twins (pass-4 migration rescue). Same read
    // surface as their network counterparts above (the agent's OWN
    // hive list + own membership), but resolved against the local
    // source chain (founder) + local DHT store (joiner) only — no
    // network authority is consulted. Granted on identical security
    // footing: the inputs scope the read to the caller's own pubkey,
    // and `list_my_hives_local` returns ONLY the caller's own hives
    // (founder branch reads the caller's chain; joiner branch filters
    // `for_agent != caller`). No new data is exposed vs the network
    // variants. `mark_migrated_v2` stays UNgranted (Rule 1 — it
    // mutates the source chain via `update_encrypted_content`).
    fns.insert((zome.clone(), "list_my_hives_local".into()));
    fns.insert((zome.clone(), "get_latest_membership_local".into()));

    // Owner-handoff reads (pass-5): public DHT data; the mutators
    // (initiate/cancel/accept/revoke) are excluded by Rule 1.
    fns.insert((zome.clone(), "get_member_hive_role".into()));
    fns.insert((zome.clone(), "list_member_hive_roles".into()));
    fns.insert((zome.clone(), "list_pending_owner_handoffs".into()));

    // pass-5 humm-tauri read helpers over PUBLIC DHT data. The local-chain
    // readers `my_pair_shared_secret_exists` + `changes_since` are
    // intentionally NOT granted (same treatment as `get_messages_since` /
    // `get_last_probe`: a remote cap-call would leak local source-chain state).
    fns.insert((zome.clone(), "get_hive_owner".into()));
    fns.insert((zome.clone(), "content_summary".into()));
    // pass-6-idempotent-writes: `content_summary_many` shares
    // `content_summary`'s public-link-space read class. This
    // generation's other externs stay UNgranted: the three
    // `find_or_create_*` externs and `remediate_hiveless_content` are
    // mutators (Rule 1 — a remote grant would let peers write to the
    // callee's chain), and `list_my_hiveless_content` is own-content
    // enumeration (same treatment as `get_my_content_by_id_link`).
    fns.insert((zome.clone(), "content_summary_many".into()));
    fns.insert((zome.clone(), "is_ownership_contested".into()));

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
    fns.insert((zome.clone(), "probe_inbox_page".into()));

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
/// envelope) is tried second; `BlobPinSignal` (pass-6-pinned-hosts
/// blob-pin hints) is tried last — new families always append.
///
/// **Why the try-decode is safe (vs. structural ambiguity).**
/// - `EncryptedContentSignal` requires `action_type` (a small enum tag)
///   and `data` (a full `EncryptedContentResponse` map with `hash` etc).
/// - `DmRemoteSignal` is `#[serde(tag = "kind")]` with variants
///   `DmDeleteRequest { thread_id, target_action_hash, … }` and
///   `DmCall(DmCallSignal)`. Each variant requires the `kind`
///   discriminator plus its own variant fields.
/// - `BlobPinSignal` is `#[serde(tag = "pin")]` — a third distinct
///   discriminator key with hint fields (`blake3`,
///   `provider_record_hash`) required by no other family.
/// - The shapes share no required field name (the `action_type` /
///   `kind` / `pin` discriminators differ, and the inner payload
///   shapes are structurally disjoint), so none can structurally
///   decode as another under msgpack. The host-side serde round-trip
///   unit tests in `encrypted_content::signals::tests` empirically pin
///   this property.
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

    // 3. Try the pass-6-pinned-hosts blob-pin hint family.
    if let Ok(mut payload) = signal.decode::<BlobPinSignal>() {
        info!(
            "recv_remote_signal[BlobPinSignal]: variant={} from_agent={}",
            match &payload {
                BlobPinSignal::Available(_) => "Available",
                BlobPinSignal::TakeNow(_) => "TakeNow",
            },
            caller_agent,
        );
        payload.stamp_from_agent(caller_agent);
        return emit_signal(payload);
    }

    // 4. Unknown payload shape — explicitly error so misrouted or
    //    malformed signals are visible in conductor logs rather than
    //    silently dropped. The cap grant is open so a misbehaving peer
    //    can absolutely send garbage; this is the audit trail.
    Err(wasm_error!(WasmErrorInner::Guest(
        "recv_remote_signal: payload did not decode as EncryptedContentSignal, DmRemoteSignal, or BlobPinSignal"
            .into()
    )))
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
// Transient local signal enum (post_commit -> emit_signal/remote_signal); boxing the
// EntryTypes-bearing variants is serde-wire-safe but the perf gain is marginal. Allow.
#[allow(clippy::large_enum_variant)]
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
    let record =
        get(delete_link.link_add_address.clone(), GetOptions::network())?.ok_or(wasm_error!(
            WasmErrorInner::Guest("Failed to fetch CreateLink action".to_string())
        ))?;
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
    let record = match get_details(action_hash.clone(), GetOptions::network())? {
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
    EntryTypes::deserialize_from_type(*zome_index, *entry_index, entry)
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
/// Pass `since_seq = 0` to replay the full chain: the range becomes
/// `(1, u32::MAX)`, returning every action after the genesis `Dna`
/// action. Any higher value is an incremental cursor — only actions
/// with sequence greater than `since_seq` are returned.
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
