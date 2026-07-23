# HummTauri Integration — pass-7 integrity fork (complete handoff, M0–M22)

> **STATUS: BRANCH-ONLY on `feat-integrity-pass-7`. NOT distributed. Do NOT hand
> to humm-tauri until the pass-7 blessing.** This is the complete branch-wide
> handoff — every client-visible change across Waves 1–4 (M0–M22) — replacing
> the earlier Wave-4-only draft (`HUMM_TAURI_WAVE4_INTEGRATION.md`, folded in
> here and deleted). The shipped contract remains **pass-6-service-meter
> v3.3.0**: DNA `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz`, happ
> sha256 `b98916f18def33731a93b70c36f60838186a52e4e41efcd58de5071f150430c8`.
>
> **Audience:** humm-tauri devs planning the pass-6 → pass-7 cutover + anyone
> consuming the content zome's wire.

## 1. TL;DR

- **One integrity fork.** Pass-7 changes the integrity zome, so it is a NEW
  DNA: a new cell, a disjoint DHT network, and one migration event
  (pass-6 content → pass-7 cell). After that single fork, everything else in
  this doc is hot-swappable coordinator surface on the pass-7 DNA.
- **Headline capabilities:**
  - **Portable content identity + happ-side migration idempotency** — content
    carries an immutable cross-generation `lineage` claim; the zome owns
    find-wins dedup, authorship probing, and old→new discovery
    (`resolve_by_prior_generation`). The client migration module shrinks
    around its identity/dedup core (§4, §11).
  - **Durable discovery** — hive + group membership discovery moves off the
    sweepable Inbox onto author-bound indexes (`HiveMembershipIndex`,
    `AgentToGroupMemberships`); a full DM sweep no longer erases hive/group
    discovery (§5.4).
  - **Batch + local reads** — bounded batch externs collapse the client's
    N+1 zome-call loops; local twins serve boot/recovery without network
    round-trips (§6).
  - **Ciphertext-free remote signals** — the cross-host content channel
    carries fetch hints, never ciphertext; provenance is conductor-stamped
    (§7).
  - **Liveness rider** — list/page reads can flag tombstoned roots
    (`include_liveness` / `tombstoned`), killing the dead-root re-delivery
    class (§5.3).
- **Integrity contract hardening** — headers, ACLs, payload sizes, update
  continuity, system-role groups, and lineage are now validator-enforced
  (reject literals L1–L23, §3). Formerly-admissible writes reject on the new
  DNA.

### Release identity

| What | Value |
|---|---|
| Shipped today (pass-6-service-meter v3.3.0) DNA | `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz` |
| Shipped integrity wasm sha256 | `2656a9100937f7e6d17e2eebd5e744a1ef16e8e36b0efa089dc2f6382a655ae2` |
| Shipped happ sha256 | `b98916f18def33731a93b70c36f60838186a52e4e41efcd58de5071f150430c8` |
| Pass-7 scratch DNA (branch tip) | `uhC0k-HAqM4zW2rCWrKSujEKDZcqybE_ATUjKxkRy2BmRjURYddxP` — scratch pin, re-derived at blessing build |
| Pass-7 scratch integrity wasm sha256 | `ec11ba8f9518cee6aee5d9e1df4fc1f7449f42584213abb4f8636cdceb90fcdd` — scratch pin |
| Pass-7 blessed DNA / integrity wasm / content wasm / happ sha / artifact | **TBD at blessing** (the blessing build re-derives every identity; do not pin the scratch values) |

## 2. Breaking & migration-required

New DNA = new cell + one data-migration event. On top of that, four contract
changes need client action:

| # | Change | Was (pass-6) | Now (pass-7) | Client action |
|---|---|---|---|---|
| B-1 | `delete_encrypted_content` response | `ActionHash` | `DeleteContentResponse { was_deleted, delete_action_hash }` | Update the TS return type. `was_deleted: false` = no-op success ("goal met OR target currently unresolvable from this node") — retry loops stop erroring on already-met goals; cross-author deleters re-probe later instead of treating it as terminal (§5.5). |
| B-2 | `recv_remote_signal` fall-through | 3-family error text | 5-family error text (§7) | Update any log matcher pinned to the old literal. |
| B-3 | Remote content fan-out | full `EncryptedContentSignal` (ciphertext on the wire) | `EncryptedContentHint` only (identifiers, no `data`) | MIGRATION REQUIRED: hint ingest + disjoint dispatcher guards (§7). |
| B-4 | Integrity strictness | headers/ACLs/payloads/updates largely unvalidated | L1–L23 enforced (§3) | Writes that violate the bounds REJECT on the new DNA. Pre-validate sizes client-side; the migration importer must satisfy the same bounds. |

Everything else is additive: no extern was removed or renamed; new wire fields
ride `#[serde(default)]` so old payloads still decode.

## 3. Integrity contract delta (the fork)

### 3.1 Bounds + structural rejects (create path)

| # | Reject literal | Enforced by |
|---|---|---|
| L1 | `header id must be 1-256 chars` | `validate_header_bounds` |
| L2 | `header content_type must be 1-128 chars` | `validate_header_bounds` |
| L3 | `header display_hive_id must be at most 256 chars` | `validate_header_bounds` |
| L4 | `public_key_acl owner must be at most 64 chars` | `validate_header_bounds` |
| L5 | `public_key_acl buckets accept at most 256 entries` | `validate_header_bounds` |
| L6 | `public_key_acl keys must be 1-64 chars` | `validate_header_bounds` |
| L7 | `public_key_acl buckets must not contain duplicate keys` | `validate_header_bounds` |
| L12 | `Public and OpenWrite payloads accept at most 1000000 bytes` | `validate_open_write_payload_size` |

### 3.2 Update continuity

| # | Reject literal | Meaning |
|---|---|---|
| L8 | `EncryptedContent updates must not change the id` | id frozen |
| L9 | `EncryptedContent updates must not change the hive context` | hive frozen |
| L10 | `EncryptedContent updates must not change the acl_spec variant` | ACL variant frozen |
| L11 | `EncryptedContent updates may only stamp content_type with the _migrated/ prefix` | content_type may gain the `_migrated/` prefix once; no other change |
| L17 | `lineage is immutable once set` | lineage None→Some allowed ONCE; change/remove reject |

Defensive (normally shadowed by the upstream same-entry-type gate):
`update original is not an EncryptedContent`.

### 3.3 System-role groups

