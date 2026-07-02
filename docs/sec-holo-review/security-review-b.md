# Security review B — humm-earth-core-happ dry-refactor

## 1. Title and scope

Full-repository security review pass B for `/home/aphix/humm-earth-core-happ` on branch `dry-refactor` / pass-6 candidate.

Scope covered:

- Coordinator extern exposure and unrestricted cap-grant surface.
- Remote signal send/receive provenance, spoofing, and reflector risk.
- Inbox / DM / offline delivery flows.
- Migration marker readers/writers and `scripts/migrate-dna.ts` export/import/marker artifact handling.
- Query tolerance versus silent failure.
- Dependency, secrets, crypto, build/release artifact posture.
- High-level sanity check of integrity authority, entry/link order, and pass-6 no-wire-shape-change claims.

Per assignment constraints, this was read-only except for this report. I did not run build/test/lint/format/package gates and did not edit source.

## 2. Method / codewalk coverage

Read first: `skill://security-review`, `skill://coding-standards`, `skill://rust-patterns`, `skill://rust-testing`, `POSTCOMPACTION.md`, `CLAUDE.md`, `AGENTS.md`, `README.md`, `.baseline-hashes.txt`, `docs/CODEMAPS/{architecture,backend,data,dependencies}.md`, relevant `docs/HUMM_TAURI_*.md`, `docs/DNA_MIGRATION_GUIDE.md`, `docs/PASS_5_DEPLOY_HANDOFF.md`, and `docs/PASS_6_DRY_REFACTOR_HANDOFF.md`.

Codewalk coverage:

- Coordinator zome: `dnas/humm_earth_core/zomes/coordinator/content/src/lib.rs`, `encrypted_content/{mod,crud,get_helpers,queries,migration/*,signals/*}`, `hive/{crud,queries,owner}`, `group/{crud,queries}`, `inbox/{crud,queries}`, `invite.rs`, and relevant linking helpers.
- Integrity zome: `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs`, `validation_dispatch/*`, `encrypted_content/{entry_validation,links/*,types}`, `hive/*`, `group/*`, `inbox.rs`, and `invite.rs` at a validation-sanity level.
- Migration/release posture: `scripts/migrate-dna.ts`, `dnas/humm_earth_core/workdir/dna.yaml`, `workdir/happ.yaml`, root/zome manifests, codemaps, and pass handoff/hash docs.
- Targeted searches: cap grants, direct `send_remote_signal`, production `unwrap`/`expect`, silent-swallow patterns, hardcoded secrets, forbidden NIST-curve terms, and security/TODO markers.

I also coordinated over IRC with sibling oracles and independently confirmed the shared `OriginalHashPointer` blocker rather than relying on their reports.

## 3. Executive verdict

**BLOCK before merge/release.** The main pass-6 refactor claims look credible at a high level: entry/link enum order is unchanged, no new coordinator extern/wire shapes were found, cap grants exclude mutators and local private readers, remote-signal provenance stamping is preserved, and migration marker reads are author-filtered.

However, one coordinator/integrity trust boundary is still release-blocking: `OriginalHashPointer` is a public DHT link type validated as unconditional `Valid`, while `update_encrypted_content` trusts the first network pointer after committing an update. This enables third-party pointer poisoning/deletion against update and migration-marker plumbing.

Additional WARNs cover availability hardening and operational security: production read helpers still panic on host invariant assumptions, and migration JSON artifacts containing sensitive metadata are written without restrictive file modes. Notes capture accepted/documentation residuals.

## 4. Findings

### BLOCK

#### B-BLOCK-1 — `OriginalHashPointer` is unvalidated public DHT state trusted by update/migration plumbing

**Evidence**

