# Pass-7 integrity candidates + next coordinator generation — batch catalogue (rev 2)

- **Status:** OPEN — catalogue only; nothing here is scheduled. §A items FORK THE CHAIN (new DNA hash → new pass + migration + multi-user validation). §B items are coordinator-only (hot-swap, DNA held) and are the candidate roll-up for the NEXT coordinator generation.
- **Operating principles (owner):**
  1. (2026-07-16) Migration is a pain point — when the next sanctioned integrity fork happens, batch AS MANY §A items as possible into that ONE migration.
  2. (2026-07-16, fleet audit) The eventual holochain-0.7/hdk-0.7 stack hop (humm-tauri `14/03` "S2") forces a DNA-hash change and a data wipe/migration REGARDLESS of validator work — schedule pass-7 to ride the SAME fork event so users eat one wipe, not two.
  3. Coordinator work NEVER rides an integrity fork as a drive-by, and vice versa.
- **Origin:** RC-critical-path docwalk (2026-07-16 planning session) + full-repo 10-librarian fleet audit of humm-tauri @ `725ed49a` (2026-07-16, every `.newTasks/` doc traced into code). All file:line evidence refers to humm-tauri at that commit unless prefixed `earth-core`.
- **Discipline:** at pass-7 scoping time, re-read every §A item, confirm still wanted, land keepers in a single integrity change-set so the DNA hash moves exactly once.

## A. Integrity candidates (each forks the DNA)

### A1. Stable cross-generation content identity  — SIMPLIFIES (highest batch leverage)
- **Class:** COORD+INTEGRITY. **Serves:** every future migration; humm-tauri `05/05` migration-runner e2e; `03/05` bundle import/export.
- **Fleet quantification (2026-07-16):** humm-tauri's `src-tauri/src/migration/` — **5,478 lines across 18 files** (bundle/export/content_codec/wire/orchestration/logic/flows/lineage/runner/generation_retirement…) — exists purely because content identity is (DNA-hash, action-hash)-keyed and dies at every fork. `content_codec.rs:22-65` `WalkEntry` extracts a portable id across per-pass wire shapes by hand; `flows.rs:435-460` `resolve_marker_dna_hash` is a cfg-gated hack around hash duality.
- **Sketch:** `create_encrypted_content_with_lineage(…, prior_generation_original_hash, prior_generation_dna_hash)` — validator checks the prior hash was authored by the same agent (agent-activity proof) before accepting the lineage claim. Collapses export→import→remap→marker toward "install new cell, same id resolves".
- **Fold-in — ✅ RESOLVED coordinator-side (pass-6-idempotent-writes, v3.2.0):** `mark_migrated_v2` now accepts `HiveGenesis` originals via a CREATE-based marker on the deterministic content-id path `[genesis_b64, "hive-migration-marker-v2"]` (founder-only; `get_migration_marker_v2` reads it back; entry-def-index dispatch, GroupGenesis explicitly rejected) — NO integrity update-gate relaxation was needed, so nothing here rides the fork anymore. Their `14/13` Finding 1 closed; `importHiveBundle.ts:36` can stop hardcoding `old_marker_action_hash_base64: None`. A1's remaining scope is purely the lineage field + validator (above).

### A2. sec-holo-review WARN follow-ups (C-WARN-2/3 + open decision points) — HARDENS
- Unchanged from rev 1: validator-level `public_key_acl` size caps (DoS floor) + discovery-link reindex-on-update. Re-triage the full WARN catalog at scoping time.

### A3. Pair-claim counterpart witnessing (RESIDUAL of sender-identity attestation) — HARDENS
- **Rev-2 re-scope (3 fleet agents concur):** the AUTHOR-ATTESTATION half is ALREADY SHIPPED — I-H link author-binding (pass-2) + C-1 `revision_author_signing_public_key == action.author` (`entry_validation.rs:7-25`, live in the generation humm-tauri bundles today). Their `05/03` batch-verification gate and `T_SECURITY_SENDER_IDENTITY_UNATTESTED` are satisfiable NOW (their doc drift, not our work).
- **Genuine residual:** Mallory can validly author (as herself) a pair-SS/DM entry whose PAYLOAD falsely claims Alice as counterpart; the DNA can't see inside encrypted bytes. Sole defense today is client-side: `sharedSecret/index.ts:352-363` reader-ACL cross-check (`pair_ss_reader_acl_missing`).
- **Sketch:** witnessed-pairing analogous to the shipped G-6.2 `recipient_witnesses` — require the claimed counterpart's signed witness/ack before a pair entry validates. Coordinate exact entry set with humm-tauri before scoping.

### A4. Directory-listing validation / moderation lever — HARDENS (release-window+)
- Confirmed with fresh evidence: `rosterDecode.ts:87-91` "Clamp on read: hostile writers aren't bound by publish validation"; `directoryListing/index.ts:55-57` "The DHT scan itself cannot be source-bounded (frozen DNA)". A generic size-cap per OpenWrite entry may serve better than per-content-type carve-outs.