| # | Reject literal | Enforced by |
|---|---|---|
| L13 | `a GroupGenesis for this hive and hive-wide role already exists on your chain` | `validate_create_group_genesis` |
| L21 | `system-role GroupGenesis display_id must be 1-256 chars` | `system_role_display_id_verdict` |
| L22 | `a system-role GroupGenesis with this display_id already exists in this hive on your chain` | `validate_unique_system_role_on_chain` |

`find_or_create_group_genesis` catches ONLY L13 in its find-wins fallback —
find-wins resolves by `(hive, role)`. **L22 propagates as a hard error**: a
display_id conflict with a DIFFERENT role's group is a real client-visible
conflict, never silently resolvable (and unreachable with globally-unique
squuids).

### 3.4 HiveGroup ACL

| # | Reject literal |
|---|---|
| L23 | `HiveGroup group_acl buckets must be disjoint: {duplicate} appears more than once` |

A group listed in two `group_acl` buckets is redundant under the witness
dominance chain; it is rejected before any authority walk.

### 3.5 Cross-generation lineage (L14–L20)

| # | Reject literal | Enforced by |
|---|---|---|
| L14 | `lineage prior dna hash is not a valid DNA hash` | `validate_lineage_shape` |
| L15 | `lineage prior action hash is not a valid action hash` | `validate_lineage_shape` |
| L16 | `lineage must cite a prior generation, not this one` | `run_content_validators` |
| L18 | `lineage prior record did not resolve in the prior-generation cell` | `probe_prior_authorship` (coordinator) |
| L19 | `lineage prior record was not authored by the caller` | `probe_prior_authorship` (coordinator) |
| L20 | `lineage prior cell is not reachable on this conductor` | `probe_prior_authorship` (coordinator) |

`Lineage` link rejects (integrity):
`Lineage link base does not match the target's lineage claim`,
`Lineage link target has no lineage claim in its header`,
`Lineage link delete must be authored by the link creator`.

`HiveMembershipIndex` link rejects (integrity):
`HiveMembershipIndex tag must be empty`,
`HiveMembershipIndex target must be a HiveMembership or HiveGenesis`,
`HiveMembershipIndex base must be the membership's for_agent`,
`HiveMembershipIndex base must be the hive genesis author`,
`HiveMembershipIndex link may only be deleted by its author (creator: …, attempted by: …)`.

### 3.6 LinkTypes appended

`Lineage` = 18, `HiveMembershipIndex` = 19. Appended — every existing link
type keeps its index.

### 3.7 Err→Invalid normalization

Local link-validator structural rejects (non-action targets, type-mismatch
decodes) moved from host `Err` (validation-retry limbo) to deterministic
`Invalid`. Every message string is byte-identical; only the reject class
moved. This affects only custom coordinators publishing malformed links —
humm-tauri's writes never hit these paths.

### 3.8 Privacy & metadata contract (blessing-time surface)

The pass-7 fork WIDENED public relationship metadata; the DHT cannot hide
these, so the client must not add semantic leakage on top:

- **`HiveMembershipIndex`** (agent pubkey → membership/genesis targets)
  exposes hive AFFILIATION enumeration for any agent.
- **`Lineage`** (plaintext prior-action tag + header lineage) CORRELATES an
  agent's content across generations.
- **Discovery paths** hash their bases, but the plaintext link TAGS remain
  visible — the Dynamic-label tag and the ACL group-hash tag are readable by
  any DHT observer.
- **CLIENT RULE (load-bearing):** any sensitive `dynamic_links` value MUST be
  either a RANDOM ≥128-bit id or a MAC of the label under a SECRET KEY the
  observer does not have — an approved construction is HMAC-SHA-512 or keyed
  BLAKE3 (NOT hash(salt || label): the key is a secret MAC key, not a public
  salt). If the key is derived, use HKDF-SHA-512 with explicit domain
  separation. NEVER a semantic/plaintext label, and NEVER a bare (unkeyed)
  hash of a guessable label (an unkeyed hash of "invoices-2026" is
  dictionary/precompute-attackable, so it leaks the same meaning). This
  prevents DIRECT SEMANTIC disclosure only: an opaque tag is still an
  equality/linkability signal (the same tag across records is correlatable)
  and exposes frequency + timing; it does NOT make the record anonymous.
  This is a client convention — the zome stores whatever label it is given.
- **S-2 accepted residual:** an unsolicited `create_hive_membership` grant
  permanently populates the grantee's `HiveMembershipIndex` base with rows
  only the GRANTOR can retract (author-only delete). UI griefing only — zero
  privilege escalation. Mitigation is client-side: a hide-list keyed on
  genesis hash (humm-tauri owns the suppression UX).

## 4. Migration moves to the happ

The load-bearing shift: content identity and migration idempotency move from
client bookkeeping into the zome.

**Today (pass-5→6, client-owned identity).** `MigrationStore.ts` drives
`migrate_hive` → `grant_memberships` → `migration_export` (whole-chain
`get_messages_since` with `since_seq: 0` per hive, re-exported per hive in
the loop) → `migration_import` (per-entry plain `create_encrypted_content` —
NOT idempotent on the wire, so `flows/import.rs` maintains an
`already_imported` HashSet from `remap.json`, plus the per-lineage-era remap
archival dance at `import.rs:182-223`) → `mark_hive_migrated` (best-effort;
per-entry `mark_migrated` is CLI-only). Content identity is
`(DNA-hash, action-hash)`-keyed and dies at every fork — the reason
`src-tauri/src/migration/` is 5,478 lines across 18 files (`WalkEntry` codec,
`resolve_marker_dna_hash` cfg hack, remap types/bundle/orchestration).

**Pass-7 (happ-owned identity).** The importer switches the target write to
`create_encrypted_content_with_lineage { create, lineage, prior_cell }` with
`lineage` = (pass-6 DNA b64, original action b64) and
`prior_cell: Some(<pass-6 cell>)`:

| Concern | Where it lives today | Where it lives at pass-7 |
|---|---|---|
| Import idempotency / dedup | `already_imported` HashSet from `remap.json` (`flows/import.rs`) | Zome find-wins keyed `(prior pair, author)` — a crash-resume re-run returns `was_created: false`, no new write |
| Authorship verification | Client-side trust bookkeeping | The L18/L19/L20 bridge probe (`prior_cell: Some`); a claim cannot silently downgrade to unverified when `prior_cell` is supplied |
| Old→new discovery | `RemapEntry` consumer lookups | `resolve_by_prior_generation` — works for ANY agent, not just the migrating author |
| Identity across generations | `remap.json` per era, renamed at each fork | Header lineage, immutable (L17); each future hop (7→8) cites its immediate prior pair — chain-of-custody without client files |