- `OriginalHashPointer` is a declared public link type: `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs:42-49`.
- Integrity create-link dispatch returns `Valid` for `OriginalHashPointer` without checking author, base, target type, tag, or update-chain relation: `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/links.rs:48-50`.
- Integrity delete-link dispatch also returns `Valid` for `OriginalHashPointer`: `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/links.rs:128-130`.
- Coordinator creates a self-pointer on content create: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:69-75`.
- `update_encrypted_content` first commits `update_entry(...)`, then fetches `OriginalHashPointer` links from the previous action using `GetStrategy::Network`, errors if none exist, and trusts `original_hash_link[0].target`: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:173-207`.
- The helper comment explicitly acknowledges that non-members can plant poison `OriginalHashPointer` targets because integrity currently validates them unconditionally: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:154-163`.
- Migration marker writers call `crate::encrypted_content::crud::update_encrypted_content(...)`, so marker writes inherit this substrate: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/migration/writers.rs:20-37` and `:45-52`.
- `EncryptedContentUpdates` links are author-bound, but that validator only checks base/target are `EncryptedContent` by the link author; it does not prove the target is the native update descendant of the base/root selected via `OriginalHashPointer`: `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/links/updates.rs:26-91`.

**Why it matters**

A modified-coordinator peer can create or delete `OriginalHashPointer` links under another agent's content action because the integrity zome accepts them unconditionally. The stock coordinator then treats those public links as authoritative update-root state. Consequences include:

- availability griefing: deleting or poisoning the pointer can make a legitimate update/marker call fail after the entry update has already been attempted;
- graph poisoning: if the poison target is another action hash authored by the same victim, the coordinator can index the victim's update under the wrong original chain;
- migration fragility: `mark_migrated*` uses the same update path, so old-DNA forward-pointer markers can be disrupted by the same link-forgery surface.

This does not appear to let a third party bypass the `EncryptedContent` update-author check, but it is a Holochain security-boundary violation: coordinator reads from public DHT state are not trusted unless integrity validation constrains who can write that state.

**Suggested remediation**

- Add integrity validators for `OriginalHashPointer` create/delete.
- Create validation should require empty tag, action-hash base/target, `EncryptedContent` base/target, link author matching the relevant content author, and a native action relationship: create self-pointer or update pointer to the chain root.
- Reject `OriginalHashPointer` deletes outright, or at minimum author-gate them; immutable is safer because this is update plumbing.
- In coordinator code, do not trust `get_links(...)[0]`; derive the root from native action headers if possible, or require exactly one validated deterministic pointer and fail loudly on ambiguity.
- Add focused tests for hostile create/delete, non-action-hash targets, duplicate/ambiguous pointers, and marker writes.

### WARN

#### B-WARN-1 — Cap-granted read path can still panic on `get_details` invariants

**Evidence**

- `get_latest_typed_from_eh` uses `.unwrap()` on `details.actions.first()` when there are no updates: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/get_helpers.rs:56-58`.
- It uses `.unwrap()` on `sortlist.last()` in the update path: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/get_helpers.rs:60-65`.
- It unwraps `record.entry().to_app_option::<T>()` after a separate error check and unwraps `record.action().entry_hash()`: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/get_helpers.rs:72-86`.
- `get_encrypted_content` calls this helper: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:118-130`.
- `get_encrypted_content`, `get_many_encrypted_content`, and list/query externs that transitively call them are in the unrestricted read cap grant: `dnas/humm_earth_core/zomes/coordinator/content/src/lib.rs:76-85`.

**Why it matters**

These unwraps likely rely on Holochain host-return invariants, not normal user input. Still, a panic traps the WASM guest. Because this helper backs remotely callable public read externs, unexpected or malformed DHT/host state should become `ExternResult` errors or tolerant `Ok(None)` results, not panics.

**Suggested remediation**

Replace unwraps with explicit matches and contextual `wasm_error!` / `Ok(None)` branches. Preserve current tolerant query semantics where absence/deletion is expected; return hard errors only for impossible/corrupt details that should be visible in logs.

#### B-WARN-2 — Migration bundle/remap files are sensitive but script writes them with default filesystem permissions

**Evidence**

