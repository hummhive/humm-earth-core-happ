//! Read-only externs for the offline DM inbox.

use content_integrity::*;
use hdk::prelude::*;

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
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    let links = get_links(
        LinkQuery::try_new(AnyLinkableHash::from(my_pubkey), LinkTypes::Inbox)?,
        GetStrategy::Network,
    )?;

    let mut items: Vec<InboxItem> = Vec::with_capacity(links.len());
    for link in links {
        let decoded = link
            .tag
            .0
            .first()
            .and_then(|b| InboxEvent::from_byte(*b).ok());
        if let Some(filter) = input.event_filter {
            // Filter mode: only include links whose decoded byte matches.
            if decoded != Some(filter) {
                continue;
            }
        }
        let Some(target_ah) = link.target.into_action_hash() else {
            continue;
        };
        items.push(InboxItem {
            link_action_hash: link.create_link_hash,
            target: target_ah,
            event: decoded,
            created_at: link.timestamp,
            sender: link.author,
        });
    }

    // Oldest-first ordering matches the watermark-sweep convention
    // shared by list_by_hive_link: callers can pin a cursor on
    // the highest returned timestamp without skipping older items.
    items.sort_by_key(|i| i.created_at);
    Ok(items)
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
