# Pass-6 deploy handoff — humm-tauri integration

**Date:** 2026-07-02
**Audience:** humm-tauri developers
**Branch:** `dry-refactor` (zome-source tip `a07dc99`; merge to `main` + tag `v3.0.0` follows this handoff)
**Change class:** integrity-zome fork — **DNA changes**; NOT coordinator-hot-swappable
**Release status:** **BLESSED 2026-07-02** — published to `~/hummhive-official-happ-versions/` and mirrored to `humm-tauri/.testdata/happs/` (both clones)

Short-form runbook for swapping humm-tauri from pass-5 to pass-6. For the
refactor/verification detail see
[`PASS_6_DRY_REFACTOR_HANDOFF.md`](./PASS_6_DRY_REFACTOR_HANDOFF.md); for the
commit-time behavior catalogue see
[`HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md`](./HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md).

## TL;DR

- **DNA hash CHANGED** from
  `uhC0k2dXMIa1yI-V4ibCWMiTY5G6-p0laq6IOAVQ2F8XXReDHSxyS` (pass-5) to
  `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz` (pass-6).
  New cell generation: existing `@5` cells cannot gossip with `@6` cells, and
  `AdminWebsocket.updateCoordinators` does NOT apply — full happ swap + DHT
  migration required.
- **Why:** two security BLOCK findings fixed (see below) plus integrity
  directory-module splits. This closes the long-standing
  `OriginalHashPointer` trust-boundary hole.
- **NO UI/API cutover this time** — unlike pass-5, the wire/API contract is
  byte-preserved: extern names, call signatures, serde tags,
  `EntryTypes`/`LinkTypes` order, entry visibility (incl. private
  `DmProbeLog`), and `recv_remote_signal` decode order are all unchanged.
  Your work is constants + provisioning + lineage plumbing + the standard
  migration, itemized below.
- **Toolchain unchanged:** holochain 0.6.1 / hdk 0.6.1 / hdi 0.7.1 — the same
  pin humm-tauri already runs.

## The security fixes (what pass-6 actually closes)

1. **C-BLOCK-1 — `OriginalHashPointer` was unvalidated public DHT state.**
   Any agent could publish pointer links; integrity accepted them
   unconditionally, and coordinator `update_encrypted_content` trusted
   pointer-link `[0]` fetched from the network to locate the update root.
   Pass-6: integrity now validates pointer create/delete links (target must be
   the root Create of an `EncryptedContent`, link author must match the base
   entry author, base author must match the target root author, tag must be
   empty, deletes are author-only), and the coordinator derives the update
   root from **native action metadata** instead of network links.
2. **C-BLOCK-2 — cross-entry-type update bypass.** Update validation
   dispatched by the NEW entry's type, so a crafted update could route a
   different entry type through the `EncryptedContent` validator and bypass
   the immutable-entry validators (`HiveGenesis`, `HiveMembership`,
   `GroupGenesis`, `GroupMembership`, owner-handoff entries). Pass-6 rejects
   any app update whose original entry type differs, before per-entry
   dispatch.

Both were found by the three-lane security/Holochain review
(`docs/sec-holo-review/`), fixed, and re-gated. No exploitation is known; the
withdrawn pre-fix candidate (below) was never distributed.

## Artifacts

```text
Label:                    pass-6-dry-refactor
Source commit:            a07dc99 (dry-refactor; later branch commits are docs/config-only)
DNA hash:                 uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz
content_integrity.wasm:   2656a9100937f7e6d17e2eebd5e744a1ef16e8e36b0efa089dc2f6382a655ae2
content.wasm:             58b1d85f3d57c2fffeccd39c2a9aab602761ce47519ee626def6ae05384a94af
humm_earth_core.dna:      0fd059306479e0500a2fb36bd4614c7a5b803576fee3fc7f3cda490d4e1d3600
humm-earth-core-happ.happ 3062de3851eac81fedd425325b30f3cabaaa2000e1e295ba7db5d4d031dda5d3
Filename:                 humm-earth-core-happ_pass-6-dry-refactor_dna-uhC0ksXs_happ-3062de38.happ
```

