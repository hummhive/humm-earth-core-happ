# HummTauri Integration — pass-6-batch-reads coordinator generation (v3.4.0)

> **Audience:** humm-tauri devs adopting the batched/local read wire + anyone
> consuming the content zome's query surface.

## 1. TL;DR + release identity

- **One coordinator generation** `pass-6-batch-reads`, tag **`v3.4.0`**, on top
  of pass-6-service-meter v3.3.0. **DNA hash HELD byte-identical** —
  coordinator-only, hot-swappable, no cell migration:
  - DNA hash `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz`
  - integrity wasm sha256 `2656a9100937f7e6d17e2eebd5e744a1ef16e8e36b0efa089dc2f6382a655ae2` (unchanged)
  - happ sha256 `601fc4499e5d4a5a5553077fe960227318d6036d0aeef9c570e52ce2f81975bc` (recorded at the v3.4.0 reproducible build)
- **What it buys you:** the many-of-a-kind, per-item zome-call loops that
  dominate boot/feed/decrypt/ACL/DM paths collapse into one bounded call each;
  the caller's own data resolves from the local store without a network
  round-trip; list/page reads can flag tombstoned roots; and the cross-host
  content signal is ciphertext-free.
- **No breaking changes.** Every new extern is additive and cap-granted beside
  its singleton twin; every new wire field rides `#[serde(default)]`; all
  legacy externs (including `delete_encrypted_content`) keep their v3.3.0 wire
  shape. Adopt incrementally.

## 2. What changed (seams)

| Seam | Externs / fields |
|---|---|
| Bounded batch reads | `list_encrypted_content_by_dynamic_links`, `list_by_hive_links_many`, `get_many_by_content_id_link`, `list_by_author_many`, `content_id_exists` |
| Local read twins | `list_my_groups_local`, `list_by_hive_link_local_page` |
| Complete rosters | `list_group_members_many` |
| Role-key closure | `role_key_closure` |
| Paged inbox | `probe_inbox_page` |
| Liveness rider (B10) | `include_liveness` on the 7 read externs + `tombstoned` on `EncryptedContentResponse` |
| Owner-handoff hint | `OwnerHandoffOfferHint` best-effort signal from `initiate_owner_handoff` |
| Durable group discovery | `list_my_groups` granted-half now survives an Inbox sweep |

## 3. Wire shapes (verbatim)

Field names are the Rust wire shape; mirror them in TS.

### 3.1 Batch content reads

```rust
pub struct ListByDynamicLinksInput {
    pub hive_genesis_hash: ActionHash,
    pub content_type: String,
    pub dynamic_links: Vec<String>,
    #[serde(default)] pub since_ts: Option<Timestamp>,
    #[serde(default)] pub limit: Option<usize>,
    #[serde(default)] pub include_liveness: bool,
}
pub struct DynamicLinkBucket {
    pub dynamic_link: String,
    pub records: Vec<EncryptedContentResponse>,
    pub truncated: bool,
}
// list_encrypted_content_by_dynamic_links(ListByDynamicLinksInput)
//   -> Vec<DynamicLinkBucket>  (one bucket per requested label, request order)

pub struct HiveLinkRequest {
    pub content_type: String,
    #[serde(default)] pub since_ts: Option<Timestamp>,
    #[serde(default)] pub limit: Option<usize>,
    #[serde(default)] pub include_liveness: bool,
}
pub struct HiveLinksBatchInput {
    pub hive_genesis_hash: ActionHash,
    pub requests: Vec<HiveLinkRequest>,
}
pub struct HiveLinksBatchBucket {
    pub content_type: String,
    pub records: Vec<EncryptedContentResponse>,
    pub truncated: bool,
}
// list_by_hive_links_many(HiveLinksBatchInput) -> Vec<HiveLinksBatchBucket>

pub struct AuthorContentLookup {
    pub author: AgentPubKey,
    pub content_type: String,
    #[serde(default)] pub limit: Option<usize>,
}
pub struct AuthorBatchBucket {
    pub author: AgentPubKey,
    pub records: Vec<EncryptedContentResponse>,
    pub truncated: bool,
}
// list_by_author_many(Vec<AuthorContentLookup>) -> Vec<AuthorBatchBucket>

pub struct ContentIdLookup { pub hive_genesis_hash: ActionHash, pub content_id: String }
pub struct ContentIdResult {
    pub hive_genesis_hash: ActionHash,
    pub content_id: String,
    #[serde(default)] pub record: Option<EncryptedContentResponse>,
}
// get_many_by_content_id_link(Vec<ContentIdLookup>) -> Vec<ContentIdResult>
//   (record key always present; None when unresolved — row never dropped)

// content_id_exists(ListByContentIdInput { hive_genesis_hash, content_id }) -> bool
```

