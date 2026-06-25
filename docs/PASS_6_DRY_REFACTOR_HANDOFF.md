# Pass-6 dry-refactor handoff — humm-tauri integration

**Date:** 2026-06-24  
**Audience:** humm-tauri developers  
**Branch:** `dry-refactor`  
**Change class:** integrity-zome structural refactor — **DNA changes**  
**Release status:** candidate only; not pushed, tagged, copied to official versions, or bundled by humm-tauri

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

- **Do not bundle pass-6 yet.** This is a candidate branch for review/testing only.
  `main` / v2.0.0 / pass-5 remains the current humm-tauri integration target.
- **Current pass-6 candidate DNA:**
  `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz`.
- **Current pass-6 candidate hApp SHA:**
  `3062de3851eac81fedd425325b30f3cabaaa2000e1e295ba7db5d4d031dda5d3`.
- **Never use `updateCoordinators` for pass-6.** This is an integrity-zome edit,
  so it is a new DNA/cell generation even though wire shapes are unchanged.
- **No UI/API code change is expected for pass-6 itself** because extern names,
  entry fields, serde tags, and TypeScript-facing shapes are unchanged.
- **A normal DNA migration/cutover is still required if pass-6 is promoted**
  because Holochain cells are keyed by DNA hash.

Pass-5/v2.0.0 remains current:

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

## Current candidate artifacts and hashes

Generated from local `dry-refactor` after the BLOCK fixes and rebuild:

```text
DNA hash:                 uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz
content_integrity.wasm:   2656a9100937f7e6d17e2eebd5e744a1ef16e8e36b0efa089dc2f6382a655ae2
content.wasm:             58b1d85f3d57c2fffeccd39c2a9aab602761ce47519ee626def6ae05384a94af
humm_earth_core.dna:      0fd059306479e0500a2fb36bd4614c7a5b803576fee3fc7f3cda490d4e1d3600
humm-earth-core-happ.happ 3062de3851eac81fedd425325b30f3cabaaa2000e1e295ba7db5d4d031dda5d3
```

The generated candidate bundle currently lives only in the earth-core workdir:

```text
/home/aphix/humm-earth-core-happ/workdir/humm-earth-core-happ.happ
```

It has **not** been copied into `~/hummhive-official-happ-versions/` and has
**not** been mirrored into humm-tauri. Do not mirror or distribute it until the
team explicitly promotes pass-6 from candidate to release.

---

## Downstream constants and fixtures

Until promotion:

1. Keep humm-tauri pointed at pass-5/v2.0.0.
2. Do not add constants for the withdrawn pre-fix candidate.
3. Do not add constants for the current pass-6 candidate unless the team chooses
   an explicit local test lane.
4. If testing locally, use scratch fixtures only; do not replace pass-5 in
   `.testdata`, `src-tauri/bin`, or official-version manifests.
5. If promoted, add only the current pass-6 hash pair:
   - DNA `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz`
   - hApp `3062de3851eac81fedd425325b30f3cabaaa2000e1e295ba7db5d4d031dda5d3`

Historical artifacts remain append-only. Do not remove pass-5, pass-4, rescue,
or older records while adding pass-6 references.

---

## Migration/cutover implication if promoted

If pass-6 is adopted later:

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
`Pass-6 dry-refactor`. That block is authoritative for the current candidate and
explicitly supersedes the withdrawn pre-fix candidate above.

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
- Both BLOCK issues are fixed in the current candidate and re-gated by the
  verification commands above.

---

## Companion validation docs

- [`HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md`](./HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md)
  remains the humm-tauri behavior catalogue for commit-time guarantees. Its DNA
  header still names pass-5 because pass-5 is current; pass-6 deliberately
  preserves those scenarios and wire shapes. If pass-6 is promoted, update that
  header/hash but keep the scenario list intact unless behavior actually changes.
- [`HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md`](./HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md)
  remains the current pass-5 UI/API cutover contract.
- [`.baseline-hashes.txt`](../.baseline-hashes.txt) is the authoritative hash
  ledger. It marks the withdrawn candidate and records the current pass-6
  candidate.
- [`POSTCOMPACTION.md`](../POSTCOMPACTION.md) is the current branch state.

---

## Humm-tauri action items now

Required now:

1. Ignore the withdrawn pre-fix pass-6 hash.
2. Keep integrating against pass-5/v2.0.0 (`42dbf9df`).
3. Do not bundle the current pass-6 candidate unless the team explicitly opens a
   candidate-test lane.

If testing pass-6 candidate locally:

1. Copy the current candidate into a scratch path only.
2. Label it candidate-only in any local manifest or notes.
3. Run the same pass-5 UI/API scenarios; behavior should match because the wire
   contract is unchanged.
4. Report any mismatch as a pass-6 candidate bug, not as a humm-tauri migration
   requirement.

Reference docs:

- Pass-5 current integration: [`PASS_5_DEPLOY_HANDOFF.md`](./PASS_5_DEPLOY_HANDOFF.md)
- Pass-5 UI/API contract: [`HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md`](./HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md)
- Commit-time BDD catalogue: [`HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md`](./HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md)
- Pass-6 candidate hash record: [`.baseline-hashes.txt`](../.baseline-hashes.txt)
- Current branch/docs state: [`POSTCOMPACTION.md`](../POSTCOMPACTION.md)
