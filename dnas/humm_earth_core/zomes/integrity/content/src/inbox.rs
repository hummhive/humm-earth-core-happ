//! Offline DM inbox primitives (pass-2 I-C).
//!
//! Adapted from the vines `notify_peer.rs` pattern. The mechanism is a
//! single `Inbox` link type:
//!
//! - **Base** — the recipient's [`AgentPubKey`] (cast to
//!   [`AnyLinkableHash`]). Any agent on the DNA may write to any other
//!   agent's inbox; that is the inbox contract.
//! - **Target** — usually the [`ActionHash`] of a freshly-committed DHT
//!   entry the recipient should know about (a DM, a hive invite, a
//!   delete tombstone). The link itself is the offline-deliverable
//!   pointer; the recipient resolves the target when they probe.
//! - **Tag** — exactly one byte: the [`InboxEvent`] discriminator. Lets
//!   the recipient filter inbox queries by event class without paying
//!   for a target-side fetch.
//!
//! [`DmProbeLog`] is a **private** entry on the recipient's own source
//! chain that records the most-recent probe state (timestamp and last
//! consumed inbox link). Used to derive unread counts without re-scanning
//! the full inbox link set on every UI tick.

use hdi::prelude::*;

/// Single-byte discriminator stamped into the `LinkTag` of every
/// [`crate::LinkTypes::Inbox`] link. New variants append at the END
/// (existing byte values stay stable to preserve forward-compat with
/// already-published links).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum InboxEvent {
    /// A new DM (or other content) is available; target is the
    /// content's `ActionHash`.
    DmCreate = 0,
    /// A previously-sent message has been tombstoned by the sender;
    /// target is the deleted entry's original `ActionHash`.
    DmDelete = 1,
    /// A hive invitation; target is the `HiveMembership` action hash.
    HiveInvite = 2,
}

impl InboxEvent {
    /// Decode the single-byte tag back to a variant. Returns `Err` for
    /// unknown bytes so a new sender variant doesn't silently slip past
    /// older receiver validators.
    pub fn from_byte(b: u8) -> Result<Self, u8> {
        match b {
            0 => Ok(Self::DmCreate),
            1 => Ok(Self::DmDelete),
            2 => Ok(Self::HiveInvite),
            other => Err(other),
        }
    }