### 3.2 Membership / group / local reads

```rust
// list_group_members_many(Vec<ActionHash>)               // group genesis hashes
//   -> Vec<{ group_genesis_hash: ActionHash, members: Vec<GroupMembershipResponse> }>
//   COMPLETE rosters (never truncates); rejects over budget (see §4).

// list_my_groups_local(()) -> Vec<ListedGroup>            // local twin of list_my_groups
// list_by_hive_link_local_page(HiveLinkPageInput) -> BoundedLinkPage  // local twin

pub struct ListedGroup {
    pub group_genesis_hash: ActionHash,
    pub hive_genesis_hash: ActionHash,
    pub display_id: String,
    pub hive_wide_role: Option<HiveRole>,
    pub role: Option<HiveRole>,   // None = founder (implicit Owner); Some = granted
}
```

### 3.3 Role-key closure

```rust
pub struct RoleKeyClosureInput { pub hive_genesis_hash: ActionHash, pub granted_role: HiveRole }
pub struct RoleClosureEntry {
    pub role: HiveRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_genesis_hash: Option<ActionHash>,
}
pub struct RoleKeyClosure { pub entries: Vec<RoleClosureEntry> }
// role_key_closure(RoleKeyClosureInput) -> RoleKeyClosure
//   Owner⊇Admin⊇Writer⊇Reader, highest→lowest; canonical lowest-b64 genesis per
//   role; None when no system-role group for that role is visible yet;
//   IDENTITIES only — no key material.
```

### 3.4 Paged inbox

```rust
pub struct ProbeInboxPageInput {
    #[serde(default)] pub event_filter: Option<InboxEvent>,
    #[serde(default)] pub since_ts: Option<Timestamp>,
    #[serde(default)] pub limit: Option<usize>,
    #[serde(default)] pub source_after_action_hash: Option<String>,
}
pub struct InboxPage {
    pub items: Vec<InboxItem>,
    pub source_count: usize,
    pub source_positions: Vec<SourcePosition>,
    pub truncated: bool,
}
pub struct InboxItem {
    pub link_action_hash: ActionHash,
    pub target: ActionHash,
    pub event: Option<InboxEvent>,
    pub created_at: Timestamp,
    pub sender: AgentPubKey,
}
// probe_inbox_page(ProbeInboxPageInput) -> InboxPage   (legacy probe_inbox unchanged)
```

### 3.5 Liveness rider + owner-handoff hint

```rust
// EncryptedContentResponse gains:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub tombstoned: Option<bool>,

pub struct OwnerHandoffOfferHint {
    pub offer_hash: ActionHash,
    pub hive_genesis_hash: ActionHash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_agent: Option<AgentPubKey>,
}
```

## 4. Caps & budgets

| Extern | item cap | per-item page | aggregate budget |
|---|---|---|---|
| `list_encrypted_content_by_dynamic_links` | 64 labels | limit (def 100, max 256) | 4096 |
| `list_by_hive_links_many` | 32 requests | limit (def 100, max 256) | 4096 |
| `list_by_author_many` | 64 lookups | limit (def 100, max 256) | 4096 |
| `get_many_by_content_id_link` | 64 lookups | 1 record/lookup | n/a |
| `list_group_members_many` | 64 groups | full roster | 4096 roster links |

Exact reject literals: `dynamic_links batch accepts at most 64 labels`,
`hive-link batch accepts at most 32 requests`,
`content-id batch accepts at most 64 lookups`,
`author batch accepts at most 64 lookups`,
`batch total requested records exceed the 4096 budget`,
`group-members batch accepts at most 64 groups`,
`group-members batch roster links exceed the 4096 budget`.

At the default per-item limit (100), the 4096 budget binds before the 64-item
caps (≈40 items fit); the 32-request hive-links cap is reachable (32×100 =
3200). Pass a smaller per-item `limit` to use full item width. A
`list_group_members_many` batch rejected on the roster-link budget falls back
to the singleton `list_group_members` per group; rosters are otherwise
COMPLETE (never truncated — ACL derivation needs every member).

## 5. Liveness rider semantics (B10)

`include_liveness: bool` (`#[serde(default)]`) rides all seven read externs
(`list_by_dynamic_link`, `list_by_hive_link`, `list_by_acl_link`,
`list_by_author`, and the three `*_page` variants; the batch/local externs
carry it per item).

