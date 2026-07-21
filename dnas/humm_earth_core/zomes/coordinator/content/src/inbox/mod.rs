//! Coordinator-side externs for the offline DM inbox (pass-2 I-C).
//!
//! Mechanism (vines `notify_peer.rs` pattern):
//! - Sender publishes an `Inbox` link from `recipient.AgentPubKey` →
//!   the relevant DHT entry (e.g. a freshly-committed
//!   `EncryptedContent` action hash) tagged with a single byte
//!   [`InboxEvent`] discriminator. The link is DHT-published so it
//!   survives sender shutdown and is discoverable by the recipient
//!   even after long offline periods.
//! - Recipient polls `probe_inbox` on login (and periodically) to
//!   discover new items, then `consume_inbox_item` to retract the link
//!   so subsequent probes don't re-surface processed items.
//! - `record_probe` / `get_last_probe` write a private source-chain
//!   `DmProbeLog` entry capturing the high-water mark — used to derive
//!   unread counts without re-scanning the full link set.

pub mod crud;
pub mod queries;

pub use crud::{
    consume_inbox_item, record_probe, send_to_inbox, RecordProbeInput, SendToInboxInput,
};
pub use queries::{
    get_last_probe, probe_inbox, probe_inbox_page, InboxItem, InboxPage, ProbeInboxInput,
    ProbeInboxPageInput,
};
