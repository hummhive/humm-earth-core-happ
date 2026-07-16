# HummTauri Integration — pass-6-idempotent-writes coordinator generation (v3.2.0)

> **Audience:** humm-tauri devs wiring crash-resume onboarding, the legacy
> hive-id SharedSecret remediation, and the multi-hive dashboard reads.
> **Status:** SHIPPED — coordinator-only hot-swap, DNA HELD.
> **Sanity-check companion:** `HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md`
> (this doc adds the idempotent-writes BDD skeletons in § 8).
> **Your tracking task:** `.newTasks/14_MISC_PlatformInfraAndDataLayer/16_Pass6IdempotentWritesIntegration.md`.

---

## 1. TL;DR

- **One coordinator generation** `pass-6-idempotent-writes`, tag **`v3.2.0`**,
  on top of pass-6-pinned-hosts/v3.1.0. **DNA hash HELD byte-identical**:
  `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz` — existing
  `humm-earth-core-happ@6` cells hot-swap the coordinator on restart via your
  startup `updateCoordinators` path; fresh profiles install the new bundle.
  No chain fork, no migration, no cell-generation bump. Integrity wasm
  byte-identical (`2656a910…a655ae2`) ⇒ every validator string in the BDD doc
  is unchanged by construction.
- **Your cutover** (all on your side): pin `CURRENT_HAPP_LABEL` /
  `CURRENT_HAPP_SHA256` to the new artifact (§ 9), bump
  `COORDINATOR_WASM_VERSION` 10→11, keep DNA/app id `humm-earth-core-happ@6`.
- **Closes** (fleet-audit §B, confirmed default-proceed 2026-07-16):
  - **B3** — crash-resume onboarding dead-end (`setupNewHive` re-run
    duplicate groups/memberships/content; your 01/03 idempotency asks, M-12):
    find-or-create family, § 3.
  - **B5** — legacy empty-hive-id SharedSecret remediation
    (02/01/03 remaining-work steps 1–3, unblocks 02_A C4): § 4 — including
    why your documented update-based step 2 is structurally impossible.
  - **B7** — `fetch_pair_ss_with_hive_check` without a pinned active hive
    (deletes the unbounded scan + 5s race + miss-cache + coalescer): § 5.
  - **A1-part** — `mark_migrated_v2` now accepts **HiveGenesis** originals
    (your 14/13 Finding 1): § 6.
  - **B8** — multi-hive `content_summary_many`: § 7.
  - **B6** — `send_dm_delete_request` family doc-deprecated (no wire change);
    you confirmed your Tier-A path is dead code being retired.

## 2. What changed (6 seams, all additive)

| Seam | Externs / fields | Cap-granted? |
|---|---|---|
| B3 — idempotent writes | `find_or_create_encrypted_content`, `find_or_create_group_genesis`, `find_or_create_group_membership` | NO (mutators) |
| B5 — remediation | `list_my_hiveless_content`, `remediate_hiveless_content` | NO (own-content / mutator) |
| B7 — pair-SS lookup | `FetchPairWithHiveCheckInput.active_hive_genesis_hash: ActionHash → Option<ActionHash>` | unchanged (granted) |
| A1-part — hive markers | `mark_migrated_v2` + `get_migration_marker_v2` accept HiveGenesis action hashes | unchanged (writer ungranted, reader granted) |
| B8 — batch summary | `content_summary_many` | **YES** (the only new grant) |
| B6 — deprecation | `send_dm_delete_request` / `DmRemoteSignal::DmDeleteRequest` doc-deprecated | unchanged |

Every legacy extern is wire-identical. The only changed REQUEST struct is
`FetchPairWithHiveCheckInput` (§ 5) — additive-compatible: your existing
calls decode as `Some(hive)` unchanged.

## 3. Find-or-create family (B3)

All three are **author-scoped**: the find half matches only CALLER-authored
entries (crash-RESUME semantics). Cross-agent duplicate prevention stays
client-side canonical-pick until the pass-7 A11 uniqueness validators.
**Find wins:** when a match exists you get the existing entry back — no
write, no Create signal, and content differences between your input and the
found entry are IGNORED. Canonical pick under multiple candidates =
**lexicographically-lowest base64 hash string** — the identical comparison
your `utils/selectCanonicalByHash.ts` performs, so client and zome always
elect the same record.

