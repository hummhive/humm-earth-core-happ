# POSTCOMPACTION — humm-earth-core-happ

> Current-state-of-the-world for devs and agents picking up this repo.
> Read first after a compaction or at session start. Git log has full history.

---

## Latest arc (2026-07-24): pass-6-accepted-keys shipped (v3.4.1, patch)

One-constant coordinator patch on the HELD pass-6 DNA: the humm-tauri app
signing key `uhCAkyyOeMalaAEDiWSFPoywDMtLOB5AaisjAhnQ-9m2y81p9xnJC` now
populates `ACCEPTED_APP_SIGNING_KEYS_B64` (service_records.rs) — attested
`publish_node_spec` stops rejecting `unrecognized app signing key`. This
closes the accepted-keys rotation promised on the 2026-07-19 mbox thread
that v3.4.0 shipped without (the item lived in the mbox roll-up ledger, not
the backport candidate table — process note: sweep BOTH before cutting a
generation). Key byte-verified against the archived request; sweettest adds
the accepted-key-fails-at-SIGNATURE-gate trap (catches empty/mistyped list)
beside the renamed foreign-key reject; service_records 10/10, host 53/53,
clippy clean, DNA hash HELD, happ sha256
`a0b8d2bb335001bf70cd55b4277e9829de4e0e5ea2cf08bc8a41a3987754790e`, content
wasm `454d968f…`. humm-tauri constants at adopt: label `pass-6-accepted-keys`,
new sha, `COORDINATOR_WASM_VERSION` 12→13. Wire note: the attestation field
is `app_signing_key_b64` (the 07-19 reply's `app_signing_public_key_b64`
naming nit was itself wrong).

## Arc (2026-07-23): pass-6-batch-reads shipped (v3.4.0)

Coordinator-only generation on the HELD pass-6 DNA
`uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz` (integrity untouched —
`hc dna hash` verified held at every gate; happ sha256
`601fc4499e5d4a5a5553077fe960227318d6036d0aeef9c570e52ce2f81975bc`). Collapses
humm-tauri's N+1 read loops into bounded batch calls and adds local read twins,
so boot/feed/decrypt/ACL/DM paths stop multiplying singleton work.

- **10 new cap-granted read externs:** `list_encrypted_content_by_dynamic_links`
  (≤64 labels), `list_by_hive_links_many` (≤32 requests), `list_by_author_many`
  (≤64 lookups), `get_many_by_content_id_link` (≤64), `content_id_exists`,
  `list_group_members_many` (complete rosters, 4096 roster-link budget,
  reject-not-truncate), `list_my_groups_local`, `list_by_hive_link_local_page`,
  `probe_inbox_page`, `role_key_closure`. Batch page reads share a 4096 resolve
  budget (`enforce_batch_resolve_budget`).
- **B10 liveness rider:** opt-in `include_liveness` on the 7 read externs +
  `tombstoned: Option<bool>` on `EncryptedContentResponse`, probed per root
  action (`root_tombstoned`/`apply_liveness`); off the default read path.
- **Owner-handoff offer hint:** best-effort `OwnerHandoffOfferHint` from
  `initiate_owner_handoff`; `recv_remote_signal` now decodes 4 families
  (fall-through literal updated). `list_my_groups` granted-half now walks the
  durable `AgentToGroupMemberships` index (survives an Inbox sweep).
- **Resolve path:** memoized tolerant `resolve_many_encrypted_content`, graceful
  `get_latest_typed_from_eh` (no panic path), early-stop paging, `GetOptions`
  threaded through the read chain (existing callers pass network — zero behavior
  change).