### A5. Typed manifest/directory shape+size caps (agent directory, sidecar manifests) — HARDENS
- **Rev-2 narrowing:** the LIST surfaces for `hummhive-core-agent-directory-v1` / `hummhive-core-sidecar-manifest-v1` are COORDINATOR-ZERO — content_type is an opaque client string (`types/coreWire.ts:9-13`) and enumeration rides `list_by_hive_link(_page)` today; their `06/02` "needs upstream DNA work" framing is wrong for items 1–2 (only item 3, validation, is ours).
- What IS integrity: shape/size caps for directory/agent-directory/manifest types. Fresh threat evidence: `SidecarDefinition.packageLocation` (unbounded, hive-writer-publishable) flows into `npm install` with zero verification (`package_manager.rs:17-33`) — DNA caps bound the DoS/shape surface (the code-execution trust gap is app-side signed-manifest work, NOT ours). Size against the PATTERN (generic caps), not current field names — the legacy types are slated for deletion (`08/03` Phase 3).
- Batch A4+A5 together (same lever).

### A6. Per-blake3 provider link type — DEMOTED to watch item
- **Rev-2 reclassification:** the need is served TODAY coordinator-side — provider publishes already stamp `dynamic_links: [blake3]` (`provider/wire.rs:143-144,360`) and the shipped `list_by_dynamic_link_page` reads exactly that tag; humm-tauri's whole-hive scan+filter (`provider/query.rs:107-130`, 3 full re-scans per admission) is a client wiring gap (§C). Integrity work only if VALIDATED tag semantics (author-bound per-blake3 index) is ever demanded by pin-host telemetry. LinkTypes stays append-only.

### A7. ~~DM deletion Tier B native delete~~ — ALREADY SHIPPED (remove from batch)
- **Rev-2 correction:** `validate_delete_encrypted_content` has authorized ANY `public_key_acl.reader` (i.e. either DM party) to author a native Delete since pass-5 (earth-core `entry_validation.rs:527-546`), unchanged through pass-6-pinned-hosts. humm-tauri's `removeDmEntry` already calls it; the residual is a one-line UI gate (`MessageBubble.tsx:471` `fromMe &&`). Their Tier-A ephemeral delete-request protocol is dead code (zero UI callsites) — and OUR mirrored `send_dm_delete_request`/`DmRemoteSignal::DmDeleteRequest` family is redundant with native delete + reader fan-out → §B6 cleanup candidate.

### A8. Self-DM sync contract — CONFIRMED ZERO (footnote only)
- Multi-device self-notes design (incl. SAS ceremony + device-set fan-out) is built entirely on the existing HiveGroup device-set shape; even option 3 needs no new AclSpec variant. Keep only as a scoping-time checkbox.

### A9. Owner-transfer finality residual — unchanged (evaluate-or-re-accept at scoping).

### A10. Cascading grant invalidation on device revocation — NEW lead (under-scoped)
- **Source:** their `docs/earth-core-handoff/HUMM_TAURI_SELF_NOTES_OBSERVABILITY.md` "F-B revocation cascade": revoking a device from a device-set roster does NOT cascade-revoke Admin grants that device itself issued.
- **Class:** INTEGRITY. **Value:** HARDENS (governance). No dedicated task doc exists on their side — needs product decision + design before it earns a place in the batch.

### A11. Per-tuple uniqueness validators (companion to §B2 find-or-create) — HARDENS
- **NEW.** A coordinator find-or-create closes the single-writer TOCTOU window (the actual crash-resume pain) but NOT cross-agent races (two agents find-or-creating the same key concurrently on different chains). If non-bypassable uniqueness ever matters, integrity rules are the only fix: at most one `GroupGenesis` per `(hive_genesis_hash, hive_wide_role)`; optionally `(hive, content_type, content_id)` for designated types. Current posture (accept duplicates + canonical-pick) is livable — include only if the pass-7 window is open anyway.

### A12. Hardware/TEE attestation for node-spec records — WATCH ITEM
- **v3.3.0 baseline:** the coordinator-side app-signature handshake is the trustworthy-enough tier. A future integrity-level rule or trusted execution environment (TEE)-backed attestation would be a pass-7 candidate if node-spec claims need hardware proof.

## B. Coordinator-only candidates — next coordinator generation roll-up (no fork; build-ready pending noted confirmations)

### B1. `EncryptedContentSignal` hive-scoping — ANSWERED (no code). Reader-scoped fan-out re-verified with fresh line evidence (`signals/outbound.rs:31-61`).