**Stays client-side (explicitly):**

- Export + transport, and the codec transform (re-pointing hive/group
  authority hashes to new-generation equivalents — the zome cannot decrypt or
  re-map semantic references).
- Hive/group bootstrap ordering.
- `mark_migrated_v2` old-cell forward pointers — UNCHANGED signatures; still
  needed for `unmigratedSourceHives` offer filtering + generation retirement.
  Lineage is the backward claim, the marker is the forward pointer; both live.
- Retirement checks.

**Named deletions when adopting** (see §11): the `flows/import.rs` dedup +
archival block, the `RemapEntry` consumer paths.
`RemapFile.entries` stops being a correctness ledger (keep `failures` as
diagnostics or drop the file). The `getMigrationMarkerV2Pass4`/`...Pass5`
per-hive N+1 stays but can batch later. The module shrinks around its
identity/dedup core; export/transform/orchestration remain.

## 5. Waves 1–3 coordinator surface (verbatim wire shapes)

Field names below are the Rust wire shape; mirror them in TS.

### 5.1 Cross-generation lineage

`CreateEncryptedContentInput` gains an optional lineage claim (raw creates
leave it `None`; the dedicated extern sets it):

```rust
pub struct CreateEncryptedContentInput {
    pub id: String,
    pub display_hive_id: String,
    pub content_type: String,
    pub revision_author_signing_public_key: String,
    pub bytes: SerializedBytes,
    pub acl_spec: AclSpec,
    pub public_key_acl: Acl,
    pub dynamic_links: Option<Vec<String>>,
    /// Pass-7: optional cross-generation provenance.
    #[serde(default)]
    pub lineage: Option<ContentLineage>,
}

/// Both hashes are base64url holohash STRINGS, not hash objects.
pub struct ContentLineage {
    pub prior_dna_hash_b64: String,
    pub prior_action_hash_b64: String,
}
```

`EncryptedContentHeader` carries the same `#[serde(default)] lineage` field —
shape-validated (both hashes parse, DNA is not this generation); the
coordinator probe and a reverse-lookup `Lineage` link enforce authorship and
discovery.

**The migration writer** (NOT cap-granted — mutator):

```rust
pub struct CreateWithLineageInput {
    pub create: CreateEncryptedContentInput,
    pub lineage: ContentLineage,
    pub prior_cell: Option<CellId>,
}

#[hdk_extern]
pub fn create_encrypted_content_with_lineage(
    input: CreateWithLineageInput,
) -> ExternResult<UpsertContentResponse>

pub struct UpsertContentResponse {
    pub response: EncryptedContentResponse,
    pub was_created: bool,
    pub was_updated: bool,
}
```

Semantics:

- **Find-wins:** a prior caller-authored claim for the same
  `(prior_dna, prior_action)` pair returns the canonical existing record
  (lowest-b64 action-hash pick) with `was_created: false` and no new write.
  Keyed per `(prior pair, author)` — two agents can each claim descent.
- **`prior_cell: Some(cell)`** = bridge-probe authorship against the prior
  generation: L18 (prior record did not resolve), L19 (not authored by the
  caller), L20 (cell unreachable) are hard errors — no unprobed downgrade.
- **`prior_cell: None`** = the claim is stored with NO verification status.
  Lineage-present ≠ probe-ran: a stamped "verified" flag would be forgeable
  through a raw create, so provenance-sensitive readers must re-derive
  through their own prior cell.

**The forward lookup** (cap-granted public DHT-link reader):

```rust
pub struct ResolveByPriorInput {
    pub prior_dna_hash_b64: String,
    pub prior_action_hash_b64: String,
}

#[hdk_extern]
pub fn resolve_by_prior_generation(
    input: ResolveByPriorInput,
) -> ExternResult<Vec<EncryptedContentResponse>>
```

Resolves the prior-generation pair forward into every content in THIS
generation claiming descent from it. Tolerant — unresolvable targets are
dropped.

### 5.2 Update reindex riders (Dynamic labels)

```rust
pub struct UpdateEncryptedContentInput {
    pub previous_encrypted_content_hash: ActionHash,
    pub updated_encrypted_content: EncryptedContent,
    #[serde(default)]
    pub dynamic_links: Option<Vec<String>>,
    #[serde(default)]
    pub remove_dynamic_links: Option<Vec<String>>,
}
```

- `dynamic_links: Some(v)` relinks each label in `v` to the update action on
  the `[hive, content_type, label]` path and retargets the caller's own older
  links on those same paths; `None` leaves Dynamic links untouched.
- `remove_dynamic_links: Some(w)` deletes the caller's own Dynamic links on
  each label path in `w` regardless of target. Applied AFTER `dynamic_links`,
  so a label in both nets to removal.
- ACL links auto-converge on a `group_acl` change; the reindex is a no-op for
  headers without a hive context.

### 5.3 Liveness rider (B10)

`EncryptedContentResponse` (the shape every content read returns):

```rust
pub struct EncryptedContentResponse {
    pub encrypted_content: EncryptedContent,
    pub hash: String,
    pub original_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_action_micros: Option<i64>,
    /// Pass-7 B10 liveness probe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tombstoned: Option<bool>,
}
```

`include_liveness: bool` (`#[serde(default)]`) rides the inputs of SEVEN read
externs: `list_by_dynamic_link`, `list_by_hive_link`, `list_by_acl_link`,
`list_by_author`, `list_by_hive_link_page`, `list_by_dynamic_link_page`,
`list_by_author_page` (the Wave-4 batch externs and the local page twin carry
it too, per item — §6).

| `tombstoned` | Meaning |
|---|---|
| `Some(false)` | Root action probed and live |
| `Some(true)` | Root action has deletes (dead root — even when a byte-identical live sibling still resolves the entry) |
| absent | Not probed (`include_liveness: false` / older coordinator) OR probe-unknown |

Probed per root ACTION, not entry — byte-identical duplicate roots sharing
one entry are distinguished (the B10 bug: entry-level liveness cannot tell a
dead root from a live sibling — humm-tauri's measured 176-phantom-hits/hr
provider-watch case). Tolerant: probe failure yields absent, never a dropped
row or a failed read. Opt-in cost: +1 `get_details` per resolved record.

### 5.4 Durable discovery (M8 — behavior-only reroute, signatures unchanged)

The four hive membership readers (`get_latest_membership`,
`get_latest_membership_local`, `list_my_hives`, `list_my_hives_local`) and
the `list_my_groups` granted-half now walk durable, author-bound,
author-only-deletable indexes (`HiveMembershipIndex`,
`AgentToGroupMemberships`) instead of the sweepable Inbox:

- Hive/group discovery SURVIVES a full Inbox/DM sweep (sweettest-proven).
- Writers additionally publish the index links (grant paths unchanged on the
  wire); Inbox `HiveInvite`/`GroupInvite` writes stay as transient
  notifications.
- Founded-GROUP discovery stays self-Inbox — accepted residual: the shipped
  humm-tauri sweep consumes only `DmCreate`, and a founder re-derives founded
  groups from their own source chain.
- Dedup contract (pre-existing, unchanged): each `create_group_membership`
  writes a fresh index link, so the same `group_genesis_hash` may appear
  multiple times in `list_my_groups` output (one entry per issuance) —
  deduplicate on `group_genesis_hash` and pair with
  `get_latest_group_membership` for the current role.

### 5.5 Idempotent delete + paged inbox (M10)

**Delete** (NOT cap-granted — mutator; breaking response shape, §2 B-1):

```rust
pub struct DeleteContentResponse {
    pub was_deleted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete_action_hash: Option<ActionHash>,
}

#[hdk_extern]
pub fn delete_encrypted_content(
    original_encrypted_content_hash: ActionHash,
) -> ExternResult<DeleteContentResponse>
```

- Idempotent tombstone: deleting an already-deleted or absent target is a
  no-op success (`was_deleted: false`) — gated on exactly the two wire-stable
  absent literals `no Record found at given hash` /
  `Could not find the EncryptedContent`; any other error still propagates.
- `was_deleted: false` is deliberately ambiguous: a network get cannot
  distinguish tombstoned from not-yet-propagated, so it means "goal met OR
  target currently unresolvable from this node". Callers deleting content
  they did not author SHOULD re-probe later rather than treat it as terminal.
- A REAL deletion authors the Delete, emits the Delete signal, and retracts
  the caller's own discovery links targeting the original
  (`EncryptedContentUpdates` links are never matched and stay immortal by
  design). The no-op path does NONE of these.

**Paged inbox probe** (cap-granted; legacy `probe_inbox` wire-unchanged):

```rust
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
```

Cursor contract (shared with the content `*_page` externs — one page engine,
byte-identical literals):

- **Sort key:** `(timestamp, create_link_hash)`; raw-byte hash order is THE
  deterministic tie-break. Replay `SourcePosition.action_hash` VERBATIM as
  the next request's `source_after_action_hash` — never re-order or compare
  client-side (b64 lexicographic order differs from raw-byte order).
- **`since_ts` alone** = inclusive legacy-watermark semantics (boundary
  duplicates possible; dedupe by action hash). **The composite cursor**
  (`since_ts` + `source_after_action_hash`) = strictly exclusive — no dupes,
  no skips at equal timestamps.
- **Limits:** `None` → 100, `Some(0)` → Guest error `limit must be >= 1`,
  oversized → clamped to 256.
- **Pairing:** a lone cursor hash rejects with
  `source_after_action_hash requires since_ts`; a malformed one with
  `source_after_action_hash is not a valid ActionHash`.
- **Poison rows:** `source_positions` are SOURCE truth — one per selected
  link even when its target is malformed (`items` may be shorter than
  `source_count`); cursor past poison rows instead of wedging.

### 5.6 Role-key closure (M11)

```rust
pub struct RoleKeyClosureInput {
    pub hive_genesis_hash: ActionHash,
    pub granted_role: HiveRole,
}

pub struct RoleClosureEntry {
    pub role: HiveRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_genesis_hash: Option<ActionHash>,
}

pub struct RoleKeyClosure {
    pub entries: Vec<RoleClosureEntry>,
}

#[hdk_extern]
pub fn role_key_closure(input: RoleKeyClosureInput) -> ExternResult<RoleKeyClosure>
```

- Dominance order Owner⊇Admin⊇Writer⊇Reader, returned highest→lowest.
- Each dominated role pairs with the hive's canonical system-role
  `GroupGenesis` action hash; cross-agent duplicates resolve to the lowest
  b64 action-hash STRING; `None` = no system-role group for that role visible
  from this node yet (eventually consistent).
- IDENTITIES only — no key material; the client holds one INDEPENDENT
  SharedSecret per returned genesis; no role's K is ever derived from
  another's.

## 6. Wave-4 batch + local read externs

Every batch/local extern is read-only, cap-granted beside its existing
singleton twin, and additive. Every response bucket keys back to its request
item in request order, so **missing ≠ misaligned**.

### 6.1 Batch content reads

**`list_encrypted_content_by_dynamic_links`**
- **Input** `{ hive_genesis_hash: ActionHash, content_type: String, dynamic_links: Vec<String>, since_ts?: Timestamp, limit?: usize, include_liveness: bool }`
- **Output** `Vec<{ dynamic_link: String, records: Vec<EncryptedContentResponse>, truncated: bool }>` (one bucket per requested label, request order).
- **Bounds** ≤ 64 labels (`dynamic_links batch accepts at most 64 labels`);
  each label is a bounded first page (`limit`, default 100, hard 256) with
  its own `truncated`; summed per-label limits ≤ 4096
  (`batch total requested records exceed the 4096 budget`).
- **Replaces** the per-blob `list_by_dynamic_link` loop in media-availability
  refresh (`mediaAvailabilityRefreshQueue.ts` → `availability.ts`) and the
  per-uncached-group SS-candidate fetches in `decryptPipeline.ts`. blake3s
  ARE the dynamic labels — pass the blake3 set as `dynamic_links`.

**`list_by_hive_links_many`**
- **Input** `{ hive_genesis_hash: ActionHash, requests: Vec<{ content_type: String, since_ts?: Timestamp, limit?: usize, include_liveness: bool }> }`
- **Output** `Vec<{ content_type: String, records: Vec<EncryptedContentResponse>, truncated: bool }>` (request order).
- **Bounds** ≤ 32 requests (`hive-link batch accepts at most 32 requests`);
  each a first page; budget ≤ 4096. Deep pagination stays on the singleton
  `list_by_hive_link_page`.
- **Replaces** the per-addon-type `list_by_hive_link` fan-out in the feed
  (`Feed/index.tsx`).

**`get_many_by_content_id_link`**
- **Input** `Vec<{ hive_genesis_hash: ActionHash, content_id: String }>`
- **Output** `Vec<{ hive_genesis_hash: ActionHash, content_id: String, record: EncryptedContentResponse | null }>` (request order; the `record` key is always present, `null` when unresolved — row never dropped).
- **Bounds** ≤ 64 lookups (`content-id batch accepts at most 64 lookups`).
  Mirrors the singleton `get_by_content_id_link` first-target selection
  EXACTLY.
