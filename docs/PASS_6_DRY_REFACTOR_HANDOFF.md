# Pass-6 dry-refactor handoff — humm-tauri integration

**Date:** 2026-06-24  
**Audience:** humm-tauri developers  
**Branch:** `dry-refactor`  
**Change class:** integrity-zome structural refactor — **DNA changes**  
**Release status:** **BLESSED 2026-07-02** — published to `~/hummhive-official-happ-versions/` + mirrored to `humm-tauri/.testdata/happs/` (both clones); merge to `main` + tag `v3.0.0` follows. Cutover runbook: [`PASS_6_DEPLOY_HANDOFF.md`](./PASS_6_DEPLOY_HANDOFF.md)

## Read first: pass numbering and withdrawn candidate

Pass-6 means the next unadopted DNA generation after pass-5/v2.0.0. It is not
pass-7 because no downstream team has adopted the earlier pass-6 candidate DNA.
That earlier candidate is withdrawn and must not become a maintained humm-tauri
constant.

Withdrawn pre-fix candidate — **BAD / DO NOT USE / DO NOT TRACK DOWNSTREAM**:

```text
DNA hash:                 uhC0kOQX5rU8yL6CIEWAfGu1G5TaNsgMcS7yp-D0fV2eG1-2bA7iJ
content_integrity.wasm:   156d3ea2a9d5c6bb484a2beffe7cd05caac7c54a0a5fb8f2759e014854f90dbc
content.wasm:             0d022f076537a1f772e7b0e32678073093a18e6d0710d73e2a4eb6c1d6238a58
humm_earth_core.dna:      48642bfc928c382d22b892c8a2829bf737587d86fae5ea109661aef8ace11f9e
humm-earth-core-happ.happ 3dcb8827d7d45f3fabc68708862c4d379ed52d0b30f609ebed3f3b6dc8524d4e
```

Why withdrawn:

- Security/Holochain review found `OriginalHashPointer` was public DHT state
  accepted without integrity validation while coordinator update-root logic
  trusted pointer-link results.
- Review found cross-entry-type updates could route through the `EncryptedContent`
  validation path and bypass immutable-entry validators.
- Those fixes touch integrity code, so the DNA hash necessarily changed again.

Decision for downstream versioning:

- Because no one is using the withdrawn candidate, the fixed candidate below
  **replaces pass-6** instead of becoming pass-7.
- Do not add humm-tauri constants, fixtures, manifests, migration handling, or UI
  labels for the withdrawn hash.
- If evidence appears that the withdrawn DNA was installed by a real user/dev
  environment, stop and re-label the fixed candidate as pass-7 with an explicit
  migration source. Current docs assume that did not happen.

---

## Current TL;DR for humm-tauri

- **Pass-6 is BLESSED (2026-07-02)** as the next integration target, replacing
  pass-5/v2.0.0. Follow [`PASS_6_DEPLOY_HANDOFF.md`](./PASS_6_DEPLOY_HANDOFF.md)
  for the swap.
- **Pass-6 DNA:**
  `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz`.
- **Pass-6 hApp SHA:**
  `3062de3851eac81fedd425325b30f3cabaaa2000e1e295ba7db5d4d031dda5d3`.
- **Never use `updateCoordinators` for pass-6.** This is an integrity-zome edit,
  so it is a new DNA/cell generation even though wire shapes are unchanged.
- **No UI/API code change is needed for pass-6 itself** because extern names,
  entry fields, serde tags, and TypeScript-facing shapes are unchanged.
- **A normal DNA migration/cutover IS required** because Holochain cells are
  keyed by DNA hash.

Pass-5/v2.0.0 (the migration SOURCE generation):

```text
pass-5 DNA:      uhC0k2dXMIa1yI-V4ibCWMiTY5G6-p0laq6IOAVQ2F8XXReDHSxyS
pass-5 hApp SHA: 42dbf9df56d88269f629651c1253d31bd2e5a664f3bdf44fe66256345034d361
pass-5 filename: humm-earth-core-happ_pass-5-owner-role_dna-uhC0k2dX_happ-42dbf9df.happ
```

---

## What pass-6 changes

Coordinator hot-swappable modules were split first:

- `encrypted_content/migration.rs` →
  `encrypted_content/migration/{mod,markers,payload,readers,writers,tests}.rs`
