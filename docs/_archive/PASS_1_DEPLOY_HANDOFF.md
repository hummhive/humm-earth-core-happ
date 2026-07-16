# Pass-1 deploy handoff — humm-tauri integration

Short-form handoff for the humm-tauri team to integrate the pass-1
coordinator changes shipped on `feat-optional-recipient-id`. For the
full reference, see the per-change tables and security model in
[`HUMM_TAURI_COORDINATOR_INTEGRATION.md`](./HUMM_TAURI_COORDINATOR_INTEGRATION.md)
and [`DNA_MIGRATION_GUIDE.md`](./DNA_MIGRATION_GUIDE.md).

## Reference labels used in this doc

All of these labels come from the planning artifact
`/mnt/c/proj/github/hummhive/holochain-ecosystem/HAPP_COORDINATOR_CHANGES.md`
(the spec this pass implements) and the per-change discussion in
[`HUMM_TAURI_COORDINATOR_INTEGRATION.md`](./HUMM_TAURI_COORDINATOR_INTEGRATION.md).
They are kept short so call sites stay scannable; read this table first
and the rest of the doc is self-contained.

| Label | What it is |
|---|---|
| **C0–C7b** | Coordinator-zome changes (no DNA-hash impact). C0 = `get_messages_since` chain-delta query (shipped pre-this-pass). C1 = receiver stamps `from_agent` from `call_info().provenance`. C2 = `list_by_hive_link` gets `since_ts` + `limit` with oldest-first sort. C3 = `count_links_by_hive` (cheap "how many" primitive). C4 = `fetch_pair_ss_with_hive_check` (author ∩ active-hive intersection). C5 = cap-grant audit (typo fix + new-extern registration + sender-side externs deliberately local-only). C6 = `send_dm_delete_request` ephemeral signal. C7 = `send_dm_call_*` WebRTC signaling. C7b = `recv_remote_signal` multi-signal dispatcher (ordered try-decode). |
| **H-1** | Security-finding category: attacker-seeded pair-shared-secret poisoning (an attacker writes a fake SS entry that the victim's host might accept as legitimate). Tracked in `humm-tauri/.doneTasks/T_SECURITY_FETCH_PAIR_FROM_AUTHOR_POISONING.md`. |
| **I-A / I-B / I-C / I-D** | Integrity-zome changes deferred to pass-2 (each bumps the DNA hash → requires the migration scaffold from commit `520bfc6`). I-A = receiver-initiated tombstone validator. I-B = dual sender-key fields in `EncryptedContentHeader`. I-C = DHT Inbox link type + `DmProbeLog` private entry. I-D = Hive/Dynamic link integrity validators (the true H-1 fix that C4 references but cannot itself close coordinator-only). |
| **SEC-2** | Security-reviewer finding from this pass: granting `send_dm_*` `Unrestricted` would let any peer reflect signals through MY agent at arbitrary recipients (DoS amplification + spoof-by-proxy that subverts C1). Resolution: `send_dm_*` deliberately NOT in `set_cap_tokens` — see disclaimer 3 below. |
| **WS-L** | The `update_coordinators` install-guard pattern from `humm-tauri/.extraResearch/decentralizedStartupSync/EXECUTION_PLAN.md` §WS-L. Tier-0 PLANNED feature that, once shipped, lets coordinator-only happ upgrades flow to existing users silently — see disclaimer 1 below. |
| **Tier A / Tier B** | Task-priority labels from `humm-tauri/.newTasks/T_DM_DELETE_IMPL.md`. Tier A = the in-payload `kind:'delete_request'` DM-deletion approach (already shipped on the humm-tauri side). Tier B = the receiver-initiated native HC tombstone approach (deferred to pass-2 as part of I-A). |
| **Tier 0** | Task-priority label meaning "must-land-before-X" — see WS-L row. |

## Source-of-truth state

**hApp branch:** `feat-optional-recipient-id` at `d32f812` (pushed to
`origin`). Four-commit series on top of `2dbeb13`:

| Commit | Scope |
|---|---|
| `c326e62` | C1 sender provenance + C2 since_ts/limit + C3 count_links_by_hive |
| `d6a972c` | C4 fetch_pair_ss_with_hive_check + C5 cap-grant audit + C6 DM-delete signal + C7 WebRTC signals + C7b multi-signal dispatcher + module split |
| `520bfc6` | Pass-2 migration scaffold (forward-pointer markers + export/import/mark-migrated script + full migration guide) |
| `d32f812` | Drivers + tasks-unblocked + transparent-migration docs consolidation |

**DNA hash:** `uhC0kT0Tkc3b6ccfa75YwWdzpSWvdkXERpdqkxIndRhfK5TJAUusY` —
**unchanged** from the prior shipped state. Coordinator-only pass; no
integrity zome touched; no user wipe required.

## What landed in `src-tauri/bin/`

Rebuilt `.happ`, 1.09 MB, sha256
`03c8f326e77e496325dd5e996d9da104145bf36b0fc511bbc186a2d56c7d2c3d`:

| Location | Absolute path |
|---|---|
| Windows mount (Windows-side dev / commit target) | `/mnt/c/proj/github/hummhive/humm-tauri/src-tauri/bin/humm-earth-core-happ.happ` |
| Linux dev mirror | `/home/aphix/humm-tauri/src-tauri/bin/humm-earth-core-happ.happ` |
| Build origin (in this happ repo) | `/mnt/c/proj/github/hummhive/humm-earth-core-happ/workdir/humm-earth-core-happ.happ` |

All three are byte-identical to the build output.

## Read first — canonical handoff docs

Absolute paths so you don't have to chase relative refs across repos:

- **`/mnt/c/proj/github/hummhive/humm-earth-core-happ/docs/HUMM_TAURI_COORDINATOR_INTEGRATION.md`**
  — per-change integration table (C1 through C7b), wire shapes for
  each new extern/signal, "Drivers + tasks now unblocked" table that
  names the `.newTasks/*.md` and `.doneTasks/*.md` files each change
  closes/enables, deploy procedure, threat-model caveats.
- **`/mnt/c/proj/github/hummhive/humm-earth-core-happ/docs/DNA_MIGRATION_GUIDE.md`**
  — pass-2 migration scaffold, security model (defenses A/B/C),
  tiered-transparency table (coordinator-only vs DNA-hash bump),
  humm-tauri GUI integration flow.
- **`/mnt/c/proj/github/hummhive/humm-earth-core-happ/scripts/migrate-dna.ts`**
  — standalone migration orchestrator reference (`export` / `import` /
  `mark-migrated` modes). Wire into humm-tauri's auto-update flow when
  pass-2 lands.
- **`/mnt/c/proj/github/hummhive/humm-earth-core-happ/.baseline-hashes.txt`**
  — DNA + wasm sha256 invariant record (pre/post pass-1).

## Upcoming-change disclaimers (read before deploying)

### 1. WS-L is the deploy prerequisite

This pass's new externs (C3 `count_links_by_hive`, C4
`fetch_pair_ss_with_hive_check`, C6 `send_dm_delete_request`, C7
`send_dm_call_*`, `get_migration_marker`) require the coordinator
hot-swap path from
`humm-tauri/.extraResearch/decentralizedStartupSync/EXECUTION_PLAN.md`
§WS-L. **WS-L is Tier-0 PLANNED, not shipped.**

