//! Signal types and remote-signal helpers for the `content` zome.
//!
//! Two unrelated signal families coexist on the single `recv_remote_signal`
//! callback (Holochain only permits one `recv_remote_signal` extern per zome,
//! so multi-feature support is achieved via an ordered try-decode dispatcher
//! in `crate::lib::recv_remote_signal`):
//!
//! 1. `EncryptedContentSignal` — the shipped DM/content path. Fired locally
//!    via `emit_signal` and pushed cross-host via `remote_signal_acl_readers`
//!    when an entry's `public_key_acl.reader` list is non-empty. The
//!    `from_agent` field is populated on the receiver from
//!    `call_info().provenance` (the conductor-attested caller), NOT from the
//!    payload — this is the C1 anti-spoof guarantee.
//!
//! 2. `DmRemoteSignal` — the new ephemeral envelope wrapping
//!    `DmDeleteRequestSignal` (C6) and `DmCallSignal` (C7). These are
//!    fire-and-forget peer-to-peer signals with NO DHT entry; offline peers
//!    miss them. Use when the message itself IS the protocol step (delete
//!    request, WebRTC SDP exchange). Each variant carries its own
//!    `from_agent` field that the dispatcher fills in with the same
//!    conductor-attested caller pubkey.

use hdk::prelude::*;

use content_integrity::Acl;

use super::EncryptedContentResponse;

/// Signal payload emitted on entry create/update/delete. Carried on the
/// local `emit_signal` channel AND on cross-host `send_remote_signal` to
/// every agent in `public_key_acl.reader` (minus the author).
///
/// `from_agent` is the C1 anti-spoof bit. On local emit it is `None`; on
/// `recv_remote_signal` arrival the dispatcher overwrites whatever the
/// payload carried with `call_info()?.provenance` — the lair-attested
/// caller pubkey. Sidecar consumers MUST trust `from_agent` as the
/// authoritative sender identity and ignore any other "from" hint in the
/// payload body.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct EncryptedContentSignal {
    pub action_type: EncryptedContentSignalType,
    pub data: EncryptedContentResponse,
    /// Populated by recv_remote_signal from call_info().provenance.
    /// None for locally-emitted signals (post_commit / create / update paths
    /// where the conductor runs on the author's own Node — no remote caller).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_agent: Option<AgentPubKey>,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub enum EncryptedContentSignalType {
    Create,
    Update,
    Delete,
}

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