- `encrypted_content/signals.rs` →
  `encrypted_content/signals/{mod,content,dm,outbound,tests}.rs`

Integrity modules were split as the pass-6 DNA candidate:

- `encrypted_content.rs` →
  `encrypted_content/{mod,types,entry_validation,links/**,tests/**}.rs`
- `hive.rs` → `hive/{mod,types,authority,membership,owner,tests}.rs`
- `group.rs` → `group/{mod,types,authority,membership,links,tests}.rs`
- validation dispatch moved from `lib.rs` into
  `validation_dispatch/{mod,entry,links,activity}.rs`

Follow-up BLOCK fixes after security/Holochain review:

- `OriginalHashPointer` create/delete validation now binds pointer links to
  `EncryptedContent` action hashes, authors, and the native update-chain root.
- Coordinator `update_encrypted_content` derives the original root from native
  action metadata instead of trusting network `OriginalHashPointer` link `[0]`.
- Update validation now rejects cross-entry-type app updates before dispatching
  to per-entry validators.

Test harness updates:

- Shared Sweettest setup moved to `crates/sweettest/tests/support/mod.rs`.
- New conductor test `recipient_witnesses.rs` proves HiveGroup
  `RecipientWitness` fetch validation accepts a real `GroupMembership`.
- `owner_and_acl.rs` covers the fetch-dependent hive grant-window containment
  rejection.

---

## Wire/API compatibility

Pass-6 preserves the pass-5 wire/API contract:

- `EntryTypes` order unchanged.
- `LinkTypes` order unchanged.
- `AclSpec`, `Acl`, `HiveMembership`, owner-handoff entries, invite entries, and
  migration marker wire fields unchanged.
- Serde tags unchanged.
- Entry visibility unchanged, including private `DmProbeLog`.
- Coordinator extern names and call signatures unchanged.
- `recv_remote_signal` decode order unchanged: content signal first, DM signal second.

Compatibility limit: unchanged wire shapes do **not** mean same DNA. The integrity
WASM bytes changed, so Holochain computes a new DNA hash and existing pass-5 cells
cannot gossip with pass-6 cells.

---

## Artifacts and hashes

Generated from local `dry-refactor` after the BLOCK fixes and rebuild:

```text
DNA hash:                 uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz
content_integrity.wasm:   2656a9100937f7e6d17e2eebd5e744a1ef16e8e36b0efa089dc2f6382a655ae2
content.wasm:             58b1d85f3d57c2fffeccd39c2a9aab602761ce47519ee626def6ae05384a94af
humm_earth_core.dna:      0fd059306479e0500a2fb36bd4614c7a5b803576fee3fc7f3cda490d4e1d3600
humm-earth-core-happ.happ 3062de3851eac81fedd425325b30f3cabaaa2000e1e295ba7db5d4d031dda5d3
```

Published 2026-07-02 (all copies sha256-verified `3062de38…`; a clean
`nix develop` rebuild at the zome-source tip `a07dc99` reproduced every hash,
including `hc dna hash` → `uhC0ksXs…`):

```text
~/hummhive-official-happ-versions/humm-earth-core-happ_pass-6-dry-refactor_dna-uhC0ksXs_happ-3062de38.happ
humm-tauri/.testdata/happs/…  (both clones; MANIFEST row parked ABOVE pass-5 — see PASS_6_DEPLOY_HANDOFF.md)
```

---

## Downstream constants and fixtures

Now that pass-6 is promoted:

1. Add only the current pass-6 hash pair:
   - DNA `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz`
   - hApp `3062de3851eac81fedd425325b30f3cabaaa2000e1e295ba7db5d4d031dda5d3`
2. Still NEVER add constants/fixtures for the withdrawn pre-fix candidate.
3. The `.testdata/happs/MANIFEST.tsv` pass-6 row is deliberately NOT last;
   flipping it last (= `provisionFromManifest.currentGenerationRow()`) is the
   first deliberate cutover step — see the deploy handoff checklist.

Historical artifacts remain append-only. Do not remove pass-5, pass-4, rescue,
or older records while adding pass-6 references.

---

## Migration/cutover

Per [`PASS_6_DEPLOY_HANDOFF.md`](./PASS_6_DEPLOY_HANDOFF.md):

1. Keep pass-5 artifacts available.
2. Install pass-6 as a new DNA/cell generation.
3. Run the existing export → import → `mark_migrated_v2` migration pipeline from
   the pass-5 cell to the pass-6 cell.
