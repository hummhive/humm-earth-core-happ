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

/// Encode `signal` and push it to `recipients` — the single source of
/// truth for every remote signal this zome sends.
///
/// **Why this pre-encodes.** The receiver's
/// `recv_remote_signal(signal: ExternIO)` (`crate::recv_remote_signal`)
/// decodes its parameter through the `#[hdk_extern]` `map_extern_preamble!`
/// double-decode: `host_args` strips one msgpack-BIN layer, then the
/// `ExternIO`-typed parameter decode (`extern_io.decode::<ExternIO>()`)
/// strips a second (hdi-0.7.0 `map_extern.rs`). HDK's `send_remote_signal`
/// applies exactly ONE `ExternIO::encode` (hdk-0.6.0 `p2p.rs:154`), and a
/// typed struct encodes to a msgpack MAP — which satisfies the first BIN
/// decode but FAILS the second, because `ExternIO` is
/// `#[serde(with = "serde_bytes")]` (a BIN, not a MAP). Pre-encoding here
/// supplies the second BIN layer the receiver's parameter type requires,
/// so the payload arrives as an `ExternIO` the dispatcher can
/// `.decode::<T>()`. Mirrors the ecosystem convention (moss
/// `group/src/lib.rs`, presence `room/src/remote_signals.rs`).
///
/// Every send path funnels through here — call this, never
/// `send_remote_signal` directly, so the encode contract cannot drift
/// per call site.
fn send_encoded_remote_signal<I>(signal: I, recipients: Vec<AgentPubKey>) -> ExternResult<()>
where
    I: Serialize + std::fmt::Debug,
{
    send_remote_signal(remote_signal_payload(&signal)?, recipients)
}

