# Handoff updated info — pass-4 in flight

**Audience:** humm-tauri devs. The currently-shipping integration
target on the humm-tauri side is pass-2.5 (coordinator hot-swap
work, nearing completion). Pass-3 and pass-4 layer on top: pass-3
reshapes `EncryptedContentHeader` (one wire-shape break) and
pass-4 adds one new required field on the `HiveGroup` variant
(`recipient_witnesses`). Once pass-2.5 lands downstream, the
recommended path is **leapfrog directly to pass-4** — both
changes are mechanical and well-documented, and pass-4 is the
current security-complete target.

Reference docs to start at:
[`HUMM_TAURI_PASS_ROADMAP.md`](./HUMM_TAURI_PASS_ROADMAP.md)
(per-pass concrete-task mapping — start here if you want a
task-list view of "what's required when we land pass-N"),
[`PASS_4_DEPLOY_HANDOFF.md`](./PASS_4_DEPLOY_HANDOFF.md) (deploy +
migration mechanics),
[`HUMM_TAURI_ACLSPEC_INTEGRATION.md`](./HUMM_TAURI_ACLSPEC_INTEGRATION.md)
(per-modal wiring + types),
[`HUMM_TAURI_FEATURE_ENABLEMENT.md`](./HUMM_TAURI_FEATURE_ENABLEMENT.md)
(feature-by-feature implementation guide).

**Purpose:** living delta between the pass-3 docs and the in-flight
pass-4 work on branch `feat-integrity-pass-4-recipient-witnesses`.
Re-pull this file periodically — it is updated after every pass-4
phase commit so you can catch upstream shifts before they bite.

**Status of pass-4 itself:** branch lives on
`feat-integrity-pass-4-recipient-witnesses` (off
`feat-integrity-pass-3-groups` tip `b1e72aa`); NOT pushed; NOT
merged to `main`. The DNA hash WILL bump again when pass-4 lands.

---

## TL;DR — what's new since pass-3

- **G-6.2 recipient-set integrity SHIPPED in pass-4** (Phase 4-A/B/C).
  `AclSpec::HiveGroup` gains a required `recipient_witnesses:
  RecipientWitness[]` field; the validator enforces every PKA
  pubkey is backed by a real `GroupMembership` in a dominating
  bucket of `group_acl`. Closes attack #5 — `public_key_acl` on
  HiveGroup is now load-bearing, not just a routing hint.
- **G-4.4 back-ported to `HiveMembership`.** An expiring Path-2
  hive grantor can no longer extend the delegation window or mint
  permanent grants. Pass-3 closed this at the group layer; pass-4
  mirrors it one level up.
- **One required humm-tauri callsite change**: stamp
  `recipient_witnesses` on every `AclSpec::HiveGroup` write. Use the
  `stampWitnessesFromGroupAcl` helper recipe documented in
  [`PASS_4_DEPLOY_HANDOFF.md`](./PASS_4_DEPLOY_HANDOFF.md) §
  "REQUIRED humm-tauri callsite update" and referenced from
  [`HUMM_TAURI_ACLSPEC_INTEGRATION.md`](./HUMM_TAURI_ACLSPEC_INTEGRATION.md)
  § 5. Non-HiveGroup variants (DM, Public, OpenWrite) are unchanged.

---

## Available features (post-pass-3, no DNA work needed)

Pass-3 already shipped every primitive needed for **pre-signed
invite links** (Discord-style one-click join). humm-tauri can ship
E.4.l in
[`HUMM_TAURI_FEATURE_ENABLEMENT.md`](./HUMM_TAURI_FEATURE_ENABLEMENT.md)
in parallel with the pass-4 wire-shape migration — no waiting on
pass-4. The flow uses only `AclSpec::Public` (invite entry) +
`AclSpec::OpenWrite` (redemption) + existing `create_hive_membership`
(mint) + existing inbox links (notification). See E.4.l for the
8-step recipe.

---

## TL;DR — original pass-3 → pass-4 transition guidance

