# POSTCOMPACTION — humm-earth-core-happ

> Current-state-of-the-world for devs and agents picking up this repo.
> Read first after a compaction or at session start. Git log has full history.

---

## Current state

**Branch:** `main` at **v3.0.0** — pass-6 blessed 2026-07-02 and merged
(`dry-refactor` → merge `2de8923`, tag `v3.0.0` on the merge commit). Pass-6 =
structural DRY refactor (integrity + coordinator directory-module splits) plus
security validation hardening. Prior: v2.0.0 (pass-5 owner role + GroupGenesis
filter, DNA `uhC0k2dX…`, happ `42dbf9df…`) — now the migration SOURCE generation.

**Pass-6 DNA (the new frozen invariant):**
`uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz`.
Integrity wasm `2656a9100937f7e6d17e2eebd5e744a1ef16e8e36b0efa089dc2f6382a655ae2`,
content wasm `58b1d85f3d57c2fffeccd39c2a9aab602761ce47519ee626def6ae05384a94af`,
DNA bundle `0fd059306479e0500a2fb36bd4614c7a5b803576fee3fc7f3cda490d4e1d3600`,
happ `3062de3851eac81fedd425325b30f3cabaaa2000e1e295ba7db5d4d031dda5d3`.
**Published:** `~/hummhive-official-happ-versions/` (MANIFEST row
`pass-6-dry-refactor`, source commit `a07dc99`) + mirrored to
`humm-tauri/.testdata/happs/` in BOTH clones — the .testdata MANIFEST row is
deliberately parked ABOVE `pass-5-owner-role` (their
`provisionFromManifest.currentGenerationRow()` = last row; flipping it is their
cutover step 1). Reproduction re-verified at blessing: clean rebuild at
`a07dc99` reproduced all hashes byte-identically. NOT pushed to GitHub (user
pushes).

**Pass-6 numbering:** this build REPLACES the earlier pre-fix pass-6
candidate `uhC0kOQX5rU8yL6CIEWAfGu1G5TaNsgMcS7yp-D0fV2eG1-2bA7iJ`
(`happ 3dcb8827...`). That candidate is **WITHDRAWN / BAD / never
distribute** because security/Holochain review found the `OriginalHashPointer`
trust-boundary bug and cross-entry-type update bypass before it was adopted.
Because nobody is using that DNA, do not mint pass-7 and do not add downstream
constants or fixtures for the withdrawn hash.


**Pass-6 change shape:** no EntryTypes/LinkTypes variants were added, removed, or
reordered; no entry fields or serde tags changed. The DNA hash changes because
integrity source/WASM bytes changed during directory-module splits plus follow-up
validation hardening for `OriginalHashPointer` and same-entry-type updates.
Migration still uses the existing DNA migration path.

**Validation:** `cargo fmt --all --check` green; `cargo test -p content_integrity --lib`
= 76/76 green; `cargo test -p content --lib` = 25/25 green; `cargo clippy
--workspace --all-targets -- -D warnings` green; Sweettest after rebuild = 12/12
active green + 1 ignored dormancy differential. Follow-up security/Holochain
BLOCK findings C-BLOCK-1 and C-BLOCK-2 were fixed and re-gated.
Blessing verification (2026-07-02): reject-string contract vs pass-5 checked —
integrity literals a strict superset (zero removals); coordinator lost only the
two old pointer-path error strings (unmatched in humm-tauri, grep-verified),
gained three native update-chain errors.

**SECURITY — documented, accepted residual:** owner transfer is NOT final
against a malicious PAST owner — any past owner can fork the lineage to
re-seize ownership (irreducible cross-chain double-spend; confirmed by
security review + oracle). Blast radius = GOVERNANCE only (Admin-grant,
revoke-protect, owner UI), NOT content decryption. Mitigation =
deterministic resolution + fork detection (`is_ownership_contested`) +
honest docs.

