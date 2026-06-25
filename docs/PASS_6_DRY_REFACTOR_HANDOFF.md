# Pass-6 dry-refactor handoff — humm-tauri integration

**Date:** 2026-06-24
**Audience:** humm-tauri developers
**Branch:** `dry-refactor`
**Working tree:** local `dry-refactor` after security/Holochain BLOCK fixes
**Change class:** integrity-zome structural refactor — **DNA changes**
**Release status:** candidate only; not pushed, tagged, copied to official versions, or bundled by humm-tauri

Pass-6 is a structural DRY refactor of earth-core. It splits large Rust modules
into maintainable directory modules, adds conductor coverage for the fetch-dependent
recipient-witness and hive grant-window paths, and includes follow-up validation
hardening for the two security/Holochain BLOCK findings found during review. It
does **not** add or remove any extern, entry type, link type, serde tag, entry
field, or TypeScript-facing wire shape.

This is still an integrity-zome edit, so the DNA hash changes. Treat this like a
new DNA generation, not a coordinator hot-swap.

This handoff follows the existing pass handoff pattern in
[`PASS_5_DEPLOY_HANDOFF.md`](./PASS_5_DEPLOY_HANDOFF.md): short deploy TL;DR,
artifact/hash pins, companion docs, and explicit verification gates.

---

## TL;DR for humm-tauri

- **Do not bundle pass-6 yet.** This is a candidate branch for review/testing only.
  `main` / v2.0.0 / pass-5 remains the current humm-tauri integration target.
- **Current pass-5 remains intact and historical.** Keep every pass-5 record and
  artifact, especially:
  - DNA `uhC0k2dXMIa1yI-V4ibCWMiTY5G6-p0laq6IOAVQ2F8XXReDHSxyS`
  - hApp `42dbf9df56d88269f629651c1253d31bd2e5a664f3bdf44fe66256345034d361`
  - filename `humm-earth-core-happ_pass-5-owner-role_dna-uhC0k2dX_happ-42dbf9df.happ`
- **Pass-6 candidate DNA:** `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz`.
- **Pass-6 candidate hApp SHA:** `3062de3851eac81fedd425325b30f3cabaaa2000e1e295ba7db5d4d031dda5d3`.
- **No UI/API work is expected for pass-6 itself** because wire shapes and extern
  names are unchanged. Any adoption still needs a normal DNA migration/cutover
  because the DNA hash changed.
- **Never use `updateCoordinators` for pass-6.** It is not coordinator-only.

---

## What changed in earth-core

Coordinator hot-swappable modules were split first:

- `encrypted_content/migration.rs` → `encrypted_content/migration/{mod,markers,payload,readers,writers,tests}.rs`
- `encrypted_content/signals.rs` → `encrypted_content/signals/{mod,content,dm,outbound,tests}.rs`

Integrity modules were split as a sanctioned pass-6 DNA candidate:

- `encrypted_content.rs` → `encrypted_content/{mod,types,entry_validation,links/**,tests/**}.rs`
- `hive.rs` → `hive/{mod,types,authority,membership,owner,tests}.rs`
- `group.rs` → `group/{mod,types,authority,membership,links,tests}.rs`
- validation dispatch moved from `lib.rs` into `validation_dispatch/{mod,entry,links,activity}.rs`

Follow-up blocker fixes after security/Holochain review:

- `OriginalHashPointer` create/delete validation now binds pointer links to
  `EncryptedContent` action hashes, authors, and the native update-chain root.
- Coordinator `update_encrypted_content` derives the original root from native
  action metadata instead of trusting network `OriginalHashPointer` link `[0]`.
- Update validation now rejects cross-entry-type updates before dispatching to
  per-entry validators.

Test harness updates:

- Shared Sweettest setup moved to `crates/sweettest/tests/support/mod.rs`.
- New conductor test: `recipient_witnesses.rs` proves HiveGroup `RecipientWitness`
  fetch validation accepts a real `GroupMembership`.