- **Keep going with the pass-2.5 integration.** Nothing in pass-3
  invalidates the pass-2 externs you're calling
  (`create_hive_genesis`, `create_hive_membership`, `list_my_hives`,
  `get_latest_membership`) or the pass-2 entry types
  (`HiveGenesis`, `HiveMembership`). Those names + signatures + wire
  shapes survive pass-3 + pass-4 unchanged.
- **One wire-shape break shipped in pass-3** (Phase C):
  `EncryptedContentHeader` reshaped — `hive_id`, `hive_genesis_hash`,
  `author_membership_hash`, and `acl` collapsed into a single
  `acl_spec: AclSpec` discriminated-union field.
- **One wire-shape addition shipping in pass-4** (Phase 4-A/B):
  `AclSpec::HiveGroup` gains `recipient_witnesses:
  RecipientWitness[]`. Required on every HiveGroup write.
- **Group / role / ACL enforcement is FULLY MOVED** from client-side
  convention to integrity-zome enforcement (pass-3 closed authority;
  pass-4 closes routing-fan-out attribution).
- **DNA hash WILL bump again with pass-4** — re-run the migration
  tooling once pass-4 ships.
---

## What is UNCHANGED — your pass-2.5 work stands

Every API surface listed below is preserved by pass-3 verbatim:

### Externs (coordinator zome `content`)

- `create_hive_genesis(CreateHiveGenesisInput { display_id }) ->
  HiveGenesisResponse { genesis, hash }`
- `create_hive_membership(CreateHiveMembershipInput {
  hive_genesis_hash, for_agent, role, grantor_membership_hash, expiry })
  -> HiveMembershipResponse { membership, hash }`
- `list_my_hives() -> Vec<ListedHive>`
- `get_latest_membership({ agent, hive_genesis_hash }) ->
  Option<HiveMembershipResponse>`
- `create_encrypted_content`, `update_encrypted_content`,
  `delete_encrypted_content` — extern *names* and call style preserved;
  one field inside the input changes in Phase C (see below).

### Entry types

- `HiveGenesis { display_id, created_at_microseconds }` — unchanged.
- `HiveMembership { hive_genesis_hash, for_agent, role,
  grantor_membership_hash, expiry }` — unchanged.
- `EncryptedContent { header, bytes }` outer shape — unchanged.
- `DmProbeLog` (private) — unchanged.

### Link types

- `Hive`, `Dynamic`, `HummContentId`, `HummContentOwner`,
  `HummContentAdmin`, `HummContentWriter`, `HummContentReader`,
  `EncryptedContentUpdates`, `OriginalHashPointer`, `TimePath`,
  `TimeItem`, `Inbox` — all preserved. Validators unchanged for the
  hive-scoped links (they still extract `hive_genesis_hash` from the
  target entry's header; the field's *location* inside the header
  changes in Phase C, but the validator follows it).

### `HiveRole` enum

`HiveRole = Owner | Admin | Writer | Reader` — variant set unchanged.

On the Rust side, the enum has been **renamed to `Role`** so it can be
shared between the hive and group layers, with a compatibility alias
`pub use self::Role as HiveRole;`. Every existing import path
(`use content_integrity::HiveRole;`, `HiveRole::Admin`, etc.) still
resolves identically. **No TS changes are required.** The serialized
form is identical (msgpack tags on variant names — those are
unchanged).

### DNA hash (latest landed)

- Pass-2 / pass-2.5 final DNA hash: `uhC0kawoZqBxv3Jjvh-TlSQ5aO4U-hwiUNtZxFzXkTOBc5ijKVatw`
- Pass-2.5 integrity wasm sha256: `cd137fcfde8632c7236497592014b3b1e80548691ee71e5ebbad12cb373275dc`

These remain the deployable invariants until pass-3 commits a new
`.baseline-hashes.txt` section. Until then, build against pass-2.5
artifacts.

### Migration tooling

- `scripts/migrate-dna.ts` four-phase hive-identity track — unchanged.
  Pass-3 *extends* this script with a group track and a
  content-type → `AclSpec` classification step; it does not break the
  existing hive flow.
