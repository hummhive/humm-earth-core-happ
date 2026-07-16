# HummTauri Integration — pass-6-pinned-hosts coordinator generation (v3.1.0)

> **Audience:** humm-tauri devs wiring the Persistent Blob Storage Keystone
> ("pinned hosts") + anyone consuming the content zome's query wire.
> **Status:** SHIPPED — coordinator-only hot-swap, DNA HELD.
> **Sanity-check companion:** `HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md`
> (this doc adds the pinned-hosts BDD skeletons in § 7).

---

## 1. TL;DR

- **One coordinator generation** `pass-6-pinned-hosts`, tag **`v3.1.0`**, on
  top of pass-6/v3.0.0. **DNA hash HELD byte-identical**:
  `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz` — existing
  `humm-earth-core-happ@6` cells hot-swap the coordinator on restart via your
  startup `updateCoordinators` path; fresh profiles install the new bundle.
  No chain fork, no migration, no cell-generation bump.
- **Your cutover** (all on your side): pin `CURRENT_HAPP_LABEL` /
  `CURRENT_HAPP_SHA256` to the new artifact (§ 9), bump
  `COORDINATOR_WASM_VERSION` 9→10, keep DNA/app id `humm-earth-core-happ@6`.
- **Closes** the deferred "Full cursor pagination (Phase 2 of C2)" item in
  `HUMM_TAURI_COORDINATOR_INTEGRATION.md`, your 02_B `list_by_author`
  pagination ask, and every coordinator seam in the 2026-07-14/15 mailbox
  batch (consolidated thread `2026-07-14T21-43-31…`).
- Your `#[ignore]`d acceptance test
  `full_source_page_replays_when_the_coordinator_emits_source_cursors`
  (`src-tauri/src/commands/blob_pinning/tests/provider/admission.rs`) can
  **re-enable unchanged** — envelope + request field names match 1:1.

## 2. What changed (4 seams, all additive)

| Seam | Externs / fields | Mailbox ask |
|---|---|---|
| A — recency | `latest_action_micros: Option<i64>` on every `EncryptedContentResponse` | 2026-07-14T19-11-43 |
| B — pin signals | `BlobPinSignal` (`Available`/`TakeNow`) + `send_blob_pin_signal` | 2026-07-15T09-52-51 / 09-57-34 |
| C+D — bounded pages | `list_by_hive_link_page`, `list_by_dynamic_link_page`, `list_by_author_page` → `BoundedLinkPage` envelope | 2026-07-15T18-33-18 / 00-00-05 / 09-54-07 |
| E — exact-own lookup | `get_my_content_by_id_link` → `{records, truncated}` | 2026-07-15T10-29-07 |

Every legacy extern is wire-identical (§ 6). New extern NAMES (not in-place
envelopes) per your 09-54-07 requirement: an old coordinator hard-fails an
unknown fn instead of silently ignoring new request fields.

## 3. Wire shapes (verbatim)

### 3.1 Response recency — `latest_action_micros`

```rust
pub struct EncryptedContentResponse {
    pub encrypted_content: EncryptedContent,
    pub hash: String,
    pub original_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_action_micros: Option<i64>,   // NEW — micros
}
```

```ts
interface HummContentHolochainGetResponse {
  // …existing fields…
  latest_action_micros?: number;  // micros of the SELECTED action
}
```

Semantics: `Some(t)` on every get/list/update/delete path — `t` is the
timestamp of the SELECTED action (the create for a never-updated entry, the
latest update otherwise). **`None` on the create-extern response** (no record
fetch happens there): consumers MUST NOT invent a time for it — the follow-up
`get` returns the authoritative value.

### 3.2 Bounded page envelope — `BoundedLinkPage`

```rust
pub struct SourcePosition {
    pub timestamp_micros: i64,
    pub action_hash: String,      // CreateLink action hash, b64 form
}

pub struct BoundedLinkPage {
    pub records: Vec<EncryptedContentResponse>,
    pub source_count: usize,
    pub source_positions: Vec<SourcePosition>,
    pub truncated: bool,
}
```

