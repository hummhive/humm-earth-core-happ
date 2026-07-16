# humm-tauri × DNA pass roadmap

Per-pass concrete-task mapping. For each shipped (or in-flight) DNA
pass: **what changed in this repo** → **what humm-tauri must change**
→ **what features unlock**.

Designed to stay short. The *why* of each change lives in the
matching `PASS_<N>_DEPLOY_HANDOFF.md`; the *how* of the wire shape
lives in `HUMM_TAURI_ACLSPEC_INTEGRATION.md`; the *which TS file*
per feature lives in `HUMM_TAURI_FEATURE_ENABLEMENT.md`. This doc
exists so a humm-tauri PM/lead can see, at a glance, the discrete
work items each DNA pass produces — without re-reading the whole
handoff bundle.

**Update cadence.** Append a section per DNA-bumping pass at commit
time. Section format is fixed; do not refactor without updating
this preamble too.

**Status legend.**
- ✅ shipped on the DNA side (you can pull + integrate now)
- 🚧 in flight on a branch (not pushed; not on `main`)
- ⏳ planned; no branch yet
- 🟢 must-do for humm-tauri (blocking integration)
- 🟡 should-do (recommended; not blocking)
- 🔵 may-do (opportunistic; unlocks new feature)

---

## Pass-1 → Pass-2.5 (historical — migration path context)

**DNA status:** ✅ shipped. Pass-2 FINAL DNA hash
`uhC0kawoZqBxv3Jjvh-TlSQ5aO4U-hwiUNtZxFzXkTOBc5ijKVatw`; pass-2.5
held the hash + extended migration tooling.

**What changed in this repo:**
- Pass-1 (`feat-optional-recipient-id`) — C-class coordinator
  hardenings + migration scaffold (V1 markers); DNA hash bump from
  pass-0.
- Pass-2 (`feat-integrity-pass-2`) — I-H validated hive membership
  infrastructure (`HiveGenesis` + `HiveMembership` + `Role` enum +
  `check_hive_authority`); I-A receiver-initiated tombstone; I-C
  offline DM inbox; intentional DNA hash bump.
- Pass-2.5 (still on `feat-integrity-pass-2`) — coordinator-only
  follow-up: `MigrationMarkerV2` + `mark_migrated_v2` +
  `get_migration_marker_v2` + `scripts/migrate-dna.ts`
  hive-identity track + `DNA_MIGRATION_GUIDE.md` rewrite. DNA hash
  HELD byte-identical to pass-2 FINAL.

**humm-tauri tasks (completed — included for migration reference):**
- 🟢 Land coordinator hot-swap of the pass-2.5 .happ into
  `src-tauri/bin/`. APP_ID bump in
  `humm-tauri/src-tauri/src/holochain/install_humm_core_happ.rs`.
- 🟢 Switch every hive-creation flow to call
  `create_hive_genesis` then `create_hive_membership` (instead of
  the pre-pass-2 `installHiveSetupEntry` pattern). Stamp the
  returned `HiveGenesis` action hash as the cryptographic hive
  identity everywhere a hive is referenced internally.
- 🟢 Switch `list_my_hives` and `get_latest_membership` consumers
  to the new return shapes (`ListedHive`, `Option<HiveMembershipResponse>`).
- 🟢 Implement the pass-2.5 migration flow UX (export → migrate-hive
  → grant-memberships → import → mark-*). User-facing wizard;
  spawn `scripts/migrate-dna.ts` as a CLI from the Tauri side or
  call its functions directly.
- 🟡 Switch `derrivePublicKeyAcl` to consume the new pass-2 hive
  membership data instead of stale local mirrors.
- 🟡 Wire the new `Signal::EntryCreated { HiveGenesis | HiveMembership }`
  + `Signal::EntryDeleted` paths into the existing zomeSignals
  dispatcher.

**Features unlocked:**
- Migrating data forward across DNA upgrades (was: cold reinstall).
- Hive owner = cryptographic root of trust (was: client-side
  convention).

**Reference docs:**
- `docs/_archive/PASS_2_DEPLOY_HANDOFF.md`
- `docs/HUMM_TAURI_COORDINATOR_INTEGRATION.md`
- `docs/DNA_MIGRATION_GUIDE.md` (the pass-2.5 rewrite is the
  primary deploy reference)

---

## Pass-3 (`feat-integrity-pass-3-groups`, tip `b1e72aa`)

