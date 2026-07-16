# Pass-7 integrity candidates — batch catalogue (DNA-fork wishlist)

- **Status:** OPEN — catalogue only; nothing here is scheduled. Every item below FORKS THE CHAIN (new DNA hash → new pass + migration + multi-user validation).
- **Operating principle (owner, 2026-07-16):** migration is a pain point — when the next sanctioned integrity fork happens, batch AS MANY of these as possible into that ONE migration instead of dribbling them across passes.
- **Origin:** RC-critical-path + task-spec docwalk during the pass-6-pinned-hosts coordinator-generation planning session (2026-07-16). Sources are humm-tauri `.newTasks/` specs (read-only), this repo's roadmap/review docs, and the pinned-hosts keystone doc.
- **Discipline:** coordinator-only work NEVER rides an integrity fork as a drive-by, and vice versa: when pass-7 is sanctioned, re-read every item here, confirm it is still wanted, and land the keepers in a single integrity change-set so the DNA hash moves exactly once.

## A. Integrity candidates (each forks the DNA)

### A1. Stable cross-generation content identity
- **Source:** `docs/HUMM_TAURI_PASS_ROADMAP.md` §"Pass-7 candidate considerations" item 1 (humm-tauri 2026-07-03 validation report).
- **Observation:** migration re-authors every entry under new action hashes; app-side processed-sets keyed by content id resurface each pass-N→N+1 (e.g. invite-redemptions re-prompted). A durable origin id carried through migration imports erases the failure class.
- **Consideration:** additive `#[serde(default)]` field(s) on `EncryptedContentHeader` (still a fork by definition). Design question: first-generation origin action-hash vs opaque squuid; must survive the export→import pipeline in `scripts/migrate-dna.ts`.

### A2. sec-holo-review WARN follow-ups (C-WARN-2/3 + open decision points)
- **Source:** `docs/HUMM_TAURI_PASS_ROADMAP.md` §Pass-7 item 2; `docs/sec-holo-review/findings-catalog.md` (C-WARN-1..7).
- **Observation:** discovery-link reindex-on-update and `public_key_acl` size bounds were reviewed as WARN (non-blocking) at pass-6; validator-level bounds are integrity work.
- **Consideration:** fold the accepted subset of C-WARN items into the same fork; re-triage the full catalog at pass-7 scoping time.

### A3. Sender-identity attestation (T_SECURITY_SENDER_IDENTITY_UNATTESTED)
- **Source:** humm-tauri `.newTasks/05_EPIC_VerificationAndE2E/03_BatchVerificationOfCodeCompleteFeatures.md:37` — "Gate: fully closes only when T_SECURITY_SENDER_IDENTITY_UNATTESTED lands (DNA `content_integrity` + holohash stamps). Close both together."
- **Observation:** cross-hive SS fetch/rescue paths carry a JSDoc SECURITY caveat (`fetchPairFromAuthor`) because sender identity is not integrity-attested; their batch-verification task cannot fully close until the DNA stamps it.
- **Consideration:** integrity-zome holohash/author stamps on the relevant entries. Coordinate the exact entry set with humm-tauri before scoping — this one closes a named security caveat on their side.

### A4. Directory-listing validation / moderation lever
- **Source:** humm-tauri `.newTasks/06_PROJECT_DiscoveryDirectoryHive/02_DnaCoupledDirectoryFollowOns.md` item 3.
- **Observation:** the shipped directory-hive MVP is OpenWrite with NO moderation lever; client-side clamps are advisory — hostile writers are bound only by DNA validation. Their acceptance: "Hostile oversized/malformed listings are rejected at the validator, not merely clamped client-side."
- **Consideration:** integrity validation for `hummhive-core-directory-listing-v1` (shape + size caps, per-author bounds where expressible). Content-type-specific validation in the integrity zome is a NEW pattern for this DNA (today validation is variant-generic) — design carefully; a generic "size cap per OpenWrite entry" may serve better than per-content-type carve-outs.

### A5. Agent/person directory + typed sidecar-manifest surface
- **Source:** humm-tauri `.newTasks/06_PROJECT_DiscoveryDirectoryHive/02_DnaCoupledDirectoryFollowOns.md` items 1–2 (T18/T19/T21).
- **Observation:** item 2 wants `hummhive-core-agent-directory-v1` (opt-in, public ACL, author-retractable) for person-by-name / signing-key→display-name lookup; item 1 wants sidecar-manifest enumeration "through a supported DNA surface (not a raw-content-type convention alone)".
- **Consideration:** the ENUMERATION half of item 1 is already servable coordinator-only TODAY via the pass-6-pinned-hosts paged hive-link externs (`list_by_hive_link_page` on the manifest content type) — no fork needed for listing. What genuinely needs integrity: validated shape/size caps for the directory/agent-directory content types (same lever as A4). Batch A4+A5 validation together.

### A6. Per-blake3 index link type (blob provider records)
- **Source:** humm-tauri `.newTasks/07_EPIC_StorageHostingAndGrowth/01_PROJECT_PersistentBlobStorageKeystone/01_IrohFsStoreCutoverPinningGc.md:25` — "a new `content_type` is DNA-free, but a new per-blake3 index link type is not (needs a DNA hash change + upstream)."
- **Observation:** the keystone deliberately shipped provider records on the generic Dynamic link (`dynamic_links: [blake3]`) to stay DNA-free. A dedicated LinkType would give provider lookups their own validated index (author binding, tag semantics) instead of overloading Dynamic.
- **Consideration:** only worth the fork if provider-record scale or validation needs outgrow Dynamic links; revisit with real pin-host telemetry. LinkTypes enum is append-only (index stability).