```ts
interface WireHivePageEnvelope {
  records: HummContentHolochainGetResponse[];
  source_count: number;
  source_positions: { timestamp_micros: number; action_hash: string }[];
  truncated: boolean;
}
```

### 3.3 Page requests

```rust
pub struct HiveLinkPageInput {
    pub hive_genesis_hash: ActionHash,          // raw-39-byte msgpack
    pub content_type: String,
    #[serde(default)] pub since_ts: Option<Timestamp>,          // i64 micros
    #[serde(default)] pub limit: Option<usize>,
    #[serde(default)] pub source_after_action_hash: Option<String>,
}

pub struct DynamicLinkPageInput {   // + dynamic_link: String
    /* hive_genesis_hash, content_type, dynamic_link,
       since_ts?, limit?, source_after_action_hash? */
}

pub struct AuthorLinkPageInput {    // author is the b64 AgentPubKey string
    /* author, content_type, since_ts?, limit?, source_after_action_hash? */
}
```

`Timestamp` is a transparent i64-micros newtype — send a bare number.

### 3.4 Exact-own lookup

```rust
pub struct MyContentByIdInput {
    pub hive_genesis_hash: ActionHash,
    pub content_id: String,
}
pub struct OwnContentRecords {
    pub records: Vec<EncryptedContentResponse>,
    pub truncated: bool,             // true above 4096 own roots
}
```

Empty match ⇒ `{ records: [], truncated: false }` — a VALID result, unlike
`get_by_content_id_link`'s error path. Only links **authored by the calling
agent** are considered (author-scoped `LinkQuery` + defensive post-filter):
foreign fixed-id collisions are structurally invisible.

Content-addressing subtlety: two publishes with BYTE-IDENTICAL content (same
header + same bytes) are ONE Holochain entry — both create actions resolve
through the entry-details walk to a single root, so `records` can carry the
same resolved root twice (dedupe by `hash` client-side if you re-publish
truly identical payloads). Distinct payloads under the same content id stay
distinct roots.

### 3.5 Blob-pin signals

```rust
pub struct BlobPinHint {
    pub hive_genesis_hash: ActionHash,
    pub blake3: String,          // stored-variant BLAKE3 hex — NEVER SHA-512
    pub byte_variant: String,    // opaque label ("raw" | "enc" | …)
    pub provider_record_hash: ActionHash,  // durable record to re-read
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_micros: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_agent: Option<AgentPubKey>,   // receiver-stamped, see § 5
}

#[serde(tag = "pin")]
pub enum BlobPinSignal {
    Available(BlobPinHint),
    TakeNow(BlobPinHint),
}

pub struct SendBlobPinSignalInput {
    pub signal: BlobPinSignal,
    pub recipients: Vec<AgentPubKey>,   // 1..=16, else Guest error
}
```

Signals are **HINTS**: no blob bytes, no logical SHA-512, every field
attacker-controlled on receipt except the stamped `from_agent`. Recipients
MUST re-read `provider_record_hash` and re-run Writer+/target/expiry/capacity
admission before any dial or write.

## 4. Cursor semantics (the load-bearing part)

- **Bounds:** `limit` `None` → 100, `Some(0)` → Guest error
  `"limit must be >= 1"`, `Some(n)` → clamped to **256**
  (`LINK_PAGE_HARD_LIMIT`). The bound applies to LINKS **before** any target
  fetch — source-side work is O(limit) even under an OpenWrite flood. (Raw
  link enumeration itself is not boundable by `LinkQuery` in hdk 0.6; the
  expensive DHT get/decrypt work is what the bound protects.)