- **Gates:** fmt + `content` host 53/53 + clippy `-D warnings` clean; DNA hash
  HELD; full sweettest green (63 tests + 1 ignored across 13 binaries, incl.
  `batch_reads` 17, `role_key_closure`, `signal_hints`, `liveness_and_reindex`,
  `inbox_and_delete`, and the `pinned_hosts` page-engine regression). Reject-
  literal superset vs v3.3.0 clean (adds = 8 batch literals + new extern/log
  strings; only intentional loss = the 3→4-family recv fall-through). Three
  review lanes (rust/security/silent-failure) APPROVE, nits accepted. Handoff:
  `docs/HUMM_TAURI_BATCH_READS_INTEGRATION.md`; tracking
  `.newTasks/PASS6_OPTIMIZATION_HANDOFF.md`. humm-tauri constants bump at adopt:
  `CURRENT_HAPP_LABEL="pass-6-batch-reads"`, new `CURRENT_HAPP_SHA256`,
  `COORDINATOR_WASM_VERSION` 11→12.

## Arc (2026-07-17, evening): pass-6-service-meter shipped (v3.3.0)

Coordinator-only generation on the HELD pass-6 DNA, merged `--no-ff` as
`311e10c`, tag `v3.3.0` on the merge commit (local; owner pushes). Headline
sequence on the branch: CI cutover (`ccb5fce`), coordinator feature
(`4c8af39`), sweettest proof (`0ff2430`), handoff docs (`765c934`), CI
fail-fast fix (`6071278`).

1. **`upsert_service_meter`** — one `EncryptedContent` per (author, hive,
   UTC day), content type `hummhive-core-service-meter-v1`, id
   `service-meter-v1:<YYYY-MM-DD>`, dynamic link = period. Counters are
   absolute cumulative u128 decimal strings, canonicalized on input,
   max-merged over the key union on update; identical merged state is a
   no-op. Corrupt priors are hard errors, never resets.
2. **`publish_node_spec`** — opt-in singleton `node-spec-v1` per
   (author, hive), REPLACE semantics, optional Ed25519 app attestation
   (`verify_signature_raw` over the canonical string) against
   `ACCEPTED_APP_SIGNING_KEYS_B64` which SHIPS EMPTY — every attestation
   rejects with `unrecognized app signing key` until humm-tauri mints the
   app key (adding it is a coordinator hot-swap).
3. **Header convergence** — every upsert converges the stored header
   (`display_hive_id`, `revision_author_signing_public_key`,
   `public_key_acl`) to the caller's values; widening the reader ACL alone
   is a real update (silent-failure lane finding, conductor-proven).
4. **CI cutover** — `.github/workflows/test.yaml` now runs host tests →
   happ build → sweettest (tryorama gate removed; it cannot boot on hc
   0.6.x). Lockfile-keyed cargo caching; libclang derivation fails fast.
5. **Wire contract** — `docs/HUMM_TAURI_SERVICE_METER_INTEGRATION.md`
   (bounds, exact reject literals, attestation canonical string + raw-UTF-8
   signing warning, client cadence guidance, payer-side zero-surface).
   Neither new extern is cap-granted. `.newTasks/rc-happ-futureproofing.md`
   F1+F3 SCOPED→shipped; pass-7 catalogue gained A12 (TEE attestation).

Gates at the merge: fmt clean; host `content` 48/48 + `content_integrity`
76/76 (integrity byte-untouched); clippy `-D warnings` clean; sweettest 37
passed / 1 ignored (28 baseline + 9 new); five serialized reviewer lanes all
APPROVE with findings applied; rebuild at `311e10c` reproduced DNA
`uhC0ksXs…`, integrity wasm `2656a910…`, content.wasm `34676ba0…`, happ
`b98916f1…` byte-identically. Known blemish: a `cargo fmt` inside
`crates/sweettest` normalized ±24 lines across five pre-existing test files
(rode the test commit; suite green on the exact tree).

Distribution: official store row `pass-6-service-meter` appended LAST.
humm-tauri `.testdata`/`bin` mirroring still DEFERRED at owner request —
**v3.2.0 AND v3.3.0 are now both owed** to both humm-tauri clones when their
testing settles (their `currentGenerationRow()` takes the LAST row; the
v3.3.0 row supersedes v3.2.0 as current unless they want both).