**Pass-4 status:** v1.0.0 (pass-4-query-tolerance, DNA `uhC0k26b`, happ
`2205337c`) remains the prior production release tag. v1.0.1
(pass-4-migration-rescue, DNA `uhC0k26b`, happ `ca1b4225`) is the
coordinator hot-swap staged for any still-dormant live `@4` cell. v2.0.0 (pass-5,
DNA `uhC0k2dX`, happ `42dbf9df`) is what humm-tauri currently bundles and runs
(their `src-tauri/bin` sha `42dbf9df`, live pass-4→5 migration verified
2026-07-01 on their side). v3.0.0 (pass-6) is the next cutover target.


## Outstanding follow-ups

1. **User pushes to GitHub** — mount `main` + `dry-refactor` + tag `v3.0.0` are
   local-only (assistant never pushes).
2. **humm-tauri pass-5→pass-6 cutover** (their team) — full runbook in
   `docs/PASS_6_DEPLOY_HANDOFF.md`. mbox sent 2026-07-02; marker-extern /
   EntryTypes / no-rescue confirmations replied with artifact evidence.
   **2026-07-03: live validation COMPLETE on the v3.0.0 canary** (GUI + 2
   relays, cache-off): DM 2×2, invite loop on `@6`, all 4 upload scopes,
   byte-exact media; ZERO DNA-side issues; remaining findings app-side
   (their `.newTasks/…/13_HeadlessMigrationFindings.md` @ `0af39311`).
   Pass-7 consideration captured: stable cross-generation content identity
   (`docs/HUMM_TAURI_PASS_ROADMAP.md` §Pass-7 candidate considerations).
3. **pass-4→pass-5 migration** for straggler hives — `migrate-dna.ts` skips
   Owner grants (lineage-conferred); direct 4→6 is unexercised — chain 4→5→6
   or validate first (deploy handoff §Straggler `@4` hives).
4. **Review WARN follow-ups** (non-blocking) — `docs/sec-holo-review/findings-catalog.md`
   C-WARN-1..7 + open decision points.

---

## Environment

- **Linux `~/humm-earth-core-happ`** — authoritative. ALL dev/build/test here.
- **Windows `/mnt/c/proj/github/hummhive/humm-earth-core-happ`** — ff-merge target (harness cwd).
- **WSL sync:** `scripts/wsl-pull.sh` / `wsl-push.sh` / `wsl-check.sh`. See `CLAUDE.md`.
- **Toolchain:** holochain/hc 0.6.1, hdi 0.7.1, hdk 0.6.1 (pinned exact), Node 24,
  nix (holonix main-0.6 @ 0.6.1, rustc 1.94). `.baseline-hashes.txt` = repro contract.
- **Build (reproducible):** `nix develop --command bash scripts/build-zomes.sh`, then
  `nix develop --command hc dna pack dnas/humm_earth_core/workdir`, then
  `nix develop --command hc app pack workdir --recursive`; `hc dna hash …` MUST print
  `uhC0ksXs…` on `main` (v3.0.0/pass-6). Pass-5/v2.0.0 was `uhC0k2dX…`.
- **Tests:** host `cargo test -p content --lib` (25) + `-p content_integrity --lib` (76).
  Conductor: `crates/sweettest` (in-process, iroh). **Tryorama CANNOT boot on
  hc 0.6.x** — do not use it.

## Conductor testing (crates/sweettest)

- Separate Cargo workspace (the conductor crate's dep tree stays out of the lean
  zome workspace; both now pin HSB `=0.0.57`). holochain rev `3bdeacc` (0.6.1),
  transport **iroh** (`transport-iroh`; tx5/datachannel dropped in 0.6.1) — the
  devShell provides `openssl` + `pkg-config`; RustCrypto pinned to holochain's RCs.
- Run: `cd crates/sweettest && nix develop ../.. --command bash -c 'export LIBCLANG_PATH=<nix clang lib dir>; cargo test -- --test-threads=1'`
  (`LIBCLANG_PATH` e.g. `/nix/store/…clang-18.1.8-lib/lib`).