- **Order:** ascending `(link.timestamp, link.create_link_hash)`.
  The tie-break is **raw-byte ActionHash order**, which is NOT b64-string
  order — never re-sort or compare cursors client-side; **replay
  `source_positions.last()` verbatim** into the next request.
- **Composite cursor** (`since_ts` + `source_after_action_hash`): strictly
  EXCLUSIVE on the pair — no dupes and no skips at equal timestamps. After a
  truncated page whose last position is `{timestamp_micros: 100,
  action_hash: "…99"}`, send `since_ts=100` AND
  `source_after_action_hash="…99"`.
- **`since_ts` alone:** INCLUSIVE (`>=`) — legacy watermark semantics;
  boundary duplicates possible, dedupe by action hash.
- **Cursor hash alone:** Guest error `"source_after_action_hash requires
  since_ts"`. Malformed hash: Guest error `"source_after_action_hash is not
  a valid ActionHash"` (never silent).
- **Positions are SOURCE truth:** one `SourcePosition` per selected link,
  always (`source_positions.len() == source_count`). A malformed, tombstoned,
  or gossip-lagged target drops from `records` but its position stays — you
  can cursor past poison rows without checkpointing undispatched records.
  Invariant: `records.len() <= source_count`.
- **Terminal page:** `truncated: false` (empty page
  `{records: [], source_count: 0, source_positions: [], truncated: false}`
  is a valid terminal, not an error). There is no `next_cursor` wire field —
  derive "next" from `source_positions.last()`.

## 5. Signal dispatch (§ C7b addendum)

`recv_remote_signal` now try-decodes THREE families, in order:
`EncryptedContentSignal` (field `action_type`) → `DmRemoteSignal`
(tag `"kind"`, inner `"type"`) → **`BlobPinSignal` (tag `"pin"`)**. The tag
literals are pairwise distinct so structural msgpack decode cannot
cross-match (host-proven both directions in
`encrypted_content::signals::tests`).

- **Provenance:** whatever the payload claims in `from_agent` is discarded —
  the sender-side extern forces it to `None`, and the receiver's dispatcher
  stamps `call_info()?.provenance` (lair-attested caller) before emitting.
  Conductor-proven cross-agent with a live forged claim
  (`blob_pin_signal_round_trips_between_agents`).
- **Fallthrough:** undecodable payloads error with
  `"recv_remote_signal: payload did not decode as EncryptedContentSignal,
  DmRemoteSignal, or BlobPinSignal"` (was "…or DmRemoteSignal" — grep-verified
  unmatched in humm-tauri before the change).
- **Double-encode invariant** unchanged: senders pre-encode via the shared
  `remote_signal_payload`, HDK applies its own `ExternIO::encode` on top
  (see `HUMM_TAURI_RECV_REMOTE_SIGNAL_FIX.md`).

## 6. Cap-grant table + unchanged contracts

| Extern | Granted (Unrestricted)? | Why |
|---|---|---|
| `list_by_hive_link_page` | ✅ | read-only public DHT link space, same class as legacy `list_by_hive_link` |
| `list_by_dynamic_link_page` | ✅ | same class as legacy `list_by_dynamic_link` |
| `list_by_author_page` | ✅ | same class as legacy `list_by_author` (already granted) |
| `send_blob_pin_signal` | ❌ | remote grant = signal reflector / amplification + spoof-by-proxy (Rule 3, like `send_dm_*`) |
| `get_my_content_by_id_link` | ❌ | "my" is provenance-derived; a remote grant lets any peer enumerate the callee's own records (like `get_messages_since`) |

Local callers (your TS BlobProviderApi / Rust Pin Host adapter through their
own conductor) bypass cap checks as chain authors — the two ungranted externs
work fine locally.

**Unchanged:** every legacy extern is wire-identical. In particular
`list_by_author` (your F1 admission invariant — called with ONLY
`{author, content_type}`, exact candidate hash required in the ≤1000-decoded
response) is byte-untouched. `create_encrypted_content` responses carry
`latest_action_micros: null`. `count_links_by_hive`, `get_by_content_id_link`,
`list_by_acl_link`, `fetch_pair_ss_with_hive_check`: untouched.