## Arc (2026-07-17): standards canon + config parity + cleanliness + futureproofing capture

Seven commits on `main` after the v3.2.0 freshness commit, no zome changes,
DNA/wasm untouched:

1. **Standards canon (`86e1b35`)** — root `CODING_STANDARDS.md` +
   `ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` are now THIS repo's canonical
   standards (adapted from humm-tauri, same section numbers, §9 MobX stubbed
   N/A); `ANTI_SLOP.md` copied verbatim as the prose bar. The
   `coding-standards` skill is an index over them; `standard-workflow`'s old
   "don't import humm-tauri standards" guard superseded.
2. **Injection fixed (`530f7c3`)** — the omp session hook was DEAD (flat
   `.omp/hooks/` is never discovered; only `hooks/pre|post/` is scanned) —
   moved to `.omp/hooks/pre/`. `hooks/session-context.mjs` now reads the
   CURRENT generation row live from `~/hummhive-official-happ-versions/
   MANIFEST.tsv` (last row) and prints MATCHES/DIFFERS — no more hardcoded
   pass labels going stale. `CONTEXT-INJECTION.md` documents all three
   wirings (omp/Claude Code/Codex).
3. **TTSR rules (`c94b9f6`)** — 12 rules adopted from humm-tauri (600/500
   file caps, ~60/~50 fn caps, TS-test hygiene incl. `ts-test-no-cwd-path`);
   not-adopted set documented in `.omp/README.md`.
4. **Toolkit parity (`39df13d`)** — new skills `slop-scan` + `search-first`;
   new agents `librarian`, `technical-researcher`, `typescript-reviewer`
   (test/tooling TS only) across `.claude/` + `.codex/`; `/skill-health`
   command; fresh `.claude/agents/README.md`.
5. **Scaffold prune (`76bec92`)** — dead `ui` workspace wiring removed from
   `package.json` (workspaces, 6 scripts, 6 devDeps, −1225 lockfile lines),
   `workdir/web-happ.yaml` + sublime-project deleted, README rewritten honest
   (nix develop; three test layers; tryorama dormant on hc 0.6.x — sweettest
   is the conductor gate). Archived docs deliberately untouched.
6. **Docs freshness (`0f0d65c`)** — architecture codemap caught up to v3.2.0
   lineage; vendored `holochain` skill indexed in AGENTS.md.
7. **Futureproofing capture (`e3600b3`)** — `.newTasks/rc-happ-futureproofing.md`
   (sibling to the pass-7 catalogue): Unyt/Holo-Hosting study — Unyt NEVER
   hosts third-party hApps (Bridge-Agent integration model), zero
   integrity-fork asks, coordinator-only F1 service-proof candidate, RC
   hygiene debt (CI runs dormant tryorama; sweettest not in CI; LICENSE
   missing), open questions incl. owner premise call (host vs pay vs
   neither-yet). Owner /plan session pending on that file.

Owner working-agreement addition: comma/slash-separated instruction lists
ending in "commit" → commit LAST, each logical piece its own commit (noted in
`CLAUDE.md`).

## Current state

**Branch:** `main` at **v3.3.0** — `pass-6-service-meter` coordinator
generation merged 2026-07-17 (branch `feat-coordinator-pass6-service-meter`
→ `--no-ff` merge `311e10c`, tag `v3.3.0` on the merge commit). Coordinator-only
hot-swap on pass-6: DNA HELD, no migration. Prior: v3.2.0
(pass-6-idempotent-writes, 2026-07-16) = idempotent-writes coordinator
generation; v3.1.0 (pass-6-pinned-hosts, same day) = blob-keystone
coordinator generation; v3.0.0 (pass-6 blessed 2026-07-02, merge `2de8923`)
= structural DRY refactor + security validation hardening; v2.0.0 (pass-5
owner role, DNA `uhC0k2dX…`, happ `42dbf9df…`) — the migration SOURCE
generation.