4. No data transform is expected for pass-6 because schemas are unchanged.
5. The migration still matters because the DNA hash changed.
6. Do not hot-swap pass-6 into a pass-5 cell with `updateCoordinators`.

---

## Verification already run

Hash and behavior proof live in `.baseline-hashes.txt` under
`Pass-6 dry-refactor`. That block is authoritative for the blessed pass-6 build
and explicitly supersedes the withdrawn pre-fix candidate above. Reproduction
re-verified at blessing time (2026-07-02): a clean rebuild at zome-source tip
`a07dc99` reproduced all hashes below byte-identically.

From `/home/aphix/humm-earth-core-happ` on `dry-refactor`:

```text
cargo fmt --all --check                                    green
cargo test -p content_integrity --lib                      76/76 green
cargo test -p content --lib                                25/25 green
cargo clippy --workspace --all-targets -- -D warnings      green
nix develop --command bash scripts/build-zomes.sh           green
nix develop --command hc dna pack dnas/humm_earth_core/workdir
                                                               green
nix develop --command hc app pack workdir --recursive       green
sha256sum target/wasm32-unknown-unknown/release/content_integrity.wasm
  -> 2656a9100937f7e6d17e2eebd5e744a1ef16e8e36b0efa089dc2f6382a655ae2
sha256sum target/wasm32-unknown-unknown/release/content.wasm
  -> 58b1d85f3d57c2fffeccd39c2a9aab602761ce47519ee626def6ae05384a94af
sha256sum dnas/humm_earth_core/workdir/humm_earth_core.dna
  -> 0fd059306479e0500a2fb36bd4614c7a5b803576fee3fc7f3cda490d4e1d3600
sha256sum workdir/humm-earth-core-happ.happ
  -> 3062de3851eac81fedd425325b30f3cabaaa2000e1e295ba7db5d4d031dda5d3
hc dna hash dnas/humm_earth_core/workdir/humm_earth_core.dna
  -> uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz
crates/sweettest cargo test -- --test-threads=1            12 active green + 1 ignored dormancy differential
```

Reviewer lanes:

- Initial Rust/security/silent-failure/DRY lanes completed before the follow-up
  Holochain/security pass.
- Follow-up reports found the two BLOCK issues listed above.
- Both BLOCK issues are fixed in the blessed build and re-gated by the
  verification commands above.

---

## Companion validation docs

- [`HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md`](./HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md)
  remains the humm-tauri behavior catalogue for commit-time guarantees. Its DNA
  header now names pass-6; the scenario list is intact because pass-6
  deliberately preserves those scenarios, wire shapes, and every pre-existing
  reject string (verified strict superset).
- [`HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md`](./HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md)
  remains the owner-role UI/API contract (introduced by pass-5, unchanged by
  pass-6).
- [`.baseline-hashes.txt`](../.baseline-hashes.txt) is the authoritative hash
  ledger. It marks the withdrawn candidate and records the blessed pass-6
  build.
- [`POSTCOMPACTION.md`](../POSTCOMPACTION.md) is the current branch state.

---

## Humm-tauri action items now

1. Follow the cutover checklist in
   [`PASS_6_DEPLOY_HANDOFF.md`](./PASS_6_DEPLOY_HANDOFF.md) (manifest flip,
   provision, constants, bundled-bin assertions, lineage plumbing, migration).
2. Still ignore the withdrawn pre-fix pass-6 hash (`3dcb8827…`).
3. Run the same pass-5 UI/API scenarios against pass-6; behavior must match
   because the wire contract is unchanged — report any mismatch as a pass-6
   bug.

Reference docs:

- Pass-6 cutover runbook: [`PASS_6_DEPLOY_HANDOFF.md`](./PASS_6_DEPLOY_HANDOFF.md)
- Pass-5 (migration source) deploy handoff: [`PASS_5_DEPLOY_HANDOFF.md`](./PASS_5_DEPLOY_HANDOFF.md)
- Owner-role UI/API contract: [`HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md`](./HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md)
- Commit-time BDD catalogue: [`HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md`](./HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md)
- Pass-6 hash record: [`.baseline-hashes.txt`](../.baseline-hashes.txt)
- Current branch/docs state: [`POSTCOMPACTION.md`](../POSTCOMPACTION.md)
