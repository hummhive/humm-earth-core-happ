# humm-tauri ⇄ earth-core — pass-7 Wave-4 integration handoff (DRAFT, blessing-gated)

> **STATUS: BRANCH-ONLY DRAFT on `feat-integrity-pass-7`. NOT distributed. Do NOT
> hand to humm-tauri until the pass-7 blessing.** These coordinator externs + signal
> shapes are built and gated on the scratch branch (DNA held after the single M16
> integrity move); they ship to humm-tauri only when pass-7 is blessed. This doc is the
> stub of the eventual `HUMM_TAURI_WAVE4_*_INTEGRATION.md` handoff — deep enough to not
> forget any new capability, with given/when/then for the humm-tauri devs to validate
> correctness. It does NOT re-explain every client touchpoint; it tracks the NEW
> earth-core surface and the "better way" upgrades (most of which came from reading
> humm-tauri's own repeated N+1 patterns).

## 0. What Wave-4 adds (and why)

Three themes, all coordinator-only (hot-swappable; DNA hash unchanged from the M16 pin):

1. **Bounded batch reads** — collapse the many-of-a-kind, per-item zome-call loops that
   dominate humm-tauri's boot/feed/decrypt/ACL paths into one bounded call each.
2. **Local read twins** — resolve the caller's OWN data from the local store (no network
   round-trip / sleep-and-retry) for boot + stranded-content recovery.
3. **Signal leakage fix** — the cross-host content signal no longer carries ciphertext;
   it is a fetch-hint, and provenance is conductor-stamped.

Every batch/local extern is read-only, cap-granted beside its existing singleton twin,
and additive (wire evolves via `#[serde(default)]`). Every response bucket keys back to
its request item in request order, so **missing ≠ misaligned**.

## 1. Batch content read externs (serve the N+1 media/feed/DM/hive load paths)

Field names below are the Rust wire shape; mirror them in TS. `EncryptedContentResponse`
is the existing shape returned by `get_encrypted_content`.

### 1.1 `list_encrypted_content_by_dynamic_links`
- **Input** `{ hive_genesis_hash: ActionHash, content_type: String, dynamic_links: Vec<String>, since_ts?: Timestamp, limit?: usize, include_liveness: bool }`
- **Output** `Vec<{ dynamic_link: String, records: Vec<EncryptedContentResponse>, truncated: bool }>` (one bucket per requested label, request order).
- **Bounds** ≤ 64 labels (`"dynamic_links batch accepts at most 64 labels"`); each label is a **bounded first page** (`limit`, default 100, hard 256) with its own `truncated`; the summed per-label limits must be ≤ 4096 (`"batch total requested records exceed the 4096 budget"`).
- **Replaces** the per-blob `list_by_dynamic_link` loop in media-availability refresh (`mediaAvailabilityRefreshQueue.ts` → `availability.ts`) and the per-uncached-group SS-candidate fetches in `decryptPipeline.ts`. blake3s ARE the dynamic labels — pass the blake3 set as `dynamic_links`.

### 1.2 `list_by_hive_links_many`
- **Input** `{ hive_genesis_hash: ActionHash, requests: Vec<{ content_type: String, since_ts?: Timestamp, limit?: usize, include_liveness: bool }> }`
- **Output** `Vec<{ content_type: String, records: Vec<EncryptedContentResponse>, truncated: bool }>` (request order).
- **Bounds** ≤ 32 requests (`"hive-link batch accepts at most 32 requests"`); each is a first page; budget ≤ 4096. Deep pagination stays on the singleton `list_by_hive_link_page`.
- **Replaces** the per-addon-type `list_by_hive_link` fan-out in the feed (`Feed/index.tsx`).

### 1.3 `get_many_by_content_id_link`
- **Input** `Vec<{ hive_genesis_hash: ActionHash, content_id: String }>`
- **Output** `Vec<{ hive_genesis_hash: ActionHash, content_id: String, record: EncryptedContentResponse | null }>` (request order; the `record` key is always present, `null` when unresolved — row never dropped).
- **Bounds** ≤ 64 lookups (`"content-id batch accepts at most 64 lookups"`). Mirrors the singleton `get_by_content_id_link` first-target selection EXACTLY.
- **Replaces** the serial per-hive resolve in `HiveApi.list()` (`hive/index.ts`).