- `MigrationMarkerV1`/`MigrationMarkerV2` decoder + well-formedness
  checker — unchanged.

---

## What is CHANGING — plan a follow-up

### `EncryptedContentHeader` wire shape (Phase C — COMMITTED *(this commit)*)

**Current pass-2 shape** (what you are integrating against today):

```rust
pub struct EncryptedContentHeader {
    pub id: String,
    pub hive_id: String,                        // routing/display
    pub hive_genesis_hash: ActionHash,          // security-load-bearing
    pub author_membership_hash: Option<ActionHash>, // security-load-bearing
    pub content_type: String,
    pub acl: Acl,                               // security-load-bearing
    pub public_key_acl: Acl,
    pub revision_author_signing_public_key: String,
}
```

**Pass-3 target shape** (the migration you will need to do):

```rust
pub struct EncryptedContentHeader {
    pub id: String,
    pub display_hive_id: String,                // was `hive_id`; routing/display
    pub content_type: String,
    pub acl_spec: AclSpec,                      // NEW: variant-dispatched authority
    pub public_key_acl: Acl,
    pub revision_author_signing_public_key: String,
}

pub enum AclSpec {
    /// In-hive group-scoped content. Validator enforces hive membership +
    /// per-group authority. Closes the modified-coordinator poisoning
    /// class for group content.
    HiveGroup {
        hive_genesis_hash: ActionHash,
        author_membership_hash: Option<ActionHash>,
        group_acl: AclByGroupGenesis,           // ActionHash refs to GroupGenesis
        author_group_membership_hash: Option<ActionHash>,
    },
    /// Cross-hive direct sender↔recipient(s). Pair or small group.
    /// Validator enforces author∈recipients + cardinality bounds.
    /// No hive/group membership check on recipients.
    DirectMessage {
        recipients: Vec<AgentPubKey>,
    },
    /// World-readable content authored under a hive context.
    /// Validator requires Writer+ in the named hive.
    Public {
        hive_genesis_hash: ActionHash,
        author_membership_hash: Option<ActionHash>,
    },
    /// Open-write content (member-request, cross-network discovery).
    /// Validator only checks author identity + target existence.
    OpenWrite {
        target_hive_genesis_hash: Option<ActionHash>,
    },
}
```

**Per-content-type mapping** (preview — the canonical table will live
in `docs/HUMM_TAURI_ACLSPEC_INTEGRATION.md` when Phase E lands):

| Current humm-tauri code path | Pass-3 `AclSpec` variant |
|---|---|
| `Group`, `GroupMemberList`, `Member`, `Invite`, `InviteAccept`, `InvitePurge`, hive-scoped UI shared state, sidecar config/install/provider | `HiveGroup` |
| `direct_message` (DM sidecar), `peer-identity-claim-v1`, pair `shared-secrets-v1` | `DirectMessage { recipients }` |
| `humm-addon-text-post-v1` (public posts), `Hive` content entry | `Public { hive_genesis_hash }` |
| `member-request-v1`, `hive-discovery-v1`, planned `agent-directory-v1`, planned `sidecar-manifest-v1` | `OpenWrite { target_hive_genesis_hash }` |

**Migration mechanics** when pass-3 ships:

- The pass-3 `scripts/migrate-dna.ts` extends to classify every entry's
  `content_type` into the right `AclSpec` variant and re-stamp on import.
- The pass-1 → pass-2 → pass-3 leapfrog is supported in a single
  migration run (you do not need to deploy pass-2 first).
- For *new* code you write today: build the pass-2 header as documented;
  when the pass-3 follow-up lands, the migration becomes mechanical —
  most call sites just wrap the existing fields in
  `acl_spec: AclSpec::HiveGroup { .. }` (or the appropriate variant per
  the table above).

### New coordinator externs (Phase B — NOT committed yet)

Group authority + roster management. None of these exist today; you do
not need them for pass-2.5 work, but they will be available when
pass-3 lands:

- `create_group_genesis({ hive_genesis_hash, display_id,
  hive_wide_role, creator_hive_membership_hash }) -> GroupGenesisResponse`