- `owner_and_acl.rs` now covers the fetch-dependent hive grant-window containment
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

Because the integrity WASM bytes changed, Holochain still computes a new DNA hash.
That means pass-6 adoption requires installing a new cell and migrating data forward.

---

## Candidate artifacts and hashes

Generated from the local `dry-refactor` working tree after the follow-up BLOCK fixes:

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

## Historical artifact rule

Do not remove historical hApps, manifest rows, or hash documentation while adding
pass-6 candidate/latest references. Migration and customer-data recovery depend on
having every historical DNA generation available.

Pass-5/v2.0.0 is still the current humm-tauri integration release:

```text
pass-5 DNA:      uhC0k2dXMIa1yI-V4ibCWMiTY5G6-p0laq6IOAVQ2F8XXReDHSxyS
pass-5 hApp SHA: 42dbf9df56d88269f629651c1253d31bd2e5a664f3bdf44fe66256345034d361
pass-5 filename: humm-earth-core-happ_pass-5-owner-role_dna-uhC0k2dX_happ-42dbf9df.happ
```

The older `8f284777` pass-5 build remains documented as a deleted bad build; do
not resurrect it as a current bundle.

---

## Migration/cutover implication

If pass-6 is adopted later:

1. Keep pass-5 artifacts available.
2. Install pass-6 as a new DNA/cell generation.
3. Run the existing export → import → `mark_migrated_v2` migration pipeline from
   the pass-5 cell to the pass-6 cell.
4. No data transform is expected for pass-6 because schemas are unchanged, but the
   migration still matters because the DNA hash changed.
5. Do not hot-swap pass-6 into a pass-5 cell with `updateCoordinators`.

---

## Verification already run

Hash and behavior proof live in `.baseline-hashes.txt` under
`Pass-6 dry-refactor`; that block preserves the full pass-5/v2.0.0 history above it.

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
  oracle pass.
- Follow-up security/Holochain reports found C-BLOCK-1 (`OriginalHashPointer`)
  and C-BLOCK-2 (cross-entry-type updates); both are fixed in this working tree
  and re-gated by the verification commands above.

### Companion validation docs

- [`HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md`](./HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md)
  remains the humm-tauri behavior catalogue for commit-time guarantees. Its DNA
  header still names pass-5 because pass-5 is current; pass-6 deliberately preserves
  those scenarios and wire shapes. If pass-6 is promoted, update that header/hash
  but keep the scenario list intact unless behavior actually changes.
- [`HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md`](./HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md)
  remains the current pass-5 UI/API cutover contract.
- [`.baseline-hashes.txt`](../.baseline-hashes.txt) is the authoritative hash
  ledger. Use its pass-5 and pass-6 blocks to verify candidate artifacts; never
  remove older pass blocks.

---

## humm-tauri action items now

No required code change for pass-6 today.

Recommended:

1. Keep integrating against pass-5/v2.0.0 (`42dbf9df`) unless the team explicitly
   decides to test pass-6 candidate.
2. If testing pass-6 candidate, copy it into a local scratch/test path only; do not
   replace pass-5 in `.testdata`, `src-tauri/bin`, or official versions.
3. Treat pass-6 test results as candidate feedback, not release evidence.
4. Keep all historical hApp artifacts and mirrored hash docs complete.

Reference docs:

- Pass-5 current integration: [`PASS_5_DEPLOY_HANDOFF.md`](./PASS_5_DEPLOY_HANDOFF.md)
- Pass-5 UI/API contract: [`HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md`](./HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md)
- Commit-time BDD catalogue: [`HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md`](./HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md)
- Pass-6 candidate hash record: [`.baseline-hashes.txt`](../.baseline-hashes.txt)
- Current branch/docs state: [`POSTCOMPACTION.md`](../POSTCOMPACTION.md)
