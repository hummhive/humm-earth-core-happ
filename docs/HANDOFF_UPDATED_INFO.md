# Handoff updated info — pass-3 in flight

**Audience:** humm-tauri devs currently integrating against the pass-2 /
pass-2.5 handoff notes
([`PASS_2_DEPLOY_HANDOFF.md`](./PASS_2_DEPLOY_HANDOFF.md),
[`HUMM_TAURI_COORDINATOR_INTEGRATION.md`](./HUMM_TAURI_COORDINATOR_INTEGRATION.md),
[`DNA_MIGRATION_GUIDE.md`](./DNA_MIGRATION_GUIDE.md)).

**Purpose:** living delta between the pass-2.5 docs and the in-flight
pass-3 work on branch `feat-integrity-pass-3-groups`. Re-pull this file
periodically — it is updated after every pass-3 phase commit so you can
catch upstream shifts before they bite.

**Status of pass-3 itself:** branch lives on
`feat-integrity-pass-3-groups` (off `chore-pass-2.5-cleanup` tip
`e82f196`); NOT pushed; NOT merged to `main`. The DNA hash will bump
again when pass-3 lands.

---

## TL;DR — what to do right now

- **Keep going with the pass-2.5 integration.** Nothing in pass-3
  invalidates the pass-2 externs you're calling
  (`create_hive_genesis`, `create_hive_membership`, `list_my_hives`,
  `get_latest_membership`) or the pass-2 entry types
  (`HiveGenesis`, `HiveMembership`). Those names + signatures + wire
  shapes survive pass-3 unchanged.
- **One wire-shape break is coming** (Phase C, not yet committed):
  `EncryptedContentHeader` will reshape — `hive_id`, `hive_genesis_hash`,
  `author_membership_hash`, and `acl` collapse into a single
  `acl_spec: AclSpec` discriminated-union field. The work you do today
  building hive-scoped headers is not wasted; it becomes one variant
  (`AclSpec::HiveGroup`) of the new enum. Plan a follow-up patch when
  pass-3 lands. See "What is CHANGING" below for the exact migration
  shape.
- **Group / role / ACL enforcement is moving from client-side
  convention to integrity-zome enforcement.** If you're stubbing or
  asserting group membership in TS today, mark those sites with a
  `// pass-3-target` comment so the future migration is grep-able.
- **DNA hash WILL bump again with pass-3** — re-run the migration
  tooling once pass-3 ships.

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
  hive_wide_role, grantor_hive_membership_hash }) -> GroupGenesisResponse`
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

## What is NOT enforced this commit (G-6.2 recipient-set integrity)

The pass-3 plan's section G-6.2 calls for the integrity validator to
verify, on every `AclSpec::HiveGroup` content commit, that every pubkey
listed in `public_key_acl.{owner,admin,writer,reader}` holds a matching
`GroupMembership` in the same-or-higher bucket of `group_acl`. This
would close the modified-coordinator forgery where Mallory inserts her
pubkey into the reader bucket of a private group post to receive the
remote-signal notification (even though she can't decrypt without the
shared secret).

**G-6.2 is documented but DEFERRED to a follow-up sub-commit (Phase
C.1).** Current state:
- Author authority (Writer+ in the hive AND in every group_acl group)
  IS enforced. A modified coordinator cannot post group content under
  groups the author doesn't have authority in.
- The `group_acl` ActionHash references are validated — each must
  resolve to a real `GroupGenesis` in the same hive (closes the
  forge-group-claim attack).
- The recipient list (`public_key_acl`) is treated as an
  unauthenticated routing hint. A modified coordinator can add or
  remove pubkeys; recipients still cannot decrypt without the shared
  secret, but routing fan-out (`send_remote_signal` recipients) can
  be manipulated.

**humm-tauri implication.** Treat `public_key_acl` as a hint, not as
proof of group membership. When you need to verify "is this pubkey
actually a group member?", call `list_group_members(group_genesis_hash)`
from the new Phase B externs — that's the cryptographic roster and
is unforgeable. Don't gate access decisions or trust claims on
`public_key_acl` alone.

The G-6.2 follow-up will add a `recipient_membership_witnesses` field
to `AclSpec::HiveGroup`: a `BTreeMap<AgentPubKey, ActionHash>` where
the writer stamps each recipient's authorising `GroupMembership` hash.
The validator iterates the map at commit time. When that ships, the
wire shape gains the field but existing entries (with no map) remain
valid via an `Option<BTreeMap>` deserialisation. Plan for that future
addition by NOT baking assumptions about `public_key_acl.reader` being
authoritative into your UI.

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
  enforce author-authority per scope. **G-6.2 deferred** — see
  "What is NOT enforced this commit" below. **Bonus hardenings this
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
  group track + AclSpec classification): **not started.**
- Phase E — Full handoff docs
  (`HUMM_TAURI_ACLSPEC_INTEGRATION.md` and
  `HUMM_TAURI_FEATURE_ENABLEMENT.md`): **not started.**
- Phase F — Verification + new `.baseline-hashes.txt` Pass-3 section +
  ff-merge: **not started.**

This file is the *interim* communication channel until Phase E ships
the full integration docs. Treat it as authoritative for "what
changed since the pass-2.5 handoff" until then.

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