- `create_group_membership({ group_genesis_hash, for_agent, role,
  grantor_membership_hash, grantor_hive_membership_hash, expiry }) ->
  GroupMembershipResponse`
- `revoke_group_membership({ membership_hash, new_expiry })` — ergonomic
  helper that issues a fresh `GroupMembership` with past `expiry`
- `get_latest_group_membership({ agent, group_genesis_hash }) ->
  Option<GroupMembershipResponse>`
- `list_group_members(group_genesis_hash) ->
  Vec<GroupMembershipResponse>` — **the cryptographic roster**
- `list_my_groups() -> Vec<ListedGroup>`
- `list_groups_in_hive(hive_genesis_hash) -> Vec<ListedGroup>`
- `get_group_genesis(action_hash) -> Option<GroupGenesisResponse>`

**`GroupMemberList` will be demoted** from authority to display cache.
When pass-3 lands, switch `derrivePublicKeyAcl` and group-member
enumeration calls from `GroupMemberListApi.get` to
`list_group_members(group_genesis_hash)`.

### New `post_commit` signal variants (Phase B)

Append to your `Signal` discriminator handler:

- `GroupGenesisCreated`
- `GroupMembershipCreated`
- `GroupMembershipRevoked`

The pass-2 signal variants are preserved.

---

## What is ALREADY COMMITTED in pass-3 — Phase A (integrity core)

The following ARE in the working tree on `feat-integrity-pass-3-groups`
and have passed all three reviewer gates (rust-reviewer,
security-reviewer, coding-standards). Awaiting Phase A commit.

### New integrity entries (appended at end of `EntryTypes`)

- `GroupGenesis { hive_genesis_hash, display_id, hive_wide_role,
  creator_hive_membership_hash, created_at_microseconds }`
- `GroupMembership { group_genesis_hash, for_agent, role,
  grantor_membership_hash, grantor_hive_membership_hash, expiry }`

Both are public, immutable (update + delete both reject), revoked via
`expiry`. Indices 4 and 5; entries 0..=3 (your `HiveGenesis`,
`HiveMembership`, `EncryptedContent`, `DmProbeLog`) keep their slots.

### New link types (appended at end of `LinkTypes`)

- `AgentToGroupMemberships` — base = `for_agent`, target =
  `GroupMembership`. Forward index ("my group memberships").
- `GroupToGroupMemberships` — base = `group_genesis_hash`, target =
  `GroupMembership`, tag = `for_agent.to_string()`. Reverse index;
  this is the **cryptographic roster** backing `list_group_members`.
- `HiveToGroups` — base = `hive_genesis_hash`, target = `GroupGenesis`.
  Enumerate a hive's groups.

Indices 12..=14; the pass-2 link types (0..=11) keep their slots.

### Shared `Role` enum

`HiveRole` renamed to `Role` in `hive.rs` with compatibility alias
`pub use self::Role as HiveRole;`. No external API change.

### `check_group_authority` helper

Three-path authority check for group-scoped content:

- **Path A** — group author (implicit Owner).
- **Path B** — hive sovereignty: any hive Admin+ of the parent hive
  controls every group in their hive. The hive genesis author (root
  sovereign) is caught here even with no witness.
- **Path C** — explicit `GroupMembership` grant.

### G-4.4 grant-window containment

An expiring grantor membership may only mint memberships with
`new_expiry <= grantor_expiry`. The check is hardened against both
the path-attribution false-positive (Path-B-and-Path-C-witnesses-both-
present case re-verifies hive authority) and the naive bypass (cannot
skip the window check by supplying a dummy hive witness).

### Validator coverage

49 host-side integrity tests green (35 baseline + 14 pass-2.5
cleanup audit + 9 new group tests). Wasm release build clean. The
fetch-dependent authority branches will be covered by Tryorama
integration tests when the conductor pipeline is exercised.

---

## What is enforced now (formerly deferred) — pass-4 G-6.2 SHIPPED