/// Best-effort remote delivery of a content signal to every agent
/// listed in the entry's `public_key_acl.reader` (other than the
/// author). Local `emit_signal` always fires first — it's the
/// existing pre-this-change behaviour and the only signal source the
/// author's own UI needs. `send_remote_signal` is purely additive:
/// it gives recipients an immediate notification instead of waiting
/// for sidecar-side DHT polling to discover the new entry.
///
/// Failures here (malformed base64, recipient offline, network error)
/// MUST NOT propagate — the entry is already committed on the source
/// chain, the local signal already fired, and the sidecar's polling
/// fallback will eventually deliver the record regardless. Logging at
/// `debug` keeps post-mortems possible without flooding production
/// logs.
///
/// Backwards compatibility: this function only READS the existing
/// `Acl::reader: Vec<String>` field. No schema changes. Every client
/// that already populates the ACL (i.e. all of them) gets remote
/// delivery transparently the moment this zome is deployed. Old
/// clients writing entries without a `public_key_acl.reader` still
/// work — the recipient list is empty, no remote signal fires, and
/// behaviour matches the pre-change zome exactly.
pub fn remote_signal_acl_readers(public_key_acl: &Acl, signal: EncryptedContentSignal) {
    use base64::Engine;
    // `Acl::reader` carries `AgentPubKey` strings produced by the
    // `@holochain/client` helper `encodeHashToBase64`, which emits the
    // multibase holohash form `'u' + URL_SAFE_NO_PAD(39 bytes)` — a
    // 53-char string like `uhCAk7VFb…`. STANDARD-base64 decoders reject
    // these on three independent grounds: the `'u'` prefix is not a
    // base64 char, the URL-safe `-`/`_` chars are not in the STANDARD
    // alphabet, and length 53 mod 4 = 1 is invalid for any padded
    // variant. Pre-2026-05-21 this function used STANDARD and silently
    // dropped every recipient — `recipients` was always empty,
    // `send_remote_signal` was NEVER called, and cross-host DMs
    // depended entirely on slow DHT gossip. Fix: strip the multibase
    // prefix and decode with URL_SAFE_NO_PAD.
    let urlsafe = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let self_pubkey = match agent_info() {
        Ok(info) => info.agent_initial_pubkey,
        Err(err) => {
            debug!("remote_signal_acl_readers: agent_info() failed: {err:?}");
            return;
        }
    };
    let raw_count = public_key_acl.reader.len();
    let recipients: Vec<AgentPubKey> = public_key_acl
        .reader
        .iter()
        .filter_map(|s| s.strip_prefix('u'))
        .filter_map(|stripped| urlsafe.decode(stripped).ok())
        .filter_map(|bytes| AgentPubKey::try_from_raw_39(bytes).ok())
        .filter(|pk| *pk != self_pubkey)
        .collect();
    // Observability: emit BOTH counts so post-mortems can distinguish
    // "ACL was empty" (raw=0) from "every entry failed to decode"
    // (raw>0, valid=0 — the pre-fix silent-drop shape). Logged at info
    // because cross-host DM delivery hinges on this path and we want
    // the breadcrumb in production logs, not only at debug.
    info!(
        "remote_signal_acl_readers: raw_count={} valid_recipients={} action_type={:?}",
        raw_count,
        recipients.len(),
        signal.action_type,
    );
    if recipients.is_empty() {
        return;
    }
    if let Err(err) = send_remote_signal(signal, recipients) {
        debug!("remote_signal_acl_readers: send_remote_signal failed (non-fatal): {err:?}");
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

/// C6 — fire an ephemeral delete request at a single peer.
///
/// `from_agent: None` on the wire; the receiver's dispatcher overwrites
/// with the conductor-attested caller. Failure to deliver (peer offline,
/// network error) is reported but caller can ignore — by design this is
/// fire-and-forget. If you need guaranteed delivery use the
/// in-payload `kind: 'delete_request'` DM path (the recipient gets it
/// when they next sweep the inbox, even if offline at send time).
#[hdk_extern]
pub fn send_dm_delete_request(input: SendDmDeleteRequestInput) -> ExternResult<()> {
    let signal = DmRemoteSignal::DmDeleteRequest(DmDeleteRequestSignal {
        thread_id: input.thread_id,
        target_action_hash: input.target_action_hash,
        from_agent: None,
    });
    send_remote_signal(signal, vec![input.recipient])
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

/// C7 — announce a call.
#[hdk_extern]
pub fn send_dm_call_init_request(input: SendDmCallInitRequestInput) -> ExternResult<()> {
    let signal = DmRemoteSignal::DmCall(DmCallSignal::InitRequest {
        call_id: input.call_id,
        from_agent: None,
    });
    send_remote_signal(signal, vec![input.recipient])
}

/// C7 — accept a call.
#[hdk_extern]
pub fn send_dm_call_init_accept(input: SendDmCallInitAcceptInput) -> ExternResult<()> {
    let signal = DmRemoteSignal::DmCall(DmCallSignal::InitAccept {
        call_id: input.call_id,
        from_agent: None,
    });
    send_remote_signal(signal, vec![input.recipient])
}

/// C7 — forward SDP / ICE data.
#[hdk_extern]
pub fn send_dm_call_sdp_data(input: SendDmCallSdpDataInput) -> ExternResult<()> {
    let signal = DmRemoteSignal::DmCall(DmCallSignal::SdpData {
        call_id: input.call_id,
        data: input.data,
        from_agent: None,
    });
    send_remote_signal(signal, vec![input.recipient])
}

// =============================================================================
// Host-side serde round-trip tests for C7b decode disambiguation.
//
// These run via plain `cargo test` (no wasm, no conductor, no tryorama). They
// prove that `EncryptedContentSignal` and `DmRemoteSignal` cannot structurally
// decode as each other under msgpack — which is the load-bearing safety
// property for the ordered try-decode dispatcher in lib::recv_remote_signal.
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use content_integrity::{Acl, EncryptedContent, EncryptedContentHeader};

    fn sample_response() -> EncryptedContentResponse {
        EncryptedContentResponse {
            encrypted_content: EncryptedContent {
                header: EncryptedContentHeader {
                    id: "id-1".into(),
                    hive_id: "hive-1".into(),
                    hive_genesis_hash: ActionHash::from_raw_36(vec![9u8; 36]),
                    author_membership_hash: None,
                    content_type: "ct-1".into(),
                    revision_author_signing_public_key: "k".into(),
                    acl: Acl {
                        owner: "o".into(),
                        admin: vec![],
                        writer: vec![],
                        reader: vec![],
                    },
                    public_key_acl: Acl {
                        owner: "o".into(),
                        admin: vec![],
                        writer: vec![],
                        reader: vec![],
                    },
                },
                bytes: UnsafeBytes::from(vec![1u8, 2, 3]).into(),
            },
            hash: "h".into(),
            original_hash: "h".into(),
        }
    }

    fn sample_target_hash() -> ActionHash {
        // 39-byte holohash: 3-byte prefix + 32-byte digest + 4-byte loc bytes.
        ActionHash::from_raw_39(
            vec![0x84u8, 0x29, 0x24]
                .into_iter()
                .chain(std::iter::repeat(0u8).take(36))
                .collect(),
        )
    }

    /// EncryptedContentSignal round-trips through ExternIO.
    #[test]
    fn encrypted_content_signal_roundtrip() {
        let sig = EncryptedContentSignal {
            action_type: EncryptedContentSignalType::Create,
            data: sample_response(),
            from_agent: None,
        };
        let io = ExternIO::encode(&sig).expect("encode");
        let back: EncryptedContentSignal = io.decode().expect("decode");
        assert!(matches!(back.action_type, EncryptedContentSignalType::Create));
        assert!(back.from_agent.is_none());
    }

    /// DmRemoteSignal round-trips through ExternIO (delete variant).
    #[test]
    fn dm_remote_signal_delete_roundtrip() {
        let sig = DmRemoteSignal::DmDeleteRequest(DmDeleteRequestSignal {
            thread_id: "t".into(),
            target_action_hash: sample_target_hash(),
            from_agent: None,
        });
        let io = ExternIO::encode(&sig).expect("encode");
        let back: DmRemoteSignal = io.decode().expect("decode");
        assert!(matches!(back, DmRemoteSignal::DmDeleteRequest(_)));
    }

    /// DmRemoteSignal round-trips through ExternIO (call variants).
    #[test]
    fn dm_remote_signal_call_roundtrips() {
        for sig in [
            DmRemoteSignal::DmCall(DmCallSignal::InitRequest {
                call_id: "c".into(),
                from_agent: None,
            }),
            DmRemoteSignal::DmCall(DmCallSignal::InitAccept {
                call_id: "c".into(),
                from_agent: None,
            }),
            DmRemoteSignal::DmCall(DmCallSignal::SdpData {
                call_id: "c".into(),
                data: "v=0\r\n…".into(),
                from_agent: None,
            }),
        ] {
            let io = ExternIO::encode(&sig).expect("encode");
            let back: DmRemoteSignal = io.decode().expect("decode");
            assert!(matches!(back, DmRemoteSignal::DmCall(_)));
        }
    }

    /// C7b load-bearing property: a serialized EncryptedContentSignal MUST
    /// fail to decode as DmRemoteSignal. The dispatcher's ordered try-decode
    /// relies on this — if these shapes could cross-decode, a sender could
    /// confuse the receiver's branch selection.
    #[test]
    fn encrypted_content_signal_does_not_decode_as_dm() {
        let sig = EncryptedContentSignal {
            action_type: EncryptedContentSignalType::Update,
            data: sample_response(),
            from_agent: None,
        };
        let io = ExternIO::encode(&sig).expect("encode");
        let dm: Result<DmRemoteSignal, _> = io.decode();
        assert!(
            dm.is_err(),
            "EncryptedContentSignal MUST NOT decode as DmRemoteSignal — \
             this is the C7b dispatcher disambiguation invariant"
        );
    }

    /// Mirror invariant: a serialized DmRemoteSignal MUST fail to decode as
    /// EncryptedContentSignal. The dispatcher tries EncryptedContentSignal
    /// FIRST (legacy wire shape priority); if a DmRemoteSignal could decode
    /// as EncryptedContentSignal, every new ephemeral signal would be
    /// misrouted to the legacy handler.
    #[test]
    fn dm_remote_signal_does_not_decode_as_encrypted_content() {
        for sig in [
            DmRemoteSignal::DmDeleteRequest(DmDeleteRequestSignal {
                thread_id: "t".into(),
                target_action_hash: sample_target_hash(),
                from_agent: None,
            }),
            DmRemoteSignal::DmCall(DmCallSignal::InitRequest {
                call_id: "c".into(),
                from_agent: None,
            }),
            DmRemoteSignal::DmCall(DmCallSignal::SdpData {
                call_id: "c".into(),
                data: "x".into(),
                from_agent: None,
            }),
        ] {
            let io = ExternIO::encode(&sig).expect("encode");
            let ecs: Result<EncryptedContentSignal, _> = io.decode();
            assert!(
                ecs.is_err(),
                "DmRemoteSignal variant {sig:?} MUST NOT decode as EncryptedContentSignal"
            );
        }
    }

    /// Sanity: stamp_from_agent overwrites the wire `from_agent` on every
    /// variant. The dispatcher invariant: receiver-attested provenance wins
    /// over whatever the sender claimed (including a forged value).
    #[test]
    fn stamp_from_agent_overwrites_every_variant() {
        let fake_caller = AgentPubKey::from_raw_39(
            vec![0x84u8, 0x20, 0x24]
                .into_iter()
                .chain(std::iter::repeat(0xAAu8).take(36))
                .collect(),
        );

        let mut a = DmRemoteSignal::DmDeleteRequest(DmDeleteRequestSignal {
            thread_id: "t".into(),
            target_action_hash: sample_target_hash(),
            from_agent: None,
        });
        a.stamp_from_agent(fake_caller.clone());
        if let DmRemoteSignal::DmDeleteRequest(s) = &a {
            assert_eq!(s.from_agent.as_ref(), Some(&fake_caller));
        } else {
            unreachable!()
        }

        let mut b = DmRemoteSignal::DmCall(DmCallSignal::SdpData {
            call_id: "c".into(),
            data: "x".into(),
            from_agent: None,
        });
        b.stamp_from_agent(fake_caller.clone());
        if let DmRemoteSignal::DmCall(DmCallSignal::SdpData { from_agent, .. }) = &b {
            assert_eq!(from_agent.as_ref(), Some(&fake_caller));
        } else {
            unreachable!()
        }
    }
}
