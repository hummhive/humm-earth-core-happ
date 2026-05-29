//! Read-only DHT query externs over the link space (hive paths, author
//! paths, dynamic paths, ACL paths, content-id paths). Each returns
//! `Vec<EncryptedContentResponse>` (or scalar variants where the query
//! shape demands it).
//!
//! Phase-1 contract: every query path uses
//! `get_links(LinkQuery, GetStrategy::Network)` — the high-level HDK API
//! — to keep the call shape uniform and reviewable. Cursor pagination
//! (Phase 2) is deferred: `LinkQuery` has no native limit/tiebreaker,
//! and a timestamp-only cursor risks dupes / gaps at microsecond
//! collisions. The single safe addition is `since_ts` + `limit` on the
//! hive query (C2) — paired with **oldest-first** sort so the
//! watermark sweep on the host side is gap-free.

use content_integrity::*;
use hdi::hash_path::path::Component;
use hdk::prelude::*;
use std::collections::HashSet;

use super::crud::{get_encrypted_content, get_many_encrypted_content};
use super::EncryptedContentResponse;

#[derive(Serialize, Deserialize, Debug)]
pub struct GetEncryptedContentByTimeAndAuthorInput {
    pub author: AgentPubKey,
    pub content_type: String,
    pub start_time: Option<Timestamp>,
    pub end_time: Option<Timestamp>,
    pub limit: Option<usize>,
}

