# Holochain correctness review — pass-6 `dry-refactor`

## 1. Title and scope

Scope: full repository review of `/home/aphix/humm-earth-core-happ` on pass-6 `dry-refactor` against `skill://holochain` guidance: coordinator/integrity split, deterministic validation, `op.flattened`, EntryTypes/LinkTypes stability, `#[serde(default)]`, HDK 0.6.1 link/get patterns, `must_get_*`, cap grants, remote signals, Sweettest coverage, DNA/package handling, and docs accuracy.

This was read-only. I did not change source, run build/test/lint/format gates, or package commands. The only write is this report.

## 2. Method / codewalk coverage

Guidance read: `skill://holochain`, `Architecture.md`, `Patterns.md`, `AccessControl.md`, `Testing.md`, and `Workflows/ReviewZome.md`.

Repo context read before conclusions: `POSTCOMPACTION.md`, `CLAUDE.md`, `AGENTS.md`, `README.md`, `.baseline-hashes.txt` pass-5/pass-6 sections, `docs/CODEMAPS/{architecture,backend,data,dependencies}.md`, `docs/PASS_6_DRY_REFACTOR_HANDOFF.md`, and relevant `docs/HUMM_TAURI_*.md` files.

Source coverage: integrity `dnas/humm_earth_core/zomes/integrity/content/src/**`; coordinator `dnas/humm_earth_core/zomes/coordinator/content/src/**`; manifests/build/package files; `crates/sweettest/**`; legacy Tryorama harness under `tests/**`.

Static checks used `read`, `search`, `ast_grep`, and targeted codewalking only. I also sanity-checked with sibling security oracles over IRC.

## 3. Executive verdict

**Verdict: BLOCK before merge/release.**

Most pass-6 structure is sound: the coordinator/integrity split is preserved, validation uses `op.flattened`, schema/order stability is documented, cap grants are scoped, remote signals are pre-encoded and provenance-stamped, and Sweettest now covers important validation paths.

Two release-blocking Holochain validation issues remain. First, `OriginalHashPointer` links are unconditionally valid in integrity while coordinator update logic trusts the first DHT link under the previous action hash. Second, update validation dispatches by the **new** entry type and `validate_update_encrypted_content` does not require the original action to be `EncryptedContent`, so same-author cross-type updates can bypass the repo's immutable-entry validators.

## 4. Findings

### BLOCK

#### BLOCK-1 — `OriginalHashPointer` is an unvalidated DHT pointer trusted by coordinator update flow

Evidence:

- Link type exists in integrity at `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs:45`.
- Integrity create-link dispatch treats `LinkTypes::OriginalHashPointer | LinkTypes::TimePath | LinkTypes::TimeItem` as automatically valid at `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/links.rs:48-50`.
- Integrity delete-link dispatch also treats those link types as automatically valid at `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/links.rs:128-130`.
- Coordinator creates the self pointer on create at `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:69-75`.
- Coordinator update fetches `OriginalHashPointer` links from the previous action at `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:181-187`, errors on none at `:189-194`, then trusts `original_hash_link[0].target` at `:195` before writing update indexes at `:196-206`.
- The source comment acknowledges the issue: `crud.rs:154-163` says the integrity zome currently validates `OriginalHashPointer` as `Valid` and that a third party could plant poison links under someone else's action hash.

Why it matters:

- Any agent can currently create or delete this link type under another agent's content action because integrity does not check base, target, author, or deletion authority.
- Coordinator update flow then consumes DHT link state as if it were a trusted root pointer. At minimum this is an availability/index-poisoning vector; if an attacker targets another valid `ActionHash`, the update can be mis-indexed under the wrong original chain [INFERENCE: exact persistence depends on Holochain zome-call scratch/rollback behavior, but the trust boundary violation is present either way].
- This is directly against ReviewZome guidance: link validators must constrain author/base/target for security-relevant links, and coordinator code cannot make unvalidated public-DHT links authoritative.

Suggested remediation:

1. Add integrity validators for `OriginalHashPointer` create/delete.
2. Require the link author to match the base `EncryptedContent` action author.
3. Require base and target to be `ActionHash` values resolving via `must_get_valid_record` to `EncryptedContent` actions with the expected author/lineage relationship.
4. Require delete-link authority to be the link creator or the content author, not any agent.
5. In coordinator update flow, stop trusting `[0]`; filter deterministically for a well-formed pointer authored by the expected agent, and fail loudly on ambiguity.
6. Add focused host/Sweettest coverage for malicious third-party pointer creation/deletion and poisoned update lookup.

#### BLOCK-2 — Cross-type updates can bypass immutable-entry validators by updating into `EncryptedContent`

Evidence:

- `validate_op` dispatches `StoreEntry::UpdateEntry` and `RegisterUpdate::Entry` by the new `app_entry` type at `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/mod.rs:22-29`; `StoreRecord::UpdateEntry` also receives only the new `app_entry` and calls `dispatch_store_record_update_entry` at `:58-60`.
- `dispatch_update_entry` routes `EntryTypes::EncryptedContent` updates directly to `validate_update_encrypted_content` at `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/entry.rs:34-41`.
- The special store-record path for `EncryptedContent` re-runs create validation on the new entry, then calls `validate_update_encrypted_content`, but still does not verify the original entry type at `validation_dispatch/entry.rs:63-80`.
- `validate_update_encrypted_content` fetches the original record and checks only that `action.author` matches `original.action().author()` before validating the **new** content at `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/entry_validation.rs:503-525`.
- The repo intends several other entry types to be immutable: `HiveGenesis` rejects updates at `dnas/humm_earth_core/zomes/integrity/content/src/hive/membership.rs:20-26`, `HiveMembership` at `:216-223`, `GroupGenesis` at `dnas/humm_earth_core/zomes/integrity/content/src/group/membership.rs:30-39`, and `GroupMembership` at `:217-226`.

Why it matters:

- A same-author update from an immutable entry action to a valid `EncryptedContent` entry appears to route through the `EncryptedContent` update validator, not the original type's immutable update validator. For example, a hive founder can author an update whose original action is their own `HiveGenesis` and whose new entry type is `EncryptedContent`; current validation checks same author and the new content's ACL, but not that the original was encrypted content.
- That violates the Holochain ReviewZome expectation that update validators enforce the invariants of the thing being updated. It also undercuts the repo's documented immutability guarantees for identity/authority entries, even if most current coordinator readers fetch those entries by original action hash and do not follow native update chains.

Suggested remediation:

1. In `validate_update_encrypted_content`, fetch and decode the original record as `EntryTypes::EncryptedContent`; reject if the original action is any other entry type.
2. Consider centralizing a cross-type update guard in validation dispatch so each update validates both original and new entry type, not just the new one.
3. Add host tests for cross-type updates from `HiveGenesis`, `HiveMembership`, `GroupGenesis`, and `GroupMembership` into `EncryptedContent`.

### WARN

#### WARN-1 — Production coordinator helper still panics via `unwrap()`

Evidence:

- `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/get_helpers.rs:56-65` calls `.unwrap()` on `record.action().entry_hash()`.
- `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/get_helpers.rs:72-85` calls `.unwrap()` after matching update/create branches.

Why it matters:

- A panic traps the WASM guest. Project rules and Holochain review guidance prefer `ExternResult` propagation over panics in zome code.
- These unwraps are in production coordinator code, not test code.

Suggested remediation: replace both unwrap sites with explicit `ok_or(wasm_error!(...))?` / branch handling that preserves the current `ExternResult<Option<TypedRecord<T>>>` contract.

#### WARN-2 — Advertised Tryorama/TypeScript test path is stale and does not match pass-6 wire shapes

Evidence:

- `README.md:25-29` still advertises `npm test` as the test command.
- `package.json:10-12` wires `test` to `npm run test:humm_earth_core`, which runs the legacy `tests` package.
- `tests/package.json:8-12` uses `tryorama`, while `POSTCOMPACTION.md:76-78` and `:88-90` document that Tryorama cannot boot this Holochain 0.6.x tree and Sweettest is the real conductor harness.
- `tests/src/humm_earth_core/content/common.ts:15-18`, `:44-47`, `:64`, and `:69-85` use broad `any`, contrary to the project `ts-no-any` rule.
- Legacy test payloads still use stale fields: `tests/src/humm_earth_core/content/list_by_hive.test.ts:15-20`, `:35-44`, and `:69-73` send `hive_id`/`acl`, while current coordinator create input uses `display_hive_id`, `acl_spec`, and `public_key_acl` at `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/mod.rs:43-68`.

Why it matters:

- The repository tells maintainers to run a harness that is stale for pass-6 and may fail for the wrong reasons or give false confidence if skipped.
- The Holochain Testing guidance expects Sweettest/in-process conductor coverage for zome validation paths; this repo does have that, but the top-level advertised path points elsewhere.

Suggested remediation: either retire the legacy Tryorama package from the advertised gate, or explicitly mark it archival and make README/package scripts point to the Sweettest workflow. If TS tests are kept, update payload mirrors and remove `any`.

#### WARN-3 — Documentation drift can send downstream integrators to the wrong generation/test flow

Evidence:

- `README.md:7-14` still says `nix-shell` and old `hc` setup, while current repo state uses holonix/flake Holochain 0.6.1 (`POSTCOMPACTION.md:72-78`, `crates/sweettest/README.md:10-17`).
- `README.md:25-29` still makes Tryorama the visible test path despite Sweettest being the active conductor harness.
- `docs/HUMM_TAURI_PASS_ROADMAP.md:363-368` still says earth-core builds against Holochain 0.6.0/HDK 0.6.0/HDI 0.7.0, while current repo docs and Cargo state 0.6.1/0.6.1/0.7.1.
- `docs/HUMM_TAURI_PASS_ROADMAP.md:374-384` instructs future DNA bumps to append pass sections, but pass-5/pass-6 are not represented there; authoritative state has moved to `.baseline-hashes.txt` and pass-specific handoffs.
- `docs/HUMM_TAURI_v1.0.0_HANDOFF_DOC.md:1-5` calls itself the canonical current-state starting point, although current downstream target is pass-5/v2.0.0 and pass-6 is an unreleased candidate per `POSTCOMPACTION.md:10-21` and `docs/PASS_6_DRY_REFACTOR_HANDOFF.md:26-38`.
- `docs/HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md:114-118` says a non-owner Owner grant should fail because only an Owner may grant Owner. Current code rejects all `HiveMembership` Owner grants at `dnas/humm_earth_core/zomes/integrity/content/src/hive/membership.rs:92-100`; `docs/HUMM_TAURI_OWNER_ROLE_INTEGRATION.md:20-22` correctly says Owner comes only through owner handoff.

Why it matters:

- The code/package state is coherent, but docs present multiple “current” entry points. That is risky for humm-tauri integration because this repo’s DNA hash lineage is load-bearing.

Suggested remediation: make one current-state entry point explicit, demote older handoffs to archival, update README setup/test commands, and correct Owner-grant BDD text.

#### WARN-4 — Query docs/comments understate actual pagination support

Evidence:

- `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/queries.rs:12-19` says only hive listing supports `since_ts` and `limit`.
- The same file defines `since_ts`/`limit` on `ListByAuthorInput` at `queries.rs:240-247` and applies them at `queries.rs:250-268`.
- `docs/HUMM_TAURI_DM_DISCOVERY_PLAN.md:188-189` still says `list_by_author` lacks `since_ts` / `limit`.
- `docs/HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md:578-586` repeats that K-2 is blocked until list-by-author pagination exists.

Why it matters:

- This is not a runtime defect, but it can cause downstream code to build unnecessary workarounds or treat a shipped behavior as unavailable.

Suggested remediation: update comments/docs to reflect that author listing now has pagination/tombstone handling, and keep any remaining gap limited to content-type/feed-specific needs.

#### WARN-5 — `update_encrypted_content` accepts link-bearing field changes but does not reindex them

Evidence:

- Coordinator create writes the full index bundle: author Hive link at `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:77-89`, hive/content-id/dynamic links at `:91-106`, and ACL links at `:108-112`.
- Coordinator update only writes `EncryptedContentUpdates` and `OriginalHashPointer` links at `crud.rs:196-206`, then emits signals from the updated entry's `public_key_acl.reader` at `:209-223`.
- Integrity update validation allows the new content to be fully revalidated as its new `acl_spec` at `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/entry_validation.rs:503-525`.
- The repo already documents the mismatch in `docs/HUMM_TAURI_ACLSPEC_MUTATION_ON_UPDATE.md:11-20` and explains stale/missing query paths at `:56-71`.

Why it matters:

- This is a correctness footgun at the Holochain link/index boundary: the DHT accepts an updated entry whose `acl_spec`, `content_type`, `id`, or hive context may no longer match the links by which coordinators discover it.
- Existing docs tell clients to re-author instead of mutate ACL in place, but the public local extern still permits the invalid state. If a client slips, `get_encrypted_content` and `list_by_*` can disagree about where the entry lives.

Suggested remediation: either enforce immutability for link-bearing header fields on update (`id`, `content_type`, `acl_spec`, hive context, and any field used by index paths), or make update re-create/revoke the affected index links deterministically. If the accepted contract remains "updates are content-bytes only", encode that in validation and tests.

#### WARN-6 — Non-DM `public_key_acl` buckets are unbounded and remote signal fan-out is not deduped

Evidence:

- `Acl` stores `owner: String` plus unbounded `admin`, `writer`, and `reader` vectors at `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/types.rs:262-269`.
- DirectMessage validation bounds and dedupes recipients (`DM_MAX_RECIPIENTS`) and requires `public_key_acl.reader` to equal the recipient set at `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/entry_validation.rs:416-480`.
- HiveGroup validation bounds `group_acl` and `recipient_witnesses` at `entry_validation.rs:119-130` and `:208-216`, but `check_witness_pka_bidirectional` iterates every `public_key_acl` entry at `:280-299` without an explicit PKA cardinality or duplicate-pubkey bound.
- Public/OpenWrite validation does not constrain `public_key_acl` at all beyond the author/header check and variant-specific hive target checks at `entry_validation.rs:63-76` and `:481-494`.
- Coordinator fan-out decodes every `public_key_acl.reader` item, filters self, and collects into a `Vec` without deduping at `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/signals/outbound.rs:30-37`; it sends the whole vector at `:39-60`.

Why it matters:

- A modified coordinator can commit non-DM content with very large or duplicate PKA buckets. For HiveGroup, one valid witness can back repeated duplicate PKA reader entries; for Public/OpenWrite, no membership witness is required.
- This can amplify validation CPU/string work and outbound remote-signal fan-out. It also weakens the documented "recipient_witnesses cover every pubkey exactly once" invariant because the witness side is deduped, but duplicate PKA entries are not rejected.

Suggested remediation: add a small `PUBLIC_KEY_ACL_MAX_RECIPIENTS`/per-bucket bound, reject duplicate pubkeys across the load-bearing buckets where duplicates have no semantic value, and dedupe `acl_reader_recipients` before `send_remote_signal`.

#### WARN-7 — Migration docs secure `hive-bundle` but not the per-entry bundle/remap files

Evidence:

- `scripts/migrate-dna.ts` writes a self-contained export bundle and an old-to-new remap as part of the migration pipeline at `scripts/migrate-dna.ts:46-59`; the remap is described as a load-bearing host handoff at `:91-99`.
- The migration guide shows `bundle.json`, `hive-bundle.json`, and `remap.json` in the pipeline at `docs/DNA_MIGRATION_GUIDE.md:88-99`, and defines `remap.json` contents including source/target app IDs, agent pubkeys, action hashes, content type, hive ID, and failures at `:278-310`.
- The same guide explicitly says only `hive-bundle` must be sent through an encrypted out-of-band channel and treats its roster leak as operationally sensitive at `docs/DNA_MIGRATION_GUIDE.md:575-581`.
- Older pass-2 handoff docs add `chmod-700` local-storage guidance for `hive-bundle` at `docs/PASS_2_DEPLOY_HANDOFF.md:343-362`, but I did not find equivalent guidance for `bundle.json` or `remap.json`.