Pass-3 deferred G-6.2 (recipient-set integrity on
`AclSpec::HiveGroup` `public_key_acl`). **Pass-4 closes it.** The
integrity validator now verifies, on every `AclSpec::HiveGroup`
content commit, that every pubkey listed in
`public_key_acl.{owner,admin,writer,reader}` holds a matching
`GroupMembership` in a dominating bucket of `group_acl`.

**Wire shape (pass-4):** `AclSpec::HiveGroup` gains
`recipient_witnesses: Vec<RecipientWitness>` where each
`RecipientWitness = { pubkey, bucket, membership_hash }`. The
validator enforces:
- Cardinality bound: `recipient_witnesses.len() <=
  HIVEGROUP_MAX_WITNESSES = 256`.
- Bidirectional set-equality between `public_key_acl` buckets and
  witnesses (with bucket dominance — an Admin-bucket witness
  covers Admin + Writer + Reader PKA entries for the same pubkey).
- Per-witness `must_get_valid_record(membership_hash)`: the cited
  `GroupMembership` must grant the named pubkey a role that
  satisfies the claimed bucket, in a group present in the
  corresponding (or higher) bucket of `group_acl`, unexpired at
  the entry's `action.timestamp`.

**humm-tauri implication.** `public_key_acl` on HiveGroup content
is now **load-bearing**. Every HiveGroup write site MUST stamp
`recipient_witnesses` covering every PKA pubkey. Use the
centralised `stampWitnessesFromGroupAcl` helper documented in
[`PASS_4_DEPLOY_HANDOFF.md`](./PASS_4_DEPLOY_HANDOFF.md) §
"REQUIRED humm-tauri callsite update" and referenced from
[`HUMM_TAURI_ACLSPEC_INTEGRATION.md`](./HUMM_TAURI_ACLSPEC_INTEGRATION.md)
§ 5. A bad / missing / expired membership in any of the helper's
`get_latest_group_membership` lookups raises an error before the
`create_encrypted_content` call — surface it as "this person is
no longer a member" rather than committing a doomed entry.

The cryptographic-roster pattern from pass-3 still applies for
authority decisions: use `list_group_members(group_genesis_hash)`
when you need "is this pubkey actually a group member?"; that
roster is the source of truth for both the witness-stamping helper
AND any UI gating logic.

## What you can do today to make the pass-3 migration easier

1. **Mark client-side group/role/ACL assertions** with a
   `// pass-3-target: replace with list_group_members(...)` comment.
   These are the call sites that move from forgeable to enforced.
2. **Treat `GroupMemberList` reads as a display cache** in your
   architecture. Don't gate any authorization decision on its contents;
   pass-3 makes this official.
3. **Don't add new code that depends on the exact field name
   `header.hive_genesis_hash`.** It moves into
   `header.acl_spec.HiveGroup.hive_genesis_hash` in Phase C. The
   migration is mechanical — easier if there are fewer call sites.
4. **For the DM sidecar:** the current `acl/publicKeyAcl` building
   logic in `src/sidecars/direct-messages/wire/content.ts` will swap
   to `AclSpec::DirectMessage { recipients }` in Phase C. Recipients
   = `[me, peer]`. No UX change.
5. **For the public-post path (Compose):** the hardcoded
   `buildPublicAcl(ownerSigningKey)` will swap to
   `AclSpec::Public { hive_genesis_hash: active_hive_genesis }` in
   Phase C. No UX change.
6. **For the member-request flow:** `AclSpec::OpenWrite {
   target_hive_genesis_hash: Some(target_hive) }`. The requester does
   NOT need hive membership to write — the validator only checks
   author identity + target existence. This unblocks the previously-
   stubbed "outsider knock" flow.
7. **For hive-discovery:** `AclSpec::OpenWrite { target: None }`.
   Cross-network publishing.

---

## Pass-3 phase status

- Phase A — Integrity core (`Role` refactor + `group.rs` +
  dispatcher + 9 host tests + 3 reviewer gates passed):
  **COMMITTED** (`00329eb`, `65fcd7a`).