| | With WS-L shipped | Without WS-L |
|---|---|---|
| Fresh installs | Get the new coordinator | Get the new coordinator |
| Existing users | Get the new coordinator silently on next launch (hot-swap via `AdminWebsocket::update_coordinators`) | Keep running the OLD coordinator; new externs are unreachable on their conductors |

Landing WS-L is therefore the deploy prerequisite for this pass to
reach existing users transparently.

### 2. C4 does NOT close H-1 against the standard adversary

`fetch_pair_ss_with_hive_check` is a defense-in-depth narrowing
against unmodified-client attackers, NOT a cryptographic guarantee.
The true H-1 fix (Hive/Dynamic link integrity validators, item I-D)
ships in pass-2 — see "What was NOT done" in the integration guide.
Until I-D ships, TS-side trust checks (`from_agent` + decrypt-and-
verify) remain the load-bearing control for shared-secret integrity.

### 3. `send_dm_*` are local-only by design

They are intentionally NOT in the cap grant. Calling them via
`call_remote` from another agent's cell will fail the cap check —
security-reviewer SEC-2 (preventing reflection DoS + spoof-by-proxy).
Local-UI calls work via AppWebsocket auth.

### 4. Pass-2 will bump the DNA hash

The integrity-validator additions (I-A through I-D, listed in the
integration guide's "What was NOT done" section) all change the DNA
hash. The migration scaffold from `520bfc6` is the forward-migration
path for existing users.

**Recommendation:** land WS-L BEFORE pass-2 ships so:
- This pass's coordinator changes flow transparently to existing users
  now (via WS-L hot-swap).
- Pass-2's DNA bump can use WS-L's `DnaHashMismatch` guard to
  prompt-then-run the migration script instead of silently breaking.

### 5. Migration markers fan out via the existing remote-signal path

`mark_migrated` delegates to `update_encrypted_content`, which fires
`remote_signal_acl_readers` — so peers in `public_key_acl.reader` who
are ONLINE when a migration marker is written get a real-time
`EncryptedContentSignal{Update}` with `content_type = _migrated/...`.

Receivers MUST apply defenses A/B/C from the migration guide before
treating the signal as authoritative (labels match
`DNA_MIGRATION_GUIDE.md` §"Security model" verbatim):

- **(A) Author binding** — re-query `get_migration_marker(old_action_hash)`
  for the author-bound authoritative marker (the reader filters
  updates to only those authored by the original entry's author, which
  is the load-bearing cryptographic check), AND cross-check the
  signal's `from_agent` **equals** the trusted partner identity.
- **(B) User consent before DNA crossover** — NEVER auto-follow the
  marker's `new_dna_hash_base64` / `new_app_id` without explicit human
  approval. Switching DNA crosses a trust boundary and must be a user
  decision.
- **(C) Cross-verify on the new DNA** — before redirecting any UI to
  the new AH, confirm `get_encrypted_content(new_action_hash)` actually
  resolves on the new DNA. Catches both attacker-forged markers AND
  the legitimate uninstall/reinstall staleness case.

## Integration sequence (recommended)

1. **Pull + audit** the new `.happ` is in place at
   `src-tauri/bin/humm-earth-core-happ.happ` (sha256 above). Confirm
   the DNA hash matches the expected value via
   `hc dna hash src-tauri/bin/humm-earth-core-happ.happ` if you want
   the belt-and-suspenders check.

2. **Wire WS-L first** (the §WS-L spec in
   `humm-tauri/.extraResearch/decentralizedStartupSync/EXECUTION_PLAN.md`).
   Without this, step 3 won't reach existing users.

3. **Bump `COORDINATOR_WASM_VERSION`** in the install guard. WS-L's
   hot-swap path picks up the bump on next launch and rotates the
   coordinator WASM in place.

4. **Adopt the per-change integrations** in priority order from the
   "Drivers + tasks now unblocked" table in
   `HUMM_TAURI_COORDINATOR_INTEGRATION.md`:

   **Now-unblocked already-spec'd tasks** (drivers already exist as
   `.newTasks/*.md`):
   - **C2** — `DmStore._sweepInbox` watermark loop per
     `T_HAPP_COORDINATOR_C2_LIST_PAGINATED.md`. Multiply JS ms × 1000
     for `since_ts` microseconds. **Note**: the host's previous
     newest-first sort was a data-loss bug at `>limit` new entries;
     the new oldest-first behavior is gap-free for the watermark sweep.
   - **C0** — `reconcileFromConductor` delta hydration per
     `T_HAPP_COORDINATOR_C0_WIRE.md` (TS-side spec already exists).
   - **C5** — `get_many_encrypted_content` cross-agent calls now
     actually granted per `T_HAPP_UPSTREAM_CAVEATS.md` §2 (the
     `get_many_encrypted_conten` typo is fixed).

   **New code paths:**
   - **C6** — ephemeral DM-delete (`DmStore.sendDeleteRequest`); pairs
     with the existing in-payload `kind:'delete_request'` Tier A path
     for offline coverage.
   - **C7** — WebRTC signaling primitives (new
     `src/sidecars/dm-webrtc/`); thread-participant authorization is
     HOST-side, the zome does not enforce it.
   - **C3** — unread badges + hive item counts + `SyncIndicator`
     progress without paying the `get_many_…` fan-out cost.
   - **C4** — `fetchPairFromAuthor` collapse to a single C4 call (with
     the I-D caveat above; keep TS-side trust checks).

   **Multi-signal dispatcher impact (C7b):**
   - `src/api/core/holochain/zomeSignals.ts` needs a discriminated-
     union type covering legacy `EncryptedContentSignal` + the new
     `DmRemoteSignal::{DmDeleteRequest, DmCall}` variants.
   - `from_agent` is stamped on every variant by the C7b dispatcher.

