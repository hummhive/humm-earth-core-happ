use hdk::prelude::*;

/// Ephemeral signal: "please tombstone this message on your side". Carries
/// only metadata — never the encrypted body, never any key material. The
/// receiver decides whether to honor it (typically: yes, if `from_agent`
/// matches a thread participant whose own messages are being recalled).
///
/// Fire-and-forget: no DHT entry, no retry, no offline delivery. The
/// in-payload "kind: delete_request" alternative (the route humm-tauri ships
/// today) IS persisted and survives the recipient being offline; this
/// ephemeral variant is the lower-latency, no-metadata-leak version for
/// online peers.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct DmDeleteRequestSignal {
    /// Application-level thread/conversation identifier. Opaque to the
    /// zome — used only to scope which recipient handlers should react.
    pub thread_id: String,
    /// Action hash of the message to delete. The receiver re-validates
    /// against its own state before acting; the zome does not enforce
    /// authorization here (the cap grant on `send_dm_delete_request` is
    /// open and the receiver MUST make the trust decision itself).
    pub target_action_hash: ActionHash,
    /// Stamped by the dispatcher from `call_info()?.provenance`. None on
    /// the wire from the sender; Some on every arrival.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_agent: Option<AgentPubKey>,
}

/// Ephemeral WebRTC signaling envelope. Three variants cover the
/// browser-to-browser handshake: who initiates (`InitRequest`), who
/// accepts (`InitAccept`), and SDP/ICE payload exchange (`SdpData`).
///
/// `data: String` for `SdpData` is an opaque blob (SDP or ICE candidate
/// JSON, depending on what the application puts on the wire). The zome
/// NEVER parses it — it is a pass-through. The application layer is
/// responsible for framing.
///
/// Sized for typical SDP exchanges (a few KB). Large media exchanges
/// would go through dedicated transport, not these signals.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
#[serde(tag = "type")]
pub enum DmCallSignal {
    InitRequest {
        /// Opaque application-level call identifier. The zome treats it
        /// as a string — uniqueness/format is the caller's concern.
        call_id: String,
        /// Stamped by the dispatcher on arrival.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_agent: Option<AgentPubKey>,
    },
    InitAccept {
        call_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_agent: Option<AgentPubKey>,
    },
    SdpData {
        call_id: String,
        /// Opaque SDP / ICE payload. Zome never inspects this.
        data: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_agent: Option<AgentPubKey>,
    },
}

/// Tagged envelope for every NEW (C6/C7) ephemeral signal. Distinct from
/// `EncryptedContentSignal` so the dispatcher in `recv_remote_signal` can
/// disambiguate via ordered try-decode of `ExternIO`.
///
/// Why a single envelope: the dispatcher only gets one chance at each
/// payload shape. By wrapping the new signal families in one tagged enum,
/// adding a third family later costs a variant, not a new try-decode
/// arm.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
#[serde(tag = "kind")]
pub enum DmRemoteSignal {
    /// C6 — ephemeral "please delete this message" request.
    ///
    /// DEPRECATED (pass-6-idempotent-writes): redundant since pass-5 —
    /// `validate_delete_encrypted_content` authorizes any
    /// `public_key_acl.reader` (either DM party) to author a durable
    /// native Delete; prefer `delete_encrypted_content`. Wire-live for
    /// old callers; removal candidate for a future generation after
    /// humm-tauri confirms no adoption.
    DmDeleteRequest(DmDeleteRequestSignal),
    /// C7 — WebRTC signaling (init request / accept / SDP data).
    DmCall(DmCallSignal),
}

impl DmRemoteSignal {
    /// Replace whatever the wire payload claimed about the sender with the
    /// conductor-attested provenance. Called from `recv_remote_signal`
    /// after a successful decode. The shape mirrors what
    /// `EncryptedContentSignal` does inline in the dispatcher.
    pub fn stamp_from_agent(&mut self, agent: AgentPubKey) {
        match self {
            DmRemoteSignal::DmDeleteRequest(s) => s.from_agent = Some(agent),
            DmRemoteSignal::DmCall(DmCallSignal::InitRequest { from_agent, .. })
            | DmRemoteSignal::DmCall(DmCallSignal::InitAccept { from_agent, .. })
            | DmRemoteSignal::DmCall(DmCallSignal::SdpData { from_agent, .. }) => {
                *from_agent = Some(agent);
            }
        }
    }
}