### B2. `GetStrategy::Local` read twins — RESOLVED for the named needs; watch for new ones
- **Rev-2:** the fleet found the concrete boot-path reads (4 TS sites: `HiveGenesisRegistry.reconcileFromConductor`, `:344` membership resolve, `reconcile.ts`, `bootSequence.js:174-190`) — and they call `list_my_hives`/`get_latest_membership`, whose `_local` twins ALREADY EXIST (and are already consumed by their own Rust at `blob_pinning/provider.rs:233`). So B2 = client wiring (§C), zero new externs, until a read WITHOUT a twin is named.

### B3. Find-or-create / idempotent-write family — ✅ SHIPPED (pass-6-idempotent-writes, v3.2.0)
- `find_or_create_encrypted_content` / `find_or_create_group_genesis` / `find_or_create_group_membership` shipped 2026-07-16 per the fleet-extracted natural keys (content → `(hive, content_id)` HummContentId path; genesis → `(hive, hive_wide_role)` [+ display_id for custom groups]; membership → `(group, for_agent, role)` unexpired). Author-scoped find, lowest-b64-string canonical pick (selectCanonicalByHash-identical), find-wins semantics, not cap-granted. Wire contract: `docs/HUMM_TAURI_IDEMPOTENT_WRITES_INTEGRATION.md` §3.
- **Remaining integrity half → A11**: cross-agent duplicate prevention (uniqueness validators) — the author-scoped coordinator find cannot close the cross-agent race by construction.

### B4. Upstream-only staleness — unchanged, no action in this lineage.