**DNA status:** ✅ shipped (branch tip pushed; not on `main`). New
DNA hash `uhC0kwO11VbVMLrFlQBqeslvnZroeHUp5VetnH1tgX68lH5FebRgC`.
Pass-3 INTENTIONALLY bumps the hash again — coordinator hot-swap
does NOT work; migration tooling required.

**What changed in this repo:**
- New entry types: `GroupGenesis`, `GroupMembership` (+ shared
  `Role` enum across hive + group layers).
- New link types: `AgentToGroupMemberships`, `GroupToGroupMemberships`,
  `HiveToGroups` (the cryptographic roster).
- `EncryptedContentHeader` reshape: four pass-2 fields
  (`hive_id`/`hive_genesis_hash`/`author_membership_hash`/`acl`)
  collapse into `acl_spec: AclSpec` discriminated union (HiveGroup
  / DirectMessage / Public / OpenWrite).
- Variant-dispatched validators with per-scope authority contracts.
- `validate_create_group_membership` rules 1-4 (no self-grant,
  Admin+ via Path A/B/C, no escalation, G-4.4 grant-window
  containment).
- M-1 (`validate_update_encrypted_content` author binding), L-1
  (`EncryptedContentUpdates` link author binding), L-2 (DM
  duplicate-recipient rejection).
- New coordinator externs: `create_group_genesis`,
  `create_group_membership`, `revoke_group_membership`,
  `get_latest_group_membership`, `list_group_members`,
  `list_my_groups`, `list_groups_in_hive`, `get_group_genesis`.
- `InboxEvent::GroupInvite = 3` discriminator (additive).
- Migration script extension: `classifyAclSpec` per-content-type +
  pass-3 wire-shape import.

**humm-tauri tasks (recommended path: leapfrog from pass-2.5
directly to pass-4; the items below are still required because
pass-4 inherits the pass-3 wire-shape change):**

| Task | Class | Files |
|---|---|---|
| Update `contentSchema.ts` to the AclSpec discriminated-union shape | 🟢 | `src/types/contentSchema.ts` |
| `addEntry` accepts `AclSpec` instead of `acl: Acl` | 🟢 | `src/api/core/hummContent/hummContentWrites.ts` |
| DM sidecar swaps `{acl, publicKeyAcl}` for `AclSpec::DirectMessage` | 🟢 | `src/sidecars/direct-messages/wire/content.ts:sendDirectMessage` |
| Compose (Public) swaps `buildPublicAcl` for `AclSpec::Public` | 🟢 | `src/containers/Compose/index.tsx` |
| Compose (Group) wraps existing fields in `AclSpec::HiveGroup` | 🟢 | `src/api/core/hummContent/hummContentWrites.ts` (callsite) |
| `ManageGroup`/`ManageMember`/`Invites` call `create_group_genesis` + `create_group_membership` (was: writing `Group`/`GroupMemberList`/`Member`/`Invite` content entries) | 🟢 | `src/containers/MembersAndGroups/Groups/ManageGroup/`, `.../Members/ManageMember/`, `.../Invites/ManageInvite/` |
| `derrivePublicKeyAcl` walks `list_group_members(group_genesis_hash)` | 🟢 | `src/api/core/acl/index.ts:derrivePublicKeyAcl` |
| Demote `GroupMemberList` reads to display cache (writes still allowed) | 🟡 | `src/api/content/groupMemberList/index.ts` |
| Add `useGroupMembersAuthoritative(group_genesis_hash)` hook | 🟡 | `src/state/group/index.ts` |
| Wire member-request flow to `AclSpec::OpenWrite { target: Some(hive) }` | 🟢 | `src/api/content/memberRequest/index.ts` |
| Implement MemberRequests pane (currently stubbed) | 🔵 | `src/containers/MemberRequests/` (NEW) |
| Hive-discovery publishes via `AclSpec::OpenWrite { target: None }` | 🟢 | `src/api/content/hiveDiscovery/index.ts` |
| Optional: HiveDirectory cross-network browser UI | 🔵 | `src/containers/HiveDirectory/` (NEW) |
| Inbox poller handles `InboxEvent::GroupInvite = 3` (or use `list_my_groups`) | 🟡 | inbox consumer of your choice |
| Add new Signal variants to dispatcher: `GroupGenesis`/`GroupMembership` create | 🟡 | `src/api/core/holochain/zomeSignals.ts` |