Locations (all three sha256-verified identical on publish, 2026-07-02):

- `~/hummhive-official-happ-versions/` + `MANIFEST.tsv` row (appended last —
  that manifest is chronological).
- `humm-tauri/.testdata/happs/` in BOTH clones (`~/humm-tauri` and the
  Windows mount) + a `MANIFEST.tsv` row **deliberately placed BEFORE
  `pass-5-owner-role`** — see the row-position warning below.

**Reproducibility:** re-verified at blessing time — a clean
`nix develop` rebuild of the zome sources (byte-identical to `a07dc99`)
reproduced all five hashes above, including `hc dna hash` →
`uhC0ksXs…`. Verification gates at capture: fmt / clippy `-D warnings` /
integrity 76/76 / coordinator 25/25 / Sweettest 12/12 active green.

## ⚠ MANIFEST row-position warning (`.testdata/happs/MANIFEST.tsv`)

`provisionFromManifest.currentGenerationRow()` returns the **LAST data row**,
and `scripts/provision-happ.mjs` / `build.ps1` bake that row's happ into
`src-tauri/bin` (see your `8fab2409` regression fix). The pass-6 row is
therefore parked ABOVE `pass-5-owner-role` so nothing changes until you flip
it. **Flipping the row to LAST is the deliberate first step of the cutover**,
not a side effect of syncing `.testdata`.

## humm-tauri cutover checklist

Constants/plumbing (no wire-shape changes anywhere):

1. **Flip the manifest + provision.** Move the `pass-6-dry-refactor` row to
   last in `.testdata/happs/MANIFEST.tsv`, run `scripts/provision-happ.mjs`,
   confirm `src-tauri/bin/humm-earth-core-happ.happ` sha256 = `3062de38…`.
