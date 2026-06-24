# POSTCOMPACTION — humm-earth-core-happ

> Current-state-of-the-world for devs and agents picking up this repo.
> Read first after a compaction or at session start. Git log has full history.

---

## Current state

**Release:** `main` carries **v2.0.0** (tag at `834335e`) =
pass-5-owner-role + GroupGenesis filter, merged onto main on top of
**v1.0.1** (tag at `de7abd8`) = pass-4-migration-rescue, which sits on
top of **v1.0.0** (tag at `db2a264`) = pass-4-query-tolerance.

**DNA FORKED** to
`uhC0k2dXMIa1yI-V4ibCWMiTY5G6-p0laq6IOAVQ2F8XXReDHSxyS` (pass-5; was
pass-4 `uhC0k26b...`). Integrity wasm `53d867f72cfa...` HELD against
pass-5 FINAL across the v2.0.0 merge (the merge brought in pass-5's
integrity wholesale). New coordinator: content.wasm
`4806534546c32caf25ceba4ed5707b6bce69f04881f3140b037a5c08ef828ee8`,
happ `42dbf9df56d88269f629651c1253d31bd2e5a664f3bdf44fe66256345034d361`
(929643 bytes). Distributed to `~/hummhive-official-happ-versions/` as
`humm-earth-core-happ_pass-5-owner-role_dna-uhC0k2dX_happ-42dbf9df.happ`
(MANIFEST row 14 + README row 29 + baseline v2.0.0 block updated; the
prior latent `8f284777` build is DELETED).

**v2.0.0 union of three lineages:**
- **pass-5 integrity + coordinator** — Owner role via offer/accept
  handshake (single owner, transferable, admin-undemotable), reader
  read-only deletes, role-grant hardening (Owner not membership-grantable;
  only the current owner grants Admin), `delete_group_genesis`,
  `InviteRedemption` soft-cap, humm-tauri read helpers, 0.6.1 toolchain.
- **pass-4 GroupGenesis filter** (`try_decode_hive_genesis`) — closes the
  silent-false-positive bug that surfaced 110 "hives" on a 6-hive chain
  via shape-decode; now uses `EntryTypes::deserialize_from_type`
  dispatch. Filter is exhaustive over pass-5's 9-variant `EntryTypes`.
- **pass-4 rescue's `_local` externs** (`list_my_hives_local`,
  `get_latest_membership_local`, `mark_migrated_v2` fail-soft) — ride
  along, additive, for dormancy-proof discovery on future cell migrations.

**Pushed to GitHub:** `db2a264` + tag `v1.0.0`. Main is ahead of GitHub
by the rescue (v1.0.1) AND pass-5+v2.0.0 commits + tags. User
`git push origin main && git push origin --tags v1.0.1 v2.0.0` pending.
Assistant never pushes.

**Validation (Phase E gate):** 71 integrity + 27 coordinator host tests
green; `cargo clippy --workspace --all-targets -- -D warnings` clean;
**Sweettest 10/10 active green on the merged DNA** including the
rescue's `founder_lists_own_hives_via_local_path` regression test as the
cross-cutting Phase-E end-to-end proof (GroupGenesis filter live on
pass-5's `list_my_hives_local` path). Reproducible rebuild reproduces
the shas. DNA-hash freshness guard added to all 4 sweettest setups.

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
coordinator hot-swap staged for the live `@4` cell. v2.0.0 (pass-5, DNA
`uhC0k2dX`, happ `42dbf9df`) is the next bundled release.


## Outstanding follow-ups

1. **Phase C done; Phases D–F pending** for the v2.0.0 landing (merge
   fix-coordinator-pass5-groupgenesis-filter → pass-5, then pass-5 → main
   as v2.0.0, then docs freshness). Tracked in `local://pass5-main-landing-plan.md`.
2. **Merge `feat-integrity-pass-5-owner-role`** (user) — eventually as
   the v2.0.0 main merge per the landing plan. Commit-local; synced
   WSL→mount via `wsl-push.sh`. Assistant never pushes.
3. **humm-tauri integration** (their team) — they hold the happ + MANIFEST row +
   the full cutover contract (`docs/HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md`):
   repoint the governance owner-gate off `authorMembershipHash===null` to
   `get_member_hive_role(me)==='Owner'`, the reject-string regexes, reader
   read-only, migration, the read helpers, the honest owner residual + microcopy.
4. **pass-4→pass-5 migration** for existing hives — `migrate-dna.ts` now skips
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