**pass-6-service-meter (v3.3.0, current coordinator generation):** DNA HELD
`uhC0ksXs…` / integrity `2656a910…` byte-identical; content wasm
`34676ba0…`, happ `b98916f1…`, artifact
`humm-earth-core-happ_pass-6-service-meter_dna-uhC0ksXs_happ-b98916f1.happ`.
New wire surface (all additive, neither granted): `upsert_service_meter` +
`publish_node_spec` returning `UpsertContentResponse { response, was_created,
was_updated }`; snapshots `ServiceMeterSnapshot` / `NodeSpecSnapshot`;
attestation dormant behind the empty `ACCEPTED_APP_SIGNING_KEYS_B64`. Reads
ride existing granted list/page externs. Full contract:
`docs/HUMM_TAURI_SERVICE_METER_INTEGRATION.md`.

**pass-6-idempotent-writes (v3.2.0):** DNA
HELD `uhC0ksXs…` / integrity `2656a910…` byte-identical; content wasm
`3b5348eb…`, happ `bfe357aa…`, artifact
`humm-earth-core-happ_pass-6-idempotent-writes_dna-uhC0ksXs_happ-bfe357aa.happ`.
New wire surface (all additive): author-scoped find-or-create family
(`find_or_create_encrypted_content` / `find_or_create_group_genesis` /
`find_or_create_group_membership` — find-wins, lowest-b64-STRING canonical
pick, NOT granted); hiveless remediation pair (`list_my_hiveless_content` +
`remediate_hiveless_content`, batch ≤64, recreate+tombstone, per-item
outcomes, NOT granted); `fetch_pair_ss_with_hive_check` optional-hive
(`active_hive_genesis_hash: Option<ActionHash>`, None → own-hive union);
`mark_migrated_v2`/`get_migration_marker_v2` accept HiveGenesis originals
(CREATE-based founder-only marker on `[genesis_b64,
"hive-migration-marker-v2"]`, entry-def-index dispatch); `content_summary_many`
(≤32 hives, ≤256 aggregate types — the ONLY new cap grant);
`send_dm_delete_request` family doc-deprecated. Legacy externs
wire-identical. Handoff + BDD:
`docs/HUMM_TAURI_IDEMPOTENT_WRITES_INTEGRATION.md`.

**pass-6-pinned-hosts (v3.1.0, prior coordinator generation):** DNA HELD
`uhC0ksXs…` / integrity `2656a910…` byte-identical; content wasm
`cc904ad6…`, happ `1c7d981b…`, artifact
`humm-earth-core-happ_pass-6-pinned-hosts_dna-uhC0ksXs_happ-1c7d981b.happ`.
New wire surface (all additive): `latest_action_micros: Option<i64>` on
`EncryptedContentResponse` (None on create); `BlobPinSignal`
(`#[serde(tag="pin")]`, Available/TakeNow) + `send_blob_pin_signal`
(local-only, ≤16 recipients); bounded source-cursor page externs
`list_by_hive_link_page` / `list_by_dynamic_link_page` / `list_by_author_page`
(BoundedLinkPage envelope, composite exclusive cursor, limit default 100 /
cap 256, cap-granted); exact-own `get_my_content_by_id_link` (author-scoped,
4096 saturation, NOT granted). Legacy externs wire-identical (F1
`list_by_author` untouched). Handoff + BDD:
`docs/HUMM_TAURI_PINNED_HOSTS_INTEGRATION.md`. Mailbox: all 9 pending
pinned-hosts asks answered + archived 2026-07-16.

**Pass-6 DNA (the frozen invariant, HELD through v3.2.0):**
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