**None are cap-granted** (a remote grant would let peers write to the
callee's chain). Call them locally only.

```rust
find_or_create_encrypted_content(input: CreateEncryptedContentInput)
    -> FindOrCreateContentResponse { response: EncryptedContentResponse, was_created: bool }
```
- Find key: `(hive_genesis_hash, input.id)` on the `HummContentId` path.
- Requires a hive context. Reject literal:
  `"find_or_create_encrypted_content requires a hive-scoped acl_spec (HiveGroup or OpenWrite with target)"`.
- `response.latest_action_micros` follows the v3.1.0 rule: `None` when the
  call created (create path), `Some` when it found (get path).

```rust
find_or_create_group_genesis(input: CreateGroupGenesisInput)
    -> FindOrCreateGroupGenesisResponse { response: GroupGenesisResponse, was_created: bool }
```
- Find key: caller-authored `GroupGenesis` under `input.hive_genesis_hash`
  where `hive_wide_role == input.hive_wide_role`, AND — only when
  `hive_wide_role` is `None` — `display_id == input.display_id`.
  System role groups are hive singletons matched on role alone (display
  drift tolerated); custom/personal groups are keyed by display_id.

```rust
find_or_create_group_membership(input: CreateGroupMembershipInput)
    -> FindOrCreateGroupMembershipResponse { response: GroupMembershipResponse, was_created: bool }
```
- Find = the grantee's latest **unexpired** membership in the group with
  `role == input.role`. A different role, an expired grant, or no grant
  falls through to create (a role change is a legitimate new grant).
  Validator errors (self-grant, authority paths) propagate unchanged.

`was_created: false` ⇒ nothing was written and no signal was emitted.

**Eventual-consistency caveat (single writer):** the find half walks DHT
links, which integrate on the cascade's cadence even for self-authored ops.
Two calls in immediate succession can BOTH create when the first call's
link has not yet integrated — for identical bytes this converges to one
content-addressed entry; for group entries your canonical-pick keeps
working. Crash-resume (seconds-to-minutes later) is exactly the reliable
window.

## 4. Hiveless SharedSecret remediation (B5)

### 4.1 Why your documented plan could not work

`03_MigrateLegacyEmptyHiveIdSharedSecrets.md` remaining-work step 2 says
"re-commit via `update_encrypted_content`". Structurally impossible:
`update_encrypted_content` writes ONLY `EncryptedContentUpdates` +
`OriginalHashPointer` links — it never creates hive/dynamic/content-id
discovery links, so the C4 dynamic-path intersection stays empty forever.
And a retroactive `Dynamic` link on the ORIGINAL create fails the frozen
integrity link validator (the old header has no hive context). The only
path is **recreate with a corrected header + tombstone the original**,
which v3.2.0 batches server-side:

### 4.2 Detection

```rust
list_my_hiveless_content(content_type: String) -> Vec<EncryptedContentResponse>
```
Returns the caller's own entries of that type whose header lacks hive
context (legacy `OpenWrite { target: None }` writes). NOT cap-granted.

### 4.3 Batch remediation

```rust
remediate_hiveless_content(input: RemediateHivelessInput) -> Vec<RemediationOutcome>

RemediateHivelessInput { items: Vec<RemediateHivelessItem> }        // max 64/call
RemediateHivelessItem  { original_action_hash: ActionHash, corrected: CreateEncryptedContentInput }
RemediationOutcome {
    original_hash: String,       // b64 echo of your input hash
    status: "recreated" | "skipped_already_correct" | "skipped_already_remediated" | "failed",
    new_hash: Option<String>,    // b64 create-action hash of the corrected entry when known
    detail: Option<String>,      // failure reason / delete-retry info
}
```
- Over-cap reject literal: `"remediate_hiveless_content: at most 64 items per call"`.
- YOU supply the corrected `CreateEncryptedContentInput` per item (the zome
  cannot decrypt payloads to recover the group id — rebuild the header with
  the hive-scoped acl_spec and set `dynamic_links: Some([groupId])`).
- **Business conditions never abort the batch** — one outcome per item, in
  input order: unresolvable original, non-EncryptedContent original,
  foreign author (`"caller is not the original author"`), already-correct
  original, corrected input without hive context
  (`"corrected input lacks hive context"`), already-remediated.
- **A create failure DOES abort the whole call** — deliberately: Holochain
  only rolls the scratch back on whole-call Err, and a caught mid-create
  failure would commit a half-linked entry that a re-run would then
  mistake for a completed remediation. On abort nothing committed; re-run.
- `recreated` ⇒ corrected entry carries ALL discovery links (incl. dynamic)
  and the original is tombstoned. New hash ≠ old hash (header differs) —
  key off the decrypted groupId as you already do.
- `skipped_already_remediated` ⇒ idempotent re-run; self-heals a leftover
  un-deleted original (detail says `"original tombstoned on retry"` /
  `"original delete retry failed: …"`).
- Create/delete emit their normal `EncryptedContentSignal`s.
- Once-per-chain gating (your step 4) stays client-side. NOT cap-granted.
- Conductor proof: sweettest `remediate_hiveless_recreates_deletes_and_skips`
  — hiveless entry invisible to `fetch_pair_ss_with_hive_check` before,
  visible after, original tombstoned, re-run skips (the 02_A C4 fast path
  end-to-end).

## 5. `fetch_pair_ss_with_hive_check` optional-hive (B7)

```rust
pub struct FetchPairWithHiveCheckInput {
    pub author: String,
    #[serde(default)]
    pub active_hive_genesis_hash: Option<ActionHash>,   // was: ActionHash
    pub content_type: String,
    pub group_id: String,
}
```
- `Some(hive)` → exactly the v3.1.0 behavior (single author∩dynamic
  intersection).
- `None` → bounded union of that intersection across every hive the
  CALLEE belongs to (`list_my_hives`-derived). Zero hives → `[]`.
  Deletes your unbounded scan + 5s race + miss-cache + coalescer.
- Wire compat: your existing calls (always send the hash) decode as `Some`
  unchanged. A `None` sent to an OLDER coordinator hard-fails its decode —
  desired (no silent misbehavior).
- Privacy: the `None` branch derives the hive set from the callee's own
  Inbox links — public DHT data any peer can already walk. Grant unchanged.

## 6. `mark_migrated_v2` accepts HiveGenesis originals (A1-part)

Your 14/13 Finding 1: hive-identity markers could never be written because
the marker mechanism was update-based and the frozen integrity gate rejects
cross-entry-type updates (EncryptedContent update on a HiveGenesis
original). v3.2.0 gives hives a **create-based** marker:

- `mark_migrated_v2 { original_action_hash: <HiveGenesis AH>, marker: <V2> }`
  → creates ONE `EncryptedContent` marker entry on the content-id path
  `[genesis_b64, "hive-migration-marker-v2"]` with content_type
  `"_migrated/hive-genesis"` and `OpenWrite { target = the genesis }`.
  Re-marking UPDATEs that entry (no duplicates).
- **Founder-only**: non-author reject literal
  `"mark_migrated_v2: only the hive founder can mark a HiveGenesis migrated"`.
  The reader trusts only founder-authored markers (link-author scoped) —
  a third-party marker on someone else's genesis path is structurally
  invisible.
- `get_migration_marker_v2(<HiveGenesis AH>)` → `Some(MigrationMarker::V2)`
  after marking; the V2 payload carries your
  `new_hive_genesis_hash_base64` / `new_hive_genesis_display_id` fields.
  The V1 reader (`get_migration_marker`) structurally never sees hive
  markers — returns `None`, as today.
- Entry-type dispatch is by entry-def index, not shape: a **GroupGenesis**
  (or any other) action hash gets the explicit reject
  `"mark_migrated_v2: original must be an EncryptedContent or HiveGenesis entry"`
  (previously: silent dormant `Ok(None)` for every non-EncryptedContent).
  EncryptedContent originals keep the exact v3.1.0 semantics, including
  dormant `warn!` + `Ok(None)`.

## 7. `content_summary_many` (B8)

```rust
content_summary_many(inputs: Vec<ContentSummaryInput>)
    -> Vec<HiveContentSummary { hive_genesis_hash: ActionHash, summaries: Vec<ContentTypeSummary> }>
```
- Order-preserving; per-hive result identical to a `content_summary` call.
- Bounds (checked before any DHT work):
  `"content_summary_many: at most 32 hives per call"` and
  `"content_summary_many: at most 256 content types per call"` (aggregate
  across all inputs).
- **Cap-granted** — the only new grant this generation (same
  public-link-space read class as `content_summary`).

## 8. BDD skeletons

- **IW-1 idempotent create** — Given a hive and an OpenWrite input with id X;
  When `find_or_create_encrypted_content` runs twice with identical input;
  Then the second returns `was_created:false` with the first call's hash,
  and a third call with same id but different bytes still returns the
  original (find wins). *(sweettest: `find_or_create_content_is_idempotent`)*
- **IW-2 hive context required** — Given `OpenWrite{target:None}`; When
  find_or_create runs; Then Guest error contains `"requires a hive-scoped
  acl_spec"`. *(`find_or_create_content_requires_hive_context`)*
- **IW-3 group idempotency** — role-keyed genesis singleton; same-role
  membership regrant found; role change creates.
  *(`find_or_create_group_genesis_and_membership_idempotent`)*
- **IW-4 remediation end-to-end** — hiveless entry invisible to C4 →
  `recreated` → visible; original tombstoned; re-run
  `skipped_already_remediated`; hiveless corrected input `failed`; order
  preserved. *(`remediate_hiveless_recreates_deletes_and_skips`)*
- **IW-5 pair union** — entries in two hives; `None` finds each under its
  group id; `Some(h1)`+g2 stays empty.
  *(`fetch_pair_none_hive_unions_across_hives`)*
- **IW-6 hive marker** — founder marks; V2 read back; re-mark updates the
  single entry; V1 reader `None`; non-founder rejected; GroupGenesis hash
  rejected with the explicit-Err literal. *(`hive_genesis_marker_roundtrip`)*
- **IW-7 batch summary** — batch == singles, order-preserved; 33 hives
  rejected. *(`content_summary_many_matches_singles`)*

## 9. Release identity + artifact

| Field | Value |
|---|---|
| Label | `pass-6-idempotent-writes` |
| Tag | `v3.2.0` |
| DNA hash (HELD) | `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz` |
| content_integrity.wasm sha256 (unchanged) | `2656a9100937f7e6d17e2eebd5e744a1ef16e8e36b0efa089dc2f6382a655ae2` |
| content.wasm sha256 | `3b5348ebf99f2de2da2886cfb3711ff9899e3e29e95431fa484bf4f6788b4077` |
| humm_earth_core.dna sha256 | `ca636a08e81415d69674d282080af875ada4902da7ba85bc7db61edb022bcf92` |
| .happ sha256 | `bfe357aa73c4ee078cdc872d8c86225cd18263c44347a75ce44a613fc4470642` |

Artifact `humm-earth-core-happ_pass-6-idempotent-writes_dna-uhC0ksXs_happ-bfe357aa.happ`
is in `~/hummhive-official-happ-versions/` (MANIFEST row LAST = current).
Your `.testdata/happs/` + `src-tauri/bin/` mirroring is DEFERRED at owner
request while your testing round is in flight — pull from the official
store (or ask for the mirror) when ready. Your constants bump:
`COORDINATOR_WASM_VERSION` 10→11, `CURRENT_HAPP_LABEL`,
`CURRENT_HAPP_SHA256`.

Verification capture: fmt + clippy(-D warnings) clean; host tests 40/40;
integrity tests 76/76 (crate untouched); sweettest 28/28 active + 1
ignored; DNA hash + integrity sha byte-identical at build and at the merge
reproduction.