    /// Encode the variant as its single byte. Mirror of `from_byte`.
    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

/// Recipient-local cursor for inbox-probe progress. Private entry —
/// never DHT-published; lives on the recipient's source chain only.
///
/// Read pattern: query my own source chain for the latest `DmProbeLog`,
/// use `last_processed_inbox_link_hash` as a pointer into "everything I
/// have already consumed". UI derives "unread" as the set of inbox links
/// targeting me that did NOT exist as of the last probe.
///
/// Write pattern: on every successful inbox sweep, the coordinator
/// commits a fresh `DmProbeLog` capturing the sweep's high-water mark.
/// Old logs are not pruned (cheap private entries; chain-bloat is
/// negligible compared to message volume).
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct DmProbeLog {
    pub probed_at_microseconds: i64,
    pub last_processed_inbox_link_hash: Option<ActionHash>,
}

// =============================================================================
// Validators
// =============================================================================

/// Validate an `Inbox` link create.
///
/// The contract is permissive on purpose: any agent may write to any
/// other agent's inbox (that is the inbox contract). The validator only
/// enforces shape constraints that protect the recipient from malformed
/// inbox traffic.
///
/// Rules:
/// 1. `base` MUST decode as a valid [`AgentPubKey`] (the recipient).
///    Otherwise the link points at gibberish and the recipient can never
///    discover it via `get_links(my_pubkey, Inbox, ...)`.
/// 2. `tag` MUST be exactly one byte AND that byte MUST be a known
///    [`InboxEvent`] variant. Older receivers MUST NOT silently accept
///    unknown event types — a sender introducing a new variant has to
///    coordinate with a receiver bump.
pub fn validate_create_link_inbox(
    _action: CreateLink,
    base_address: AnyLinkableHash,
    _target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    if base_address.clone().into_agent_pub_key().is_none() {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "Inbox link base {base_address} is not a valid AgentPubKey",
        )));
    }
    if tag.0.len() != 1 {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "Inbox link tag must be exactly 1 byte; got {} bytes",
            tag.0.len(),
        )));
    }
    if let Err(unknown) = InboxEvent::from_byte(tag.0[0]) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "Inbox link tag carries unknown InboxEvent discriminator 0x{unknown:02x}",
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Validate an `Inbox` link delete.
///
/// Two parties may delete an inbox link:
/// - The **sender** (link author) — sender-side retraction.
/// - The **recipient** (whose pubkey is the link's base) — recipient-side
///   consumption (the recipient takes the inbox item, processes it, and
///   removes the pointer so subsequent probes don't re-surface it).
///
/// Any other agent attempting to delete the link is invalid; that would
/// be a third-party censorship attack against the recipient's mailbox.
pub fn validate_delete_link_inbox(
    action: DeleteLink,
    original_action: CreateLink,
    base: AnyLinkableHash,
    _target: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    // Sender retraction.
    if action.author == original_action.author {
        return Ok(ValidateCallbackResult::Valid);
    }
    // Recipient consumption.
    if let Some(recipient) = base.into_agent_pub_key() {
        if action.author == recipient {
            return Ok(ValidateCallbackResult::Valid);
        }
    }
    Ok(ValidateCallbackResult::Invalid(
        "Inbox link delete must be authored by the original sender or the recipient".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_event_round_trips_through_byte() {
        for variant in [
            InboxEvent::DmCreate,
            InboxEvent::DmDelete,
            InboxEvent::HiveInvite,
        ] {
            let byte = variant.as_byte();
            let decoded = InboxEvent::from_byte(byte).expect("known byte should decode");
            assert_eq!(variant, decoded);
        }
    }

    #[test]
    fn inbox_event_unknown_byte_is_err() {
        // Sentinel: a future variant adding byte 3 would still need to
        // bump receivers; pre-bump, byte 3 must be Invalid.
        assert!(InboxEvent::from_byte(3).is_err());
        assert!(InboxEvent::from_byte(255).is_err());
    }

    #[test]
    fn inbox_event_bytes_are_stable() {
        // Wire-stability guard: existing on-DHT links carry these exact
        // bytes. Renumbering would silently invalidate stored data.
        assert_eq!(InboxEvent::DmCreate.as_byte(), 0);
        assert_eq!(InboxEvent::DmDelete.as_byte(), 1);
        assert_eq!(InboxEvent::HiveInvite.as_byte(), 2);
    }

    fn agent_pubkey(byte: u8) -> AgentPubKey {
        AgentPubKey::from_raw_36(vec![byte; 36])
    }

    fn action_hash(byte: u8) -> ActionHash {
        ActionHash::from_raw_36(vec![byte; 36])
    }

    fn make_create_link(author: AgentPubKey) -> CreateLink {
        CreateLink {
            author,
            timestamp: Timestamp(0),
            action_seq: 0,
            prev_action: action_hash(0),
            base_address: AnyLinkableHash::from(agent_pubkey(0)),
            target_address: AnyLinkableHash::from(action_hash(1)),
            zome_index: 0.into(),
            link_type: 0.into(),
            tag: LinkTag::new(vec![]),
            weight: Default::default(),
        }
    }

    fn make_delete_link(author: AgentPubKey, create_hash: ActionHash) -> DeleteLink {
        DeleteLink {
            author,
            timestamp: Timestamp(0),
            action_seq: 1,
            prev_action: action_hash(0),
            base_address: AnyLinkableHash::from(agent_pubkey(0)),
            link_add_address: create_hash,
        }
    }

    // ---------------------------------------------------------------------
    // validate_create_link_inbox
    // ---------------------------------------------------------------------

    #[test]
    fn create_link_inbox_accepts_valid_recipient_and_tag() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let action = make_create_link(alice);
        let result = validate_create_link_inbox(
            action,
            AnyLinkableHash::from(bob),
            AnyLinkableHash::from(action_hash(3)),
            LinkTag::new(vec![InboxEvent::DmCreate.as_byte()]),
        )
        .expect("validator runs without host calls in this case");
        assert!(matches!(result, ValidateCallbackResult::Valid));
    }

    #[test]
    fn create_link_inbox_rejects_non_agent_base() {
        let alice = agent_pubkey(1);
        let action = make_create_link(alice);
        let result = validate_create_link_inbox(
            action,
            AnyLinkableHash::from(action_hash(7)), // not an agent
            AnyLinkableHash::from(action_hash(3)),
            LinkTag::new(vec![InboxEvent::DmCreate.as_byte()]),
        )
        .expect("validator runs without host calls in this case");
        assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
    }

    #[test]
    fn create_link_inbox_rejects_wrong_tag_length() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let action = make_create_link(alice);
        // Empty tag.
        let result = validate_create_link_inbox(
            action.clone(),
            AnyLinkableHash::from(bob.clone()),
            AnyLinkableHash::from(action_hash(3)),
            LinkTag::new(vec![]),
        )
        .expect("validator runs without host calls in this case");
        assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
        // Two-byte tag.
        let result = validate_create_link_inbox(
            action,
            AnyLinkableHash::from(bob),
            AnyLinkableHash::from(action_hash(3)),
            LinkTag::new(vec![0, 0]),
        )
        .expect("validator runs without host calls in this case");
        assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
    }

    #[test]
    fn create_link_inbox_rejects_unknown_event_byte() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let action = make_create_link(alice);
        let result = validate_create_link_inbox(
            action,
            AnyLinkableHash::from(bob),
            AnyLinkableHash::from(action_hash(3)),
            LinkTag::new(vec![99]), // not in InboxEvent
        )
        .expect("validator runs without host calls in this case");
        assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
    }

    // ---------------------------------------------------------------------
    // validate_delete_link_inbox
    // ---------------------------------------------------------------------

    #[test]
    fn delete_link_inbox_accepts_sender_retraction() {
        let alice = agent_pubkey(1); // sender
        let bob = agent_pubkey(2); // recipient
        let create = make_create_link(alice.clone());
        let delete = make_delete_link(alice, action_hash(10));
        let result = validate_delete_link_inbox(
            delete,
            create,
            AnyLinkableHash::from(bob),
            AnyLinkableHash::from(action_hash(3)),
            LinkTag::new(vec![InboxEvent::DmCreate.as_byte()]),
        )
        .expect("validator runs without host calls in this case");
        assert!(matches!(result, ValidateCallbackResult::Valid));
    }

    #[test]
    fn delete_link_inbox_accepts_recipient_consumption() {
        let alice = agent_pubkey(1); // sender
        let bob = agent_pubkey(2); // recipient (= base agent)
        let create = make_create_link(alice);
        let delete = make_delete_link(bob.clone(), action_hash(10));
        let result = validate_delete_link_inbox(
            delete,
            create,
            AnyLinkableHash::from(bob),
            AnyLinkableHash::from(action_hash(3)),
            LinkTag::new(vec![InboxEvent::DmCreate.as_byte()]),
        )
        .expect("validator runs without host calls in this case");
        assert!(matches!(result, ValidateCallbackResult::Valid));
    }

    #[test]
    fn delete_link_inbox_rejects_third_party_deleter() {
        let alice = agent_pubkey(1); // sender
        let bob = agent_pubkey(2); // recipient
        let mallory = agent_pubkey(99); // attacker — neither sender nor recipient
        let create = make_create_link(alice);
        let delete = make_delete_link(mallory, action_hash(10));
        let result = validate_delete_link_inbox(
            delete,
            create,
            AnyLinkableHash::from(bob),
            AnyLinkableHash::from(action_hash(3)),
            LinkTag::new(vec![InboxEvent::DmCreate.as_byte()]),
        )
        .expect("validator runs without host calls in this case");
        assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
    }
}