### B5. Hiveless-content remediation — ✅ SHIPPED (pass-6-idempotent-writes, v3.2.0) — as recreate+delete, NOT update
- Shipped as `list_my_hiveless_content` + `remediate_hiveless_content` (batch ≤64, per-item outcomes `recreated|skipped_already_correct|skipped_already_remediated|failed`, client supplies corrected inputs — the zome can't decrypt group ids). **The update-based design this entry originally sketched is structurally impossible**: `update_encrypted_content` writes only update-chain links, never discovery links, and retroactive Dynamic links fail the frozen validator. Their 02/01/03 steps 1–3 closed; once-per-chain gating stays client-side. Wire contract: handoff §4.

### B6. Deprecate the `send_dm_delete_request` ephemeral family — doc-deprecated v3.2.0; removal candidate
- Doc-deprecation SHIPPED (pass-6-idempotent-writes): deprecation notices on the extern + `DmRemoteSignal::DmDeleteRequest` variant. humm-tauri confirmed 2026-07-16 their Tier-A ephemeral path is dead code being retired (no dependency). Actual extern + variant REMOVAL stays a later-generation item (wire-surface removal — never a drive-by).

### B7. `fetch_pair_ss_with_hive_check` optional-hive — ✅ SHIPPED (pass-6-idempotent-writes, v3.2.0)
- `active_hive_genesis_hash` is now `Option<ActionHash>` (`#[serde(default)]`, msgpack-compatible); `None` → bounded union of the author∩dynamic intersection across the callee's own hives. Deletes their unbounded scan + 5s race + miss-cache + coalescer (`sharedSecretCrud.ts:296-325`). Grant posture unchanged. Wire contract: handoff §5.

### B8. Multi-hive `content_summary` batch — ✅ SHIPPED (pass-6-idempotent-writes, v3.2.0)
- `content_summary_many` (≤32 hives, ≤256 aggregate content types, order-preserving, cap-granted). Single-hive `content_summary` adoption remains client wiring (§C).

### B9. `BlobPinHint` enrichment for the linked-device TakeNow protocol — decision-gated; do not scope yet
- `TakeNow` is payload-identical to `Available` today; their EdgeHosting Phases 3–5 handshake would need requestId/session-challenge/destination-agent/placement-hash fields. Whether TakeNow drives that protocol is an OPEN product question on their side. Additive `#[serde(default)]` fields when decided — coordinator-only.

### B10. Opt-in per-root liveness flag on list/page reads — NEW (mbox 2026-07-18, humm-tauri measured)
- **Problem (measured, 10h live run):** `list_by_author` re-delivers TOMBSTONED provider-record roots every watch tick — 176 needle hits / ~29 phantom mutation events per hour, forever. Root cause verified in source: the list resolve chain (`get_many_encrypted_content` → `get_encrypted_content` → `get_latest_typed_from_eh`) gates ONLY on `entry_dht_status != Live` (`get_helpers.rs:56-58`) — per-ENTRY, never per-ACTION. Byte-identical duplicate provider roots content-address to ONE entry; deleting N-1 roots leaves the entry Live through the survivor, so every dead root resolves through the shared entry indefinitely. Unique-bytes roots go entry-Dead and drop correctly — only duplicate-root surfaces exhibit it.
- **Sketch (coordinator-only, additive):** list/page inputs gain `#[serde(default)] include_liveness: bool`; when true, each resolved record's ROOT action is probed with `get_details(action_hash)` and the response carries `#[serde(default)] tombstoned: Option<bool>` (`None` = not probed / old coordinator). Opt-in because the probe costs +1 DHT get per resolved record; existing callers stay byte-identical. EXCLUSION of dead roots was considered and rejected: silently changes granted-extern semantics, and the reporter's reconciliation wants dead roots visible-but-flagged.
- **Acceptance:** sweettest fixture with deliberate byte-identical duplicate roots (v3.1.0 lesson — trivial to build), delete one, assert the flagged listing; humm-tauri to attach reproduction counts to the 2026-07-18 mbox thread. Client interim: Deleted-vs-AlreadyGone discrimination on the wire-stable `Could not find the EncryptedContent` literal + per-boot known-tombstoned set (landing on their side now).

### B11. Timestamp-insensitive node-spec no-op (opt-in) — decision-gated; do not scope yet (mbox 2026-07-18)
- **Observation (humm-tauri, live):** a `publish_node_spec` re-publish with an unchanged spec map but fresh `declared_at_micros` is a real REPLACE (`was_updated: true`) — a boot-time re-publish policy grows the singleton's update fan by one action per boot. DELIBERATE current semantics: `declared_at_micros` is the "still true NOW" re-assertion readers judge staleness by, and once app attestation goes live the timestamp sits INSIDE the signed canonical string (a zome-side timestamp-insensitive no-op would silently discard valid fresh attestations — never acceptable).
- **Containment (right layer, landing their side):** client skips re-publish when map unchanged AND last publish younger than a staleness window.
- **IF fleet data ever shows material fan growth despite client policy:** additive opt-in input flag (`#[serde(default)]`), never a behavior change to the shipped extern. Gate on their staleness-window numbers.

### B12. Fetch-hint content signal + explicit fetched-author surface — NEW (mbox 2026-07-21, humm-tauri RC.4 security pass)
- **Problem (their finding, client-side fix already shipping):** `EncryptedContentSignal` payload fields are attacker-controllable via the unrestricted `recv_remote_signal` grant (the shipped threat-model doc says exactly this: signal = HINT). Their client cached signal-embedded `encrypted_content` bytes before gossip; a forged payload claiming the victim's own pubkey poisoned the self-authored trust bypass (sticky decrypt DoS; confidentiality already blocked by signcrypt). Their fix: drop ingest unless conductor-attested `from_agent` matches the claimed author — which relies on our receiver-side stamp.
- **Facts confirmed (2026-07-21, shipped code):** `recv_remote_signal` overwrites `from_agent = Some(call_info().provenance)` on EVERY remote `EncryptedContentSignal` delivery; stamping is RECEIVER-side, so the floor is the client's OWN conductor, not any peer's — landed at C1 (`c326e62`), in every tagged generation v1.0.0+. `from_agent: None` reaches the app layer ONLY via local `post_commit` self-emit. Integrity already binds `header.revision_author_signing_public_key == action.author` (`entry_validation.rs` `check_author_matches_header`), so a FETCHED record's header field is the signed-action author.
- **Sketch (coordinator-only, additive, next generation):** (a) fetch-hint signal variant — emit `action_hash` + `action_type` only, omit embedded `encrypted_content`; additive new variant/shape beside the shipped one (old receivers keep working), client retires the payload-bytes cache path and fetch-validates. (b) Optional: response field surfacing the fetched signed-action author explicitly (redundant with the validator-bound header field; only worth it if they want AgentPubKey-b64 without header parsing).
- **Acceptance:** sweettest forging a signal at a victim (existing blob-pin forged-sender pattern) proving the hint shape carries no trustable content bytes; client confirms ingest parity on the thread.
- **NOTE:** captured on the scratch branch copy 2026-07-21 — MIRROR this entry to main's copy at the next main-side session (B-series rides main's coordinator cadence, not the fork).

## C. Client-wiring-only (humm-tauri side; ZERO earth-core work — communicated 2026-07-16)

