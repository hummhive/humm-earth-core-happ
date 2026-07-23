use base64::Engine;
use content_integrity::Acl;
use hdk::prelude::*;

use super::blob_pin::{BlobPinSignal, BLOB_PIN_SIGNAL_MAX_RECIPIENTS};
use super::content::EncryptedContentHint;
use super::dm::{DmCallSignal, DmDeleteRequestSignal, DmRemoteSignal};

fn send_encoded_remote_signal<I>(signal: I, recipients: Vec<AgentPubKey>) -> ExternResult<()>
where
    I: Serialize + std::fmt::Debug,
{
    send_remote_signal(remote_signal_payload(&signal)?, recipients)
}

pub(super) fn remote_signal_payload<I>(signal: &I) -> ExternResult<ExternIO>
where
    I: Serialize + std::fmt::Debug,
{
    ExternIO::encode(signal).map_err(|e| wasm_error!(e))
}

fn decode_acl_reader_pubkey(encoded: &str) -> Option<AgentPubKey> {
    let stripped = encoded.strip_prefix('u')?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(stripped)
        .ok()?;
    AgentPubKey::try_from_raw_39(bytes).ok()
}

fn acl_reader_recipients(public_key_acl: &Acl, self_pubkey: &AgentPubKey) -> Vec<AgentPubKey> {
    public_key_acl
        .reader
        .iter()
        .filter_map(|reader| decode_acl_reader_pubkey(reader))
        .filter(|reader| reader != self_pubkey)
        .collect()
}

pub fn remote_signal_acl_readers(public_key_acl: &Acl, hint: EncryptedContentHint) {
    let self_pubkey = match agent_info() {
        Ok(info) => info.agent_initial_pubkey,
        Err(err) => {
            debug!("remote_signal_acl_readers: agent_info() failed: {err:?}");
            return;
        }
    };
    let raw_count = public_key_acl.reader.len();
    let recipients = acl_reader_recipients(public_key_acl, &self_pubkey);
    info!(
        "remote_signal_acl_readers: raw_count={} valid_recipients={} action_type={:?}",
        raw_count,
        recipients.len(),
        hint.action_type,
    );
    if recipients.is_empty() {
        return;
    }
    if let Err(err) = send_encoded_remote_signal(hint, recipients) {
        debug!("remote_signal_acl_readers: remote signal send failed (non-fatal): {err:?}");
    }
}

/// C6 input: target a single recipient with an ephemeral "please delete"
/// request. The recipient's `recv_remote_signal` will see a
/// `DmRemoteSignal::DmDeleteRequest` with `from_agent` stamped.
#[derive(Serialize, Deserialize, Debug)]
pub struct SendDmDeleteRequestInput {
    pub thread_id: String,
    pub target_action_hash: ActionHash,
    pub recipient: AgentPubKey,
}

/// C7 input: announce a call to a single recipient. Pairs with
/// `send_dm_call_init_accept` (acceptance) and `send_dm_call_sdp_data`
/// (subsequent SDP exchange).
#[derive(Serialize, Deserialize, Debug)]
pub struct SendDmCallInitRequestInput {
    pub call_id: String,
    pub recipient: AgentPubKey,
}

/// C7 input: accept a previously-announced call.
#[derive(Serialize, Deserialize, Debug)]
pub struct SendDmCallInitAcceptInput {
    pub call_id: String,
    pub recipient: AgentPubKey,
}

/// C7 input: forward an opaque SDP / ICE blob. The zome NEVER parses
/// `data` — application layer (humm-tauri's `dm-webrtc-store.ts`)
/// owns the WebRTC state machine.
#[derive(Serialize, Deserialize, Debug)]
pub struct SendDmCallSdpDataInput {
    pub call_id: String,
    pub data: String,
    pub recipient: AgentPubKey,
}

fn send_dm_remote_signal(signal: DmRemoteSignal, recipient: AgentPubKey) -> ExternResult<()> {
    send_encoded_remote_signal(signal, vec![recipient])
}

/// DEPRECATED (pass-6-idempotent-writes): redundant since pass-5 —
/// `validate_delete_encrypted_content` authorizes any
/// `public_key_acl.reader` (either DM party) to author a durable native
/// Delete; prefer `delete_encrypted_content`. This ephemeral
/// best-effort family remains wire-live for old callers and is a
/// removal candidate for a future generation after humm-tauri confirms
/// no adoption.
#[hdk_extern]
pub fn send_dm_delete_request(input: SendDmDeleteRequestInput) -> ExternResult<()> {
    let signal = DmRemoteSignal::DmDeleteRequest(DmDeleteRequestSignal {
        thread_id: input.thread_id,
        target_action_hash: input.target_action_hash,
        from_agent: None,
    });
    send_dm_remote_signal(signal, input.recipient)
}

#[hdk_extern]
pub fn send_dm_call_init_request(input: SendDmCallInitRequestInput) -> ExternResult<()> {
    let signal = DmRemoteSignal::DmCall(DmCallSignal::InitRequest {
        call_id: input.call_id,
        from_agent: None,
    });
    send_dm_remote_signal(signal, input.recipient)
}

#[hdk_extern]
pub fn send_dm_call_init_accept(input: SendDmCallInitAcceptInput) -> ExternResult<()> {
    let signal = DmRemoteSignal::DmCall(DmCallSignal::InitAccept {
        call_id: input.call_id,
        from_agent: None,
    });
    send_dm_remote_signal(signal, input.recipient)
}

#[hdk_extern]
pub fn send_dm_call_sdp_data(input: SendDmCallSdpDataInput) -> ExternResult<()> {
    let signal = DmRemoteSignal::DmCall(DmCallSignal::SdpData {
        call_id: input.call_id,
        data: input.data,
        from_agent: None,
    });
    send_dm_remote_signal(signal, input.recipient)
}

/// Input for `send_blob_pin_signal`: one hint fanned out to an explicit
/// recipient list (bounded — see `BLOB_PIN_SIGNAL_MAX_RECIPIENTS`).
#[derive(Serialize, Deserialize, Debug)]
pub struct SendBlobPinSignalInput {
    pub signal: BlobPinSignal,
    pub recipients: Vec<AgentPubKey>,
}

/// Local-only sender (NOT cap-granted — a remote grant would make the
/// agent a signal reflector). `from_agent` is forced to `None` before
/// send; the receiver's dispatcher stamps conductor-attested provenance.
#[hdk_extern]
pub fn send_blob_pin_signal(input: SendBlobPinSignalInput) -> ExternResult<()> {
    if input.recipients.is_empty() {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "send_blob_pin_signal: recipients must be non-empty".into()
        )));
    }
    if input.recipients.len() > BLOB_PIN_SIGNAL_MAX_RECIPIENTS {
        return Err(wasm_error!(WasmErrorInner::Guest(format!(
            "send_blob_pin_signal: at most {BLOB_PIN_SIGNAL_MAX_RECIPIENTS} recipients per call"
        ))));
    }
    let mut signal = input.signal;
    signal.clear_from_agent();
    send_encoded_remote_signal(signal, input.recipients)
}