## 7. BDD sanity-check skeletons

Tagging: `[coordinator]` = enforced by this zome (conductor-proven here);
`[humm-tauri]` = your side's obligation.

**A — recency**
- Given an entry updated twice / When `get_encrypted_content` / Then
  `latest_action_micros` = the LATEST update's action time `[coordinator]`.
- Given a fresh create response / Then `latest_action_micros` is null and the
  consumer must not fabricate a time `[humm-tauri]`.

**C — paging**
- Given 250 provider records / When paging with limit 100 replaying
  `source_positions.last()` / Then pages 100+100+50 with `truncated`
  true/true/false, zero dupes, zero skips `[coordinator]`.
- Given a page of 100 where every target is malformed / Then `records=[]`,
  `source_positions.len()==100`, cursor advances `[coordinator]`; the watcher
  checkpoints only dispatched records `[humm-tauri]`.
- Given >100 links sharing one timestamp / Then the composite cursor
  exhausts them exactly once `[coordinator]`.
- Given an OpenWrite flood by a non-member author / Then source work stays
  bounded at `limit` before target fetch `[coordinator]`.

**E — exact-own**
- Given Alice+Bob both publish id X in hive H / When Alice calls
  `get_my_content_by_id_link` / Then only Alice's roots `[coordinator]`.
- Given 4097 own roots / Then 4096 + `truncated=true` and the caller performs
  ZERO create/update/delete (fail closed) `[humm-tauri]`.
- Given id X in hives H and H2 / Then exact hive scoping `[coordinator]`.

**B — signals**
- Given a hint sent to a verified AgentPubKey / When received / Then
  `from_agent` == conductor-attested sender regardless of payload claims
  `[coordinator]`.
- Given a single-encoded payload / Then the receiver rejects it (never a
  silent drop) `[coordinator]`.
- Given any hint / Then the recipient re-reads the durable record and re-runs
  Writer+/target/expiry/capacity admission before any dial/write
  `[humm-tauri]`.
- Given expiry/target mismatch in the hint / Then the drop is YOUR admission
  decision — the coordinator does not validate hint fields `[humm-tauri]`.

## 8. Testing posture

**Conductor-proven here (sweettest, 21 active tests green, in-process
hc 0.6.1 @ 3bdeacc against the HELD DNA):**
`hive_page_walks_multiple_pages_without_dupes_or_skips` (7 → 3/3/1 exact),
`dynamic_page_mirrors_hive_page_scoping`,
`author_page_scopes_to_author_and_pages` (Alice 2+1 via cursor, Bob excluded),
`deleted_entry_drops_from_page_records_and_positions`,
`exact_own_lookup_excludes_foreign_collisions_and_scopes_by_hive`,
`latest_action_micros_populated_on_get_none_on_create`,
`blob_pin_signal_dispatch_accepts_family_and_rejects_junk`,
`blob_pin_signal_round_trips_between_agents` (real 2-conductor network,
forged `from_agent` overwritten), and
`legacy_hive_link_since_ts_limit_watermark_sweep`.

**Host-proven (35 zome unit tests):** the double-encode chain, cursor math
(strict-exclusive composite, inclusive watermark, order + tie-break, clamp,
4096 saturation over synthetic links), and 3-way signal-family cross-decode
rejection in both directions.

**Deliberately delegated to your harness:** live 1001/4097 conductor-scale
saturation, author-offline e2e, remote-signal delivery under real WAN churn.

**Note on 02_A:** your spec's requested "earth-core tryorama behavioral
proof" for legacy `list_by_hive_link` `since_ts`/`limit` is delivered as the
sweettest `legacy_hive_link_since_ts_limit_watermark_sweep` — tryorama cannot
boot on the hc 0.6.x line (known repo constraint); sweettest is this repo's
conductor-proof harness.