/// Build the `ExternIO` payload that [`send_encoded_remote_signal`] hands
/// to HDK's `send_remote_signal`. Extracted so the host test module can
/// exercise the exact wire encoding without a conductor; see
/// [`send_encoded_remote_signal`] for the full rationale.
fn remote_signal_payload<I>(signal: &I) -> ExternResult<ExternIO>
where
    I: Serialize + std::fmt::Debug,
{
    ExternIO::encode(signal).map_err(|e| wasm_error!(e))
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
    if let Err(err) = send_encoded_remote_signal(signal, recipients) {
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
    send_encoded_remote_signal(signal, vec![input.recipient])
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
    send_encoded_remote_signal(signal, vec![input.recipient])
}

/// C7 — accept a call.
#[hdk_extern]
pub fn send_dm_call_init_accept(input: SendDmCallInitAcceptInput) -> ExternResult<()> {
    let signal = DmRemoteSignal::DmCall(DmCallSignal::InitAccept {
        call_id: input.call_id,
        from_agent: None,
    });
    send_encoded_remote_signal(signal, vec![input.recipient])
}

/// C7 — forward SDP / ICE data.
#[hdk_extern]
pub fn send_dm_call_sdp_data(input: SendDmCallSdpDataInput) -> ExternResult<()> {
    let signal = DmRemoteSignal::DmCall(DmCallSignal::SdpData {
        call_id: input.call_id,
        data: input.data,
        from_agent: None,
    });
    send_encoded_remote_signal(signal, vec![input.recipient])
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
    use content_integrity::{Acl, AclByGroupGenesis, AclSpec, EncryptedContent, EncryptedContentHeader};

    fn sample_response() -> EncryptedContentResponse {
        EncryptedContentResponse {
            encrypted_content: EncryptedContent {
                header: EncryptedContentHeader {
                    id: "id-1".into(),
                    display_hive_id: "hive-1".into(),
                    content_type: "ct-1".into(),
                    revision_author_signing_public_key: "k".into(),
                    acl_spec: AclSpec::HiveGroup {
                        hive_genesis_hash: ActionHash::from_raw_36(vec![9u8; 36]),
                        author_membership_hash: None,
                        group_acl: AclByGroupGenesis {
                            owner: ActionHash::from_raw_36(vec![10u8; 36]),
                            admin: vec![],
                            writer: vec![],
                            reader: vec![],
                        },
                        author_group_membership_hash: None,
                        recipient_witnesses: vec![],
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

    /// RED→GREEN driver for the recv_remote_signal drop bug.
    ///
    /// GIVEN the legacy content signal the fan-out pushes,
    /// WHEN it travels the coordinator's remote-signal path and HDK's
    ///      `send_remote_signal` applies its single `ExternIO::encode`
    ///      (hdk-0.6.0 `p2p.rs:154`) onto whatever payload it is handed,
    /// THEN the recipient's `recv_remote_signal(signal: ExternIO)`
    ///      parameter decode (the `#[hdk_extern]` `map_extern_preamble!`
    ///      step `extern_io.decode::<ExternIO>()`) must succeed and the
    ///      dispatcher must recover the typed signal.
    ///
    /// The `on_wire` line below models exactly what the SEND PATH hands to
    /// `send_remote_signal`. The pre-fix call sites passed the bare typed
    /// struct, so the wire bytes are a single-encoded msgpack MAP — which
    /// cannot decode into the serde_bytes-BIN `ExternIO` parameter, and
    /// the signal is dropped at the param boundary before any handler runs.
    #[test]
    fn content_signal_round_trips_through_send_path() {
        let typed = EncryptedContentSignal {
            action_type: EncryptedContentSignalType::Create,
            data: sample_response(),
            from_agent: None,
        };
        // Fixed send path: the coordinator pre-encodes via
        // `remote_signal_payload`, then HDK's `send_remote_signal` applies
        // its own `ExternIO::encode` (hdk p2p.rs:154) on top.
        let payload = remote_signal_payload(&typed).expect("payload");
        let on_wire = ExternIO::encode(&payload).expect("hdk send encode");
        // Recipient `recv_remote_signal(signal: ExternIO)` param decode:
        let param: ExternIO = on_wire
            .decode()
            .expect("recv_remote_signal ExternIO param must decode (dropped-signal bug)");
        // Dispatcher try-decode:
        let back: EncryptedContentSignal =
            param.decode().expect("dispatcher must recover the typed signal");
        assert!(matches!(back.action_type, EncryptedContentSignalType::Create));
    }

    /// Characterization guard locking the bug shut: a typed signal handed
    /// to `send_remote_signal` directly (ONE `ExternIO::encode`) lands on
    /// the wire as a msgpack MAP, which MUST NOT decode into the
    /// serde_bytes-BIN `ExternIO` parameter of `recv_remote_signal`. If
    /// this ever starts succeeding (an HDK change, or someone dropping the
    /// pre-encode), the fan-out would silently regress to dropped signals.
    #[test]
    fn single_encode_payload_is_rejected_by_receiver_externio_param() {
        let typed = EncryptedContentSignal {
            action_type: EncryptedContentSignalType::Create,
            data: sample_response(),
            from_agent: None,
        };
        let single = ExternIO::encode(&typed).expect("encode");
        // First msgpack byte must be a MAP marker (fixmap 0x80..=0x8f,
        // map16 0xde, map32 0xdf) — NOT a BIN. This is precisely why the
        // ExternIO param decode below fails. Matches the captured
        // conductor log (`[130, ...]` = 0x82 fixmap).
        assert!(!single.0.is_empty(), "ExternIO::encode produced empty bytes");
        let marker = single.0[0];
        assert!(
            (0x80..=0x8f).contains(&marker) || marker == 0xde || marker == 0xdf,
            "expected a msgpack map marker, got {marker:#x}"
        );
        let as_param: Result<ExternIO, _> = single.decode();
        assert!(
            as_param.is_err(),
            "single-encoded MAP must NOT decode into the ExternIO param"
        );
    }

    /// The C6/C7 envelope must survive the same receiver double-decode the
    /// content signal does — every signal family funnels through
    /// `remote_signal_payload`, so prove the DM path end to end too.
    #[test]
    fn dm_remote_signal_round_trips_through_send_path() {
        // Cover both top-level DmRemoteSignal variants (DmDeleteRequest and
        // a DmCall variant) so the guard is exhaustive for the DM family.
        let signals = [
            DmRemoteSignal::DmDeleteRequest(DmDeleteRequestSignal {
                thread_id: "t".into(),
                target_action_hash: sample_target_hash(),
                from_agent: None,
            }),
            DmRemoteSignal::DmCall(DmCallSignal::SdpData {
                call_id: "c".into(),
                data: "v=0\r\n…".into(),
                from_agent: None,
            }),
        ];
        for typed in signals {
            let payload = remote_signal_payload(&typed).expect("payload");
            let on_wire = ExternIO::encode(&payload).expect("hdk send encode");
            let param: ExternIO = on_wire
                .decode()
                .expect("recv_remote_signal ExternIO param must decode");
            let back: DmRemoteSignal = param.decode().expect("dispatcher decode");
            // The same top-level variant survives the double-encode round-trip.
            assert!(matches!(
                (&typed, &back),
                (
                    DmRemoteSignal::DmDeleteRequest(_),
                    DmRemoteSignal::DmDeleteRequest(_)
                ) | (DmRemoteSignal::DmCall(_), DmRemoteSignal::DmCall(_))
            ));
        }
    }
}