5. **Stage the migration-scaffold integration** ahead of pass-2 (no
   need to wire it up live yet, but `scripts/migrate-dna.ts` is the
   reference implementation; plan how the Rust side of humm-tauri will
   invoke it via `holochain_client_rust` or as a child process).

## Verification on the humm-tauri side

After integrating, key invariants to verify in tests:

- **Existing DM flow still works** — C7b dispatcher's
  `EncryptedContentSignal` arm is byte-for-byte compatible with the
  old `recv_remote_signal`.
- **`from_agent` populated on every received signal** (C1).
- **`DmStore._sweepInbox` watermark loop is gap-free with `>limit` new
  entries** (C2 oldest-first fix; this is the load-bearing regression
  test).
- **Cross-agent `get_many_encrypted_content` calls succeed** (C5 typo
  fix).
- **`send_dm_*` from a peer's `call_remote` fails the cap check**
  (SEC-2 — confirms local-only enforcement).

## What's NOT done in this pass (deferred to pass-2)

All DNA-hash-bumping. Listed in detail in
[`HUMM_TAURI_COORDINATOR_INTEGRATION.md`](./HUMM_TAURI_COORDINATOR_INTEGRATION.md)
"What was NOT done + why":

- **I-A** — receiver-initiated tombstone validator in
  `validate_delete_encrypted_content`.
- **I-B** — dual sender-key fields in `EncryptedContentHeader` (the
  Tauri-keyring Ed25519 vs DNA-attestation pubkey split).
- **I-C** — DHT Inbox link type + `DmProbeLog` private entry for
  offline-deliverable DM signaling.
- **I-D** — Hive/Dynamic link integrity validators (the true H-1 fix
  that C4 references but cannot itself close).

Pass-2 will branch off `d32f812` and ship these together as one
DNA-hash bump.