2. **`src-tauri/src/migration/flows.rs`** — add the pass-6 DNA constant
   (keep `PASS_5_DNA_HASH_BASE64` for source-side detection of `@5` cells,
   the same way pass-4's constant survived the pass-5 cutover):
   `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz`.
3. **`src-tauri/src/util/holochain/coordinator.rs`** — the bundled-bin
   pass-5 literal assertions (e.g. `bundled_dna_hash_is_pass_5_literal`)
   must become pass-6 expectations in the same change that flips the bin,
   or the build gate fails exactly as designed.
4. **`src-tauri/src/util/holochain/happ_install.rs`** — extend the version
   history doc (v8 = pass-5-owner-role → v9 = pass-6-dry-refactor, new DNA,
   full install not `update_coordinators`).
5. **Generalize the migration lineage plumbing** from its hardcoded
   pass-4→pass-5 shape to pass-5→pass-6: banner CTA copy ("Migrate to
   pass-5"), `pass4AliasHashes`-style suppression aliases,
   `filterUnmigratedPass4Hives`-analog detection for `@5` hives, and the
   post-import bootstrap (`_bootstrapMigratedHiveGroups` +
   `backfillBootstrapForKnownHives`) which should apply to `@6` identically.
6. **Optional:** `tests/e2e/paths.ts` — add `HAPP_PATH_PASS_6` for
   mixed-generation e2e (fixtures resolve by label/filename,
   position-independent).

Migration (standard pipeline, no data transform):

7. Run the existing `scripts/migrate-dna.ts` export → import →
   `mark_migrated_v2` pipeline from each `@5` cell to the new `@6` cell.
   Schemas are unchanged, so imports are 1:1; owner lineage carries via
   genesis/handoff replay exactly as in pass-4→5 (Owner grants stay
   lineage-conferred — the migration must NOT mint Owner `HiveMembership`
   grants, same rule as pass-5).
8. **Straggler `@4` hives:** the proven lanes are pass-4→5 (your live-verified
   flow + the v1.0.1 rescue coordinator for dormant `@4` cells) then 5→6.
   A direct 4→6 import is expected to work (pass-6 accepts all pass-5
   shapes) but has NOT been exercised — validate in your harness before
   relying on it.

## Reject-string contract (verified against `main`/pass-5)

- **Integrity strings: strict superset.** A literal-set diff of the integrity
  zome between pass-5 and pass-6 shows ZERO removals — every reject string in
  the BDD catalogue is byte-identical. Your existing failure-path assertions
  keep working.
- **New integrity rejections** (fire only on ops that were previously —
  wrongly — accepted): the `OriginalHashPointer` validation family
  (`"OriginalHashPointer target must be an ActionHash"`, `"… must be the root
  Create action"`, `"… must match the native update-chain root"`, `"… tag must
  be empty"`, author-binding variants, author-only delete) and the
  cross-type-update gate (`"Updates must preserve the original app entry
  type; create a new entry instead"`, `"Update original action must be an
  entry action"`).
- **Coordinator:** the rewritten update-root path replaced two old error
  strings (`"OriginalHashPointer target is not an ActionHash"`, `"Could not
  find the hash of the original EncryptedContent that is trying to be
  updated"`) with three new ones (`"Could not resolve EncryptedContent
  update-chain action"`, `"EncryptedContent update-chain action must be a
  Create or Update"`, `"Update-chain action does not reference an
  EncryptedContent"`). humm-tauri matches neither removed string at runtime
  (grep-verified 2026-07-02) — if you have local tooling matching them, remap.
- Coordinator extern behavior is otherwise unchanged: `update_encrypted_content`
  still accepts ANY revision hash as `previous` (root derivation is native-chain
  now, no longer pointer-link-dependent — strictly more robust).

## Withdrawn pre-fix candidate — NEVER distribute

An earlier pass-6 candidate was withdrawn before adoption when review found
the two BLOCKs above. **Do not bundle, track, or add constants for:**

```text
DNA:  uhC0kOQX5rU8yL6CIEWAfGu1G5TaNsgMcS7yp-D0fV2eG1-2bA7iJ
happ: 3dcb8827d7d45f3fabc68708862c4d379ed52d0b30f609ebed3f3b6dc8524d4e
```

No customers ever ran it, which is why the fixed build keeps the pass-6 name
instead of becoming pass-7.

## Residuals (unchanged from pass-5, disclosed)

- Owner transfer is not final against a malicious PAST owner (irreducible
  cross-chain fork re-seizure; governance blast radius only, never content
  decryption). Mitigation unchanged: deterministic resolution + fork
  detection via `is_ownership_contested` + honest UI copy.
- Review WARN items (coordinator `unwrap` hardening, `public_key_acl`
  fan-out bounds, discovery-link reindex-on-update, migration-artifact file
  permissions, stale legacy Tryorama harness, doc drift) are tracked
  repo-side in `docs/sec-holo-review/findings-catalog.md` — none block this
  release and none change downstream behavior.

## References

- Refactor/verification detail: [`PASS_6_DRY_REFACTOR_HANDOFF.md`](./PASS_6_DRY_REFACTOR_HANDOFF.md)
- Behavior catalogue (header now pass-6): [`HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md`](./HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md)
- Owner-role UI/API contract (unchanged by pass-6): [`HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md`](./HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md)
- Security review synthesis: [`sec-holo-review/findings-catalog.md`](./sec-holo-review/findings-catalog.md)
- Hash ledger: [`.baseline-hashes.txt`](../.baseline-hashes.txt)
- Migration tooling: [`../scripts/migrate-dna.ts`](../scripts/migrate-dna.ts) + [`DNA_MIGRATION_GUIDE.md`](./DNA_MIGRATION_GUIDE.md)