**Features unlocked (forgery-proof; cross-hive viable):**
- Forgery-proof groups/roles/ACLs (E.4.g).
- Cross-hive group chat (E.4.b) via `humm-sidecar-group-message-v1`
  content type + `AclSpec::HiveGroup`.
- Member-request outsider knock with verifiable inbox (E.4.d).
- Cross-network hive-discovery (E.4.c).
- Per-content ACL picker on Compose (E.4.f).
- Local media library with selective sharing (E.4.e).
- Note-to-Self / personal vault (E.4.j) — single **and** multi-device,
  pass-4 today, **no DNA change**: a user-authored *device-set* group +
  empty-PKA self shape + SharedSecret self-wrap. Spec:
  `docs/HUMM_TAURI_SELF_NOTES_INTEGRATION.md`.
- **Pre-signed invite links (Discord-style) — E.4.l**. Uses only
  `Public` + `OpenWrite` + `create_hive_membership`; ship anytime
  after pass-3 lands. NOT blocked on pass-4.
- Streaming meta-layer hardening (E.4.h, forward-looking).
- Sidecar marketplace (E.4.i, forward-looking).

**What is NOT closed by pass-3:**
- Recipient-list forgery on HiveGroup `public_key_acl` (G-6.2).
  Pass-3 docs warn "treat public_key_acl as routing hint"; pass-4
  closes it cryptographically.
- Hive-layer G-4.4 (group layer closed; hive layer mirrored by
  pass-4).

**Reference docs:**
- `docs/_archive/PASS_3_DEPLOY_HANDOFF.md`
- `docs/HUMM_TAURI_ACLSPEC_INTEGRATION.md` (canonical wire shape)
- `docs/HUMM_TAURI_FEATURE_ENABLEMENT.md` (per-feature wiring,
  including E.4.l invite links)
- `docs/HUMM_TAURI_SELF_NOTES_INTEGRATION.md` (note-to-self
  architecture, wire shapes, BDD scenarios, security footguns L1–L9)
- `docs/HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md` (Given/When/Then
  validator sanity checks across the whole trust chain)
- `docs/HUMM_TAURI_SELF_NOTES_OBSERVABILITY.md` (order-of-operations +
  observability/security touchpoints to log)
- `docs/HUMM_TAURI_DM_MESSAGING_INTEGRATION.md` (direct messaging:
  humm:// link + `enc`, in-hive + cross-hive, first-contact handshake,
  BDD + observability + security)

---

## Pass-4 (`feat-integrity-pass-4-recipient-witnesses`, tip `8503b48`)

**DNA status:** ✅ shipped. DNA hash
`uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`;
hApp SHA256 (pass-4 FINAL) `d74e5f2f…1861`. The DNA hash is UNCHANGED across all
subsequent coordinator hot-swaps, so the humm-tauri DNA-hash pins (`coordinator.rs`,
`tests/bdd/conductor.ts:23`, `.testdata/happs/MANIFEST.tsv`) do NOT move — only the
bundled happ sha advances. **Current bundled target: `v1.0.0` / `2205337c`** (see
"Pass-4 coordinator follow-ups + downstream RC status" below).

**What changed in this repo:**
- G-6.2 — `AclSpec::HiveGroup` gains REQUIRED
  `recipient_witnesses: Vec<RecipientWitness>` field. Each
  witness names a real `GroupMembership` for a pubkey in
  `public_key_acl`; validator enforces bidirectional cross-check
  with bucket dominance + per-witness `must_get_valid_record`.
  `HIVEGROUP_MAX_WITNESSES = 256` cardinality bound. Closes
  attack #5 (recipient-list forgery for routing fan-out).
- G-4.4 hive back-port — `enforce_hive_grant_window` mirrors the
  pass-3 group rule; an expiring Path-2 hive grantor can no longer
  extend the delegation window or mint permanent grants.
- New types exposed in the wire shape: `AclBucket` enum
  (`Owner` > `Admin` > `Writer` > `Reader`), `RecipientWitness`
  struct.
- Coordinator surface unchanged (`acl_spec` is passed through
  verbatim; the variant carries its new field internally).
- Migration script (`scripts/migrate-dna.ts`) doc-comment refresh
  reflecting the new wire shape; classifier still throws on
  HiveGroup until Phase D.1 ships.

**humm-tauri tasks (in addition to the pass-3 list above):**

