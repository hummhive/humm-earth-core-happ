# POSTCOMPACTION — humm-earth-core-happ

> Current-state-of-the-world for devs and agents picking up this repo.
> Read first after a compaction or at session start. Git log has full history.

---

## Current state

**Release:** `main` carries **v1.0.1** (tag at merge `de7abd8`) =
pass-4-migration-rescue (coordinator hot-swap, no integrity bump) on top of
**v1.0.0** (tag at `db2a264`) = pass-4-query-tolerance. **DNA**
`uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV` HELD across the rescue
(coordinator-only change). v1.0.1 = `02151d3` (rescue base: `_local` twins +
`mark_migrated_v2` fail-soft) → `0ff26b3` (GroupGenesis filter fix) → `d980c51`
(reviewer SHOULDs: helper warn+None, exhaustive match, site-2 parity warn) →
`9a3a73d` (baseline shas), `--no-ff` merged at `de7abd8` preserving the rescue
block. Rescue happ artifact `ca1b4225…` already in
`~/hummhive-official-happ-versions/` (commit `9a3a73d`, MANIFEST row 15);
integrity wasm `06b01fb3…` byte-identical to v1.0.0 (only coordinator
rebuilt — hot-swap, no chain fork).
**Pushed to GitHub:** `db2a264` + tag `v1.0.0`. Main is ahead of GitHub by the
rescue block + merge commit + tag `v1.0.1`; user `git push origin main` +
`git push origin --tags v1.0.1` pending. Assistant never pushes.

**DNA:** pass-4, frozen. Integrity wasm `06b01fb3…` byte-identical across all
pass-4 coordinator gens; every coordinator change this session was a hot-swap
(DNA hash held → no chain fork).

**Coordinator gen lineage this session (all DNA uhC0k26b):**
- pass-4 FINAL `d74e5f2f` → recv-signal-fix `4aacd52f` (content.wasm cb51c376) →
  **query-tolerance `2205337c` (content.wasm 78f0602e) = v1.0.0** → clippy/fmt
  `bef54a1c` (content.wasm c2a2a2fa) → **N1 griefing fix `bdefd0b2` (content.wasm
  0538f18f) = current HEAD**. All hot-swaps; integrity wasm + DNA held throughout.

### What landed (merged to main as v1.0.0)

1. **recv_remote_signal ExternIO pre-encode fix** (coordinator) — all 5 send sites
   funnel through `send_encoded_remote_signal`/`remote_signal_payload`. Red→green
   host tests. (`docs/HUMM_TAURI_RECV_REMOTE_SIGNAL_FIX.md`.)
2. **pass-4-query-tolerance** (coordinator, Mike) — `get_many_encrypted_content`
   `filter_map(.ok())`; `list_my_hives`/`_groups` + `get_latest_membership`/
   `_group_membership` `.ok().flatten()` (cross-type Inbox + dangling targets no
   longer poison reads). Proven by `crates/sweettest` (2/2 green).
3. **Agent toolkit**: codemaps, CLAUDE.md, AGENTS.md, WSL sync scripts,
   `.claude/` (commands/agents/skills incl. standard-workflow + update-docs-workflow
   + reviewer agents).
4. **Integration handoff docs** (`docs/HUMM_TAURI_*`): self-notes, DM messaging,
   SharedSecrets public-ACL, content-type+witness, acl_spec mutation, pass roadmap.

### Recent session

- Answered + archived 4 mbox messages (DHT size cap 4,000,000 B; DirectMessage has
  no content_type constraint; non-member first entry = OpenWrite not Public;
  cross-hive >4MB = chunked DM entries; the list_my_hives wart is fixed in 2205337c).
- Cloned + pruned standard-workflow / update-docs-workflow skills + reviewer agents.
- Released v1.0.0: merged to main, tagged, full gate ladder green.
- Fixed N1 griefing bug at `697fde0` — `update_encrypted_content` now guards the
  `OriginalHashPointer` target via `original_pointer_action_hash`
  (`let Some(ah) = …into_action_hash() else { Err }`) instead of `.unwrap()`, so a
  poison non-ActionHash link returns a clean error rather than trapping the
  author's update. 2 host regression tests; coordinator hot-swap, DNA held,
  content.wasm `0538f18f` / happ `bdefd0b2`.

## Outstanding follow-ups

1. **`git push origin main` + `git push origin --tags v1.0.1`** (user) — main is
   ahead of GitHub by the rescue merge `de7abd8` + the 4 rescue commits + the
   `v1.0.1` tag; v1.0.0 tag already pushed. Assistant never pushes.
2. **Phases C–F still pending** for the pass-5 owner-role landing as v2.0.0
   (port the GroupGenesis fix to pass-5, merge fix→pass-5, merge pass-5→main,
   docs freshness). Tracked in the active todo + `local://pass5-main-landing-plan.md`.