Why it matters:

- `bundle.json` contains migrated `EncryptedContent` payloads and plaintext headers/ACL metadata; `remap.json` exposes old/new action hashes, app IDs, agent pubkeys, content types, hive IDs, and failure modes. The body bytes may be encrypted, but the metadata is still sensitive operational data.
- The Holochain packaging/release handoff should tell operators to keep all migration artifacts in a private directory, not just the shared hive roster file.

Suggested remediation: update `DNA_MIGRATION_GUIDE.md` and the script header to treat `bundle.json`, `hive-bundle.json`, and `remap.json` as sensitive local artifacts; recommend a chmod-700 workdir, encrypted transfer where files cross devices/users, and deletion/archival policy after cutover.

### NOTE

#### NOTE-1 — `TimePath` / `TimeItem` remain unused but permissive link types

Evidence:

- `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs:47-48` still defines `TimePath` and `TimeItem`.
- Integrity currently marks these links valid on create/delete together with `OriginalHashPointer` at `validation_dispatch/links.rs:48-50` and `:128-130`.
- `docs/CODEMAPS/data.md:102-103` says these variants are declared but never created by coordinator code.

Why it matters:

- I did not find a coordinator consumer, so this is not release-blocking like `OriginalHashPointer`. It is still public DHT surface that accepts arbitrary junk links.

Suggested remediation: if these variants are truly dead, reject future create/delete in the next integrity pass or document them as intentionally inert compatibility variants.

#### NOTE-2 — Some entries duplicate action-header metadata for historical UI contracts

Evidence:

- `HiveGenesis.created_at_microseconds` is defined at `dnas/humm_earth_core/zomes/integrity/content/src/hive/types.rs:17-25`.
- `GroupGenesis.created_at_microseconds` is defined at `dnas/humm_earth_core/zomes/integrity/content/src/group/types.rs:28-37`.
- `DmProbeLog.probed_at` is defined at `dnas/humm_earth_core/zomes/integrity/content/src/inbox.rs:9-18`.

Why it matters:

- `skill://holochain/Patterns.md` recommends not duplicating author/timestamp data already present in action headers. These fields are established wire shape and should not be removed casually, but new entry types should avoid the pattern.

Suggested remediation: keep existing fields for compatibility; avoid adding new created/updated timestamp fields unless they are genuinely domain data and validated as such.

#### NOTE-3 — Tolerant decode/filter paths appear intentional, not accidental silent failure

Evidence:

- `get_many_encrypted_content` tolerates missing/deleted/undecodable hashes at `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:133-151`.
- `get_typed_entry` returns `Ok(None)` for non-matching entry shapes at `dnas/humm_earth_core/zomes/coordinator/content/src/lib.rs:15-23`.
- `docs/CODEMAPS/backend.md:46-55` documents this partial-success batch behavior.

Why it matters:

- This matches the documented query-tolerance design. I did not classify these as silent-failure bugs.

## 5. Explicit passes / no-findings for checked risk categories