### 1.4 `list_by_author_many`
- **Input** `Vec<{ author: AgentPubKey, content_type: String, limit?: usize }>`
- **Output** `Vec<{ author: AgentPubKey, records: Vec<EncryptedContentResponse>, truncated: bool }>` (request order).
- **Bounds** ≤ 64 lookups (`"author batch accepts at most 64 lookups"`); first page per lookup, oldest-first; budget ≤ 4096.
- **Replaces** the up-to-31 sequential member + author scans in group-DM first contact (`sidecarSharedSecret.ts`). Keep the client-side member-over-inline precedence + X25519 validation.

### 1.5 `content_id_exists`
- **Input** `{ hive_genesis_hash: ActionHash, content_id: String }` → **Output** `bool`.
- Resolves ZERO records — link-set non-emptiness only.
- **Replaces** `checkEntryExists()` fetching a full ciphertext record for `Boolean(record)` (`hummContentReads.ts`).

## 2. Membership / group / local read externs

### 2.1 `get_latest_memberships_local_many` (LOCAL, self-scoped)
- **Input** `{ hive_genesis_hashes: Vec<ActionHash> }` → **Output** `Vec<{ hive_genesis_hash: ActionHash, membership: HiveMembershipResponse | null }>` (request order; the `membership` key is always present, `null` when the caller has no membership in that hive).
- Agent is derived from `agent_info()` ONLY — never an arbitrary-agent parameter. Newest-unexpired membership per hive, selected by the same policy as `get_latest_membership_local`.
- **Bounds** ≤ 64 hives (`"membership batch accepts at most 64 hives"`).
- **Replaces** the per-hive `get_latest_membership_local` loop in boot reconciliation (`HiveGenesisRegistry.ts`).

### 2.2 `list_group_members_many`
- **Input** `Vec<ActionHash>` (group genesis hashes) → **Output** `Vec<{ group_genesis_hash: ActionHash, members: Vec<GroupMembershipResponse> }>` (request order).
- **Complete rosters** — ACL derivation needs every member, so this never truncates. **Bounds** ≤ 64 groups (`"group-members batch accepts at most 64 groups"`) AND an aggregate roster-link budget of 4096 (`"group-members batch roster links exceed the 4096 budget"`). If a batch is REJECTED on the budget, fall back to the singleton `list_group_members` per group.
- **Replaces** the serial per-group roster fetch in `deriveHiveGroupPublicKeyAcl.ts`.

### 2.3 `list_my_groups_local` (LOCAL twin of `list_my_groups`)
- **Input** `()` → **Output** `Vec<ListedGroup>` (identical shape to `list_my_groups`; founded rows role `null`, granted rows role set; expired grants filtered).
- **Replaces** the ≤9 network `list_my_groups` polls per hive-boot in role-group + device-set bootstrap (`bootstrapRoleGroups.ts`, `deviceSet/bootstrap.ts`).

### 2.4 `list_by_hive_link_local_page` (LOCAL twin of `list_by_hive_link_page`)
- **Input/Output** identical to `list_by_hive_link_page`.
- **Replaces** the sleep + network-page-retry loop for self-authored records in stranded-group recovery (`setupNewHive.ts`).

## 3. Signal hardening — MIGRATION REQUIRED (M21)

The cross-host remote-signal channel changed. **A client that ingests the OLD full
payload from remote signals must migrate.**