Shipped surface with zero adoption at `725ed49a` (only `latest_action_micros` is consumed):
1. DM sweep → `list_by_author_page` (deletes `dmSweepBudget.ts`, the deliberate limit-drop at `wire/content.ts:519-535`, and the 5s `WATERMARK_LOOKBACK_US` fudge).
2. Blob provider lookups → `list_by_dynamic_link_page` on the already-written `[blake3]` tag (collapses `provider_records_for_blob` whole-hive triple-scan to O(records-per-blob); keep their client-side blake3 re-check — tags are self-asserted).
3. Directory roster → `list_by_hive_link_page` (makes `DIRECTORY_ROSTER_MAX`/`DECODE_MAX` real DHT bounds instead of post-fetch JS slices).
4. `BlobPinSignal` consumption: extend their TS signal union + bridge to the existing `PinWatchRegistry::trigger` Tauri command (closes EdgeHosting Phase 6 "no fixed-interval polling").
5. Boot-path reads → existing `_local` twins (4 cited TS sites); no app-level zome-call timeout wrapper exists anywhere — their hardening, not ours.
6. `latest_action_micros` threading through the raw key-binding passthrough (`RawAuthoredEntry`) so `selectVerifiedKeyBindingX25519` stops trusting self-reported `created_at_ms` (their LOW.3).
7. peerIdentityClaim → `OpenWrite{None}` reclassification (still broken at HEAD: `CONTENT_TYPE_TO_ACL_SPEC.ts:82`; our validator answer stands — push works via real reader pubkeys).
8. DM delete-for-everyone: relax the `fromMe &&` UI gate; receiver-initiated native delete already validates (pass-5).
9. Group-DM thread discovery: their `dynamicLinks: [threadId]` on DirectMessage entries is a SILENT NO-OP (our documented `hive_context()` gate) — enumerate via `list_by_author_page` instead; do NOT ask for Dynamic-link validation on non-hive variants.
10. Status-drift corrections delivered: 02_B "blocked on earth-core" (stale since pass-5), 05/03 A3 gate (closeable), `14/13` Finding 14 (already fixed 2026-07-04), migration-runner un-skip items (already implemented), stale trust comments (`hummContentReads.ts:444-446`, `hummContentTypes.ts:33-36` — I-H shipped pass-2), `.testdata/happs/README.md` "awaiting clarification" (answered same-day 2026-05-31).

## D. Earth-core repo/infra (zero DNA impact; not part of any generation)

### D1. Tag-triggered GitHub Release workflow + CI-callable verify script — DEFERRED (owner, 2026-07-16)
- **Deferral:** owner call — release-automation work batches with the other RC-window release tasks close to the initial RC; not now. Mitigation in the meantime: humm-tauri dev mode treats the repo-local `.testdata/happs/` mirror AS the versioned endpoint, so nothing is blocked until a downloadable-RC build is actually cut. Tracked in `.newTasks/github-release-automation-happ-registry.md`.
- Their production `DEFAULT_HAPP_SOURCE` is hard-pinned to `https://github.com/hummhive/humm-earth-core-happ/releases/latest/download/` (`app_config/schema.rs:56-61`) — **that URL 404s today** (no release workflow exists; `.github/workflows/test.yaml` only runs tests, and still pins stale Nix 2.12.0). Blocks their `04/03` ReproducibleHappBuildCi Phases 2–3 and `14/15` HostedHappRegistry once the RC window opens.
- Build (when un-deferred): `scripts/verify-happ-dna-hash.sh <commit> <expected-dna-hash>` (wraps the existing reproducible pipeline) + a tag-triggered release job publishing `.happ` + MANIFEST row + SHA256SUMS as GitHub Release assets. Also bump the CI Nix pin.

### D2. Handoff-doc refresh: `docs/HUMM_TAURI_DM_MESSAGING_INTEGRATION.md` still specifies an in-bytes Ed25519 signature for `humm-dm-keybinding-v1`; live client code relies solely on the shipped author-binding validator ("no in-bytes signature on the DHT path"). Refresh so nobody re-implements a redundant check.

## E. Cross-cutting release blocker

### E1. LICENSE application (DecraLicense) — unchanged from rev 1 (§C1): text still unrecorded (owner confirmed not at hand 2026-07-16); legally blocks the downloadable RC; zero wasm/DNA impact; apply at repo root the moment it exists.

## F. Validated negatives (do NOT wishlist — fleet-confirmed 2026-07-16)

- Capacity/transfer governor, public-web media serving (F2), S3-style API (F3), referral/growth: all app-side; no DNA surface implied.
- **Payments countersigned receipts — posture change (owner, 2026-07-16): leaning YES**, likely involving Unyt. Holochain countersigning so service logs exist only when provider AND customer both sign the served-transfer record. No countersigning-session primitive exists in this DNA today → genuine INTEGRITY candidate when scoped; promote to §A at pass-7 scoping if the Unyt/rails decision lands by then. Further out: a hardware-validator attestation path (results signed by us, CPU-Z-style "post validated results" flow) — needs a trust story for running + validating output before any DNA surface is designable; parked as research, no design implied yet.
- Presence: reuses the existing remote-signal pattern; never committed to chain/DHT.
- T12 Holochain content-delivery cell: explicitly dead (iroh-blobs supersedes).
- Endpoint-binding entry type for edge hosting: existing author-binding already gives the needed guarantee — an integrity entry type would be over-scoped fork spend.
- `BlobApi.addWithoutDedup` (`03/02` "new earth-core zome call"): wrong — dedup is their app-side pre-check; fresh-squuid `add()` already creates distinct entries.
- Legacy identity migration (`03/04`): pre-Holochain local key formats; no DNA angle.

## H. Wave-2 candidates (2026-07-21 mbox-derived; POST-M7 — supersede/extend M1–M7 where noted)

