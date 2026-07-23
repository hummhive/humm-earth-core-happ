use super::outbound::remote_signal_payload;
use super::*;
use crate::encrypted_content::EncryptedContentResponse;
use content_integrity::{
    Acl, AclByGroupGenesis, AclSpec, EncryptedContent, EncryptedContentHeader,
};
use hdk::prelude::*;

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
                lineage: None,
            },
            bytes: UnsafeBytes::from(vec![1u8, 2, 3]).into(),
        },
        hash: "h".into(),
        original_hash: "h".into(),
        latest_action_micros: None,
        tombstoned: None,
    }
}

fn sample_target_hash() -> ActionHash {
    // 39-byte holohash: 3-byte prefix + 32-byte digest + 4-byte loc bytes.
    ActionHash::from_raw_39(
        vec![0x84u8, 0x29, 0x24]
            .into_iter()
            .chain(std::iter::repeat_n(0u8, 36))
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
    assert!(matches!(
        back.action_type,
        EncryptedContentSignalType::Create
    ));
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
            .chain(std::iter::repeat_n(0xAAu8, 36))
            .collect(),
    );

    let mut delete = DmRemoteSignal::DmDeleteRequest(DmDeleteRequestSignal {
        thread_id: "t".into(),
        target_action_hash: sample_target_hash(),
        from_agent: None,
    });
    delete.stamp_from_agent(fake_caller.clone());
    if let DmRemoteSignal::DmDeleteRequest(signal) = &delete {
        assert_eq!(signal.from_agent.as_ref(), Some(&fake_caller));
    } else {
        unreachable!()
    }

    let mut init_request = DmRemoteSignal::DmCall(DmCallSignal::InitRequest {
        call_id: "c".into(),
        from_agent: None,
    });
    init_request.stamp_from_agent(fake_caller.clone());
    if let DmRemoteSignal::DmCall(DmCallSignal::InitRequest { from_agent, .. }) = &init_request {
        assert_eq!(from_agent.as_ref(), Some(&fake_caller));
    } else {
        unreachable!()
    }

    let mut init_accept = DmRemoteSignal::DmCall(DmCallSignal::InitAccept {
        call_id: "c".into(),
        from_agent: None,
    });
    init_accept.stamp_from_agent(fake_caller.clone());
    if let DmRemoteSignal::DmCall(DmCallSignal::InitAccept { from_agent, .. }) = &init_accept {
        assert_eq!(from_agent.as_ref(), Some(&fake_caller));
    } else {
        unreachable!()
    }

    let mut sdp = DmRemoteSignal::DmCall(DmCallSignal::SdpData {
        call_id: "c".into(),
        data: "x".into(),
        from_agent: None,
    });
    sdp.stamp_from_agent(fake_caller.clone());
    if let DmRemoteSignal::DmCall(DmCallSignal::SdpData { from_agent, .. }) = &sdp {
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
    let back: EncryptedContentSignal = param
        .decode()
        .expect("dispatcher must recover the typed signal");
    assert!(matches!(
        back.action_type,
        EncryptedContentSignalType::Create
    ));
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
    assert!(
        !single.0.is_empty(),
        "ExternIO::encode produced empty bytes"
    );
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

fn sample_blob_pin_hint() -> BlobPinHint {
    BlobPinHint {
        hive_genesis_hash: ActionHash::from_raw_36(vec![7u8; 36]),
        blake3: "b3-hex".into(),
        byte_variant: "enc".into(),
        provider_record_hash: sample_target_hash(),
        expires_at_micros: Some(1_234_567),
        from_agent: None,
    }
}

/// BlobPinSignal round-trips through ExternIO for both variants.
#[test]
fn blob_pin_signal_roundtrip() {
    for sig in [
        BlobPinSignal::Available(sample_blob_pin_hint()),
        BlobPinSignal::TakeNow(sample_blob_pin_hint()),
    ] {
        let io = ExternIO::encode(&sig).expect("encode");
        let back: BlobPinSignal = io.decode().expect("decode");
        assert_eq!(back, sig);
    }
}

/// Dispatcher disambiguation invariant, blob-pin → legacy direction: a
/// serialized BlobPinSignal MUST NOT decode as either earlier-tried
/// family, or every blob-pin hint would be misrouted.
#[test]
fn blob_pin_signal_does_not_decode_as_encrypted_content_or_dm() {
    for sig in [
        BlobPinSignal::Available(sample_blob_pin_hint()),
        BlobPinSignal::TakeNow(sample_blob_pin_hint()),
    ] {
        let io = ExternIO::encode(&sig).expect("encode");
        let ecs: Result<EncryptedContentSignal, _> = io.decode();
        assert!(
            ecs.is_err(),
            "BlobPinSignal variant {sig:?} MUST NOT decode as EncryptedContentSignal"
        );
        let dm: Result<DmRemoteSignal, _> = io.decode();
        assert!(
            dm.is_err(),
            "BlobPinSignal variant {sig:?} MUST NOT decode as DmRemoteSignal"
        );
    }
}

/// Dispatcher disambiguation invariant, legacy → blob-pin direction:
/// neither established family may decode as BlobPinSignal, or the new
/// arm could shadow-capture legacy payloads after a reorder.
#[test]
fn encrypted_content_and_dm_do_not_decode_as_blob_pin() {
    let ecs = EncryptedContentSignal {
        action_type: EncryptedContentSignalType::Create,
        data: sample_response(),
        from_agent: None,
    };
    let io = ExternIO::encode(&ecs).expect("encode");
    let pin: Result<BlobPinSignal, _> = io.decode();
    assert!(
        pin.is_err(),
        "EncryptedContentSignal MUST NOT decode as BlobPinSignal"
    );

    for dm in [
        DmRemoteSignal::DmDeleteRequest(DmDeleteRequestSignal {
            thread_id: "t".into(),
            target_action_hash: sample_target_hash(),
            from_agent: None,
        }),
        DmRemoteSignal::DmCall(DmCallSignal::InitRequest {
            call_id: "c".into(),
            from_agent: None,
        }),
    ] {
        let io = ExternIO::encode(&dm).expect("encode");
        let pin: Result<BlobPinSignal, _> = io.decode();
        assert!(
            pin.is_err(),
            "DmRemoteSignal variant {dm:?} MUST NOT decode as BlobPinSignal"
        );
    }
}

/// The blob-pin family must survive the same receiver double-decode
/// chain the other families do — every signal funnels through
/// `remote_signal_payload` + HDK's own `ExternIO::encode`.
#[test]
fn blob_pin_signal_round_trips_through_send_path() {
    let typed = BlobPinSignal::TakeNow(sample_blob_pin_hint());
    let payload = remote_signal_payload(&typed).expect("payload");
    let on_wire = ExternIO::encode(&payload).expect("hdk send encode");
    let param: ExternIO = on_wire
        .decode()
        .expect("recv_remote_signal ExternIO param must decode");
    let back: BlobPinSignal = param
        .decode()
        .expect("dispatcher must recover the typed signal");
    assert_eq!(back, typed);
}

/// Receiver-attested provenance wins over any sender claim, on both
/// variants.
#[test]
fn blob_pin_stamp_from_agent_overwrites_both_variants() {
    let fake_caller = AgentPubKey::from_raw_39(
        vec![0x84u8, 0x20, 0x24]
            .into_iter()
            .chain(std::iter::repeat_n(0xBBu8, 36))
            .collect(),
    );

    for mut sig in [
        BlobPinSignal::Available(sample_blob_pin_hint()),
        BlobPinSignal::TakeNow(sample_blob_pin_hint()),
    ] {
        sig.stamp_from_agent(fake_caller.clone());
        let (BlobPinSignal::Available(hint) | BlobPinSignal::TakeNow(hint)) = &sig;
        assert_eq!(hint.from_agent.as_ref(), Some(&fake_caller));
    }
}

fn sample_hint() -> EncryptedContentHint {
    EncryptedContentHint {
        action_type: EncryptedContentSignalType::Create,
        hash: "h".into(),
        original_hash: "h".into(),
        from_agent: None,
    }
}

/// EncryptedContentHint round-trips through ExternIO.
#[test]
fn encrypted_content_hint_roundtrip() {
    let io = ExternIO::encode(sample_hint()).expect("encode");
    let back: EncryptedContentHint = io.decode().expect("decode");
    assert!(matches!(
        back.action_type,
        EncryptedContentSignalType::Create
    ));
    assert_eq!(back.hash, "h");
    assert!(back.from_agent.is_none());
}

/// The hint carries NO ciphertext: the full EncryptedContentSignal (with
/// `data`) and the hint MUST NOT decode as each other — the dispatcher's
/// arm selection relies on this disjointness.
#[test]
fn encrypted_content_hint_and_full_signal_are_disjoint() {
    let hint_io = ExternIO::encode(sample_hint()).expect("encode");
    assert!(
        hint_io.decode::<EncryptedContentSignal>().is_err(),
        "hint MUST NOT decode as the full EncryptedContentSignal"
    );
    let full = EncryptedContentSignal {
        action_type: EncryptedContentSignalType::Create,
        data: sample_response(),
        from_agent: None,
    };
    let full_io = ExternIO::encode(&full).expect("encode");
    assert!(
        full_io.decode::<EncryptedContentHint>().is_err(),
        "full signal MUST NOT decode as the fetch-hint"
    );
}

/// The hint MUST NOT cross-decode as the DM, blob-pin, or owner-handoff families.
#[test]
fn encrypted_content_hint_does_not_cross_decode() {
    let hint_io = ExternIO::encode(sample_hint()).expect("encode");
    assert!(hint_io.decode::<DmRemoteSignal>().is_err());
    assert!(hint_io.decode::<BlobPinSignal>().is_err());
    assert!(hint_io
        .decode::<crate::hive::OwnerHandoffOfferHint>()
        .is_err());
}

/// OwnerHandoffOfferHint round-trips and is disjoint from every other
/// dispatcher family, so recv_remote_signal routes it to its own arm.
#[test]
fn owner_handoff_offer_hint_roundtrip_and_disjoint() {
    use crate::hive::OwnerHandoffOfferHint;
    let hint = OwnerHandoffOfferHint {
        offer_hash: sample_target_hash(),
        hive_genesis_hash: sample_target_hash(),
        from_agent: None,
    };
    let io = ExternIO::encode(&hint).expect("encode");
    let back: OwnerHandoffOfferHint = io.decode().expect("decode");
    assert_eq!(back.offer_hash, sample_target_hash());
    assert!(back.from_agent.is_none());
    assert!(io.decode::<EncryptedContentSignal>().is_err());
    assert!(io.decode::<EncryptedContentHint>().is_err());
    assert!(io.decode::<DmRemoteSignal>().is_err());
    assert!(io.decode::<BlobPinSignal>().is_err());
}