- **Local (author's own conductor):** still emits the full `EncryptedContentSignal`
  `{ action_type, data: EncryptedContentResponse, from_agent }` on create/update/delete.
- **Remote (cross-host, to `public_key_acl.reader` minus self):** OUR fan-out now sends
  `EncryptedContentHint` `{ action_type, hash: String, original_hash: String, from_agent?: AgentPubKey }`
  — no `data`, no ciphertext. The recipient re-queries (`get_encrypted_content` by
  `hash`/`original_hash`) and `get`-verifies.
- **Owner handoff:** `initiate_owner_handoff` also sends `OwnerHandoffOfferHint`
  `{ offer_hash: ActionHash, hive_genesis_hash: ActionHash, from_agent?: AgentPubKey }`
  to the recipient (best-effort). Governance UI reacts without polling
  `list_pending_owner_handoffs`; keep one list-on-mount as durable recovery.
- **Provenance:** `recv_remote_signal` overwrites `from_agent = call_info().provenance`
  (the conductor-attested caller) on EVERY delivery, discarding any sender-supplied value.
- **TRUST MODEL (load-bearing):** `recv_remote_signal` is UNRESTRICTED and still decodes a
  legacy full `EncryptedContentSignal` FIRST, so a hostile peer CAN deliver a remote body
  carrying attacker-controlled `data` plus a stamped `from_agent: Some(...)`. Therefore any
  signal with `from_agent: Some(...)` is a REMOTE, UNTRUSTED FETCH TRIGGER — ignore/cache
  none of its embedded `data`, and resolve + verify the content by `hash`. ONLY
  `from_agent: None` (the author's LOCAL self-emit) identifies a trusted full payload. (A
  future hardening MAY drop the legacy remote full-signal arm entirely, since Wave-4
  fan-out is hint-only.)

**Client migration checklist:**
1. Retire the signal-embedded-bytes cache path (`sharedSecretSignalIngest.ts`,
   `dmIngest.ts`/`dmPersistence.ts` provisional-plaintext persist). Ingest the hint →
   fetch → validate.
2. Add the `EncryptedContentHint` + `OwnerHandoffOfferHint` shapes to the TS signal union.
3. Treat every remote-delivered signal (`from_agent: Some`) as an UNTRUSTED fetch trigger:
   never trust or cache its embedded `data`; resolve + verify by `hash`. Trust a full
   payload only when `from_agent: None` (local self-emit). Do not assume a remote signal
   omits `data` — a hostile peer can still send the legacy full shape.
4. **Fix the signal dispatcher discrimination** (`zomeSignals.ts`): `isEncryptedContentSignal`
   currently matches on `action_type` ALONE, which the hint ALSO carries — so `processSignal`
   (which tests it first) would misroute every `EncryptedContentHint` to the full-signal
   handler that dereferences `data`. Make the guards DISJOINT: the full signal requires
   `data`; the hint requires `hash` + `original_hash` and NO `data`; `OwnerHandoffOfferHint`
   requires `offer_hash`. Route hints to a fetch handler; after fetch reuse
   `isSignalFromAgentAttested(from_agent, <fetched revision author>)` for the trust check.

## 4. Caps & budgets (reference)

| Extern | item cap | per-item page | aggregate budget |
|---|---|---|---|
| list_encrypted_content_by_dynamic_links | 64 labels | limit (def 100, max 256) | 4096 |
| list_by_hive_links_many | 32 requests | limit (def 100, max 256) | 4096 |
| list_by_author_many | 64 lookups | limit (def 100, max 256) | 4096 |
| get_many_by_content_id_link | 64 lookups | 1 record/lookup | n/a |
| get_latest_memberships_local_many | 64 hives | 1 membership/hive | n/a |
| list_group_members_many | 64 groups | full roster | 4096 roster links |

Note: at the default per-item limit (100), the 4096 budget binds before the 64-item caps
(≈40 items fit); the 32-request hive-links cap is reachable (32×100 = 3200). Pass a
smaller per-item `limit` to use full item width.

## 5. Client-adoption "better way" checklist

Also see the earth-core `.newTasks/pass-7-integrity-candidates.md` §I for evidence lines.
Zero-DNA client hygiene enabled independently of Wave-4 (already-shipped surface):
- DM inbox drain/retry → adopt the shipped `get_many_encrypted_content` (bypassed today).
- `RoleGroupAnchorResolver` → trust the typed `list_groups_in_hive` response (drop the
  redundant `getGroupGenesis` re-fetch).
- Adopt `content_summary_many` (shipped, zero callers today).
- Sidecar manifest O(N²) re-listing + directory roster per-row decode → client
  orchestration cleanup (no new extern).
- Clear `SharedSecretCache` AND the decrypt FIFO on keyring lock (both currently unbounded
  / survive the lock — a decrypted-material lifetime leak).
- Companion pin-state IPC batch for the media-availability path (Tauri IPC side; pairs with
  `list_encrypted_content_by_dynamic_links`).

## 6. BDD acceptance (given / when / then — for humm-tauri validation)

Batch ordering + alignment:
- **Given** a batch request of N items, **when** the extern returns, **then** there are
  exactly N buckets in request order, one per item (duplicates included), and an
  unresolved item is `null`/empty — never dropped or reordered.

Bounds (per extern reject literal in §1–§2):
- **Given** a request exceeding an item cap, **when** called, **then** it rejects with the
  exact literal (e.g. `"author batch accepts at most 64 lookups"`).
- **Given** a page-based batch whose summed per-item limits exceed 4096, **when** called,
  **then** it rejects with `"batch total requested records exceed the 4096 budget"`.
- **Given** a `list_group_members_many` batch whose total roster links exceed 4096,
  **when** called, **then** it rejects with `"group-members batch roster links exceed the
  4096 budget"` (fall back to the singleton per group); rosters are otherwise COMPLETE.

Page-bounded first page:
- **Given** a dynamic label with 3 records and `limit: 2`, **when** called, **then** the
  bucket has 2 records and `truncated: true`; with `limit: 5`, 3 records and
  `truncated: false`.

Local twins:
- **Given** self-authored content on a peerless cell, **when** `list_by_hive_link_local_page`
  is called, **then** it returns the records from the local store (no network) matching the
  network twin on integrated data.
- **Given** the caller is a member of hive A (newest grant Writer, superseding an earlier
  Reader) and not a member of hive B, **when** `get_latest_memberships_local_many([A,B,A])`
  is called, **then** A resolves to the Writer grant (both occurrences), B is `null`, and
  each bucket equals the singleton `get_latest_membership_local` for that hive.

Membership/roster correctness (close-but-wrong):
- **Given** two grants (Reader then Writer) for one group to one agent, **when**
  `list_my_groups_local` / `list_group_members_many` runs, **then** the newest grant wins
  (roster shows one row for that agent), and an EXPIRED grant is filtered out.

Signal leakage fix:
- **Given** an author creates HiveGroup content with a remote reader, **when** the reader's
  conductor receives the signal, **then** it decodes as `EncryptedContentHint` with NO
  `data`/ciphertext and `from_agent` == the author (conductor-stamped); the author's own
  LOCAL signal still carries the full payload.
- **Given** a peer forges a hint with a false `from_agent`, **when** `recv_remote_signal`
  processes it, **then** `from_agent` is overwritten with the real caller provenance — the
  forged value never survives.
- **Given** the client dispatcher receives an `EncryptedContentHint` (carries `action_type`
  + `hash` + `original_hash`, no `data`), **when** `processSignal` classifies it, **then**
  the disjoint guards route it to the FETCH handler (never the full-signal `data` handler),
  and after fetch `isSignalFromAgentAttested(from_agent, fetched revision author)` gates trust.
- **Given** a hostile peer delivers a REMOTE full `EncryptedContentSignal` with
  attacker-controlled `data` and a claimed author, **when** the client's signal handler
  sees it (`from_agent: Some`, stamped by recv to the real sender), **then** the client
  ignores the embedded `data`, resolves the content by `hash`, and validates — the forged
  bytes never enter any cache or trust path.
- **Given** `initiate_owner_handoff` to a recipient, **when** it commits, **then** the
  recipient receives an `OwnerHandoffOfferHint` with the offer + hive hashes and stamped
  `from_agent`.

Client hygiene:
- **Given** the keyring is locked, **when** the lock event fires, **then** the
  `SharedSecretCache` and the decrypt FIFO are cleared — no decrypted key material or
  plaintext survives the lock.

## 7. Privacy & metadata contract (blessing-time surface)

The pass-7 fork widened public relationship metadata; the DHT cannot hide these, so the
client must not add semantic leakage on top:
- **`HiveMembershipIndex`** (agent pubkey → membership/genesis targets) exposes hive
  AFFILIATION enumeration for any agent.
- **`Lineage`** (plaintext prior-action tag + header lineage) CORRELATES an agent's
  content across generations.
- **Discovery paths** hash their bases, but the plaintext link TAGS remain visible — the
  Dynamic-label tag and the ACL group-hash tag are readable by any DHT observer.
- **CLIENT RULE (load-bearing):** any sensitive `dynamic_links` value MUST be either a
  RANDOM ≥128-bit id or a KEYED hash (HMAC/keyed-BLAKE with a SECRET salt the observer
  does not have) — NEVER a semantic/plaintext label AND never a bare (unsalted) hash of a
  guessable label (an unsalted hash of `"invoices-2026"` is dictionary/precompute-attackable,
  so it leaks the same meaning). This prevents DIRECT SEMANTIC disclosure only. An opaque
  tag is still an equality/linkability signal (same tag across records is correlatable) and
  exposes frequency + timing; it does NOT make the record anonymous. This is a client
  convention — the zome stores whatever label it is given.

- **Given** a sensitive discovery need (e.g. a private collection), **when** the client
  publishes content with a `dynamic_links` label, **then** the label is a random ≥128-bit
  id (or a secret-keyed hash) — a passive DHT observer cannot recover the content's meaning
  from the tag, though tag equality/frequency/timing correlation across records remains
  (unavoidable at the DHT layer).