### A7. DM deletion protocol Tier B (native delete)
- **Source:** humm-tauri `.newTasks/00_RC_CRITICAL_PATH.md:51` / `99_PROJECT_PostRcMessagingExtensions/02_DmDeletionProtocol.md` — "optional Tier B DNA native-delete … awaiting user signoff on 5 decisions."
- **Observation:** Tier A (additive protocol-layer delete) shipped without DNA change; Tier B native-delete semantics would be integrity work.
- **Consideration:** decision-gated on their side (5 open decisions). Do not scope until signed off; if signed off near a pass-7 window, batch it.

### A8. Self-DM / same-identity sync contract (conditional)
- **Source:** humm-tauri `.newTasks/02_EPIC_LiveMessagingAndContentCorrectness/01_PROJECT_SelfDmAndCrossHiveDelivery/01_DecideAndImplementSelfDmValidatorFork.md`.
- **Observation:** local Note-to-Self routing shipped with "No validator fork or cell-generation bump … required". The remaining product decision (device-local vs Hive-synchronized vs separate same-identity protocol) MAY imply validator work only if a synchronized contract needs a relaxed/new DM shape.
- **Consideration:** likely ZERO integrity work (option 1 and 2 need none; option 3 "does not weaken the pair-DM validator" by requirement). Keep on the list only as a conditional check at pass-7 scoping.

### A9. Owner-transfer finality residual (documented, accepted)
- **Source:** `POSTCOMPACTION.md` §SECURITY; `docs/sec-holo-review/findings-catalog.md`.
- **Observation:** owner transfer is not final against a malicious PAST owner (cross-chain fork re-seizure; governance-only blast radius). Accepted residual at pass-6 with deterministic resolution + `is_ownership_contested` detection.
- **Consideration:** if a validator-level mitigation design ever lands (e.g. transfer finality anchoring), it is integrity work — evaluate at pass-7 scoping; otherwise re-accept explicitly.

## B. Coordinator-only candidates deliberately NOT in pass-6-pinned-hosts (no fork; next coordinator generation fodder)

### B1. `EncryptedContentSignal` hive-scoping (upstream checklist item 4)
- **Source:** humm-tauri `.newTasks/04_PROJECT_ReleasePackagingAndDistribution/04_UpstreamHappCaveatsAndLicense.md` item 4.
- **Observation:** the ask ("filter by hive_id to avoid O(agents×writes) bandwidth") predates the current fan-out: `remote_signal_acl_readers` already sends ONLY to `public_key_acl.reader` minus self (undecodable entries skipped), so the O(agents) concern is structurally solved in this lineage.
- **Consideration:** answer the checklist item with evidence (done in the pass-6-pinned-hosts handoff); no code change unless humm-tauri shows a real hot path where reader-lists are broad.

### B2. `GetStrategy::Network` on all reads (offline UX, upstream item 6)
- **Observation:** every query path uses `GetStrategy::Network`; offline reads degrade. The pass-4 rescue added targeted `_local` twins (`list_my_hives_local`, `get_latest_membership_local`) rather than a blanket switch.
- **Consideration:** extend the `_local`-twin pattern per proven need; a blanket strategy flip is a semantics change requiring humm-tauri sign-off per read.

### B3. Conductor-side find-or-create wire shape (crash idempotency)
- **Source:** humm-tauri `.newTasks/01_EPIC_FreshInstallBootStability/03_CrashIdempotencyAuditForOnboardingFlows.md` remaining-work item 6(b) — "whether a conductor-side find-or-create wire shape is worth an earth-core ask".
- **Observation:** undecided on their side; would make onboarding writes (hive/group genesis, membership) idempotent at the zome instead of probe-before-create in TS.
- **Consideration:** wait for their decision; if asked, it is coordinator-only (query-then-create inside one extern) and slots into any future coordinator generation.

### B4. Upstream-only staleness (no action in this lineage)
- `get_encrypted_content_by_time_and_author` stub (upstream item 5): already ABSENT from this repo's coordinator (grep-verified 2026-07-16) — upstream-repo cleanup only.
- Cap-grant typo `get_many_encrypted_conten` (upstream item 2): fixed in this lineage since pass-4-coordinator-cleanup; remaining work is the upstream PR/merge record, owned by humm-tauri's checklist.

## C. Cross-cutting release blocker (not integrity, tracked here so it is not lost)

### C1. LICENSE application (DecraLicense)
- **Source:** humm-tauri `.newTasks/04_PROJECT_ReleasePackagingAndDistribution/04_UpstreamHappCaveatsAndLicense.md` item 1 + owner override 2026-07-04.
- **Observation:** license DECIDED (DecraLicense, owner's term) but the text, exact spelling/provenance, and who applies it are UNRECORDED in any repo. Legally blocks redistributing the bundled `.happ` in a downloadable RC. Zero wasm/DNA impact (repo-root text file).
- **Consideration:** blocked purely on obtaining the license text; apply at repo root the moment it exists and record provenance here + in humm-tauri's checklist.