---

## Environment

- **Linux `~/humm-earth-core-happ`** — authoritative. ALL dev/build/test here.
- **Windows `/mnt/c/proj/github/hummhive/humm-earth-core-happ`** — ff-merge target (harness cwd).
- **WSL sync:** `scripts/wsl-pull.sh` / `wsl-push.sh` / `wsl-check.sh`. See `CLAUDE.md`.
- **Toolchain:** holochain/hc 0.6.0, hdi 0.7.0, hdk 0.6.0 (pinned exact), Node 24,
  nix (holonix main-0.6). `.baseline-hashes.txt` = reproducibility contract.
- **Build (reproducible):** `nix develop --command bash -c 'bash scripts/build-zomes.sh && hc dna pack dnas/humm_earth_core/workdir && hc app pack workdir --recursive'`,
  then `hc dna hash …` MUST print `uhC0k26b…`.
- **Tests:** host `cargo test -p content --lib` (25) + `-p content_integrity --lib` (69).
  Conductor behavior: `crates/sweettest` (in-process). **Tryorama CANNOT boot on
  hc 0.6.0** (quic→webrtc CLI rename) — do not use it.

## Conductor testing (crates/sweettest)

- Separate Cargo workspace (holochain conductor needs sbt `=0.0.57` vs zomes `=0.0.56`).
- Run: `cd crates/sweettest && nix develop ../.. --command bash -c 'export LIBCLANG_PATH=<nix clang lib dir>; cargo test -- --test-threads=1'`.
  **Must set `LIBCLANG_PATH`** to a nix clang lib dir (e.g. `/nix/store/…clang-18.1.8-lib/lib`)
  or datachannel-sys bindgen falls back to the broken system clang-14 (missing libLLVM-14).
- First compile ~1.5-40 min (conductor + wasmer + libdatachannel). 2/2 green on v1.0.0.

## Other branches (committed, NOT merged to main)

| Branch | Tip | What |
|---|---|---|
| `feat-integrity-pass-4-recipient-witnesses` | `8503b48` | Pass-4 integrity (G-6.2 witnesses) — **DNA-changing** |
| `feat-migration-d1-group-track` | `aca142b` | D.1 migration tooling |
| `feat-coordinator-pass4-migration-rescue` | (this session) | pass-4 dormancy rescue: `list_my_hives_local` + `get_latest_membership_local` + `mark_migrated_v2` fail-soft. Coordinator-only, DNA `uhC0k26b…` HELD. |
| `test-tryorama-integrity-coverage` | `bf9fad8` | Old tryorama-free e2e harness (superseded by crates/sweettest) |

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
- Sweettest needs `LIBCLANG_PATH` (see above); tryorama can't boot on hc 0.6.0.
- AdminWebsocket 400 → pass `wsClientOptions: { origin: "<anything>" }`.
- Two agents, one conductor: same `network_seed` → shared DHT → offline cross-agent validation.
- Reproducibility requires `nix develop` (`wasm-opt`) + `codegen-units = 1`.
- **Editing the integrity crate forks the DNA** — even a `cargo fmt` reflow or an
  inserted `#[allow]` shifts rustc's embedded `#[track_caller]` panic-`Location` line
  numbers → integrity wasm sha bumps at identical byte length → DNA fork (proven this
  session). Integrity is frozen: NEVER fmt or source-edit it; suppress its clippy lints
  via `content_integrity/Cargo.toml [lints.clippy]` (zero codegen effect, survives
  `-D warnings`). Coordinator is free to fmt/fix (content.wasm may change). Workspace
  clippy is clean as of `25ad4df`. Safe fmt = `cargo fmt -p content` (coordinator only);
  NEVER `cargo fmt --all` (reflows integrity → fork). A `rustfmt.toml` `ignore` does NOT
  guard this — it is nightly-only (warns + no-ops on stable rustfmt), so don't bother.

## Key references

- Codemaps: `docs/CODEMAPS/` · Agent toolkit: `AGENTS.md` + `.claude/` · Session brief: `CLAUDE.md`
- Conductor tests: `crates/sweettest/README.md` · Reproducibility: `.baseline-hashes.txt`
- Build: `scripts/build-zomes.sh` + `scripts/strip-wasms.sh`
- Official happ binaries: `~/hummhive-official-happ-versions/` + `MANIFEST.tsv` (mirrored in `humm-tauri/.testdata/happs/`)
- Handoffs: `docs/PASS_4_DEPLOY_HANDOFF.md`, `docs/HUMM_TAURI_RECV_REMOTE_SIGNAL_FIX.md`, `docs/HUMM_TAURI_*` (SharedSecrets / content-type+witness / acl_spec-mutation / roadmap)