**Validation (v3.2.0):** `cargo fmt --all --check` green; `cargo test -p
content_integrity --lib` = 76/76 green (untouched); `cargo test -p content
--lib` = 40/40 green (35 v3.1.0 baseline + 5 new); `cargo clippy --workspace
--all-targets -- -D warnings` green; Sweettest = 28/28 active green + 1
ignored (21 pre-existing + 7 new idempotent_writes). Serialized 5-lane
reviewer loop (rust-wire / security / silent-failure / standards / DRY)
converged APPROVE (key catches applied: entry-def-index marker dispatch —
GroupGenesis is a serde field-superset of HiveGenesis; b64-string canonical
pick; aggregate summary bound; atomic create propagation in remediation).
Pass-6 blessing history: C-BLOCK-1/2 fixed and re-gated.
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
   Pass-7 considerations captured: full batch catalogue at
   `.newTasks/pass-7-integrity-candidates.md` (A1–A9 integrity candidates,
   B1–B4 coordinator deferrals, C1 LICENSE blocker) + roadmap §Pass-7.
3. **pass-4→pass-5 migration** for straggler hives — `migrate-dna.ts` skips
   Owner grants (lineage-conferred); direct 4→6 is unexercised — chain 4→5→6
   or validate first (deploy handoff §Straggler `@4` hives).
4. **Review WARN follow-ups** (non-blocking) — `docs/sec-holo-review/findings-catalog.md`
   C-WARN-1..7 + open decision points.
5. **humm-tauri pinned-hosts adoption** (their team) — constants bump
   (`COORDINATOR_WASM_VERSION` 9→10, `CURRENT_HAPP_LABEL`,
   `CURRENT_HAPP_SHA256`), re-enable their ignored cursor acceptance test;
   contract in `docs/HUMM_TAURI_PINNED_HOSTS_INTEGRATION.md`.
6. **LICENSE (DecraLicense)** — RC legal blocker; text unrecorded (user
   confirmed not at hand 2026-07-16). Catalogued
   `.newTasks/pass-7-integrity-candidates.md` §C1; apply at repo root the
   moment the text exists (zero wasm/DNA impact).

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
- **Tests:** host `cargo test -p content --lib` (35) + `-p content_integrity --lib` (76).
  Conductor: `crates/sweettest` (in-process, iroh). **Tryorama CANNOT boot on
  hc 0.6.x** — do not use it.

## Conductor testing (crates/sweettest)

- Separate Cargo workspace (the conductor crate's dep tree stays out of the lean
  zome workspace; both now pin HSB `=0.0.57`). holochain rev `3bdeacc` (0.6.1),
  transport **iroh** (`transport-iroh`; tx5/datachannel dropped in 0.6.1) — the
  devShell provides `openssl` + `pkg-config`; RustCrypto pinned to holochain's RCs.
- Run: `cd crates/sweettest && nix develop ../.. --command bash -c 'export LIBCLANG_PATH=<nix clang lib dir>; cargo test -- --test-threads=1'`
  (`LIBCLANG_PATH` e.g. `/nix/store/…clang-18.1.8-lib/lib`).
- **21/21 active green on the v3.1.0 build** (coordinator_cleanup 2,
  coordinator_query_tolerance 2, owner_and_acl 4, migration_rescue 3 active +1
  ignored, recipient_witnesses 1, pinned_hosts 9). Shared wire mirrors live in
  `tests/support/mod.rs`. First compile slow (conductor + wasmer + iroh).

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
- Handoffs: `docs/HUMM_TAURI_PINNED_HOSTS_INTEGRATION.md` (v3.1.0 pinned-hosts wire + BDD); `docs/PASS_6_DEPLOY_HANDOFF.md` (pass-6 cutover runbook) + `docs/PASS_6_DRY_REFACTOR_HANDOFF.md` (pass-6 detail); `docs/_archive/PASS_5_DEPLOY_HANDOFF.md` + `docs/HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md` (pass-5 owner role); `docs/_archive/PASS_4_DEPLOY_HANDOFF.md`, `docs/HUMM_TAURI_*` (recv-signal / SharedSecrets / content-type+witness / acl_spec-mutation / roadmap)