- Phase B — Coordinator group module (`create_group_genesis`,
  `create_group_membership`, `revoke_group_membership`,
  `list_group_members`, `list_my_groups`, `list_groups_in_hive`,
  `get_latest_group_membership`, `get_group_genesis`) + cap grants
  for the read externs + `InboxEvent::GroupInvite = 3` discriminator
  + cross-cutting `list_my_hives` security fix (author guard on
  founded-hive path): **COMMITTED** *(this commit)*. Existing
  `Signal` enum auto-dispatches the new group entries/links —
  humm-tauri receives `Signal::EntryCreated { app_entry:
  EntryTypes::GroupGenesis(..) }` etc. via the generic pathway.
- Phase C — `AclSpec` reshape + variant-dispatch validators (the
  wire-breaking change): **COMMITTED** *(this commit)*. New shape is
  live on the branch — `EncryptedContentHeader { id, display_hive_id,
  content_type, acl_spec: AclSpec, public_key_acl,
  revision_author_signing_public_key }`. The four `AclSpec` variants
  enforce author-authority per scope. **G-6.2 SHIPPED in pass-4**
  (see Pass-4 phase status below). **Bonus hardenings this
  commit:** M-1 (`validate_update_encrypted_content` now requires
  `action.author == original_action.author` — closes update-chain
  hijack across ALL variants; was pre-existing pass-1 gap that pass-3
  amplified), L-1 (`EncryptedContentUpdates` link now requires link
  author == base author == target author — closes app-level
  update-graph poisoning), L-2 (DM rejects duplicate recipients —
  degenerate self-DM hygiene), plus a `GROUP_ACL_MAX_GROUPS = 64`
  validator-amplification bound on HiveGroup (analogous to
  `DM_MAX_RECIPIENTS = 32`).
- Phase D — Migration tooling extends (`scripts/migrate-dna.ts`
  content-type → AclSpec classifier + new wire-shape import +
  schema_version 1/2 acceptance): **COMMITTED** *(this commit)*.
  `DNA_MIGRATION_GUIDE.md` updated with the pass-3 wire-shape
  migration section + classification table. **Phase D.1** (the
  legacy-group → `GroupGenesis` track + per-bundle
  `classification-overrides.json`) was deferred at pass-3 time;
  it now ships on branch `feat-migration-d1-group-track` — see the
  Phase D.1 status section below.
- Phase E — Full handoff docs (
  [`HUMM_TAURI_ACLSPEC_INTEGRATION.md`](./HUMM_TAURI_ACLSPEC_INTEGRATION.md),
  [`PASS_3_DEPLOY_HANDOFF.md`](./PASS_3_DEPLOY_HANDOFF.md),
  [`HUMM_TAURI_FEATURE_ENABLEMENT.md`](./HUMM_TAURI_FEATURE_ENABLEMENT.md),
  + banner updates on
  [`HUMM_TAURI_COORDINATOR_INTEGRATION.md`](./HUMM_TAURI_COORDINATOR_INTEGRATION.md)
  and [`PASS_2_DEPLOY_HANDOFF.md`](./PASS_2_DEPLOY_HANDOFF.md)):
  **COMMITTED** *(this commit)*. The four pass-3 docs are now the
  canonical reference set for humm-tauri's pass-1 → pass-3 leapfrog
  integration. This file remains the rolling delta view (e.g.
  Phase F outcomes will land here when the wasm builds + hashes are
  recorded).
- Phase F — Verification + new `.baseline-hashes.txt` Pass-3 section +
  ff-merge: **COMMITTED** (pass-3 final tip `b1e72aa`).

---

## Pass-4 phase status

- Phase 4-A/B/C — Integrity zome G-6.2 (`RecipientWitness` +
  `AclBucket` + reshape `AclSpec::HiveGroup` +
  `validate_recipient_witnesses` w/ bidirectional PKA cross-check
  + per-witness fetch + bucket dominance + 9 host tests) AND G-4.4
  hive back-port (`enforce_hive_grant_window` + rule 4 in
  `validate_create_hive_membership` + 2 host tests) + coordinator
  fixture updates: **COMMITTED** (`9dc0690`). 69 host integrity +
  22 host coordinator tests green; release wasm clean, zero
  warnings; 3 reviewer gates passed.