| Task | Class | Files |
|---|---|---|
| Extend `contentSchema.ts` types with `AclBucket` + `RecipientWitness` + `recipient_witnesses` field on `AclSpec::HiveGroup` | 🟢 | `src/types/contentSchema.ts` |
| Implement `stampWitnessesFromGroupAcl` helper (recipe in `_archive/PASS_4_DEPLOY_HANDOFF.md` § "REQUIRED humm-tauri callsite update") | 🟢 | `src/api/core/acl/stampWitnesses.ts` (NEW) |
| Call `stampWitnessesFromGroupAcl` from every `AclSpec::HiveGroup` write site before `create_encrypted_content` | 🟢 | every callsite under `src/api/content/**` + `src/containers/MembersAndGroups/**` + Compose-with-group-scope |
| Surface witness-stamping errors as user-facing "this person is no longer a group member" rather than committing a doomed entry | 🟢 | wrapper around `create_encrypted_content` in the callsite layer |
| OPTIONAL: surface "your admin role expires at X" hint in Invite/Manage flows when inviter holds an expiring hive admin membership (G-4.4 hive-layer UX) | 🔵 | `src/containers/MembersAndGroups/Invites/`, `Members/ManageMember/` |
| Update APP_ID for the pass-4 hApp bundle | 🟢 | `humm-tauri/src-tauri/src/holochain/install_humm_core_happ.rs` |
| Run pass-4 migration wizard (re-uses pass-3 migration script; no new operator commands) | 🟢 | reuse the pass-3 wizard |

**Features unlocked (delta from pass-3):**
- Recipient-list forgery closed cryptographically (attack #5).
  `public_key_acl` on HiveGroup is now load-bearing.
- Hive-layer delegation-window discipline matches group-layer.
- No new product capabilities — pass-4 is a security closure pass.

**Features unblocked by pass-3 that humm-tauri may now start (no
need to wait for pass-4):**
- E.4.l pre-signed invite links — uses only `Public` + `OpenWrite`;
  pass-4 leaves both untouched.

**Reference docs:**
- `docs/_archive/PASS_4_DEPLOY_HANDOFF.md`
- `docs/HUMM_TAURI_ACLSPEC_INTEGRATION.md` (§ 4 + § 5 updated)
- `docs/HUMM_TAURI_FEATURE_ENABLEMENT.md`
- `docs/_archive/HANDOFF_UPDATED_INFO.md` § "What is enforced now (formerly
  deferred) — pass-4 G-6.2 SHIPPED"

---

## Leapfrog path (pass-2.5 → pass-4, skipping pass-3)

humm-tauri shipped the leapfrog: the production app bundles the pass-4
hApp directly, skipping pass-3's intermediate `acl_spec`-without-witnesses
shape. **Both** the pass-3 and pass-4
humm-tauri task tables above still apply (pass-4 inherits the pass-3
wire-shape change) — the leapfrog just means doing them in a single
integration pass instead of two.

The five ingredients, at a glance:

| # | Ingredient | Where |
|---|---|---|
| (a) | **Fields removed** from the top-level header: `hive_genesis_hash`, `author_membership_hash`, legacy squuid `acl`; plus `hive_id` → `display_hive_id` rename (display-only, not validator-trusted) | ACLSPEC § 1, § 11 |
| (b) | **Fields added**: `acl_spec` discriminated union; `recipient_witnesses` on the `HiveGroup` variant (pass-4 G-6.2) | ACLSPEC § 1, § 4 |
| (c) | **Per-content-type AclSpec variant** selection | ACLSPEC § 2 (classification table) |
| (d) | **`recipient_witnesses` stamping** via `stampWitnessesFromGroupAcl` | `_archive/PASS_4_DEPLOY_HANDOFF.md` § "REQUIRED humm-tauri callsite update"; ACLSPEC § 5 |
| (e) | **Roster reads** switch to `list_group_members(group_genesis_hash)` | ACLSPEC § 5 (`deriveHiveGroupPublicKeyAcl`) |

The five `pass-3-target` markers in the humm-tauri codebase map
one-to-one to drop-in recipes in ACLSPEC § 11:

- `hummContentTransforms.ts:21` (`entryToCamelCase`, decode) → Recipe A
- `hummContentTransforms.ts:58`/`:62` (`entryToSnakeCase`, encode) → Recipe B
- `hummContentWrites.ts:165` (`addEntry`, write) → Recipe C
- `SidecarCapabilitiesService.ts:674` (`createEncryptedContent`, sidecar write) → Recipe D