- **Status:** OPEN — catalogue for the SECOND pass-7 build wave, on branch `feat-integrity-pass-7` (M0–M7 already built @ `9493169`). Same fork discipline: integrity items ride the SAME sanctioned fork as M1–M4 (batch into one DNA-hash move); coordinator items are the fresh pass-7 coordinator, no extra fork. NOTHING here is distributed or mentioned to humm-tauri.
- **Origin:** this-session mbox arc — the crown-fix SharedSecret threads + the `find_or_create_group_genesis` security review + the role-K closure ruling (all grounded in shipped pass-6). Thread substance is inlined per-item below; self-contained, no external ref.
- **Anchor discipline (LOAD-BEARING):** every `file:line` below is the SHIPPED pass-6 baseline (main @ `63cba86`), the rationale source. M1–M4 already moved integrity code on this branch, so RE-GROUND every anchor against branch HEAD before building — same rule the M0–M7 plan carries.
- **Supersession is expected (owner, 2026-07-21):** we are holding distribution precisely to get pass-7 as full/forward-looking/correct as possible. Where a Wave-2 item extends or supersedes an M1–M7 design (flagged per item), the newer learning wins; re-validate the affected milestone at scoping.

### H1. Inbox membership-index split — CRITICAL (humm-tauri G-#8)
- **Class:** INTEGRITY (new append-only `LinkTypes` → wasm change → rides the fork). **Serves:** the `list_my_hives []` failure class (DM-sweep retracts the Inbox HiveInvite links that are the only joiner discovery path) + durable membership enumeration.
- **Problem:** the `Inbox` link namespace does double duty — one-shot events (`HiveInvite`/`GroupInvite`, `crud.rs:82-88`, self-sent at `crud.rs:8-10`) AND the de-facto durable membership/join index that `list_my_hives` enumerates (`hive/queries.rs:191`). humm-tauri's DM-sweep RETRACTS `Inbox` `HiveInvite` links, erasing the only joiner-side discovery path; joined hives have no retraction-safe surface today (link = rebuildable cache).
- **Sketch:** introduce dedicated integrity link types `HiveMembershipIndex` + `GroupMembershipIndex` (append-only, next indices after `Lineage=18`, i.e. 19/20 — doc the stable range per house convention). Durable, never swept; `Inbox` reverts to transient events only. Coordinator enumerates membership from the new index (retraction-safe) with the existing author-binding + entry-type-discriminator pattern (never shape-decode; `try_decode_hive_genesis` precedent).
- **Relationship to M1–M7:** independent new milestone; no overlap.
- **Acceptance:** sweettest — grant→retract-Inbox→membership still enumerates via the new index; joiner lists a granted hive after an Inbox sweep; link validator author-binds to the membership target.

