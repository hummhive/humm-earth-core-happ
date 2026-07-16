//! Blob-pinning hint signals (pass-6-pinned-hosts).
//!
//! A `BlobPinSignal` is a HINT, never authority: it carries no blob
//! bytes and no logical SHA-512, and every field is attacker-controlled
//! on receipt (the cap grant on `recv_remote_signal` is open). The
//! recipient MUST re-read the durable provider record referenced by
//! `provider_record_hash` and re-run its own admission checks before
//! any dial or write.

use hdk::prelude::*;

pub(crate) const BLOB_PIN_SIGNAL_MAX_RECIPIENTS: usize = 16;

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct BlobPinHint {
    pub hive_genesis_hash: ActionHash,
    /// Stored-variant BLAKE3 (hex string) — NEVER the logical SHA-512.
    pub blake3: String,
    /// Opaque byte-variant label (e.g. "raw" | "enc"); the coordinator
    /// does not validate it.
    pub byte_variant: String,
    /// Durable provider-record reference (create action hash) the
    /// recipient re-reads for authoritative admission.
    pub provider_record_hash: ActionHash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_micros: Option<i64>,
    /// Stamped by the `recv_remote_signal` dispatcher with the
    /// conductor-attested caller; sender-supplied values are discarded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_agent: Option<AgentPubKey>,
}

/// Tag literal "pin" is deliberately distinct from "kind"
/// (DmRemoteSignal), "type" (DmCallSignal), and the `action_type` field
/// (EncryptedContentSignal) so the dispatcher's ordered structural
/// try-decode cannot cross-match families.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
#[serde(tag = "pin")]
pub enum BlobPinSignal {
    Available(BlobPinHint),
    TakeNow(BlobPinHint),
}

impl BlobPinSignal {
    fn hint_mut(&mut self) -> &mut BlobPinHint {
        match self {
            BlobPinSignal::Available(hint) | BlobPinSignal::TakeNow(hint) => hint,
        }
    }

    /// Replace whatever the wire payload claimed about the sender with
    /// the conductor-attested provenance (same contract as
    /// `DmRemoteSignal::stamp_from_agent`).
    pub fn stamp_from_agent(&mut self, agent: AgentPubKey) {
        self.hint_mut().from_agent = Some(agent);
    }

    pub(crate) fn clear_from_agent(&mut self) {
        self.hint_mut().from_agent = None;
    }
}