🟢 (a)–(d) are blocking for any `AclSpec::HiveGroup` write; (e) is
🟢 for correct PKA derivation. 🔵 the G-4.4 hive-layer "your role
expires at X" hint is opportunistic UX.

---

## D.1 — group migration track (`feat-migration-d1-group-track`)

**DNA status:** ⏳ planned (separate branch; no DNA bump — tooling
only). Required to migrate **pre-existing group-scoped content**
from pass-1/2 to a HiveGroup wire shape on the pass-4 DNA. Without
D.1, the classifier defaults to `Public` for unknown content types
and operators must manually re-stamp HiveGroup entries on the new
DNA after migration.

**What will change in this repo (D.1 scope):**
- `scripts/migrate-dna.ts` gains `migrate-group`,
  `grant-group-memberships`, `mark-group-migrated` CLI commands
  (parallel to the existing hive-identity track).
- Per-bundle `classification-overrides.json` mechanism — operator
  authors per-old-action-hash overrides before running `import`.
- `classifyAclSpec` HiveGroup branch becomes functional: walks
  `get_latest_group_membership` per PKA pubkey to populate
  `recipient_witnesses`.
- `docs/DNA_MIGRATION_GUIDE.md` gains a "Group track +
  classification overrides" section.

**humm-tauri tasks (when D.1 ships):**
- 🔵 Surface a post-migration UI wizard that consumes the D.1 CLI
  to materialise legacy groups + memberships on the new DNA. (UX
  alternative to operator-run CLI.)
- 🔵 Re-emit affected `EncryptedContent` entries via the wizard
  using `AclSpec::HiveGroup { ..., recipient_witnesses: ... }`
  populated by `stampWitnessesFromGroupAcl`.
- The actual `stampWitnessesFromGroupAcl` work is already on the
  pass-4 task list above; D.1 just supplies the migrated groups
  + memberships it needs.

**Features unlocked:**
- Forward-migrating legacy group-scoped content to the pass-4
  wire shape with full G-6.2 enforcement (no operator manual
  re-stamping step).

**Reference docs:**
- `docs/DNA_MIGRATION_GUIDE.md` (will gain the D.1 section)
- The D.1 branch's own deploy handoff (TBA when the branch ships).

---

## Integration test coverage (`test-tryorama-integrity-coverage`)

**DNA status:** ✅ shipped (separate branch; no DNA bump — test infra
only). **Tryorama 0.19.2 is broken** on holochain/hc 0.6.0 (the
`quic` → `webrtc` sandbox-CLI rename produces a K2Error panic).
Integration tests use **tryorama-free in-process conductor harnesses**:
`crates/sweettest/` (this repo — in-process holochain conductor, the current path,
2/2 green on v1.0.0) and `tests/bdd/` (humm-tauri, 20+ scenarios incl. DM,
note-to-self, AclSpec). Note: humm-tauri's S1 stack hop REVIVED tryorama 0.19.2 on
their **0.6.1** stack (specs 00/01/10 green); in THIS repo on **0.6.0** tryorama
still can't boot (quic→webrtc CLI rename) — use Sweettest here.

**humm-tauri tasks:** None. This branch ships test infrastructure
and does not affect the wire shape, externs, or DNA hash.

---
## Pass-4 coordinator follow-ups + downstream RC status (2026-06-13)

Two coordinator hot-swaps shipped on top of pass-4 FINAL — DNA hash HELD `uhC0k26b`
(integrity wasm byte-identical `06b01fb3`; no chain fork, no migration). Lineage:
pass-4 FINAL `d74e5f2f` → recv-signal-fix `4aacd52f` → **query-tolerance `2205337c`
= `v1.0.0` (current)**.

| Coordinator gen | hApp | content.wasm | What changed |
|---|---|---|---|
| recv-signal-fix | 4aacd52f | cb51c376 | `recv_remote_signal` ExternIO pre-encode — cross-host signals deliver |
| query-tolerance (v1.0.0) | 2205337c | 78f0602e | decode-tolerant queries (`get_many` filter_map; `list_my_hives`/`_groups` + `get_latest_membership` `.ok().flatten()`) |