- `loadHiveBundle` / `saveHiveBundle` read and write the hive-bundle JSON path directly; `saveHiveBundle` creates the parent directory and calls `writeFile` without an explicit restrictive mode: `scripts/migrate-dna.ts:515-543`.
- `doExport` serializes every live exported `EncryptedContent` payload and bytes into a JSON bundle and writes it with default `writeFile` permissions: `scripts/migrate-dna.ts:700-745`.
- `doImport` writes `remap.json` with old/new action-hash mappings and failure details via default `writeFile`: `scripts/migrate-dna.ts:1231-1232`.
- `doMarkMigrated` appends marker failures back into the remap file via default `writeFile`: `scripts/migrate-dna.ts:1394-1400`.
- The migration guide describes the hive-bundle as load-bearing and containing `owner_pubkey_base64`, member pubkeys, roles, and membership hashes: `docs/DNA_MIGRATION_GUIDE.md:130-183`.
- The pass-2 handoff explicitly says `hive-bundle.json` reveals the full member roster and should be shared over an encrypted out-of-band channel; on multi-user hosts it should be moved out of `/tmp` into a `chmod-700` directory: `docs/PASS_2_DEPLOY_HANDOFF.md:343-349` and `:359-363`.

**Why it matters**

The scripts are commonly invoked with `/tmp/migrate/...` paths in docs. On a multi-user host or a permissive umask, default `writeFile` behavior can leave migration bundles/remaps readable by other local users. The files may include private application ciphertext, content metadata, full hive rosters, role assignments, membership hashes, and old/new action-hash mappings. That is not necessarily enough to decrypt content, but it is sensitive operational metadata and can aid social or migration-targeting attacks.

**Suggested remediation**

Create migration directories with mode `0o700`, write bundle/remap/hive-bundle files with mode `0o600`, and warn or refuse when the output directory is world/group-readable. Keep the existing docs guidance about encrypted out-of-band sharing, but enforce safe local defaults in the script.

#### B-WARN-3 — Legacy advertised test path can create false security confidence

**Evidence**

- Top-level `README.md` still advertises `npm test` as “Running the backend tests”: `README.md:25-29`.
- Current repo state says Tryorama cannot boot on hc 0.6.x and Sweettest is the active in-process conductor harness: `POSTCOMPACTION.md:76-90`.
- `docs/CODEMAPS/dependencies.md` also states Sweettest is the conductor harness and Tryorama cannot boot on hc 0.6.x: `docs/CODEMAPS/dependencies.md:56-65`.
- `docs/PASS_6_DRY_REFACTOR_HANDOFF.md` records pass-6 verification through host tests, clippy, zome packing, hash capture, and Sweettest, not the legacy top-level `npm test`: `docs/PASS_6_DRY_REFACTOR_HANDOFF.md:141-167`.

**Why it matters**

This is not a runtime vulnerability. It is a release-process risk: maintainers following the visible README command can spend time on a stale harness or mistakenly believe they exercised current Holochain 0.6.x conductor behavior. For a security-sensitive DNA hash line, the advertised test path should point at the actual gate.

**Suggested remediation**

Update README/package docs to identify Sweettest and the pass-specific host-test/build/hash ladder as the authoritative current gate. If legacy Tryorama tests are retained, label them archival and not a security/release gate.

### NOTE

#### B-NOTE-1 — Owner-transfer re-seizure residual is accepted and documented

**Evidence**

- `POSTCOMPACTION.md` documents the accepted residual: a malicious past owner can fork/re-seize governance ownership; blast radius is governance, not content decryption, and mitigation is deterministic resolution plus `is_ownership_contested`: `POSTCOMPACTION.md:34-40`.
- Humm Tauri pass-5 docs describe `is_lineage_owner` as an ever-owner predicate and call out the residual explicitly: `docs/HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md:69-89`.
- Integrity code matches that model: `is_lineage_owner` proves genesis root or prior accepted handoff recipient, not current owner: `dnas/humm_earth_core/zomes/integrity/content/src/hive/owner.rs:19-35`.
- Coordinator resolution sorts and folds deterministically and exposes contested ownership: `dnas/humm_earth_core/zomes/coordinator/content/src/hive/owner.rs:121-151` and `:168-216`.

