# POSTCOMPACTION — humm-earth-core-happ

> Current-state-of-the-world for devs and agents picking up this repo.
> Read first after a compaction or at session start. Git log has full history.

---

## Current state

**Release:** `main` carries **v1.0.0** (tag at `db2a264`) = coordinator gen
**pass-4-query-tolerance**, hApp `2205337c`, DNA
`uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV` (held).
`feat-self-notes-architecture` merged `--no-ff` into `main` + tagged `v1.0.0`.
**HEAD = N1 griefing fix** (`697fde0`, coordinator-only) + a `chore(baseline)`
commit recording its hashes — NOT a release. Earlier commits on top of v1.0.0:
`1dee7f3` fmt (coordinator + sweettest) · `25ad4df` clippy-clean (workspace
`-D warnings`) · `83ec309`/`b3ed816` baseline + fmt-safety docs · `5c2cdab` docs.
Current coordinator content.wasm → `0538f18f`, happ → `bdefd0b2`; **integrity wasm
`06b01fb3` + DNA `uhC0k26b` HELD** (integrity frozen — clippy lints suppressed via
`content_integrity/Cargo.toml [lints]`, NEVER source-edited; see Gotchas +
`.baseline-hashes.txt`). The v1.0.0 tag (`db2a264`) + distributed happ (`2205337c`)
are UNCHANGED; none of these cleanup/fix happs are distributed.
**Pushed to GitHub:** `db2a264` + tag `v1.0.0`. **main is 8 commits ahead** (docs +
cleanup + N1 fix) — user `git push origin main` pending. Assistant never pushes.

**Active branch:** `fix-coordinator-pass4-cleanup` (UNMERGED) — coordinator-only
delete-link cleanup + dead time-index removal + `get_messages_since` resync-doc fix.
content.wasm `6df18693`, happ `8e8d3773`; integrity wasm `06b01fb3` + DNA `uhC0k26b`
HELD (hot-swap, no fork). Awaiting review/merge.

**DNA:** pass-4, frozen. Integrity wasm `06b01fb3…` byte-identical across all
pass-4 coordinator gens; every coordinator change this session was a hot-swap
(DNA hash held → no chain fork).

**Coordinator gen lineage this session (all DNA uhC0k26b):**
- pass-4 FINAL `d74e5f2f` → recv-signal-fix `4aacd52f` (content.wasm cb51c376) →
  **query-tolerance `2205337c` (content.wasm 78f0602e) = v1.0.0** → clippy/fmt
  `bef54a1c` (content.wasm c2a2a2fa) → **N1 griefing fix `bdefd0b2` (content.wasm
  0538f18f) = main HEAD** → **cleanup (content.wasm 6df18693, happ 8e8d3773) on
  branch `fix-coordinator-pass4-cleanup`, UNMERGED**. All hot-swaps; integrity wasm
  + DNA held throughout.

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
- **Branch `fix-coordinator-pass4-cleanup` (coordinator-only, this session):**
  `delete_encrypted_content` now sweeps the deleting agent's own discovery
  CreateLinks (Hive / Dynamic / HummContentId / HummContent*) targeting the
  tombstoned entry — a self-scoping local-chain query (a foreign I-A delete is a
  no-op; residue cleaned only by the original author) — closing the
  `// TODO: delete links` gap + the C3 count over-count. Dead time-index surface
  removed (the permanent-stub `get_encrypted_content_by_time_and_author` + orphan
  `time_indexed_links.rs` + its cap-grant + mod re-exports + the commented
  `time_indexing` dep); integrity `TimePath`/`TimeItem` variants LEFT in place
  (removing forks the DNA). `get_messages_since` doc corrected (since_seq=0 = full
  replay; the old `u32::MAX`-wraps claim was false — `saturating_add` doesn't wrap).
  Stale "alerts all agents in all hives" signal comments removed (code is
  ACL-scoped). Verified: clippy `-D warnings` clean, 27+69 host tests green,
  **Sweettest 2/2** (`crates/sweettest/tests/coordinator_cleanup.rs`), wasm rebuilt
  → DNA + integrity wasm HELD byte-identical, content.wasm `6df18693` / happ
  `8e8d3773`. The integrity `Cargo.toml [lints.clippy]` gained
  `uninlined_format_args = "allow"` (manifest-level, wasm-neutral — same pattern as
  the clippy/fmt row). Deferred (optional, owner discretion): the B4.2-B4.4
  code-reduction dedups (marker-builder twins, hive inbox-walk, decode-policy) —
  kept this hot-swap focused on correctness.

## Outstanding follow-ups

1. **`git push origin main`** (user) — main is 8 commits ahead of GitHub (docs +
   clippy/fmt cleanup + N1 griefing fix); v1.0.0 tag already pushed. Assistant
   never pushes.
2. **Merge `fix-coordinator-pass4-cleanup`** + decide whether to cut a distributed
   gen — delete-cleanup is the first behavior change since v1.0.0 that downstream
   humm-tauri would want via a `.happ` swap + `COORDINATOR_WASM_VERSION` bump. The
   deferred B4.2-B4.4 dedups can ride a later low-risk PR.

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