## 9. Artifact + hashes + install

| What | Value |
|---|---|
| Generation label | `pass-6-pinned-hosts` |
| Git tag | `v3.1.0` (merge commit on `main`) |
| DNA hash | `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz` (**HELD**) |
| integrity wasm sha256 | `2656a9100937f7e6d17e2eebd5e744a1ef16e8e36b0efa089dc2f6382a655ae2` (**unchanged**) |
| content.wasm sha256 | `cc904ad6b4e94ba9c396224666f6c9f106ae721f5a7c9a7f5ec9d197b0c88a76` |
| happ sha256 | `1c7d981bd1919f853c4551e6c38a0184acfc5c6e1dc7b09b92db11575b116136` |
| Artifact filename | `humm-earth-core-happ_pass-6-pinned-hosts_dna-uhC0ksXs_happ-1c7d981b.happ` |

Install: the artifact + MANIFEST row are in your `.testdata/happs/` (row is
LAST = current generation for `provisionFromManifest.currentGenerationRow()`)
and `src-tauri/bin/` (local convenience copy). Your constants to bump:
`COORDINATOR_WASM_VERSION` 9→10
(`src-tauri/src/util/holochain/happ_install.rs`), `CURRENT_HAPP_LABEL` +
`CURRENT_HAPP_SHA256` (`src-tauri/src/constants.rs`).

## 10. RC-scan answers (evidence-cited, no code needed)

- **Upstream caveat item 4 (EncryptedContentSignal fan-out O(agents×writes)):**
  already structurally solved in this lineage — `remote_signal_acl_readers`
  fans out ONLY to `public_key_acl.reader` minus self; undecodable reader
  entries are skipped. Hive-scoping beyond reader-lists is unnecessary unless
  you show a hot path with genuinely broad reader-lists (tracked as B1 in
  `.newTasks/pass-7-integrity-candidates.md`).
- **Upstream item 5 (`get_encrypted_content_by_time_and_author` stub):**
  ABSENT from this lineage's coordinator (grep-verified) — upstream-repo
  staleness only.
- **Upstream item 2 (cap-grant typo `get_many_encrypted_conten`):** fixed in
  this lineage since pass-4-coordinator-cleanup; carried through v3.0.0+.
- **peerIdentityClaim ACL conformance (your Pass4 task 03):**
  `AclSpec::OpenWrite { target_hive_genesis_hash: None }` is CORRECT for
  peer-identity-claim-v1 (same class as humm-dm-keybinding-v1). Push delivery
  via `remote_signal_acl_readers` requires real b64 AgentPubKeys in
  `public_key_acl.reader` — `[]` or `["*"]` fan out to nobody (`"*"` fails
  pubkey decode and is skipped), so delivery is PULL unless you populate real
  reader pubkeys.
- **Migration-runner `PASS_2_DNA_HASH_BASE64` divergence (your 05 suite):**
  a hash duality, not a bug — `uhC0kawoZ…` is the HISTORICAL live pass-2 hash
  (built pre-reproducibility-pipeline; the hash real old cells carry —
  correct for your `flows.rs` constant), while this repo's MANIFEST row
  `uhC0kRHiJeJC…` is the post-repro-pipeline rebuild of the same source (see
  `.baseline-hashes.txt`). Keep using the historical hash for live-cell
  matching.
- **Directory/sidecar-manifest enumeration (your 06/02 item 1):** the paged
  hive-link externs serve typed enumeration TODAY without a fork — page the
  manifest content type. Validated shape/size caps remain integrity work
  (A4/A5 in the pass-7 catalogue).
- **LICENSE (DecraLicense):** decided by owner word (2026-07-04) but the text
  is not at hand — NO LICENSE file ships this generation; it remains the RC
  legal blocker, catalogued at `.newTasks/pass-7-integrity-candidates.md` §C1.