**Why it matters**

This is not a pass-6 regression. It should remain visible in release notes and UI microcopy so “owner handoff” is not described as irrevocable cryptographic removal of former owners.

**Suggested remediation**

No source fix in this pass unless product requirements change. Keep contested-owner detection and honest UI wording.

#### B-NOTE-2 — `TimePath` / `TimeItem` remain unused but permissively valid link types

**Evidence**

- `TimePath` and `TimeItem` remain declared link variants: `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs:47-48`.
- Create and delete dispatch validate them unconditionally alongside `OriginalHashPointer`: `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/links.rs:48-50` and `:128-130`.
- The data codemap says both variants are declared but never created: `docs/CODEMAPS/data.md:102-103`.

**Why it matters**

I did not find a coordinator consumer, so this is not a release blocker like `OriginalHashPointer`. It is still public DHT junk surface: arbitrary peers can create/delete these links and validators will accept them.

**Suggested remediation**

If truly dead, reject future create/delete in the next integrity pass or document them as intentionally inert compatibility variants.

#### B-NOTE-3 — Migration marker payloads may omit new DNA/app identifiers by operator choice

**Evidence**

- `mark-migrated` warns but proceeds with `new_dna_hash_base64=""` if `NEW_DNA_HASH_BASE64` is unset: `scripts/migrate-dna.ts:1333-1341`.
- `mark-hive-migrated` likewise warns but proceeds with empty `new_dna_hash_base64` and/or `new_app_id`: `scripts/migrate-dna.ts:931-960`.
- CLI usage documents those environment variables as marker payload inputs: `scripts/migrate-dna.ts:1425-1430`.
- Script security comments require receivers to validate marker identity, get explicit user approval before DNA/app crossover, and cross-verify `new_action_hash_base64` on the new DNA: `scripts/migrate-dna.ts:100-119`.

**Why it matters**

This is an accepted operator-flexibility tradeoff, not a direct vulnerability. Empty marker routing fields make downstream resolution more dependent on out-of-band context; receiver-side approval and cross-verification remain mandatory.

**Suggested remediation**

For release runs, treat missing `NEW_DNA_HASH_BASE64` / `NEW_APP_ID` as a checklist failure unless there is an explicit migration runbook exception.

#### B-NOTE-4 — `DNA_MIGRATION_GUIDE.md` security model is stale relative to current update validation

**Evidence**

- `docs/DNA_MIGRATION_GUIDE.md` still says the integrity update validator only checks `action.author == header.revision_author_signing_public_key` and not original-entry authorship: `docs/DNA_MIGRATION_GUIDE.md:474-482`.
- Current pass-6 integrity validates `EncryptedContent` updates by checking the original action author before running validators on the new content: `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/entry_validation.rs:503-526`.

**Why it matters**

Runtime behavior is stronger than the old doc claims, but a stale “LOAD-BEARING” security section can mislead downstream migration reviewers about which layer is authoritative.

**Suggested remediation**

Update the migration guide in a docs pass: current pass-5/pass-6 integrity rejects cross-author updates; migration marker readers’ author filter remains defense-in-depth and historical compatibility context.

## 5. Explicit passes / no-findings for checked risk categories