/// Stub kept for callsite compat while the time-indexing crate path is
/// still on hold (see commented-out code in the original
/// `encrypted_content.rs`). Returns empty without erroring so the
/// upstream `humm-tauri` callsite continues to compile and behave as
/// it did before the refactor.
#[hdk_extern]
pub fn get_encrypted_content_by_time_and_author(
    _input: GetEncryptedContentByTimeAndAuthorInput,
) -> ExternResult<Vec<EncryptedContentResponse>> {
    Ok(vec![])
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListByDynamicLinkInput {
    pub hive_id: String,
    pub content_type: String,
    pub dynamic_link: String,
}

#[hdk_extern]
pub fn list_by_dynamic_link(
    input: ListByDynamicLinkInput,
) -> ExternResult<Vec<EncryptedContentResponse>> {
    let path = Path::from(vec![
        Component::from(input.hive_id),
        Component::from(input.content_type),
        Component::from(input.dynamic_link.clone()),
    ]);

    let links = get_links(
        LinkQuery::try_new(path.path_entry_hash()?, LinkTypes::Dynamic)?,
        GetStrategy::Network,
    )?;
    let hashes: Vec<ActionHash> = links
        .into_iter()
        .filter_map(|link| link.target.into_action_hash())
        .collect();
    get_many_encrypted_content(hashes)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListByHiveInput {
    pub hive_id: String,
    pub content_type: String,
    /// When set, only links created after this timestamp are returned
    /// (boundary inclusivity follows the conductor's `LinkQuery::after`
    /// implementation — treat as approximately exclusive and dedupe by
    /// action hash on the host side). Pair with `limit` for the
    /// watermark-sweep pagination pattern.
    #[serde(default)]
    pub since_ts: Option<Timestamp>,
    /// Maximum number of results to return. None = unbounded (legacy
    /// behaviour). Truncation runs on OLDEST-FIRST sorted links so a
    /// `(since_ts, limit)` sweep is gap-free: the next call uses the
    /// max returned timestamp as the new `since_ts` and never skips
    /// older entries that arrived after a network partition.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// List entries linked off the hive path (`[hive_id, content_type]` →
/// `Hive`). Supports time-windowed sweeps via `since_ts` and result
/// capping via `limit`.
///
/// **C2 fix.** Earlier this function sorted newest-first then truncated to
/// `limit`. For a watermark sweep with `>limit` new entries that is a
/// data-loss bug: the older entries past `limit` are dropped, the host
/// advances its watermark past them, and they are never re-fetched.
/// The fix sorts ASCENDING by `link.timestamp` (oldest-first) then
/// truncates. The host then sets `next_since_ts = max(returned.timestamp)`
/// and re-sweeps; entries that didn't fit in this batch survive into the
/// next one. Microsecond collisions on the boundary timestamp would
/// duplicate (caller dedupes via action hash) but never drop.
#[hdk_extern]
pub fn list_by_hive_link(input: ListByHiveInput) -> ExternResult<Vec<EncryptedContentResponse>> {
    let path = Path::from(vec![
        Component::from(input.hive_id),
        Component::from(input.content_type.clone()),
    ]);
    let path_hash = path.path_entry_hash()?;

    let mut query = LinkQuery::try_new(path_hash, LinkTypes::Hive)?;
    if let Some(ts) = input.since_ts {
        query = query.after(ts);
    }
    let mut all_links = get_links(query, GetStrategy::Network)?;

    // OLDEST-FIRST sort + truncate. See doc-comment above for why this is
    // load-bearing for the watermark sweep.
    all_links.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    if let Some(limit) = input.limit {
        all_links.truncate(limit);
    }

    let hashes: Vec<ActionHash> = all_links
        .into_iter()
        .filter_map(|l| l.target.into_action_hash())
        .collect();
    get_many_encrypted_content(hashes)
}

/// C3 input. Distinct from `ListByHiveInput` because counting has no
/// `limit` field — including one would be confusing dead weight in the
/// wire shape.
#[derive(Serialize, Deserialize, Debug)]
pub struct CountByHiveInput {
    pub hive_id: String,
    pub content_type: String,
    /// Same semantics as `ListByHiveInput::since_ts` — links created
    /// after the supplied timestamp (boundary inclusivity follows the
    /// conductor's `LinkQuery::after`; dedupe by action hash on the
    /// host side). When `None`, takes the efficient `count_links` path
    /// (no link fan-out); when `Some`, falls back to `get_links(...).len()`
    /// because the host's `count_links` does not support a time filter.
    #[serde(default)]
    pub since_ts: Option<Timestamp>,
}

/// C3 — count links on the hive path. Returns 0 (not an error) for an
/// empty path. Use this for unread badges, item-count UI, and the
/// sync-indicator without paying for the full link fan-out.
#[hdk_extern]
pub fn count_links_by_hive(input: CountByHiveInput) -> ExternResult<usize> {
    let path = Path::from(vec![
        Component::from(input.hive_id),
        Component::from(input.content_type),
    ]);
    let path_hash = path.path_entry_hash()?;

    if let Some(ts) = input.since_ts {
        // count_links only accepts LinkQuery (no timestamp filter); fall back
        // to fetching the links and counting them.
        let query = LinkQuery::try_new(path_hash, LinkTypes::Hive)?.after(ts);
        let all_links = get_links(query, GetStrategy::Network)?;
        return Ok(all_links.len());
    }

    count_links(LinkQuery::try_new(path_hash, LinkTypes::Hive)?)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListByContentIdInput {
    pub hive_id: String,
    pub content_id: String,
}

#[hdk_extern]
pub fn get_by_content_id_link(
    input: ListByContentIdInput,
) -> ExternResult<EncryptedContentResponse> {
    let path = Path::from(vec![
        Component::from(input.hive_id.clone()),
        Component::from(input.content_id.clone()),
    ]);
    let links = get_links(
        LinkQuery::try_new(path.path_entry_hash()?, LinkTypes::HummContentId)?,
        GetStrategy::Network,
    )?;

    let hashes: Vec<ActionHash> = links
        .into_iter()
        .filter_map(|link| link.target.into_action_hash())
        .collect();

    if hashes.is_empty() {
        return Err(wasm_error!(WasmErrorInner::Guest(format!(
            "Could not find the content with id: \"{}\"",
            input.content_id
        ))));
    }
    get_encrypted_content(hashes[0].clone())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListByAclInput {
    pub hive_id: String,
    pub content_type: String,
    /// `Owner` | `Admin` | `Writer` | `Reader`. String because the
    /// historical `serde` setup for the enum form was flaky; kept as
    /// string for wire stability.
    pub acl_role: String,
    pub entity_id: String,
}

#[hdk_extern]
pub fn list_by_acl_link(input: ListByAclInput) -> ExternResult<Vec<EncryptedContentResponse>> {
    let path = Path::from(vec![
        Component::from(input.hive_id),
        Component::from(input.content_type),
        Component::from(input.entity_id.clone()),
    ]);

    let links = match input.acl_role.as_str() {
        "Owner" => get_links(
            LinkQuery::try_new(path.path_entry_hash()?, LinkTypes::HummContentOwner)?,
            GetStrategy::Network,
        )?,
        "Admin" => get_links(
            LinkQuery::try_new(path.path_entry_hash()?, LinkTypes::HummContentAdmin)?,
            GetStrategy::Network,
        )?,
        "Writer" => get_links(
            LinkQuery::try_new(path.path_entry_hash()?, LinkTypes::HummContentWriter)?,
            GetStrategy::Network,
        )?,
        "Reader" => get_links(
            LinkQuery::try_new(path.path_entry_hash()?, LinkTypes::HummContentReader)?,
            GetStrategy::Network,
        )?,
        _ => {
            return Err(wasm_error!(WasmErrorInner::Guest(String::from(
                "Invalid acl_role"
            ))))
        }
    };

    let hashes: Vec<ActionHash> = links
        .into_iter()
        .filter_map(|link| link.target.into_action_hash())
        .collect();
    get_many_encrypted_content(hashes)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListByAuthorInput {
    pub author: String,
    pub content_type: String,
}

#[hdk_extern]
pub fn list_by_author(input: ListByAuthorInput) -> ExternResult<Vec<EncryptedContentResponse>> {
    let path = Path::from(vec![
        Component::from(input.author),
        Component::from(input.content_type),
    ]);
    let links = get_links(
        LinkQuery::try_new(path.path_entry_hash()?, LinkTypes::Hive)?,
        GetStrategy::Network,
    )?;

    let hashes: Vec<ActionHash> = links
        .into_iter()
        .filter_map(|link| link.target.into_action_hash())
        .collect();
    get_many_encrypted_content(hashes)
}

// =============================================================================
// C4 — `fetch_pair_ss_with_hive_check`
// =============================================================================

/// Input for `fetch_pair_ss_with_hive_check` (C4).
///
/// All fields are required. `active_hive_id` is the host's currently
/// active hive (humm-tauri tracks this in `ActiveHiveStore`); the C4
/// guarantee only holds within that hive's writer set, so the host MUST
/// pass it explicitly rather than letting the zome infer one.
#[derive(Serialize, Deserialize, Debug)]
pub struct FetchPairWithHiveCheckInput {
    /// The author whose pair-SS entries the caller wants. Matches on the
    /// `[author, content_type]` → `Hive` author path. This path is
    /// Holochain-attested: an entry is on this path iff that agent is
    /// the action.author.
    pub author: String,
    /// The active hive the caller trusts. The zome only returns entries
    /// also reachable from `[active_hive_id, content_type, group_id]` →
    /// `Dynamic` — a path a writer of THIS hive must have created. A
    /// poisoned SS authored elsewhere is on the author path but not on
    /// this hive's dynamic path, so the intersection excludes it.
    pub active_hive_id: String,
    pub content_type: String,
    /// The pair/group identifier used as the third component of the
    /// dynamic path. Opaque to the zome.
    pub group_id: String,
}

/// **C4** — return only entries reachable from BOTH the author path
/// AND the active hive's dynamic path. Intersects two `ActionHash` sets
/// drawn from `get_links` and resolves the survivors via
/// `get_many_encrypted_content`.
///
/// ## What this DOES defend against
///
/// Against an **unmodified-client** attacker who can only invoke the
/// stock `create_encrypted_content` extern with arbitrary inputs, the
/// intersection narrows results to entries that are both
/// authored-by-target AND placed under the caller's chosen
/// `(active_hive_id, content_type, group_id)` dynamic path. Such an
/// attacker, lacking access to put their poison under the victim's
/// active hive's dynamic path via the normal create flow, will at most
/// place their entry under their OWN `hive_id` — which fails the
/// intersection.
///
/// ## What this DOES NOT defend against (load-bearing — read this)
///
/// **This function does NOT close H-1** against any attacker willing to
/// run modified coordinator WASM (the standard Holochain adversary —
/// coordinator code is not a security boundary). Today the integrity
/// zome validators for `LinkTypes::Hive` and `LinkTypes::Dynamic` are
/// no-op `Ok(Valid)` stubs
/// (`dnas/humm_earth_core/zomes/integrity/content/src/lib.rs:119,127`
/// and `:302,310`); a modified-coordinator Mallory can directly publish
/// arbitrary `Hive` and `Dynamic` links pointing at her poison entry,
/// landing it in BOTH sets the intersection consults — including the
/// victim's active hive's dynamic path. The intersection therefore
/// returns the poisoned entry.
///
/// Closing H-1 properly requires integrity-level validators that prove
/// (a) the `Hive` author-path link's base equals the link author, and
/// (b) the `Dynamic` link's author has writer rights to the hive named
/// by the base path. Both are DNA-hash-bumping changes deferred per the
/// plan's I-A/I-B/I-C class. Until those ship, the TS-side trust checks
/// in `humm-tauri` (`from_agent` from C1 + decrypt-and-verify the SS)
/// remain the load-bearing control; C4 is a defense-in-depth narrowing,
/// not a cryptographic guarantee.
///
/// ## Empty-result semantics
///
/// Returns `[]` (not an error) when the intersection is empty — which
/// also covers the eventual-consistency case where one side of the
/// intersection has not yet propagated to the caller's arc. Callers
/// MUST treat `[]` as "not visible yet, retry" rather than "definitely
/// does not exist".
///
/// ## Robustness — single-bad-entry isolation (SEC-3 mitigation)
///
/// The intersection set is fetched **best-effort**: hashes that fail to
/// resolve (dead entries, transient DHT propagation gaps, attacker-
/// injected garbage AHs) are dropped from the result rather than
/// failing the whole call. This prevents an attacker-injected
/// unresolvable hash in the intersection from acting as a targeted
/// denial-of-service against legitimate pair-SS lookups.
#[hdk_extern]
pub fn fetch_pair_ss_with_hive_check(
    input: FetchPairWithHiveCheckInput,
) -> ExternResult<Vec<EncryptedContentResponse>> {
    let author_path = Path::from(vec![
        Component::from(input.author),
        Component::from(input.content_type.clone()),
    ]);
    let author_links = get_links(
        LinkQuery::try_new(author_path.path_entry_hash()?, LinkTypes::Hive)?,
        GetStrategy::Network,
    )?;
    let author_hashes: HashSet<ActionHash> = author_links
        .into_iter()
        .filter_map(|link| link.target.into_action_hash())
        .collect();

    // Short-circuit: if the author has no entries on this content_type,
    // skip the second DHT round-trip.
    if author_hashes.is_empty() {
        return Ok(vec![]);
    }

    let hive_path = Path::from(vec![
        Component::from(input.active_hive_id),
        Component::from(input.content_type),
        Component::from(input.group_id),
    ]);
    let hive_links = get_links(
        LinkQuery::try_new(hive_path.path_entry_hash()?, LinkTypes::Dynamic)?,
        GetStrategy::Network,
    )?;
    let hive_hashes: HashSet<ActionHash> = hive_links
        .into_iter()
        .filter_map(|link| link.target.into_action_hash())
        .collect();

    // Intersection driven by the SMALLER side (one HashSet::contains
    // per smaller-side element). Authors typically have many entries
    // across many groups; a single active group's dynamic path is much
    // narrower — but we don't know a priori which is smaller, so pick
    // at runtime.
    let (small, large) = if author_hashes.len() <= hive_hashes.len() {
        (author_hashes, &hive_hashes)
    } else {
        (hive_hashes, &author_hashes)
    };
    let intersection: Vec<ActionHash> = small
        .into_iter()
        .filter(|h| large.contains(h))
        .collect();
    if intersection.is_empty() {
        return Ok(vec![]);
    }
    // Best-effort fetch (SEC-3): skip un-resolvable hashes rather than
    // failing the whole query. An attacker who managed to inject one
    // garbage AH into the intersection (only possible under the
    // modified-coordinator scenario the docstring acknowledges) would
    // otherwise DoS legitimate pair-SS lookups.
    let mut out: Vec<EncryptedContentResponse> = Vec::with_capacity(intersection.len());
    for ah in intersection {
        match get_encrypted_content(ah.clone()) {
            Ok(resp) => out.push(resp),
            Err(err) => debug!(
                "fetch_pair_ss_with_hive_check: skipping unresolvable AH {ah}: {err:?}"
            ),
        }
    }
    Ok(out)
}