**Downstream RC status** — reconciled against humm-tauri `dm-invite-media-pickup`@`f61dbeaa`:
`ROADMAP.md`@`198b1127` (2026-06-11) + `.newTasks/00_RC_CRITICAL_PATH.md`@`cfcafd61`
(2026-06-13), both clean (committed). They're mid merge/commit dance — if these SHAs
have moved, re-check before trusting the statuses below:
- 🟢 **recv_remote_signal fix SHIPPED + live-proven** — activated via a bundled
  fix-coordinator + `COORDINATOR_WASM_VERSION` hot-swap (DNA unchanged); relay→relay
  real-time `EncryptedContentSignal` PUSH verified live.
- 🟢 **pass-4 wire shape live-proven** at humm-tauri's S1 (relay↔relay + GUI↔relay
  decrypted round-trips; auto-ack chains).
- 🟡 **Bundled-happ refresh** (their RC task `02/04/01`, "content_integrity 0.0.2 real
  author validation") = adopt the real pass-4 happ → satisfied by `v1.0.0`/`2205337c`
  (integrity `06b01fb3` = content_integrity 0.0.2, carrying pass-1 `check_author_matches_header`).
- 🟡 **Coordinator wire C0/C2/C5** in progress downstream
  (`02_PROJECT_CoordinatorWireAndDmScaling`): C0 `get_messages_since` dispatch, C2
  `list_by_hive_link` pagination, C5 `get_many` cap-grant. query-tolerance already lands
  the `get_many` all-or-nothing fix.
- ⏳ **Cross-hive DM body decrypt** (their `02_A`) is humm-tauri-side (`extractRecord` /
  pass-4 `entryToCamelCase` shim + pair-SS `groupId`) — not an earth-core change.

**Stack skew to track:** humm-tauri's S1 migrated fleet + GUI to **holochain 0.6.1 /
kitsune2 0.4.1 / iroh transport** (DNA hash deliberately preserved). earth-core still
builds on **0.6.0** (hdi 0.7.0 / hdk 0.6.0). The `2205337c` happ (0.6.0-built, DNA
`uhC0k26b`) is consensus-compatible with their 0.6.1 conductor today; a future
earth-core bump to the 0.6.1 line should preserve the DNA hash (verify on rebuild) to
stay a hot-swap.

---
## Pass-5 (owner role, v2.0.0) + Pass-6 (validation hardening, v3.0.0) — short form

Both passes carry full per-pass deploy handoffs; this roadmap keeps only the
lineage line. Detail lives there:

- **Pass-5 / v2.0.0** — ✅ shipped + adopted. DNA `uhC0k2dX…`, happ `42dbf9df…`.
  Hive Owner role (offer/accept handshake) + reader read-only + role-grant
  hardening + GroupGenesis EntryType filter. Cutover contract:
  [`_archive/PASS_5_DEPLOY_HANDOFF.md`](./_archive/PASS_5_DEPLOY_HANDOFF.md) +
  [`HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md`](./HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md).
- **Pass-6 / v3.0.0** — ✅ shipped, blessed 2026-07-02. DNA `uhC0ksXs…`, happ
  `3062de38…`. Integrity module refactor + `OriginalHashPointer` link
  validation (native update-chain root) + cross-entry-type update gate; wire
  contract byte-preserved from pass-5. Runbook:
  [`PASS_6_DEPLOY_HANDOFF.md`](./PASS_6_DEPLOY_HANDOFF.md).
  **2026-07-03: humm-tauri live validation COMPLETE on the v3.0.0 canary**
  (GUI + 2 relays, cache-off): DM 2×2, invite loop on `@6`, uploads all 4
  scopes, byte-exact media links; zero DNA-side issues. Remaining findings
  were app-side (their `.newTasks/…/13_HeadlessMigrationFindings.md` @
  `0af39311`).

---
## Pass-6 coordinator generation — idempotent-writes (v3.2.0, 2026-07-16)

Coordinator-only hot-swap on top of pass-6-pinned-hosts/v3.1.0 — **DNA hash
HELD `uhC0ksXs…`** (integrity wasm byte-identical `2656a910…`; no chain
fork, no migration). Ships the fleet-audit §B roll-up: crash-resume
idempotent writes, the legacy hive-id SharedSecret remediation, and the
multi-hive read batch. Full wire contract + BDD sanity skeletons:
[`HUMM_TAURI_IDEMPOTENT_WRITES_INTEGRATION.md`](./HUMM_TAURI_IDEMPOTENT_WRITES_INTEGRATION.md).

| Coordinator gen | hApp | content.wasm | What changed |
|---|---|---|---|
| pass-6-idempotent-writes (v3.2.0) | bfe357aa | 3b5348eb | find-or-create family (`find_or_create_encrypted_content` / `find_or_create_group_genesis` / `find_or_create_group_membership`); hiveless remediation pair (`list_my_hiveless_content` + `remediate_hiveless_content`); `fetch_pair_ss_with_hive_check` optional-hive union; `mark_migrated_v2`/`get_migration_marker_v2` HiveGenesis create-based markers; `content_summary_many`; `send_dm_delete_request` doc-deprecated |

**humm-tauri tasks:** pin new label/SHA (`CURRENT_HAPP_LABEL`,
`CURRENT_HAPP_SHA256`), bump `COORDINATOR_WASM_VERSION` 10→11, keep DNA/app
id `humm-earth-core-happ@6`; adopt per their
`.newTasks/14_MISC_PlatformInfraAndDataLayer/16_Pass6IdempotentWritesIntegration.md`.
Every legacy extern is wire-identical; the only changed request struct is
`FetchPairWithHiveCheckInput` (additive-compatible).

---
## Pass-6 coordinator generation — pinned-hosts (v3.1.0, 2026-07-16)

Coordinator-only hot-swap on top of pass-6/v3.0.0 — **DNA hash HELD
`uhC0ksXs…`** (integrity wasm byte-identical `2656a910…`; no chain fork, no
migration). Ships the seams humm-tauri's Persistent Blob Storage Keystone
("pinned hosts") waits on. Full wire contract + BDD sanity skeletons:
[`HUMM_TAURI_PINNED_HOSTS_INTEGRATION.md`](./HUMM_TAURI_PINNED_HOSTS_INTEGRATION.md).

| Coordinator gen | hApp | content.wasm | What changed |
|---|---|---|---|
| pass-6-pinned-hosts (v3.1.0) | 1c7d981b | cc904ad6 | `latest_action_micros` on responses; `BlobPinSignal` family + `send_blob_pin_signal`; bounded source-cursor page externs (`list_by_hive_link_page` / `list_by_dynamic_link_page` / `list_by_author_page`); exact-own `get_my_content_by_id_link` |

**humm-tauri tasks:** pin new label/SHA (`CURRENT_HAPP_LABEL`,
`CURRENT_HAPP_SHA256`), bump `COORDINATOR_WASM_VERSION` 9→10, keep DNA/app id
`humm-earth-core-happ@6`; re-enable the ignored acceptance test
`full_source_page_replays_when_the_coordinator_emits_source_cursors`
(envelope field names match 1:1). Legacy externs (incl. F1-critical
`list_by_author`) are wire-identical — no other callsite changes.

## Pass-7 candidate considerations (unscheduled)

Collected from downstream field reports; NOT commitments. Full batch
catalogue (fork once, carry everything — owner principle 2026-07-16):
`.newTasks/pass-7-integrity-candidates.md`.

1. **Stable cross-generation content identity** (humm-tauri, 2026-07-03
   validation report). Migration re-authors every entry under new action
   hashes, so any app-side processed-set keyed by content id resurfaces
   after each pass-N→N+1 (e.g. accepted invite-redemptions re-prompted;
   they fixed it semantically — already-a-member ⇒ suppressed). A durable
   content identity that survives re-authoring (e.g. a first-generation
   origin id carried through migration imports) would erase this failure
   class wholesale. Wire-shape impact: additive field(s) with
   `#[serde(default)]` — still a DNA fork by definition (integrity change).
2. Review WARN follow-ups from the pass-6 sec-holo review remain candidates
   for any next integrity fork: discovery-link reindex-on-update,
   `public_key_acl` bounds (`sec-holo-review/findings-catalog.md`
   C-WARN-2/3 + open decision points).

---


## How to add a new pass section

When a new DNA pass commits + the deploy handoff lands:

1. Append a new `## Pass-N (...)` section to this file using the
   template above.
2. Update the prior pass's "What is NOT closed" callout to
   reflect what the new pass closes.
3. Refresh the humm-tauri-task table: keep rows that still apply
   (pass-3 rows still apply post-pass-4), add rows for the new
   pass's deltas.
4. Cross-link the new pass's `PASS_<N>_DEPLOY_HANDOFF.md` and any
   other doc updates.

The goal is a fixed-format, append-only changelog from
humm-tauri's perspective. Avoid back-editing the section headers
or task IDs (downstream commits may reference them).
