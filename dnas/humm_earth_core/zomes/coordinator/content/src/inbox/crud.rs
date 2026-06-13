//! Mutating externs for the offline DM inbox.

use content_integrity::*;
use hdk::prelude::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct SendToInboxInput {
    /// The pubkey of the recipient whose inbox should receive a
    /// pointer to `target`.
    pub recipient: AgentPubKey,
    /// Action hash being announced (typically an `EncryptedContent`
    /// the recipient should know about; may be any DHT-reachable
    /// `ActionHash`).
    pub target: ActionHash,
    /// Single-byte event discriminator: `DmCreate` for new content,
    /// `DmDelete` for sender-side tombstones, `HiveInvite` when the
    /// target is a `HiveMembership` or `HiveGenesis`. New variants
    /// MUST be appended to [`InboxEvent`] before they can be sent —
    /// the integrity validator rejects unknown discriminator bytes.
    pub event: InboxEvent,
}

/// Publish an Inbox pointer for `recipient`. The DHT propagates the
/// link to the recipient's authority arc; the recipient's `probe_inbox`
/// surfaces it on next poll.
///
/// Permissionless contract: any DNA peer may write to any other peer's
/// inbox. Recipients SHOULD treat inbox items as routing hints
/// (resolve the target via `get` before trusting any payload) — see
/// the I-C threat model in the module-level doc.
#[hdk_extern]
pub fn send_to_inbox(input: SendToInboxInput) -> ExternResult<ActionHash> {
    create_link(
        AnyLinkableHash::from(input.recipient),
        AnyLinkableHash::from(input.target),
        LinkTypes::Inbox,
        LinkTag::new(vec![input.event.as_byte()]),
    )
}

/// Remove an Inbox link by its action hash. The integrity validator
/// permits the link's author (sender retraction) or the link's base
/// agent (recipient consumption) to delete; any other caller's request
/// is rejected at validation time.
#[hdk_extern]
pub fn consume_inbox_item(create_link_hash: ActionHash) -> ExternResult<ActionHash> {
    delete_link(create_link_hash, GetOptions::network())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RecordProbeInput {
    /// The most-recent inbox link the caller has fully processed (or
    /// `None` on a first-ever probe with no prior cursor).
    pub last_processed_inbox_link_hash: Option<ActionHash>,
}

/// Commit a [`DmProbeLog`] private entry capturing the caller's probe
/// progress. Private — never DHT-published; lives on the caller's
/// source chain only. Subsequent `get_last_probe` calls return the
/// most-recent log entry.
#[hdk_extern]
pub fn record_probe(input: RecordProbeInput) -> ExternResult<ActionHash> {
    let now = sys_time()?;
    let probe = DmProbeLog {
        probed_at_microseconds: now.as_micros() as i64,
        last_processed_inbox_link_hash: input.last_processed_inbox_link_hash,
    };
    create_entry(&EntryTypes::DmProbeLog(probe))
}