- **Replaces** the serial per-hive resolve in `HiveApi.list()`
  (`hive/index.ts`).

**`list_by_author_many`**
- **Input** `Vec<{ author: AgentPubKey, content_type: String, limit?: usize }>`
- **Output** `Vec<{ author: AgentPubKey, records: Vec<EncryptedContentResponse>, truncated: bool }>` (request order).
- **Bounds** ≤ 64 lookups (`author batch accepts at most 64 lookups`); first
  page per lookup, oldest-first; budget ≤ 4096.
- **Replaces** the up-to-31 sequential member + author scans in group-DM
  first contact (`sidecarSharedSecret.ts`). Keep the client-side
  member-over-inline precedence + X25519 validation.

**`content_id_exists`**
- **Input** `{ hive_genesis_hash: ActionHash, content_id: String }` → **Output** `bool`.
- Resolves ZERO records — link-set non-emptiness only.
- **Replaces** `checkEntryExists()` fetching a full ciphertext record for
  `Boolean(record)` (`hummContentReads.ts`).

### 6.2 Membership / group / local reads

**`get_latest_memberships_local_many`** (LOCAL, self-scoped)
- **Input** `{ hive_genesis_hashes: Vec<ActionHash> }` → **Output** `Vec<{ hive_genesis_hash: ActionHash, membership: HiveMembershipResponse | null }>` (request order; the `membership` key is always present, `null` when the caller has no membership in that hive).
- Agent derived from `agent_info()` ONLY — never an arbitrary-agent
  parameter. Newest-unexpired membership per hive, selected by the same
  policy as `get_latest_membership_local`.
- **Bounds** ≤ 64 hives (`membership batch accepts at most 64 hives`).
- **Replaces** the per-hive `get_latest_membership_local` loop in boot
  reconciliation (`HiveGenesisRegistry.ts`).

**`list_group_members_many`**
- **Input** `Vec<ActionHash>` (group genesis hashes) → **Output** `Vec<{ group_genesis_hash: ActionHash, members: Vec<GroupMembershipResponse> }>` (request order).
- **Complete rosters** — ACL derivation needs every member, so this never
  truncates. **Bounds** ≤ 64 groups
  (`group-members batch accepts at most 64 groups`) AND an aggregate
  roster-link budget of 4096
  (`group-members batch roster links exceed the 4096 budget`). A batch
  REJECTED on the budget falls back to the singleton `list_group_members`
  per group.
- **Replaces** the serial per-group roster fetch in
  `deriveHiveGroupPublicKeyAcl.ts`.

**`list_my_groups_local`** (LOCAL twin of `list_my_groups`)
- **Input** `()` → **Output** `Vec<ListedGroup>` (identical shape to
  `list_my_groups`; founded rows role `null`, granted rows role set; expired
  grants filtered).
- **Replaces** the ≤9 network `list_my_groups` polls per hive-boot in
  role-group + device-set bootstrap (`bootstrapRoleGroups.ts`,
  `deviceSet/bootstrap.ts`).

**`list_by_hive_link_local_page`** (LOCAL twin of `list_by_hive_link_page`)
- **Input/Output** identical to `list_by_hive_link_page`.
- **Replaces** the sleep + network-page-retry loop for self-authored records
  in stranded-group recovery (`setupNewHive.ts`).

### 6.3 Caps & budgets (reference)

| Extern | item cap | per-item page | aggregate budget |
|---|---|---|---|
| `list_encrypted_content_by_dynamic_links` | 64 labels | limit (def 100, max 256) | 4096 |
| `list_by_hive_links_many` | 32 requests | limit (def 100, max 256) | 4096 |
| `list_by_author_many` | 64 lookups | limit (def 100, max 256) | 4096 |
| `get_many_by_content_id_link` | 64 lookups | 1 record/lookup | n/a |
| `get_latest_memberships_local_many` | 64 hives | 1 membership/hive | n/a |
| `list_group_members_many` | 64 groups | full roster | 4096 roster links |

Note: at the default per-item limit (100), the 4096 budget binds before the
64-item caps (≈40 items fit); the 32-request hive-links cap is reachable
(32×100 = 3200). Pass a smaller per-item `limit` to use full item width.

## 7. Signals — MIGRATION REQUIRED (M21)

The cross-host remote-signal channel changed. **A client that ingests the OLD
full payload from remote signals must migrate.**