- **Coordinator/integrity split:** PASS. One integrity zome plus one coordinator zome are declared in `dnas/humm_earth_core/workdir/dna.yaml:4-17`. Integrity owns entry/link enums and validation in `integrity/content/src/lib.rs:18-70` and `validation_dispatch/**`; coordinator owns CRUD/query/signals/cap grants in `coordinator/content/src/**`.
- **Deterministic validation and `op.flattened`:** PASS. Root validate delegates at `integrity/content/src/lib.rs:84-86`; dispatch uses `op.flattened::<EntryTypes, LinkTypes>()?` at `validation_dispatch/mod.rs:16-17`. I found no `op.to_type` usage and no raw `get`, `get_links`, `agent_info`, or `sys_time` inside integrity validation.
- **Validation dependency fetches:** PASS. Integrity uses `must_get_valid_record` / `must_get_action` for DHT dependencies, for example encrypted-content witness/link validation and hive/group authority modules. I did not find coordinator-only reads being used from integrity.
- **EntryTypes/LinkTypes order and visibility:** PASS. `EntryTypes` order and `DmProbeLog` private visibility are anchored at `integrity/content/src/lib.rs:18-37`; `LinkTypes` order at `:40-70`. `.baseline-hashes.txt` pass-6 notes state no EntryTypes/LinkTypes/wire-shape changes.
- **`#[serde(default)]` for evolution fields:** PASS. Representative examples: `HiveMembership.grantor_owner_accept_hash` at `hive/types.rs:85-87`; migration marker v2 optional genesis fields at `coordinator/content/src/encrypted_content/migration/markers.rs:143-153`; signal `from_agent` fields at `coordinator/content/src/encrypted_content/signals/dm.rs:23-26` and `:50-62`; query optional pagination fields at `queries.rs:66-74` and `:244-247`.
- **HDK 0.6.1 link/get APIs:** PASS. `delete_link` is called with `GetOptions` in coordinator delete paths, e.g. `coordinator/content/src/lib.rs:35` and `inbox.rs:47`. `LinkQuery` / `GetStrategy` are used in query code, e.g. `encrypted_content/queries.rs:46-49`, `:98-102`, `:146-154`; local source-chain lookups use `GetStrategy::Local`, e.g. `hive/queries.rs:160-163` and `:359-362`.
- **Capability grants:** PASS. `init` grants only `recv_remote_signal` plus explicit read/discovery functions at `coordinator/content/src/lib.rs:42-71` and `:76-170`. Mutators are deliberately not granted, and comments at `:60-66` explain that DM signal send helpers are local-author calls, not remote-call APIs.
- **Remote-signal setup:** PASS. Receiver cap grant is installed at `coordinator/content/src/lib.rs:160-170`. `recv_remote_signal` decodes content signal first, then DM signal, stamps provenance from `call_info()?.provenance`, and rejects unknown payloads at `:237-264`. Outbound DM signals pre-encode payloads at `encrypted_content/signals/outbound.rs:8-20` before `send_remote_signal`.
- **Sweettest coverage:** PARTIAL. Active Sweettest docs show 11 active + 1 ignored at `crates/sweettest/README.md:38-49`. Shared hash guard lives at `crates/sweettest/tests/support/mod.rs:10-35`. Recipient-witness and grant-window behavior tests are present at `crates/sweettest/tests/recipient_witnesses.rs:83-163` and `crates/sweettest/tests/owner_and_acl.rs:60-115`; missing targeted coverage is called out in BLOCK-1/BLOCK-2/WARN-5/WARN-6, and WARN-2 covers the stale top-level Tryorama path.
- **DNA/package/release lineage:** PASS for pass-5/pass-6 state tracking. `docs/PASS_6_DRY_REFACTOR_HANDOFF.md:26-38` clearly marks pass-6 as unreleased candidate and pass-5 as current downstream target. `.baseline-hashes.txt:625-688` records pass-6 DNA/hash/package outputs and verification commands/results. `POSTCOMPACTION.md:10-21` and `:84-104` keep pass-5 vs pass-6 state explicit. Migration artifact handling is separately covered by WARN-7.
- **No NIST curve / crypto regression:** PASS by codewalk. I found Ed25519/HDK agent-key usage and no `p256`, `p384`, `p521`, `prime256v1`, or `secp*r1` crate/source usage.
- **No cross-DNA/cell-clone complexity:** PASS. This repo remains one DNA / one role; I found no cross-DNA call surface or clone-cell packaging requirement.

## 6. Open questions

1. Should the legacy `tests/**` Tryorama harness be removed entirely, or kept as archival documentation but removed from `npm test`? The code state points to Sweettest as the real gate, but top-level scripts still expose Tryorama.
2. Is `docs/HUMM_TAURI_PASS_ROADMAP.md` meant to remain a historical roadmap, or should it be updated as an authoritative pass lineage? Current authoritative lineage is `.baseline-hashes.txt` plus pass handoffs.
3. For `OriginalHashPointer`, no accepted residual risk was documented in the files I read. If the team intended it as a trusted-local-only hint, coordinator code should enforce that locally and integrity should still reject hostile public-DHT pointer writes.
