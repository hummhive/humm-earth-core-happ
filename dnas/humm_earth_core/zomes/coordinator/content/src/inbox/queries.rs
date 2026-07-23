//! Read-only externs for the offline DM inbox.

use content_integrity::*;
use hdk::prelude::*;

use crate::encrypted_content::{
    decode_paired_cursor, page_links, resolve_page_limit, source_positions_of, SourcePosition,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct ProbeInboxInput {
    /// Optional event-type filter. When `None`, every inbox event for
    /// the calling agent is returned; when `Some`, only links whose
    /// single-byte tag matches the requested variant are surfaced.
    #[serde(default)]
    pub event_filter: Option<InboxEvent>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InboxItem {
    /// Action hash of the `CreateLink` that placed this pointer. Pass
    /// to `consume_inbox_item` to retract.
    pub link_action_hash: ActionHash,
    /// The DHT entry the pointer references.
    pub target: ActionHash,
    /// Decoded event type. Always `Some` when the link's tag is a
    /// recognised single-byte discriminator; `None` here is unreachable
    /// for valid links (the integrity validator rejects unknown bytes),
    /// but the probe surface tolerates malformed tags gracefully by
    /// surfacing `None` rather than failing the whole call.
    pub event: Option<InboxEvent>,
    /// Link creation timestamp — useful for UI ordering without paying
    /// for a per-link `get` round-trip.
    pub created_at: Timestamp,
    /// The sender pubkey (link author).
    pub sender: AgentPubKey,
}

/// Return every inbox pointer addressed to the calling agent, optionally
/// filtered by event type. Each `InboxItem` carries the sender's
/// pubkey (link author — the C1 anti-spoof guarantee applies here too:
/// the conductor cryptographically attests the link's author), the
/// target action hash, the decoded event byte, and the link timestamp.
///
/// Targets that fail to decode as known shapes are still surfaced —
/// the caller decides how to handle unknown targets (typical pattern:
/// `get` the target on demand and ignore if unparseable).
#[hdk_extern]
pub fn probe_inbox(input: ProbeInboxInput) -> ExternResult<Vec<InboxItem>> {
    let mut items: Vec<InboxItem> = my_inbox_links()?
        .into_iter()
        .filter(|link| admits_event_filter(link, input.event_filter))
        .filter_map(inbox_item_from_link)
        .collect();
    // Oldest-first ordering matches the watermark-sweep convention
    // shared by list_by_hive_link: callers can pin a cursor on
    // the highest returned timestamp without skipping older items.
    items.sort_by_key(|i| i.created_at);
    Ok(items)
}

fn my_inbox_links() -> ExternResult<Vec<Link>> {
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    get_links(
        LinkQuery::try_new(AnyLinkableHash::from(my_pubkey), LinkTypes::Inbox)?,
        GetStrategy::Network,
    )
}

fn admits_event_filter(link: &Link, filter: Option<InboxEvent>) -> bool {
    match filter {
        None => true,
        Some(wanted) => {
            link.tag
                .0
                .first()
                .and_then(|b| InboxEvent::from_byte(*b).ok())
                == Some(wanted)
        }
    }
}

/// `None` only for a non-action-hash target (malformed writer); the tag
/// byte decodes to `None` gracefully rather than failing the whole read.
fn inbox_item_from_link(link: Link) -> Option<InboxItem> {
    let event = link
        .tag
        .0
        .first()
        .and_then(|b| InboxEvent::from_byte(*b).ok());
    let target = link.target.into_action_hash()?;
    Some(InboxItem {
        link_action_hash: link.create_link_hash,
        target,
        event,
        created_at: link.timestamp,
        sender: link.author,
    })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProbeInboxPageInput {
    #[serde(default)]
    pub event_filter: Option<InboxEvent>,
    #[serde(default)]
    pub since_ts: Option<Timestamp>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub source_after_action_hash: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InboxPage {
    pub items: Vec<InboxItem>,
    pub source_count: usize,
    pub source_positions: Vec<SourcePosition>,
    pub truncated: bool,
}

/// Paged twin of [`probe_inbox`]: same event-byte filter, plus the
/// composite source cursor shared with the content `*_page` externs
/// (`since_ts` alone inclusive; the full pair strictly exclusive; limit
/// default 100 / cap 256). `source_positions` are SOURCE truth — one per
/// selected link even when its target is malformed — so callers cursor
/// past poison rows.
#[hdk_extern]
pub fn probe_inbox_page(input: ProbeInboxPageInput) -> ExternResult<InboxPage> {
    let limit = resolve_page_limit(input.limit)?;
    let after_hash = decode_paired_cursor(
        input.source_after_action_hash.as_deref(),
        input.since_ts.as_ref(),
    )?;
    let filtered: Vec<Link> = my_inbox_links()?
        .into_iter()
        .filter(|link| admits_event_filter(link, input.event_filter))
        .collect();
    let (selected, truncated) = page_links(filtered, input.since_ts, after_hash, limit);
    let source_positions = source_positions_of(&selected);
    let items: Vec<InboxItem> = selected
        .into_iter()
        .filter_map(inbox_item_from_link)
        .collect();
    Ok(InboxPage {
        items,
        source_count: source_positions.len(),
        source_positions,
        truncated,
    })
}

/// Return the most-recent [`DmProbeLog`] entry the caller has committed,
/// or `None` if they have never called `record_probe`.
///
/// Source-chain-local — no DHT round-trip. Cheap to call on every UI
/// tick to derive unread-count badges.
#[hdk_extern]
pub fn get_last_probe(_: ()) -> ExternResult<Option<DmProbeLog>> {
    let probe_entry_type = EntryType::App(EntryTypesUnit::DmProbeLog.try_into()?);
    let filter = ChainQueryFilter::new()
        .entry_type(probe_entry_type)
        .include_entries(true);
    let mut records = query(filter)?;
    // Newest-first.
    records.sort_by_key(|r| std::cmp::Reverse(r.action().timestamp()));
    let Some(record) = records.into_iter().next() else {
        return Ok(None);
    };
    let probe: Option<DmProbeLog> = record.entry().to_app_option().map_err(|e| wasm_error!(e))?;
    Ok(probe)
}