- **Local (author's own conductor):** still emits the full
  `EncryptedContentSignal` `{ action_type, data: EncryptedContentResponse,
  from_agent }` on create/update/delete.
- **Remote (cross-host, to `public_key_acl.reader` minus self):** the fan-out
  now sends `EncryptedContentHint`
  `{ action_type, hash: String, original_hash: String, from_agent?: AgentPubKey }`
  — no `data`, no ciphertext. The recipient re-queries
  (`get_encrypted_content` by `hash`/`original_hash`) and `get`-verifies.
- **Owner handoff:** `initiate_owner_handoff` also best-effort sends an
  `OwnerHandoffOfferHint`
  `{ offer_hash: ActionHash, hive_genesis_hash: ActionHash, from_agent?: AgentPubKey }`
  to the recipient (warn-never-block: a failed hint send never blocks the
  committed offer). Governance UI reacts without polling
  `list_pending_owner_handoffs`; keep one list-on-mount as durable recovery.
- **Provenance:** `recv_remote_signal` overwrites
  `from_agent = call_info().provenance` (the conductor-attested caller) on
  EVERY delivery, discarding any sender-supplied value.
- **Decode order:** `recv_remote_signal` try-decodes FIVE families in this
  order: `EncryptedContentSignal` → `DmRemoteSignal` → `BlobPinSignal` →
  `EncryptedContentHint` → `OwnerHandoffOfferHint`. An unknown payload is an
  explicit Guest ERROR (the audit trail for garbage from open-cap peers):
  `recv_remote_signal: payload did not decode as EncryptedContentSignal, EncryptedContentHint, DmRemoteSignal, BlobPinSignal, or OwnerHandoffOfferHint`
  — replacing the pass-6 3-family text (§2 B-2).
- **TRUST MODEL (load-bearing):** `recv_remote_signal` is UNRESTRICTED and
  still decodes a legacy full `EncryptedContentSignal` FIRST, so a hostile
  peer CAN deliver a remote body carrying attacker-controlled `data` plus a
  stamped `from_agent: Some(...)`. Therefore any signal with
  `from_agent: Some(...)` is a REMOTE, UNTRUSTED FETCH TRIGGER — ignore/cache
  none of its embedded `data`, and resolve + verify the content by `hash`.
  ONLY `from_agent: None` (the author's LOCAL self-emit) identifies a trusted
  full payload. (A future hardening MAY drop the legacy remote full-signal
  arm entirely, since the fan-out is hint-only.)

**Client migration checklist:**
1. Retire the signal-embedded-bytes cache path
   (`sharedSecretSignalIngest.ts`, `dmIngest.ts`/`dmPersistence.ts`
   provisional-plaintext persist). Ingest the hint → fetch → validate.
2. Add the `EncryptedContentHint` + `OwnerHandoffOfferHint` shapes to the TS
   signal union.
3. Treat every remote-delivered signal (`from_agent: Some`) as an UNTRUSTED
   fetch trigger: never trust or cache its embedded `data`; resolve + verify
   by `hash`. Trust a full payload only when `from_agent: None` (local
   self-emit). Do not assume a remote signal omits `data` — a hostile peer
   can still send the legacy full shape.
4. **Fix the signal dispatcher discrimination** (`zomeSignals.ts`):
   `isEncryptedContentSignal` currently matches on `action_type` ALONE, which
   the hint ALSO carries — so `processSignal` (which tests it first) would
   misroute every `EncryptedContentHint` to the full-signal handler that
   dereferences `data`. Make the guards DISJOINT: the full signal requires
   `data`; the hint requires `hash` + `original_hash` and NO `data`;
   `OwnerHandoffOfferHint` requires `offer_hash`. Route hints to a fetch
   handler; after fetch reuse
   `isSignalFromAgentAttested(from_agent, <fetched revision author>)` for the
   trust check. (The three hint/full shapes are structurally disjoint by
   construction: the hint lacks `data`, the full signal lacks
   `hash`/`original_hash`, the offer hint lacks `action_type`.)

## 8. Cap-grant table

**12 grants ADDED vs pass-6** (all read-only; granted Unrestricted beside
their singleton twins in `set_cap_tokens`):

| Extern | Wave |
|---|---|
| `resolve_by_prior_generation` | 1 (M4) |
| `probe_inbox_page` | 2 (M10) |
| `role_key_closure` | 2 (M11) |
| `list_encrypted_content_by_dynamic_links` | 4 (M19) |
| `list_by_hive_links_many` | 4 (M19) |
| `get_many_by_content_id_link` | 4 (M19) |
| `list_by_author_many` | 4 (M19) |
| `content_id_exists` | 4 (M19) |
| `get_latest_memberships_local_many` | 4 (M20) |
| `list_group_members_many` | 4 (M20) |
| `list_my_groups_local` | 4 (M20) |
| `list_by_hive_link_local_page` | 4 (M20) |

**NOT granted (mutators — the client calls them on its own cells only):**
`create_encrypted_content_with_lineage`, `mark_migrated`,
`mark_migrated_v2`, `mark_hive_migrated`, `delete_encrypted_content`,
`initiate_owner_handoff`.

Full granted list: `set_cap_tokens` in the coordinator `lib.rs` (44 entries
at this tip) is the source of truth.

## 9. Client-adoption map (their file → old pattern → new extern)

| humm-tauri seam | Old pattern | New surface |
|---|---|---|
| `mediaAvailabilityRefreshQueue.ts` → `availability.ts` | per-blob `list_by_dynamic_link` loop | `list_encrypted_content_by_dynamic_links` |
| `sharedSecretCrud.ts:441-450` | per-group SS-candidate fetch | `list_encrypted_content_by_dynamic_links` |
| `rescueStrandedGroups.ts` | per-group fetch loop | `list_encrypted_content_by_dynamic_links` |
| `Feed/index.tsx` | per-addon-type `list_by_hive_link` fan-out | `list_by_hive_links_many` |
| `HiveApi.list()` (`hive/index.ts`) | serial per-hive resolves | `get_many_by_content_id_link` |
| `hummContentReads.ts` `checkEntryExists` | full-record fetch for `Boolean(record)` | `content_id_exists` |
| `sidecarSharedSecret.ts` | ≤31 sequential member/author scans | `list_by_author_many` |
| `HiveGenesisRegistry.ts` | per-hive `get_latest_membership_local` loop | `get_latest_memberships_local_many` |
| `deriveHiveGroupPublicKeyAcl.ts` | serial per-group rosters | `list_group_members_many` |
| `bootstrapRoleGroups.ts` / `deviceSet/bootstrap.ts` | ≤9 network `list_my_groups` polls per hive-boot | `list_my_groups_local` |
| `setupNewHive.ts` | sleep + network-page retry for own records | `list_by_hive_link_local_page` |
| `dmSweep.ts:269-287` | per-item consume + drain | `probe_inbox_page` + shipped `get_many_encrypted_content` |
| client role-K fan-out | per-role group resolution | `role_key_closure` |
| `ownerHandoff.ts` | polling `list_pending_owner_handoffs` | `OwnerHandoffOfferHint` + one list-on-mount |

Zero-DNA client hygiene enabled independently of pass-7 (already-shipped
surface): adopt `get_many_encrypted_content` for the DM inbox drain (bypassed
today); trust the typed `list_groups_in_hive` response in
`RoleGroupAnchorResolver` (drop the redundant `getGroupGenesis` re-fetch);
adopt `content_summary_many` (shipped, zero callers today); sidecar manifest
O(N²) re-listing + directory roster per-row decode are client orchestration
cleanups (no new extern); clear `SharedSecretCache` AND the decrypt FIFO on
keyring lock (both currently unbounded / survive the lock — a
decrypted-material lifetime leak); companion pin-state IPC batch for the
media-availability path (Tauri IPC side; pairs with
`list_encrypted_content_by_dynamic_links`).

## 10. BDD acceptance (given / when / then)

Tagging: `[coordinator]` = enforced by this zome (conductor-proven here);
`[humm-tauri]` = your side's obligation.

### 10.1 Lineage + migration identity

- `[coordinator]` **Given** pass-6 content authored by agent A and both cells
  on A's conductor, **when** A calls `create_encrypted_content_with_lineage`
  with the (pass-6 DNA, original action) pair and
  `prior_cell: Some(pass-6 cell)`, **then** the create succeeds with
  `was_created: true` and the header carries the immutable lineage claim.
- `[coordinator]` **Given** the same claim already exists (crash-resume),
  **when** the call re-runs, **then** it returns the canonical existing
  record with `was_created: false` and writes nothing (find-wins).
- `[coordinator]` **Given** a bogus prior action hash, a prior record
  authored by someone else, or an unreachable prior cell, **when** the probe
  runs (`prior_cell: Some`), **then** it rejects with exactly L18 / L19 / L20
  respectively — never an unprobed downgrade.
- `[coordinator]` **Given** `prior_cell: None`, **when** the create commits,
  **then** the claim is stored with no verification status; readers needing
  provenance re-derive through their own prior cell.
- `[coordinator]` **Given** content in this generation claiming descent from
  a prior pair, **when** any agent calls `resolve_by_prior_generation` with
  that pair, **then** every descendant returns (tolerant: unresolvable
  targets dropped, never an error).
- `[coordinator]` **Given** a lineage citing THIS generation's DNA, **when**
  the create validates, **then** it rejects with L16; malformed b64 pairs
  reject with L14/L15.
- `[coordinator]` **Given** a header with lineage set, **when** an update
  changes or removes it, **then** it rejects with L17; a None→Some stamp is
  allowed exactly once.

### 10.2 Integrity bounds + continuity

- `[coordinator]` **Given** a create violating any bound (id length,
  content_type length, display_hive_id length, ACL owner/keys/bucket sizes,
  duplicate keys, Public/OpenWrite payload > 1,000,000 bytes), **when**
  validation runs, **then** it rejects with the exact table literal
  (L1–L7, L12) — table-driven over-limit creates prove each one.
- `[coordinator]` **Given** an update changing `id`, hive context, or
  `acl_spec` variant, **when** validation runs, **then** it rejects with
  L8/L9/L10; a `content_type` change is admitted ONLY as the one-time
  `_migrated/` prefix stamp (L11).
- `[coordinator]` **Given** a founder minting a second GroupGenesis for the
  same hive + hive-wide role on one chain, **when** validation runs, **then**
  it rejects with L13 — and `find_or_create_group_genesis` absorbs ONLY L13
  (find-wins by `(hive, role)`); an L22 display_id reuse propagates as a hard
  error; L21 bounds the system-role display_id.
- `[coordinator]` **Given** a HiveGroup whose `group_acl` lists one group in
  two buckets, **when** validation runs, **then** it rejects with L23.

### 10.3 Durable discovery

- `[coordinator]` **Given** an agent with hive + group memberships, **when**
  every Inbox link is swept/consumed, **then** `list_my_hives`,
  `get_latest_membership*`, and the granted-half of `list_my_groups` still
  return the memberships (durable index, not Inbox).
- `[humm-tauri]` **Given** repeated role grants for one group, **when**
  `list_my_groups` returns multiple rows for one `group_genesis_hash`,
  **then** the client dedupes by genesis and resolves the current role via
  `get_latest_group_membership`.

### 10.4 Idempotent delete

- `[coordinator]` **Given** live content, **when** its author deletes it,
  **then** `was_deleted: true` + the delete action hash, a Delete signal is
  emitted, and the author's own discovery links to it are retracted.
- `[coordinator]` **Given** the same hash deleted again, **when**
  `delete_encrypted_content` re-runs, **then** it returns
  `was_deleted: false` with NO error, NO signal, and NO link writes (double
  delete is an idempotent no-op).
- `[humm-tauri]` **Given** `was_deleted: false` on content the caller did not
  author, **when** remediation interprets it, **then** it re-probes later
  (the flag means "already gone OR not resolvable from here", not failure).

### 10.5 Inbox paging

- `[coordinator]` **Given** more inbox links than `limit`, **when**
  `probe_inbox_page` pages with the replayed `SourcePosition.action_hash` +
  `since_ts`, **then** consecutive pages neither duplicate nor skip items at
  equal timestamps (composite cursor strictly exclusive; `since_ts` alone
  inclusive).
- `[coordinator]` **Given** `limit: 0`, a lone cursor hash, or a malformed
  cursor hash, **when** called, **then** it rejects with exactly
  `limit must be >= 1` / `source_after_action_hash requires since_ts` /
  `source_after_action_hash is not a valid ActionHash`.
- `[coordinator]` **Given** a malformed link target among the selected page,
  **when** the page returns, **then** `source_positions` still carries that
  row (`source_count` > `items.len()`) so the caller cursors past it.

### 10.6 Role closure

- `[coordinator]` **Given** a hive with system-role groups, **when**
  `role_key_closure(hive, Admin)` runs, **then** entries are
  [Admin, Writer, Reader] in that order, each with the canonical (lowest-b64)
  genesis, and a missing role's entry carries `group_genesis_hash: None`.

### 10.7 Liveness rider

- `[coordinator]` **Given** `include_liveness: false` (or omitted), **when**
  any list/page read returns, **then** `tombstoned` is absent on every row
  (byte-identical pre-B10 behavior).
- `[coordinator]` **Given** `include_liveness: true`, **when** the read
  returns, **then** a live root carries `Some(false)` and an
  ordinarily-deleted root drops from the listing; a dead duplicate root that
  still resolves through a byte-identical live sibling carries `Some(true)`
  (production multi-node case; single-conductor tests prove the deterministic
  live/absent contract).

### 10.8 Reindex riders

- `[coordinator]` **Given** content discoverable under label X, **when** an
  update passes `dynamic_links: ["X"]`, **then** the caller's old X-link is
  retargeted to the update action; **when** it passes
  `remove_dynamic_links: ["X"]`, **then** the caller's X-links are deleted;
  a label in BOTH nets to removal; content without a hive context is a
  no-op.

### 10.9 Batch reads + local twins (Wave-4)

Batch ordering + alignment:
- `[coordinator]` **Given** a batch request of N items, **when** the extern
  returns, **then** there are exactly N buckets in request order, one per
  item (duplicates included), and an unresolved item is `null`/empty — never
  dropped or reordered.

Bounds (per extern reject literal in §6):
- `[coordinator]` **Given** a request exceeding an item cap, **when** called,
  **then** it rejects with the exact literal (e.g.
  `author batch accepts at most 64 lookups`).
- `[coordinator]` **Given** a page-based batch whose summed per-item limits
  exceed 4096, **when** called, **then** it rejects with
  `batch total requested records exceed the 4096 budget`.
- `[coordinator]` **Given** a `list_group_members_many` batch whose total
  roster links exceed 4096, **when** called, **then** it rejects with
  `group-members batch roster links exceed the 4096 budget` (fall back to the
  singleton per group); rosters are otherwise COMPLETE.

Page-bounded first page:
- `[coordinator]` **Given** a dynamic label with 3 records and `limit: 2`,
  **when** called, **then** the bucket has 2 records and `truncated: true`;
  with `limit: 5`, 3 records and `truncated: false`.

Local twins:
- `[coordinator]` **Given** self-authored content on a peerless cell,
  **when** `list_by_hive_link_local_page` is called, **then** it returns the
  records from the local store (no network) matching the network twin on
  integrated data.
- `[coordinator]` **Given** the caller is a member of hive A (newest grant
  Writer, superseding an earlier Reader) and not a member of hive B, **when**
  `get_latest_memberships_local_many([A,B,A])` is called, **then** A resolves
  to the Writer grant (both occurrences), B is `null`, and each bucket equals
  the singleton `get_latest_membership_local` for that hive.

Membership/roster correctness (close-but-wrong):
- `[coordinator]` **Given** two grants (Reader then Writer) for one group to
  one agent, **when** `list_my_groups_local` / `list_group_members_many`
  runs, **then** the newest grant wins (roster shows one row for that agent),
  and an EXPIRED grant is filtered out.

### 10.10 Signal hardening

- `[coordinator]` **Given** an author creates HiveGroup content with a remote
  reader, **when** the reader's conductor receives the signal, **then** it
  decodes as `EncryptedContentHint` with NO `data`/ciphertext and
  `from_agent` == the author (conductor-stamped); the author's own LOCAL
  signal still carries the full payload.
- `[coordinator]` **Given** a peer forges a hint with a false `from_agent`,
  **when** `recv_remote_signal` processes it, **then** `from_agent` is
  overwritten with the real caller provenance — the forged value never
  survives.
- `[humm-tauri]` **Given** the client dispatcher receives an
  `EncryptedContentHint` (carries `action_type` + `hash` + `original_hash`,
  no `data`), **when** `processSignal` classifies it, **then** the disjoint
  guards route it to the FETCH handler (never the full-signal `data`
  handler), and after fetch
  `isSignalFromAgentAttested(from_agent, fetched revision author)` gates
  trust.
- `[humm-tauri]` **Given** a hostile peer delivers a REMOTE full
  `EncryptedContentSignal` with attacker-controlled `data` and a claimed
  author, **when** the client's signal handler sees it (`from_agent: Some`,
  stamped by recv to the real sender), **then** the client ignores the
  embedded `data`, resolves the content by `hash`, and validates — the forged
  bytes never enter any cache or trust path.
- `[coordinator]` **Given** `initiate_owner_handoff` to a recipient, **when**
  it commits, **then** the recipient receives an `OwnerHandoffOfferHint` with
  the offer + hive hashes and stamped `from_agent`.

### 10.11 Client hygiene

- `[humm-tauri]` **Given** the keyring is locked, **when** the lock event
  fires, **then** the `SharedSecretCache` and the decrypt FIFO are cleared —
  no decrypted key material or plaintext survives the lock.

### 10.12 Migration end-to-end

- `[humm-tauri]` **Given** a pass-6 hive with content and both cells
  installed, **when** the importer runs TWICE with lineage + probe
  (`prior_cell: Some`), **then** the second run is a find-wins no-op
  (`was_created: false` per entry, no duplicates),
  `resolve_by_prior_generation` maps every old pair to its new record, and
  `mark_migrated_v2` still writes the old-cell forward pointer.

## 11. Migration runbook delta (their code, concrete)

What changes in `~/humm-tauri` when adopting §4:

1. **`src-tauri/src/migration/flows/import.rs`** — the per-entry create loop
   (~:54-91) switches from `create_encrypted_content` to
   `create_encrypted_content_with_lineage` (lineage = pass-6 pair,
   `prior_cell: Some(pass-6 cell)`); DELETE the `already_imported` dedup +
   per-era remap archival block (~:182-223). Idempotency is the zome's
   find-wins; re-runs are safe by construction.
2. **`RemapEntry` consumer paths** — replace remap-file lookups with
   `resolve_by_prior_generation` where old→new mapping is needed;
   `RemapFile.entries` stops being a correctness ledger (keep `failures` as
   diagnostics or drop the file + its per-era rename rules).
3. **`MigrationStore.ts`** — orchestration order unchanged
   (`migrate_hive` → `grant_memberships` → export → import →
   `mark_hive_migrated`); the import step's retry/resume logic simplifies to
   "re-run the call" (`was_created: false` = already done).
4. **UNCHANGED:** `mark_migrated_v2` / `mark_hive_migrated` signatures and
   the `unmigratedSourceHives` offer filtering; export/transport; the codec
   transform re-pointing hive/group authority hashes; bootstrap ordering;
   retirement checks. `getMigrationMarkerV2Pass4`/`...Pass5` per-hive N+1
   stays (batchable later).
5. **Importer must satisfy §3 bounds** — pre-validate L1–L7/L12 client-side
   for legacy records that might exceed them (e.g. oversized ACL buckets);
   a violating record needs a client-side decision (trim/split/skip+report),
   not a blind write.

## 12. Testing posture + install

**Conductor-proven here (sweettest, in-process hc 0.6 against the branch
DNA): 84 passed + 1 ignored across 16 binaries** — including `batch_reads`
(19), `signal_hints` (3), `lineage_cross_generation` (4, incl. the
two-generation conductor proof riding a vendored pass-6 DNA fixture),
`inbox_and_delete` (4), `role_key_closure` (2), `liveness_and_reindex` (3),
`pinned_hosts` (9, page-engine regression), `group_uniqueness` (9),
`idempotent_writes` (7), `service_records` (9), plus host-side unit suites
(integrity + coordinator) and clippy `-D warnings` / fmt clean. The one
ignored test is a network-dormancy differential that is e2e-only by design
(covered by humm-tauri's tryorama).

**Install-time constants bump (THEIR repo, at blessing):**

| Constant | Today | Pass-7 |
|---|---|---|
| `CURRENT_HAPP_LABEL` | `pass-6-service-meter` | pass-7 label (TBD at blessing) |
| `CURRENT_HAPP_SHA256` | `b98916f18def33731a93b70c36f60838186a52e4e41efcd58de5071f150430c8` | TBD at blessing |
| `CURRENT_HAPP_DNA_HASH_BASE64` | `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz` | TBD at blessing (`PASS_6_DNA_HASH_BASE64` stays — lineage markers must not move) |
| `HUMM_EARTH_CORE_HAPP_ID` | `humm-earth-core-happ@6` | `humm-earth-core-happ@7` |
| `COORDINATOR_WASM_VERSION` | 11 | 12 (monotonic stamp) |

MANIFEST/registry row appended LAST, after the artifact exists and its
sha256 is verified (house rule; official store + both `.testdata` mirrors).
