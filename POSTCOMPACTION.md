# POSTCOMPACTION — humm-earth-core-happ

> Current-state-of-the-world for devs and agents picking up this repo.
> Read first after a compaction or at session start. Git log has full history.

---

## Current state

**Release:** `main` carries **v1.0.0** (tag at `db2a264`) = coordinator gen
**pass-4-query-tolerance**, hApp `2205337c`, DNA
`uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV` (held).
`feat-self-notes-architecture` merged `--no-ff` into `main` + tagged `v1.0.0`.
**HEAD `b3ed816`** = clippy/fmt cleanup (NOT a release), 4 commits on top of v1.0.0:
`1dee7f3` cargo fmt (coordinator + sweettest only) · `25ad4df` clippy-clean (workspace
`-D warnings`) · `83ec309` baseline row · `b3ed816` fmt-safety note. Coordinator
content.wasm → `c2a2a2fa`, happ → `bef54a1c`; **integrity wasm `06b01fb3` + DNA
`uhC0k26b` HELD** (integrity frozen — clippy lints suppressed via
`content_integrity/Cargo.toml [lints]`, NEVER source-edited; see Gotchas +
`.baseline-hashes.txt`). The v1.0.0 tag (`db2a264`) + distributed happ (`2205337c`)
are UNCHANGED; the cleanup happ (`bef54a1c`) is not distributed.
**Pushed to GitHub:** `db2a264` + tag `v1.0.0`. **main is 5 commits ahead** (docs +
cleanup) — user `git push origin main` pending. Assistant never pushes.

**DNA:** pass-4, frozen. Integrity wasm `06b01fb3…` byte-identical across all
pass-4 coordinator gens; every coordinator change this session was a hot-swap
(DNA hash held → no chain fork).

**Coordinator gen lineage this session (all DNA uhC0k26b):**
- pass-4 FINAL `d74e5f2f` → recv-signal-fix `4aacd52f` (content.wasm cb51c376) →
  **query-tolerance `2205337c` (content.wasm 78f0602e) = v1.0.0 (current)**.

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

## Outstanding follow-ups

1. **N1 — griefing bug in `update_encrypted_content`** (pre-existing, coordinator,
   DNA-safe; surfaced by rust review during the clippy cleanup — NOT a clippy warning,
   NOT yet fixed). In
   `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs`,
   `update_encrypted_content` does `original_hash_link[0].clone().target.into_action_hash().unwrap()`
   (two sites) which traps (wasm panic) on a non-ActionHash link target. Integrity
   validates `LinkTypes::OriginalHashPointer` as unconditionally `Valid` (no target-type
   / author check), so a remote peer can plant a poison link and the victim's next
   update traps. Severity P2 (availability/griefing; source-chain rolls back, no
   corruption). **Fix:** replace each `.unwrap()` with `let Some(ah) =
   …into_action_hash() else { return Err(wasm_error!(WasmErrorInner::Guest(
   "OriginalHashPointer target is not an ActionHash".to_string()))) };`, mirroring
   `probe_inbox`'s existing guard. Add a host test. Coordinator-only → content.wasm
   changes, DNA `uhC0k26b` must stay held; rebuild + confirm integrity `06b01fb3` +
   add a `.baseline-hashes.txt` row. Do NOT touch the integrity crate.
2. **`git push origin main`** (user) — `b3ed816`, 5 commits ahead of GitHub; v1.0.0
   tag already pushed.

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