### H2. Per-entry-type ACL validators — anchor (humm-tauri G-#1)
- **Class:** INTEGRITY. **Serves:** the ACL-validation gaps the crown-fix threads surfaced.
- **Problem:** `validate_hivegroup_acl` (`entry_validation.rs:109-181`) + the G-6.2 recipient-witness machine (`:183-410`) are the only per-type ACL validators; invites/DMs/pair-SS get generic-or-none. Instances: **(a)** invite `max_uses`/HMAC binding is advisory-only (humm-tauri G-#2); **(b)** DM `pair_hash` pinning — the A3 residual: a payload counterpart claim is unverifiable, DNA can't read encrypted bytes; **(c)** pair-SS reader-ACL cross-check lives client-only (`sharedSecret/index.ts:352-363`, `pair_ss_reader_acl_missing`).
- **Sketch:** per-variant validator arms (dispatched off `AclSpec`/entry type) that bind the ACL-relevant header fields at validation time; each is a pure header/link check (no DHT read in `validate()`). Exact entry set for (b) needs humm-tauri coordination at blessing (A3 is SKETCH-only in the M-plan) — build (a)/(c) first.
- **Relationship to M1–M7:** complements M1 header bounds (`validate_header_bounds`); shares the bounds-helper spine.
- **Acceptance:** host unit tables per new reject literal + sweettest negatives; superset-only vs pass-6 reject strings (no literal removed).

### H3. Typed owner-attested squuid→role mapping — EXTENDS/supersedes M3 + the (ii) client steer
- **Class:** INTEGRITY. **Serves:** the crown-fix identity decision (the owner-attested squuid→role binding settled with humm-tauri: interim (ii) re-key by genesis action hash, this item is its DNA-level completion).
- **Problem:** the shipped current-gen answer we gave humm-tauri is (ii) — re-key role SharedSecrets by the existing system-role `GroupGenesis` ACTION HASH (owner-attested singleton), with the display_id=squuid convention as the only other anchor. Both are conventions layered on `GroupGenesis` (`group/types.rs:32-38`); the binding is not a first-class VALIDATED construct.
- **Sketch (DNA-strengthened, forward shape):** make squuid→role a validated owner-signed binding — either (1) an optional owner-only squuid field on the system-role `GroupGenesis` validated against the same owner-gate as `hive_wide_role` (`group/membership.rs:16-27`), or (2) a dedicated `RoleAnchor` entry type keyed `(hive, hive_wide_role, squuid)` with per-tuple uniqueness. Pick at scoping.
- **Relationship to M1–M7:** **EXTENDS M3** (per-author system-role `GroupGenesis` uniqueness, `a2350d7`) — M3 already enforces the singleton (hive,role) that (ii) keys to, so this is the natural DNA-level completion. **This supersedes the display_id=squuid convention** with a validated construct; the (ii) client re-key remains the interim current-gen path until this ships.
- **Acceptance:** owner-only mint enforced (non-owner rejects); per-(hive,role,squuid) uniqueness (reuse M3's `must_get_agent_activity` tombstone-aware walk); classification resolves squuid→role from validated state with zero self-asserted field.

### H4. Idempotent delete + tombstone-tolerant unpaged lists — coordinator (humm-tauri G-#6)
- **Class:** COORDINATOR (hot-swap, DNA held). **Serves:** the delete/liveness contract; complements M6.
- **Problem:** delete of already-deleted content surfaces the wire-stable `"Could not find the EncryptedContent"` (delete-not-found), forcing client Deleted-vs-AlreadyGone discrimination on a string; unpaged list reads have no tombstone tolerance (M6 added `include_liveness` on PAGED reads only).
- **Sketch:** idempotent delete extern returns success on an already-tombstoned target (was_deleted:false, still-absent = ok); extend the M6 `include_liveness`/`tombstoned` rider to unpaged list surfaces. Coordinator-only, additive `#[serde(default)]`.
- **Relationship to M1–M7:** direct complement to M6 (`63c6ae2`) — reuse `root_tombstoned`/`apply_liveness`.
- **Acceptance:** sweettest — double-delete second call succeeds idempotently; unpaged list flags/excludes tombstoned per flag.

### H5. Structurally-verified role-K closure enumeration — coordinator
- **Class:** COORDINATOR. **Serves:** the missed-tier silent discoverable-but-undecryptable failure class.
- **Problem:** a role grant must distribute the FULL downward SharedSecret closure (Admin→{Admin K, Writer K, Reader K}); the client fan-out is manual and O(roles), so a missed tier fails SILENTLY (member discovers lower-role content via `acl_links` dominance but can't decrypt — `linking/acl_links.rs:94-108` is discovery-only, confers zero decrypt).
- **Sketch:** coordinator extern enumerates the exact dominated role-SS set a grant must cover (genesis-anchored SS ids per the H3/(ii) identity) so the client reconciler can PROVE closure completeness. **KEEP independent per-role Ks** — explicitly NOT a deterministic HKDF-derived hierarchy (correction sent humm-tauri 2026-07-21T14-19-44: a derived chain makes down-tier revocation impossible without full-root rollover and breaks clean per-tier rotation — rotation-rigidity trade, not a win).
- **Relationship to M1–M7:** new; sits beside the shipped `acl_links` dominance fan-out.
- **Acceptance:** extern returns the correct dominated-role-SS id set for a given (hive, granted role); reconciler test proves a missing tier is detected (not silently discoverable-but-undecryptable).

### H6. `probe_inbox` since/cursor pagination — coordinator (genuine gap)
- **Class:** COORDINATOR. **Serves:** bounded inbox reads.
- **Problem:** `probe_inbox` has `event_filter` (`#[serde(default)]`) but NO since/cursor — an unbounded read as the inbox grows.
- **Sketch:** add the composite exclusive cursor the shipped `list_by_*_page` family uses (`BoundedLinkPage`, `paging.rs:186-196`; `(since_ts, source_after_action_hash)`), additive `#[serde(default)]`.
- **Relationship to M1–M7:** independent; mirrors shipped paging.
- **Acceptance:** sweettest — page cursor round-trips exclusively-ascending; limit clamp; back-compat when cursor omitted.

### Wave-2 build ordering (for the /plan)
Integrity items (H1, H2, H3) batch into the SAME fork as M1–M4 — one DNA-hash move, re-pin `EXPECTED_DNA_HASH` + ledger row per integrity milestone. Coordinator items (H4, H5, H6) ride the fresh pass-7 coordinator (DNA held). Suggested milestone lanes: **M8 = H1**, **M9 = H2**, **M10 = H3** (integrity, hash moves each); **M11 = H4+H6**, **M12 = H5** (coordinator, hash held). Re-ground all anchors against branch HEAD first. Per-milestone gate ladder: rebuild → `EXPECTED_DNA_HASH` re-pin (integrity milestones) → host + sweettest gates → clippy `-D warnings` + fmt → reject-string superset diff vs pass-6 → 5-lane review; append the DNA-hash + reject-literal rows to `docs/PASS_7_SCRATCH.md` per milestone (the branch-only ledger that already tracks M0–M7).

## I. Wave-4 client-adoption catalogue (branch-only; next-coordinator externs)

- **Status:** OPEN — client-wiring roll-up for the pass-7 Wave-4 coordinator externs
  (M17–M21 on branch `feat-integrity-pass-7`; DNA held after the M16 integrity move).
  Each row is humm-tauri work that a shipped-at-blessing Wave-4 extern enables; ZERO
  further earth-core work. Evidence file:line refers to humm-tauri at `a0fd5e4`.
- **NOTE:** captured on the scratch-branch copy — MIRROR §I to main's copy at the next
  main-side session; these externs ride main's coordinator cadence at blessing, not the
  fork. Nothing here is distributed or mentioned to humm-tauri until blessing.

### I.1 New batch/local externs → collapse N+1 load paths
- **Media availability refresh** (`mediaAvailabilityRefreshQueue.ts:365-390` →
  `availability.ts:345-356`, one `list_by_dynamic_link` per blob, ~100 roundtrips) →
  `list_encrypted_content_by_dynamic_links` (blake3s ARE the dynamic labels; pass
  `content_type` + the blake3 set). Win: ~100 calls → 1 (bounded ≤64 labels/page).
- **Decrypt-pipeline SS candidates** (`decryptPipeline.ts:395-416`,
  `sharedSecretCrud.ts:142-146`, per-uncached-group per-message) →
  `list_encrypted_content_by_dynamic_links` grouped by ACL scope. Win: one key-resolve
  + one call per scope for a 50-msg page.
- **`HiveApi.list()` serial hive resolve** (`hive/index.ts:103-135`, 10–30 serial) →
  `get_many_by_content_id_link`. Win: serial critical path → one batch (≤64), missing
  rows come back `record: None` (aligned to request order).
- **Feed addon fan-out** (`Feed/index.tsx:36-45`, one `list_by_hive_link` per addon
  type) → `list_by_hive_links_many` (≤32 requests, per-request first page + truncated).
- **Group-DM first contact** (`sidecarSharedSecret.ts:177-199,412-418,520-527`, up to
  31 sequential member + author scans) → `list_by_author_many` (buckets by author;
  client keeps member-over-inline precedence + X25519 validation).
- **ACL expansion roster fetch** (`deriveHiveGroupPublicKeyAcl.ts:55-64`, serial per
  group) → `list_group_members_many` (complete rosters; if a batch is rejected on the
  roster-link budget, fall back to the singleton per group).
- **Boot membership reconciliation** (`HiveGenesisRegistry.ts:310-318,352-374`, per-hive
  `get_latest_membership_local`) → `get_latest_memberships_local_many` (self-scoped).
- **Role-group + device-set bootstrap** (`bootstrapRoleGroups.ts:274-285`,
  `deviceSet/bootstrap.ts:269-281`, ≤9 NETWORK `list_my_groups` polls/hive-boot) →
  `list_my_groups_local` (kills both poll families).
- **Stranded-group recovery** (`setupNewHive.ts:390-423`, sleeps + re-lists network
  pages 3× for self-authored records) → `list_by_hive_link_local_page`.
- **Existence probe** (`hummContentReads.ts:115-140` `checkEntryExists()` fetches a full
  ciphertext record for `Boolean(record)`) → `content_id_exists` (scalar, resolves zero
  records).

### I.2 Signal-channel adoption (M21)
- **Content signal ingest** (`sharedSecretSignalIngest.ts:72-294`, `dmIngest.ts:300-390`,
  `dmPersistence.ts:112-148`) → retire the signal-embedded-bytes cache path; ingest the
  new `EncryptedContentHint` (no ciphertext), then fetch + `get`-verify. Trust the
  conductor-stamped `from_agent`, not pre-validation bytes.
- **Owner-handoff governance panel** (`ownerHandoffHooks.ts:177-212`, polls
  `list_pending_owner_handoffs` every 15s) → react to the `OwnerHandoffOfferHint` remote
  signal; keep one list-on-mount as durable recovery.

### I.3 Zero-DNA client hygiene (no Wave-4 extern needed)
- **DM inbox drain/retry** (`dmSweep.ts:245-292,351-356,194-197`) already-shipped
  `get_many_encrypted_content` (bypassed today — per-target `get_encrypted_content`).
- **RoleGroupAnchorResolver** (`RoleGroupAnchorResolver.ts:95-118`) re-fetches
  GroupGenesis records `list_groups_in_hive` already decoded — trust the typed response.
- **`content_summary_many`** shipped (pass-6) with ZERO humm-tauri callers — adopt for
  multi-hive summary.
- **Sidecar manifest** O(N²) re-listing (`sidecar/index.ts:299-326`) + **directory
  roster** per-row decode — client orchestration, no new extern.
- **SS cache lifetime** (`SharedSecretCache` unbounded, survives keyring lock; decrypt
  FIFO not cleared on lock) — client hardening.
- **Companion pin-state IPC batch** for the media-availability path — humm-tauri-side
  (Tauri IPC), not a zome extern.