- **Entry/link/wire-shape stability:** PASS. `EntryTypes` order remains in `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs:18-40`; `LinkTypes` order remains in `:42-70`. Codemap and pass-6 handoff state no EntryTypes/LinkTypes/serde/wire changes: `docs/CODEMAPS/data.md:12-14`, `docs/PASS_6_DRY_REFACTOR_HANDOFF.md:66-80`.
- **Integrity dispatch shape:** PASS except `OriginalHashPointer` / unused time links above. Root validate delegates to `validation_dispatch::validate_op`: `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs:84-86`; dispatch uses explicit link handling in `validation_dispatch/links.rs:5-131`.
- **Hive/group/content authority:** PASS at high level. Hive and group authority modules bind grantor, target hive/group, roles, and expiry; pass-5 docs and code reject Owner-via-membership and use coordinator current-owner prechecks for Admin grants: `dnas/humm_earth_core/zomes/coordinator/content/src/hive/crud.rs:85-117`.
- **Cap grants:** PASS. `set_cap_tokens` grants listed public reads and `recv_remote_signal` only; comments explicitly exclude mutators, local source-chain readers, marker writers, and sender-side `send_dm_*` reflectors: `dnas/humm_earth_core/zomes/coordinator/content/src/lib.rs:42-71` and `:76-170`.
- **Remote signal spoofing/stamping:** PASS. Outbound sends funnel through `remote_signal_payload` / pre-encoded `send_remote_signal`: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/signals/outbound.rs:8-20`. Receiver stamps `from_agent` from `call_info()?.provenance` and treats unknown payloads as errors: `dnas/humm_earth_core/zomes/coordinator/content/src/lib.rs:237-275`.
- **Remote signal reflector risk:** PASS. Sender-side `send_dm_delete_request` / call signaling externs exist at `signals/outbound.rs:103-139`, but they are deliberately excluded from cap grants with a specific reflector/spoof-by-proxy rationale: `coordinator/content/src/lib.rs:60-66`.
- **Inbox/DM flow:** PASS. `Inbox` create validation constrains recipient base shape and one-byte known event tags; delete validation allows only sender or recipient, preventing third-party mailbox censorship: `dnas/humm_earth_core/zomes/integrity/content/src/inbox.rs:91-161`. DM validators enforce recipient count, author inclusion, no duplicates, empty owner/admin/writer, and reader set equality per docs/codewalk.
- **Query tolerance:** PASS with B-WARN-1 caveat. Tolerant `filter_map` / `.ok().flatten()` paths are documented as resilience against dangling links, wrong-type inbox targets, and gossip lag, not accidental silent failure; representative code: `coordinator/content/src/encrypted_content/crud.rs:133-151`, `coordinator/content/src/lib.rs:15-24`, and `docs/CODEMAPS/backend.md:46-55`.
- **Migration marker forge resistance:** PASS apart from shared `OriginalHashPointer` substrate. Marker readers use trusted-author filtering and return `Ok(None)` for non-marker/malformed marker bytes: `coordinator/content/src/encrypted_content/migration/readers.rs:14-18` and `:77-81`; marker writers are not cap-granted per `coordinator/content/src/lib.rs:87-97` and `scripts/migrate-dna.ts:1267-1271`.
- **Secrets / dependency posture:** PASS by targeted search. No hardcoded API keys, passwords, private-key material, or forbidden NIST-curve source/dependency usage was found in the repo scan. Holochain SDK versions are pinned in codemap/manifests, and no external database/cloud service is part of this hApp: `docs/CODEMAPS/dependencies.md:14-24` and `:67-71`.
- **Build/release artifact safety:** PASS with B-WARN-3 process caveat. Pass-6 is clearly marked candidate-only, not pushed/tagged/distributed, with pass-5 remaining downstream target: `POSTCOMPACTION.md:10-21`, `docs/PASS_6_DRY_REFACTOR_HANDOFF.md:24-38`. Hashes and verification commands are recorded in the pass-6 handoff: `docs/PASS_6_DRY_REFACTOR_HANDOFF.md:84-104` and `:141-167`.

## 6. Open questions

1. Should `OriginalHashPointer` remain a public DHT link after validation is fixed, or should update-root derivation move entirely to native Holochain action headers?
2. Should migration scripts enforce `0o700` directories / `0o600` files by default, or should the team keep that as a runbook/operator responsibility?
3. Are `TimePath` and `TimeItem` compatibility placeholders still needed? If not, the next integrity pass should reject them explicitly.
