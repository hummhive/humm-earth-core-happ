---
name: update-docs-workflow
description: The full documentation-freshness pass for humm-earth-core-happ — run after a feature lands, before a release/merge to main, or whenever docs may have drifted. Audits stale stack/version mentions, reconciles .newTasks/.doneTasks, refreshes the codemaps, and verifies the DNA-hash / hApp-sha / version pins against reality. Use as the closing step of standard-workflow.
---

# Update-Docs Workflow

Keep the durable docs honest with the code. Run the lanes below (parallel where
independent); each is read-mostly until it has a concrete edit to make. Docs are
NEVER a build gate — but stale docs mislead the next session and the humm-tauri
devs who read the `HUMM_TAURI_*` handoffs, so treat drift as a defect.

## Lane 1 — stack / version mention audit

`search` the docs (`docs/`, `CLAUDE.md`, `AGENTS.md`, `POSTCOMPACTION.md`,
`README.md`) for stale references: toolchain versions (holochain/hc 0.6.0, hdi
0.7.0, hdk 0.6.0, holonix main-0.6, Node), pinned DNA hash, happ shas, pass/gen
labels, script names, and any "current = pass-N" claims. Reconcile every mention
against the actual `Cargo.toml` pins, `flake.nix`, `.baseline-hashes.txt`, and the
official-versions `MANIFEST.tsv`. Fix or delete anything that no longer holds.

## Lane 2 — task-tracker reconciliation

Walk `.newTasks/`: any spec whose work is FULLY shipped (built + reviewed + gated +
distributed) gets `git mv`'d to `.doneTasks/`; any spec that drifted from reality
gets corrected or dropped. `POSTCOMPACTION.md` "what's on this branch" + "recent
session" must match the real commit log — no phantom or missing items.

## Lane 3 — codemaps + pins

Run `/update-codemaps` (or the `doc-updater` agent) to regenerate
`docs/CODEMAPS/{architecture,backend,data,dependencies}.md`. Diff <30% → update in
place; >30% → surface the diff for approval. Then verify the **pins** are real,
not aspirational:
- DNA hash in docs == `hc dna hash` of the freshly packed DNA.
- hApp sha(s) in docs / MANIFEST == `sha256sum workdir/*.happ` of the current build.
- `.baseline-hashes.txt` integrity-wasm sha unchanged for the line (else a chain
  fork slipped in — STOP and investigate).
- official-versions `MANIFEST.tsv` and `humm-tauri/.testdata/happs/MANIFEST.tsv`
  carry a row for the current happ.
Refresh the freshness header on each codemap and append a dated section to
`.reports/codemap-diff.txt`.

## Lane 4 — dependency / build reality + the full ladder

If deps moved (`Cargo.lock`, `flake.nix`, `tests/package.json`): record what
changed and why in the relevant doc. Then run the FULL verification ladder (this
is the release-grade gate, not the day-to-day one):
- `cargo test -p content --lib` + `cargo test -p content_integrity --lib`
- `cargo check` / `cargo clippy` — zero warnings
- the `crates/sweettest/` in-process conductor suite (the conductor-test path;
  tryorama can't boot on hc 0.6.0)
- reproducible build:
  `nix develop --command bash -c 'bash scripts/build-zomes.sh && hc dna pack dnas/humm_earth_core/workdir && hc app pack workdir --recursive'`,
  then verify the DNA hash is held and capture the wasm/happ shas.
(There is no pnpm/eslint/tsc/vitest ladder here — that is humm-tauri's stack.)

## Failure modes to avoid

- **Wrong-tree trap (WSL).** Build/test/verify ONLY in `~/humm-earth-core-happ/`
  (the authoritative Linux clone); never run `cargo`/build from `/mnt/c/...`. A
  cross-tree build silently corrupts `target/` and produces non-reproducible
  hashes. Sync the mirrors with `scripts/wsl-*.sh`.
- **Docs gated by accident.** Docs freshness never blocks a build; but a stale
  DNA hash / happ sha / "current pass" claim IS a defect — fix it.
- **History vs current.** Codemaps and POSTCOMPACTION describe the CURRENT state;
  the per-pass history belongs in git + `.baseline-hashes.txt`, not duplicated
  into the codemaps.

## Closing

Update `POSTCOMPACTION.md`, sync the mirrors (`scripts/wsl-push.sh`), and leave
the tree clean. Never push to origin or cut a tag without explicit instruction.