- **12/12 active green on the pass-6 build (now `main`/v3.0.0)** (coordinator_cleanup 2,
  coordinator_query_tolerance 2, owner_and_acl 4, migration_rescue 3 active +1
  ignored, recipient_witnesses 1). First compile slow (conductor + wasmer + iroh).

## Other branches (committed; pass-6 now landed on main)

| Branch | Tip | What |
|---|---|---|
| `dry-refactor` | `2bc4740` | **MERGED → main as v3.0.0** (this landing): pass-6 DRY refactor + validation hardening — DNA-forked `uhC0ksXs` |
| `feat-pass1-coordinator-marker-v2` | `28d7012` | pass-1 coordinator hot-swap fixture (marker-v2) for humm-tauri pass1→2 e2e |
| `feat-integrity-pass-5-owner-role` | `e1a55a5` | MERGED → main as v2.0.0: owner role + reader read-only + 0.6.1 — DNA-forked `uhC0k2dX` |
| `fix-coordinator-pass4-cleanup` | `0196d23` | pass-4 coordinator cleanup (pass-5 branched off it) |
| `feat-integrity-pass-4-recipient-witnesses` | `8503b48` | Pass-4 integrity (G-6.2 witnesses) |

## Constraints

- NEVER push/merge to origin without explicit user instruction.
- NEVER edit `humm-tauri/**` except `.testdata` (when explicitly authorized).
- NEVER run cargo/npm from the Windows mount.
- Append-only for EntryTypes/LinkTypes enums (index stability); integrity changes fork the chain.
- Commit identity: `Mike <mike@hummhive.com>` (repo-local).

## Gotchas

- **A Cargo version bump changes content.wasm** (embedded `CARGO_PKG_VERSION`
  survives wasm-opt strip) → new happ sha. Keep crate versions stable to preserve a
  released happ; the release identity is the git tag + DNA hash + happ sha, not the crate version.
- Bumping the **integrity** crate version risks the integrity wasm sha → DNA hash → chain fork. Leave it frozen.
- Sweettest needs `LIBCLANG_PATH` (see above); tryorama can't boot on hc 0.6.x.
- AdminWebsocket 400 → pass `wsClientOptions: { origin: "<anything>" }`.
- Two agents, one conductor: same `network_seed` → shared DHT → offline cross-agent validation.
- Reproducibility requires `nix develop` (`wasm-opt`) + `codegen-units = 1`.
- **Editing the integrity crate forks the DNA.** Pass-6 did this INTENTIONALLY
  (new DNA `uhC0ksXs`). Going forward, pass-6's integrity wasm `2656a910…` +
  DNA `uhC0ksXs…` are the frozen invariant on `main` — hold them byte-identical;
  coordinator hot-swaps are free (content.wasm may change). rustc embeds
  `#[track_caller]` line numbers, so ANY integrity edit shifts the wasm sha —
  only allowed for the next sanctioned pass.

## Key references

- Codemaps: `docs/CODEMAPS/` · Agent toolkit: `AGENTS.md` + `.claude/` · Session brief: `CLAUDE.md`
- Conductor tests: `crates/sweettest/README.md` · Reproducibility: `.baseline-hashes.txt`
- Build: `scripts/build-zomes.sh` + `scripts/strip-wasms.sh`
- Official happ binaries: `~/hummhive-official-happ-versions/` + `MANIFEST.tsv` (mirrored in `humm-tauri/.testdata/happs/`)
- Handoffs: `docs/PASS_6_DEPLOY_HANDOFF.md` (pass-6 cutover runbook) + `docs/PASS_6_DRY_REFACTOR_HANDOFF.md` (pass-6 detail); `docs/_archive/PASS_5_DEPLOY_HANDOFF.md` + `docs/HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md` (pass-5 owner role); `docs/_archive/PASS_4_DEPLOY_HANDOFF.md`, `docs/HUMM_TAURI_*` (recv-signal / SharedSecrets / content-type+witness / acl_spec-mutation / roadmap)