| `tombstoned` | Meaning |
|---|---|
| `Some(false)` | Root action probed and live |
| `Some(true)` | Root action has deletes (dead root, even when a byte-identical live sibling still resolves the entry) |
| absent | Not probed (`include_liveness: false` / older coordinator) OR probe-unknown |

Probed per root ACTION, not entry. Opt-in cost: +1 `get_details` per resolved
record — off the default read path. Tolerant: a probe failure yields absent,
never a dropped row. Use case: distinguish a dead root that still re-delivers
through a live sibling (the measured 176-phantom-hits/hr provider-watch case)
from a genuinely live record.

## 6. Signal dispatch (4 families)

`recv_remote_signal` try-decodes FOUR families in order: `EncryptedContentSignal`
→ `DmRemoteSignal` → `BlobPinSignal` → `OwnerHandoffOfferHint`. An unknown
payload is an explicit Guest error (the audit trail for open-cap peers):

```
recv_remote_signal: payload did not decode as EncryptedContentSignal, DmRemoteSignal, BlobPinSignal, or OwnerHandoffOfferHint
```

- `initiate_owner_handoff` best-effort sends `OwnerHandoffOfferHint` to the
  recipient (warn-never-block: a failed send never fails the committed offer).
  The governance panel reacts without polling — keep one
  `list_pending_owner_handoffs` on-mount as durable recovery.
- Provenance: `recv_remote_signal` overwrites `from_agent =
  call_info().provenance` on every delivery; a forged value never survives.
- Dispatcher guards must be disjoint: `OwnerHandoffOfferHint` is identified by
  `offer_hash` (and lacks `action_type`), so route it distinctly from the
  content/DM/blob families.

## 7. `probe_inbox_page` cursor semantics

Shares the content `*_page` engine — byte-identical cursor/limit literals.

- **Sort key** `(timestamp, create_link_hash)`; raw-byte hash order is the
  deterministic tie-break. Replay `SourcePosition.action_hash` VERBATIM as the
  next request's `source_after_action_hash` — never re-order or compare it
  client-side (b64 order differs from raw-byte order).
- **`since_ts` alone** = inclusive watermark (boundary dupes possible; dedupe by
  action hash). **Composite cursor** (`since_ts` + `source_after_action_hash`)
  = strictly exclusive.
- **Limits:** `None` → 100, `Some(0)` → `limit must be >= 1`, oversized →
  clamped to 256. A lone cursor hash → `source_after_action_hash requires
  since_ts`; a malformed one → `source_after_action_hash is not a valid
  ActionHash`.
- **Poison rows:** `source_positions` carries one entry per selected link even
  when the target is malformed (`source_count` may exceed `items.len()`), so
  callers cursor past poison rows.

## 8. N+1 → batch adoption map

| humm-tauri seam | Old pattern | New surface |
|---|---|---|
| `mediaAvailabilityRefreshQueue.ts` → `availability.ts` | per-blob `list_by_dynamic_link` loop | `list_encrypted_content_by_dynamic_links` |
| `sharedSecretCrud.ts:441-450` | per-group SS-candidate fetch | `list_encrypted_content_by_dynamic_links` |
| `rescueStrandedGroups.ts` | per-group fetch loop | `list_encrypted_content_by_dynamic_links` |
| `Feed/index.tsx` | per-addon-type `list_by_hive_link` fan-out | `list_by_hive_links_many` |
| `HiveApi.list()` (`hive/index.ts`) | serial per-hive resolves | `get_many_by_content_id_link` |
| `hummContentReads.ts` `checkEntryExists` | full-record fetch for `Boolean(record)` | `content_id_exists` |
| `sidecarSharedSecret.ts` | ≤31 sequential member/author scans | `list_by_author_many` |
| `deriveHiveGroupPublicKeyAcl.ts` | serial per-group rosters | `list_group_members_many` |
| `bootstrapRoleGroups.ts` / `deviceSet/bootstrap.ts` | ≤9 network `list_my_groups` polls | `list_my_groups_local` |
| `setupNewHive.ts` | sleep + network-page retry for own records | `list_by_hive_link_local_page` |
| `dmSweep.ts:269-287` | per-item consume + drain | `probe_inbox_page` + shipped `get_many_encrypted_content` |
| client role-K fan-out | per-role group resolution | `role_key_closure` |
| `ownerHandoff.ts` | polling `list_pending_owner_handoffs` | `OwnerHandoffOfferHint` + one list-on-mount |

`list_my_groups` (network) granted-half now walks the durable
`AgentToGroupMemberships` index, so granted-group discovery survives a full DM
inbox sweep; founded-group discovery stays self-Inbox (a founder re-derives
founded groups from their own chain).

