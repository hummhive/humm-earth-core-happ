# POSTCOMPACTION — humm-earth-core-happ

> Current-state-of-the-world for devs and agents picking up this repo.
> Read first after a compaction or at session start. Git log has full history.

---

## Current state

**Active branch:** `feat-integrity-pass-5-owner-role` (UNMERGED, commit-local) —
the first integrity bump since pass-4. **DNA FORKED** to
`uhC0k2dXMIa1yI-V4ibCWMiTY5G6-p0laq6IOAVQ2F8XXReDHSxyS` (was pass-4 `uhC0k26b`).
Toolchain bumped to **holochain 0.6.1 / hdk 0.6.1 / hdi 0.7.1 / HSB 0.0.57**
(`nix flake update holonix`; sweettest transport `datachannel-vendored` → iroh).

**Pass-5 shipped (4 commits `f053570`→`046da6e`, + this docs commit):**
- `chore(build)`: 0.6.1 bump + iroh sweettest (RustCrypto RC pins matching
  holochain's lock; `await_consistency_s`; devShell + openssl/pkg-config). The
  only zome-facing 0.6.1 break was `GetOptions{strategy}` → `GetOptions::network()`.
- `feat(integrity)`: hive **Owner** role — `HiveOwnerHandoffOffer/Accept` entries
  + `AgentToOwnerHandoffs`/`HiveToOwnerHandoffs` links + `is_lineage_owner`
  induction; single owner, handshake-transferable, admin-undemotable. Reader
  **read-only** (variant-aware delete; reader dropped on non-DM). Role hardening:
  Owner not membership-grantable; only a lineage owner grants Admin; founder not
  re-castable. `delete_group_genesis` author-gated; `InviteRedemption`.
- `feat(coordinator)`: owner handshake externs + deterministic
  `resolve_current_owner` (offer-keyed fold, sort-before-bound,
  smallest-offer-hash tiebreak, fork detection) + `get_member_hive_role` /
  `list_member_hive_roles` / `get_hive_owner` / `is_ownership_contested` + Admin
  current-owner precheck + `revoke_hive_membership` (owner-protected) +
  `redeem_invite_grant` + `list_by_author` bounds + the 4 humm-tauri reads
  (`content_summary`, `my_pair_shared_secret_exists`, `changes_since`,
  `get_hive_owner`). `migrate-dna.ts` skips Owner grants.
- `docs`: handoff + integration + deploy + lineage + baseline + codemaps.

**Hashes (reproducible, 0.6.1):** DNA `uhC0k2dX…`, integrity wasm `53d867f7…`,
content wasm `32fae851…`, happ `8f284777…`. Distributed to
`~/hummhive-official-happ-versions/` (`pass-5-owner-role` row + happ). NOT copied
into `humm-tauri` — the team integrates it into their own final-gates commit; an
mbox was sent with the artifact + hashes + the exact MANIFEST row + steps.

**Validation:** integrity host 71/71, coordinator 27/27, workspace clippy
`-D warnings` clean, fmt clean. **Sweettest 7/7 on iroh** incl. `owner_and_acl.rs`
3/3 (handshake+admin-authority+owner-reject; two-transfer cross-node determinism;
revoke owner-protect). Reproducible rebuild reproduces the hashes.

**SECURITY — documented, accepted residual:** owner transfer is NOT final against
a malicious PAST owner — any past owner can fork the lineage to re-seize ownership
(irreducible cross-chain double-spend; confirmed by security review + oracle).
Blast radius = GOVERNANCE only (Admin-grant, revoke-protect, owner UI), NOT
content decryption. Mitigation = deterministic resolution + fork detection
(`is_ownership_contested`) + honest docs. User chose "accept + expose
`is_ownership_contested`".

**Pass-4 status:** `main` still carries v1.0.0 (pass-4-query-tolerance, DNA
`uhC0k26b`, happ `2205337c`). pass-5 is the next bundle; pass-4 stays the released
production until the team cuts over.

## Outstanding follow-ups

1. **Merge `feat-integrity-pass-5-owner-role`** (user) — commit-local; synced
   WSL→mount via `wsl-push.sh`. Assistant never pushes.
2. **humm-tauri integration** (their team) — they hold the happ + MANIFEST row +
   the full cutover contract (`docs/HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md`):
   repoint the governance owner-gate off `authorMembershipHash===null` to
   `get_member_hive_role(me)==='Owner'`, the reject-string regexes, reader
   read-only, migration, the read helpers, the honest owner residual + microcopy.
3. **pass-4→pass-5 migration** for existing hives — `migrate-dna.ts` now skips
   Owner grants (lineage-conferred); integration doc §5.

---

## Environment

- **Linux `~/humm-earth-core-happ`** — authoritative. ALL dev/build/test here.
- **Windows `/mnt/c/proj/github/hummhive/humm-earth-core-happ`** — ff-merge target (harness cwd).
- **WSL sync:** `scripts/wsl-pull.sh` / `wsl-push.sh` / `wsl-check.sh`. See `CLAUDE.md`.
- **Toolchain:** holochain/hc 0.6.1, hdi 0.7.1, hdk 0.6.1 (pinned exact), Node 24,
  nix (holonix main-0.6 @ 0.6.1, rustc 1.94). `.baseline-hashes.txt` = repro contract.
- **Build (reproducible):** `nix develop --command bash -c 'bash scripts/build-zomes.sh && hc dna pack dnas/humm_earth_core/workdir && hc app pack workdir --recursive'`,
  then `hc dna hash …` MUST print `uhC0k2dX…` (pass-5).
- **Tests:** host `cargo test -p content --lib` (27) + `-p content_integrity --lib` (71).
  Conductor: `crates/sweettest` (in-process, iroh). **Tryorama CANNOT boot on
  hc 0.6.x** — do not use it.

## Conductor testing (crates/sweettest)

- Separate Cargo workspace (the conductor crate's dep tree stays out of the lean
  zome workspace; both now pin HSB `=0.0.57`). holochain rev `3bdeacc` (0.6.1),
  transport **iroh** (`transport-iroh`; tx5/datachannel dropped in 0.6.1) — the
  devShell provides `openssl` + `pkg-config`; RustCrypto pinned to holochain's RCs.
- Run: `cd crates/sweettest && nix develop ../.. --command bash -c 'export LIBCLANG_PATH=<nix clang lib dir>; cargo test -- --test-threads=1'`
  (`LIBCLANG_PATH` e.g. `/nix/store/…clang-18.1.8-lib/lib`).
- **7/7 green on pass-5** (coordinator_cleanup 2, coordinator_query_tolerance 2,
  owner_and_acl 3). First compile slow (conductor + wasmer + iroh).

## Other branches (committed, NOT merged to main)

| Branch | Tip | What |
|---|---|---|
| `feat-integrity-pass-5-owner-role` | HEAD (this) | **Pass-5**: owner role + reader read-only + 0.6.1 — DNA-forked `uhC0k2dX` |
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
- Sweettest needs `LIBCLANG_PATH` (see above); tryorama can't boot on hc 0.6.0.
- AdminWebsocket 400 → pass `wsClientOptions: { origin: "<anything>" }`.
- Two agents, one conductor: same `network_seed` → shared DHT → offline cross-agent validation.
- Reproducibility requires `nix develop` (`wasm-opt`) + `codegen-units = 1`.
- **Editing the integrity crate forks the DNA.** Pass-5 did this INTENTIONALLY
  (new DNA `uhC0k2dX`); the pass-4 "integrity frozen" rule is dead on this branch.
  Going forward, pass-5's integrity wasm `53d867f7` + DNA `uhC0k2dX` are the new
  invariant — hold them byte-identical on this branch; coordinator hot-swaps are
  free (content.wasm may change). `cargo fmt --all` is fine on pass-5 (the fork
  already moved the integrity wasm). rustc embeds `#[track_caller]` line numbers,
  so any integrity edit shifts the wasm sha — expected for a sanctioned fork,
  forbidden once a line is frozen.

## Key references

- Codemaps: `docs/CODEMAPS/` · Agent toolkit: `AGENTS.md` + `.claude/` · Session brief: `CLAUDE.md`
- Conductor tests: `crates/sweettest/README.md` · Reproducibility: `.baseline-hashes.txt`
- Build: `scripts/build-zomes.sh` + `scripts/strip-wasms.sh`
- Official happ binaries: `~/hummhive-official-happ-versions/` + `MANIFEST.tsv` (mirrored in `humm-tauri/.testdata/happs/`)
- Handoffs: `docs/PASS_5_DEPLOY_HANDOFF.md` + `docs/HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md` (pass-5 owner role); `docs/PASS_4_DEPLOY_HANDOFF.md`, `docs/HUMM_TAURI_*` (recv-signal / SharedSecrets / content-type+witness / acl_spec-mutation / roadmap)
