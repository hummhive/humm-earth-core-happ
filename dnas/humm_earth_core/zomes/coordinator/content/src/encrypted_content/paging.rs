//! Bounded source-cursor page externs (pass-6-pinned-hosts).
//!
//! New extern names beside the wire-stable legacy queries: old
//! coordinators must hard-fail an unknown fn name rather than silently
//! ignore new request fields. Every page extern applies its bound to
//! LINKS before any target fetch, so the expensive DHT get/decrypt work
//! is O(limit) even under an OpenWrite flood. `source_positions` are
//! SOURCE truth: one position per selected link, present even when the
//! link's target never resolves, so a caller can cursor past poison
//! rows without checkpointing undispatched records.

use content_integrity::*;
use hdi::hash_path::path::Component;
use hdk::prelude::*;
use std::collections::HashSet;

use super::crud::get_encrypted_content;
use super::EncryptedContentResponse;

const LINK_PAGE_DEFAULT_LIMIT: usize = 100;
const LINK_PAGE_HARD_LIMIT: usize = 256;
const MY_CONTENT_HARD_LIMIT: usize = 4096;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SourcePosition {
    pub timestamp_micros: i64,
    /// CreateLink action hash in b64 string form. Replay verbatim as the
    /// next request's `source_after_action_hash` — never re-order or
    /// compare client-side (the server tie-break is raw-byte hash order,
    /// which differs from b64 lexicographic order).
    pub action_hash: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BoundedLinkPage {
    pub records: Vec<EncryptedContentResponse>,
    pub source_count: usize,
    pub source_positions: Vec<SourcePosition>,
    pub truncated: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HiveLinkPageInput {
    pub hive_genesis_hash: ActionHash,
    pub content_type: String,
    #[serde(default)]
    pub since_ts: Option<Timestamp>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub source_after_action_hash: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DynamicLinkPageInput {
    pub hive_genesis_hash: ActionHash,
    pub content_type: String,
    pub dynamic_link: String,
    #[serde(default)]
    pub since_ts: Option<Timestamp>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub source_after_action_hash: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AuthorLinkPageInput {
    pub author: String,
    pub content_type: String,
    #[serde(default)]
    pub since_ts: Option<Timestamp>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub source_after_action_hash: Option<String>,
}

/// Paged twin of `list_by_hive_link` — same path, same link type, plus
/// the composite source cursor (`since_ts` + `source_after_action_hash`).
#[hdk_extern]
pub fn list_by_hive_link_page(input: HiveLinkPageInput) -> ExternResult<BoundedLinkPage> {
    let path = Path::from(vec![
        Component::from(input.hive_genesis_hash.to_string()),
        Component::from(input.content_type),
    ]);
    link_page(
        path.path_entry_hash()?,
        LinkTypes::Hive,
        input.since_ts,
        input.limit,
        input.source_after_action_hash,
    )
}

/// Paged twin of `list_by_dynamic_link`.
#[hdk_extern]
pub fn list_by_dynamic_link_page(input: DynamicLinkPageInput) -> ExternResult<BoundedLinkPage> {
    let path = Path::from(vec![
        Component::from(input.hive_genesis_hash.to_string()),
        Component::from(input.content_type),
        Component::from(input.dynamic_link),
    ]);
    link_page(
        path.path_entry_hash()?,
        LinkTypes::Dynamic,
        input.since_ts,
        input.limit,
        input.source_after_action_hash,
    )
}

/// Paged twin of `list_by_author` (author-shape Hive link every entry
/// gets at create). The legacy unbounded `list_by_author` stays
/// untouched — downstream admission checks depend on its exact
/// semantics.
#[hdk_extern]
pub fn list_by_author_page(input: AuthorLinkPageInput) -> ExternResult<BoundedLinkPage> {
    let path = Path::from(vec![
        Component::from(input.author),
        Component::from(input.content_type),
    ]);
    link_page(
        path.path_entry_hash()?,
        LinkTypes::Hive,
        input.since_ts,
        input.limit,
        input.source_after_action_hash,
    )
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MyContentByIdInput {
    pub hive_genesis_hash: ActionHash,
    pub content_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OwnContentRecords {
    pub records: Vec<EncryptedContentResponse>,
    pub truncated: bool,
}

/// Exact-own lookup on the content-id path: only links AUTHORED BY THE
/// CALLING AGENT are considered, so foreign fixed-id collisions are
/// excluded at the link layer before any target fetch. Empty result is
/// `{records: [], truncated: false}`, distinguishable from the legacy
/// `get_by_content_id_link` error path. NOT cap-granted: "my" is
/// provenance-derived, and a remote grant would let any peer enumerate
/// the callee's own records.
#[hdk_extern]
pub fn get_my_content_by_id_link(input: MyContentByIdInput) -> ExternResult<OwnContentRecords> {
    let me = agent_info()?.agent_initial_pubkey;
    let (records, truncated) =
        content_id_records_by_author(&input.hive_genesis_hash, &input.content_id, &me)?;
    Ok(OwnContentRecords { records, truncated })
}

/// Author-scoped walk of the `HummContentId` path
/// `[hive_genesis_hash_b64, content_id]`: only links AUTHORED BY
/// `author` are considered, so foreign fixed-id collisions are excluded
/// at the link layer before any target fetch. Shared probe behind
/// [`get_my_content_by_id_link`], the find-or-create family, hiveless
/// remediation, and the HiveGenesis migration marker.
pub(crate) fn content_id_records_by_author(
    hive_genesis_hash: &ActionHash,
    content_id: &str,
    author: &AgentPubKey,
) -> ExternResult<(Vec<EncryptedContentResponse>, bool)> {
    let path = Path::from(vec![
        Component::from(hive_genesis_hash.to_string()),
        Component::from(content_id),
    ]);
    let query = LinkQuery::try_new(path.path_entry_hash()?, LinkTypes::HummContentId)?
        .author(author.clone());
    let mut links = get_links(query, GetStrategy::Network)?;
    // Defensive post-filter: the author scoping is load-bearing for
    // exclusion of foreign collisions, so never trust the query filter alone.
    links.retain(|link| link.author == *author);
    sort_by_source_position(&mut links);
    let deduped = dedupe_by_target(links);
    let (selected, truncated) = page_links(deduped, None, None, MY_CONTENT_HARD_LIMIT);
    Ok((resolve_targets(selected), truncated))
}

/// Canonical pick when multiple candidates share a content-id path:
/// lexicographically-lowest base64 `hash` STRING wins — the identical
/// comparison humm-tauri's `utils/selectCanonicalByHash.ts` performs in
/// JS, so both sides always elect the same record. (Base64url string
/// order deliberately differs from raw-byte order; the STRING is the
/// contract.)
pub(crate) fn canonical_lowest_hash(
    records: Vec<EncryptedContentResponse>,
) -> Option<EncryptedContentResponse> {
    records.into_iter().min_by(|a, b| a.hash.cmp(&b.hash))
}

/// Shared page engine behind the three `*_page` externs.
fn link_page(
    path_hash: EntryHash,
    link_type: LinkTypes,
    since_ts: Option<Timestamp>,
    limit: Option<usize>,
    source_after_action_hash: Option<String>,
) -> ExternResult<BoundedLinkPage> {
    let limit = resolve_page_limit(limit)?;
    let after_hash = source_after_action_hash
        .as_deref()
        .map(decode_cursor_hash)
        .transpose()?;
    if after_hash.is_some() && since_ts.is_none() {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "source_after_action_hash requires since_ts".into()
        )));
    }
    // `LinkQuery::after` is deliberately NOT used with a composite
    // cursor: its boundary is approximate, and an approximate boundary
    // under the strict composite filter could drop equal-timestamp rows.
    let links = get_links(
        LinkQuery::try_new(path_hash, link_type)?,
        GetStrategy::Network,
    )?;
    let (selected, truncated) = page_links(links, since_ts, after_hash, limit);
    let source_positions: Vec<SourcePosition> = selected
        .iter()
        .map(|link| SourcePosition {
            timestamp_micros: link.timestamp.as_micros(),
            action_hash: link.create_link_hash.to_string(),
        })
        .collect();
    Ok(BoundedLinkPage {
        source_count: source_positions.len(),
        source_positions,
        records: resolve_targets(selected),
        truncated,
    })
}

fn resolve_page_limit(limit: Option<usize>) -> ExternResult<usize> {
    match limit {
        None => Ok(LINK_PAGE_DEFAULT_LIMIT),
        Some(0) => Err(wasm_error!(WasmErrorInner::Guest(
            "limit must be >= 1".into()
        ))),
        Some(n) => Ok(n.min(LINK_PAGE_HARD_LIMIT)),
    }
}

fn decode_cursor_hash(encoded: &str) -> ExternResult<ActionHash> {
    ActionHash::try_from(encoded).map_err(|_| {
        wasm_error!(WasmErrorInner::Guest(
            "source_after_action_hash is not a valid ActionHash".into()
        ))
    })
}

/// Pure cursor core: sort by `(timestamp, create_link_hash)` (raw-byte
/// hash order is THE deterministic tie-break), apply the cursor filter,
/// then truncate to `limit`. Returns `(selected page, truncated)`.
fn page_links(
    mut links: Vec<Link>,
    since_ts: Option<Timestamp>,
    after_hash: Option<ActionHash>,
    limit: usize,
) -> (Vec<Link>, bool) {
    sort_by_source_position(&mut links);
    let mut selected: Vec<Link> = links
        .into_iter()
        .filter(|link| cursor_admits(link, since_ts.as_ref(), after_hash.as_ref()))
        .collect();
    let truncated = selected.len() > limit;
    selected.truncate(limit);
    (selected, truncated)
}

/// Composite cursor (`since_ts` + `after_hash`) is STRICTLY EXCLUSIVE —
/// no dupes and no skips at equal timestamps. `since_ts` alone keeps the
/// inclusive legacy-watermark semantics (boundary duplicates possible;
/// callers dedupe by action hash). A lone `after_hash` cannot be
/// positioned and is rejected by the extern before this runs.
fn cursor_admits(
    link: &Link,
    since_ts: Option<&Timestamp>,
    after_hash: Option<&ActionHash>,
) -> bool {
    match (since_ts, after_hash) {
        (Some(since), Some(after)) => (link.timestamp, &link.create_link_hash) > (*since, after),
        (Some(since), None) => link.timestamp >= *since,
        (None, _) => true,
    }
}

fn sort_by_source_position(links: &mut [Link]) {
    links.sort_by(|a, b| {
        (a.timestamp, &a.create_link_hash).cmp(&(b.timestamp, &b.create_link_hash))
    });
}

fn dedupe_by_target(links: Vec<Link>) -> Vec<Link> {
    let mut seen = HashSet::new();
    links
        .into_iter()
        .filter(|link| seen.insert(link.target.clone()))
        .collect()
}

/// Per-target failure isolation: a malformed, tombstoned, or
/// gossip-lagged target drops from `records` while its source position
/// survives, so callers cursor past poison rows. This `.ok()` is the
/// documented list-read contract (`get_many_encrypted_content` doc).
fn resolve_targets(links: Vec<Link>) -> Vec<EncryptedContentResponse> {
    links
        .into_iter()
        .filter_map(|link| link.target.into_action_hash())
        .filter_map(|ah| get_encrypted_content(ah).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_with_hash(hash: &str) -> EncryptedContentResponse {
        EncryptedContentResponse {
            encrypted_content: EncryptedContent {
                header: EncryptedContentHeader {
                    id: "id".into(),
                    display_hive_id: String::new(),
                    content_type: "t".into(),
                    revision_author_signing_public_key: String::new(),
                    acl_spec: AclSpec::OpenWrite {
                        target_hive_genesis_hash: None,
                    },
                    public_key_acl: Acl {
                        owner: String::new(),
                        admin: vec![],
                        writer: vec![],
                        reader: vec![],
                    },
                },
                bytes: UnsafeBytes::from(vec![1u8]).into(),
            },
            hash: hash.to_string(),
            original_hash: hash.to_string(),
            latest_action_micros: None,
        }
    }

    #[test]
    fn canonical_lowest_hash_picks_lexicographically_smallest_b64_string() {
        let hashes: Vec<String> = [[9u8; 36], [3u8; 36], [7u8; 36]]
            .into_iter()
            .map(|raw| ActionHash::from_raw_36(raw.to_vec()).to_string())
            .collect();
        let expected = hashes.iter().min().expect("non-empty").clone();

        let picked = canonical_lowest_hash(
            hashes.iter().map(|hash| response_with_hash(hash)).collect(),
        )
        .expect("candidates non-empty");
        assert_eq!(picked.hash, expected);

        assert!(canonical_lowest_hash(Vec::new()).is_none());
    }

    fn hash_bytes(index: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; 36];
        bytes[0] = (index >> 8) as u8;
        bytes[1] = (index & 0xFF) as u8;
        bytes
    }

    fn link_at(timestamp_micros: i64, index: u16) -> Link {
        Link {
            author: AgentPubKey::from_raw_36(vec![1u8; 36]),
            base: EntryHash::from_raw_36(vec![2u8; 36]).into(),
            target: ActionHash::from_raw_36(hash_bytes(index)).into(),
            timestamp: Timestamp(timestamp_micros),
            zome_index: 0.into(),
            link_type: LinkType(0),
            tag: LinkTag::new(Vec::new()),
            create_link_hash: ActionHash::from_raw_36(hash_bytes(index)),
        }
    }

    fn indices(links: &[Link]) -> Vec<u16> {
        links
            .iter()
            .map(|link| {
                let raw = link.create_link_hash.get_raw_36();
                (u16::from(raw[0]) << 8) | u16::from(raw[1])
            })
            .collect()
    }

    #[test]
    fn page_links_orders_by_timestamp_then_create_link_hash() {
        let shuffled = vec![
            link_at(200, 5),
            link_at(100, 3),
            link_at(100, 1),
            link_at(100, 2),
        ];
        let (page, truncated) = page_links(shuffled, None, None, 10);
        assert!(!truncated);
        assert_eq!(indices(&page), vec![1, 2, 3, 5]);
    }

    #[test]
    fn composite_cursor_is_strictly_exclusive_and_gapless() {
        let all: Vec<Link> = (1..=5).map(|i| link_at(100, i)).collect();

        let (page1, truncated1) = page_links(all.clone(), None, None, 2);
        assert!(truncated1);
        assert_eq!(indices(&page1), vec![1, 2]);

        let cursor1 = page1.last().expect("page1 non-empty");
        let (page2, truncated2) = page_links(
            all.clone(),
            Some(cursor1.timestamp),
            Some(cursor1.create_link_hash.clone()),
            2,
        );
        assert!(truncated2);
        assert_eq!(indices(&page2), vec![3, 4]);

        let cursor2 = page2.last().expect("page2 non-empty");
        let (page3, truncated3) = page_links(
            all,
            Some(cursor2.timestamp),
            Some(cursor2.create_link_hash.clone()),
            2,
        );
        assert!(!truncated3);
        assert_eq!(indices(&page3), vec![5]);
    }

    #[test]
    fn since_ts_only_is_inclusive_at_boundary() {
        let links = vec![link_at(99, 1), link_at(100, 2), link_at(101, 3)];
        let (page, truncated) = page_links(links, Some(Timestamp(100)), None, 10);
        assert!(!truncated);
        assert_eq!(indices(&page), vec![2, 3]);
    }

    #[test]
    fn limit_zero_rejected_and_oversize_clamped() {
        let err = resolve_page_limit(Some(0)).expect_err("limit 0 must be rejected");
        assert!(err.to_string().contains("limit must be >= 1"));

        assert_eq!(
            resolve_page_limit(Some(LINK_PAGE_HARD_LIMIT + 1)).expect("clamp"),
            LINK_PAGE_HARD_LIMIT
        );
        assert_eq!(
            resolve_page_limit(None).expect("default"),
            LINK_PAGE_DEFAULT_LIMIT
        );

        let many: Vec<Link> = (0..(LINK_PAGE_HARD_LIMIT + 44) as u16)
            .map(|i| link_at(100, i))
            .collect();
        let (page, truncated) = page_links(many, None, None, LINK_PAGE_HARD_LIMIT);
        assert!(truncated);
        assert_eq!(page.len(), LINK_PAGE_HARD_LIMIT);
    }

    #[test]
    fn saturation_flags_truncated_at_hard_limit() {
        let mut links: Vec<Link> = (0..(MY_CONTENT_HARD_LIMIT + 1) as u16)
            .map(|i| link_at(100, i))
            .collect();
        links.push(link_at(100, 0));

        sort_by_source_position(&mut links);
        let deduped = dedupe_by_target(links);
        assert_eq!(
            deduped.len(),
            MY_CONTENT_HARD_LIMIT + 1,
            "duplicate target must collapse"
        );

        let (page, truncated) = page_links(deduped, None, None, MY_CONTENT_HARD_LIMIT);
        assert!(truncated);
        assert_eq!(page.len(), MY_CONTENT_HARD_LIMIT);
    }
}