- Phase 4-D — `scripts/migrate-dna.ts` classifier comment refresh
  (no behavior change; classifier still throws on HiveGroup until
  D.1 ships): **COMMITTED** (`be6a93b`).
- Phase 4-E — Handoff docs (this file + new
  [`PASS_4_DEPLOY_HANDOFF.md`](./PASS_4_DEPLOY_HANDOFF.md) +
  updates to
  [`HUMM_TAURI_ACLSPEC_INTEGRATION.md`](./HUMM_TAURI_ACLSPEC_INTEGRATION.md)
  + banner on
  [`PASS_3_DEPLOY_HANDOFF.md`](./PASS_3_DEPLOY_HANDOFF.md) +
  E.4.l section in
  [`HUMM_TAURI_FEATURE_ENABLEMENT.md`](./HUMM_TAURI_FEATURE_ENABLEMENT.md)):
  **COMMITTED** *(this commit)*.
- Phase 4-F — Verification + new `.baseline-hashes.txt` Pass-4
  section + ff-merge: **COMMITTED** (pass-4 final tip `7b918f7`;
  DNA hash `uhC0kNS2JM6lqmdxr3Q8VK2uhDJFF-wRBz-W73JjJKZnTTMyT8_JS`).

---

## Phase D.1 status (branch `feat-migration-d1-group-track`)

Non-DNA-changing migration tooling (off the pass-4 tip). Lets
operators materialise legacy humm-tauri groups as real
`GroupGenesis` entries on the new DNA + populate the pass-4
`recipient_witnesses` on migrated HiveGroup content.

- New CLI commands in `scripts/migrate-dna.ts`: `migrate-group`,
  `grant-group-memberships`, `mark-group-migrated`.
- New per-bundle `classification-overrides.json` mechanism (optional
  5th arg to `import`) — forces specific entries to
  `AclSpec::HiveGroup`, resolving old group squuids → new
  `GroupGenesis` hashes + stamping `recipient_witnesses` from the
  live group rosters.
- Witness-bucket dominance arithmetic extracted to
  `scripts/acl-bucket.ts` (pure, dependency-free) + pinned by 9
  vitest unit tests (`tests/src/migration/witness-bucket.test.ts`)
  — sanity-checks the bucket assignment without needing a conductor.
- `tsc --noEmit scripts/migrate-dna.ts`: holds at the 7-error
  pre-existing baseline (zero new errors).
- `DNA_MIGRATION_GUIDE.md` § "Pass-4 + Phase D.1" documents all
  three commands + the overrides file format.
- DNA hash UNCHANGED — coordinator/tooling only. No integrity-zome
  edits on this branch.
- humm-tauri impact: none beyond the pass-4 wire-shape work already
  on the roadmap. D.1 is operator-side migration tooling; the
  post-migration re-stamp it enables is the same
  `stampWitnessesFromGroupAcl` flow documented in
  [`PASS_4_DEPLOY_HANDOFF.md`](./PASS_4_DEPLOY_HANDOFF.md).

This file is the *interim* communication channel for pass-4 + D.1.
Treat it as authoritative for "what changed since the pass-3 handoff".

---

## Where to find more

- **Canonical pass-3 plan** (full feature matrix, attack matrix,
  decisions D1..D14, content-type → AclSpec mapping):
  `local://pass-3-groupmembership-aclspec-sovereign.md` (session
  artifact; not in the repo tree).
- **Pass-3 branch tip + reviewer-fix details**: read the integrity
  source itself on `feat-integrity-pass-3-groups`:
  - `dnas/humm_earth_core/zomes/integrity/content/src/group.rs`
    (new, ~950 lines, fully documented)
  - `dnas/humm_earth_core/zomes/integrity/content/src/hive.rs`
    (Role refactor + compat alias)
  - `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs`
    (dispatcher additions)
- **Questions?** This file is updated after every pass-3 phase
  commit. Re-pull and check the "Pass-3 phase status" section
  near the bottom.