## 9. BDD acceptance (given / when / then)

`[coordinator]` = conductor-proven here; `[humm-tauri]` = your obligation.

- `[coordinator]` **Given** a batch of N items, **when** any batch extern
  returns, **then** there are exactly N buckets in request order (duplicates
  included), an unresolved item is `null`/empty — never dropped or reordered.
- `[coordinator]` **Given** a request over an item cap, **when** called,
  **then** it rejects with the exact §4 literal; **given** summed per-item
  limits over 4096, **then** `batch total requested records exceed the 4096
  budget`; **given** a roster batch over 4096 links, **then**
  `group-members batch roster links exceed the 4096 budget` (rosters otherwise
  complete).
- `[coordinator]` **Given** a dynamic label with 3 records and `limit: 2`,
  **then** the bucket has 2 records and `truncated: true`; `limit: 5` →
  3 records, `truncated: false`.
- `[coordinator]` **Given** self-authored content on a peerless cell, **when**
  `list_by_hive_link_local_page` runs, **then** it returns local-store records
  (no network) matching the network twin; **given** two grants (Reader then
  Writer) for one group, **when** `list_my_groups_local` /
  `list_group_members_many` runs, **then** the newest grant wins and an expired
  grant is filtered out.
- `[coordinator]` **Given** a hive with system-role groups, **when**
  `role_key_closure(hive, Admin)` runs, **then** entries are
  [Admin, Writer, Reader] with each canonical (lowest-b64) genesis, and a
  missing role's entry is `None`.
- `[coordinator]` **Given** `include_liveness: false`, **then** `tombstoned` is
  absent on every row; **given** `true`, **then** a live root is `Some(false)`
  and an ordinarily-deleted root drops from the listing (single-node); the ACL
  list carries the same rider.
- `[coordinator]` **Given** more inbox links than `limit`, **when**
  `probe_inbox_page` pages with the replayed cursor, **then** pages neither
  duplicate nor skip at equal timestamps; `limit: 0` / lone cursor / bad cursor
  reject with the exact §7 literals.
- `[coordinator]` **Given** `initiate_owner_handoff` to a recipient, **then**
  the recipient receives an `OwnerHandoffOfferHint` stamped with the real
  provenance; a forged `from_agent` is overwritten.
- `[humm-tauri]` **Given** a remote-delivered signal, **when** the dispatcher
  classifies it, **then** disjoint guards route `OwnerHandoffOfferHint`
  (`offer_hash`) distinctly from the content/DM/blob families.

## 10. Cap grants + unchanged contracts

10 new grants (all read-only, Unrestricted, beside their singleton twins):
`probe_inbox_page`, `role_key_closure`,
`list_encrypted_content_by_dynamic_links`, `list_by_hive_links_many`,
`get_many_by_content_id_link`, `list_by_author_many`, `content_id_exists`,
`list_group_members_many`, `list_my_groups_local`,
`list_by_hive_link_local_page`.

All legacy externs are wire-identical to v3.3.0. `delete_encrypted_content` is
UNCHANGED in this generation (still returns `ActionHash`).

## 11. Testing posture + install

Conductor-proven here (in-process sweettest against the HELD DNA): the batch
externs (ordering/alignment, item caps, per-item first-page bounds, aggregate
budget rejects, complete-roster budget reject), the local twins
(`list_my_groups_local`, `list_by_hive_link_local_page` vs their network
twins), `role_key_closure` (dominance order + missing-role `None`), the
liveness rider (off/on/deleted-root, ACL list), `probe_inbox_page` (cursor
replay, limit/cursor rejects), and the owner-handoff hint (delivery +
forged-provenance overwrite). Host unit suites + `cargo clippy -D warnings` +
fmt clean; the `pinned_hosts` suite remains green as the page-engine
regression.

Install-time constants bump (humm-tauri):

| Constant | Today | v3.4.0 |
|---|---|---|
| `CURRENT_HAPP_LABEL` | `pass-6-service-meter` | `pass-6-batch-reads` |
| `CURRENT_HAPP_SHA256` | `b98916f1…` | `601fc4499e5d4a5a5553077fe960227318d6036d0aeef9c570e52ce2f81975bc` |
| `CURRENT_HAPP_DNA_HASH_BASE64` | `uhC0ksXs…` | `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz` (held) |
| `COORDINATOR_WASM_VERSION` | 11 | 12 |

The MANIFEST / registry row is appended LAST, after the artifact exists and its
sha256 is verified.
