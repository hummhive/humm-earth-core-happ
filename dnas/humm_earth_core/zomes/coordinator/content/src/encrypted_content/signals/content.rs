use hdk::prelude::*;

use crate::encrypted_content::EncryptedContentResponse;

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

/// Fetch-hint variant of [`EncryptedContentSignal`] for the cross-host channel:
/// it carries the identifiers a reader re-queries but NEVER the ciphertext, so
/// an attacker-controllable signal cannot re-broadcast durable content bytes.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct EncryptedContentHint {
    pub action_type: EncryptedContentSignalType,
    pub hash: String,
    pub original_hash: String,
    /// Stamped by recv_remote_signal from call_info().provenance; any
    /// sender-supplied value is discarded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_agent: Option<AgentPubKey>,
}
