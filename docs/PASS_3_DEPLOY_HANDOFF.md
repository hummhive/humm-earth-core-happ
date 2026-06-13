# Pass-3 deploy handoff — humm-tauri integration

> **Superseded for new deployments by pass-4.** Pass-4 ships on
> branch `feat-integrity-pass-4-recipient-witnesses` and bumps the
> DNA hash again, closing G-6.2 (recipient-set integrity for
> `AclSpec::HiveGroup` `public_key_acl`) and back-porting G-4.4 to
> `HiveMembership`. New integrators should start at
> [`PASS_4_DEPLOY_HANDOFF.md`](./PASS_4_DEPLOY_HANDOFF.md); the
> pass-3 → pass-4 wire-shape delta is one new field on the
> `HiveGroup` variant (`recipient_witnesses: RecipientWitness[]`),
> documented in
> [`HUMM_TAURI_ACLSPEC_INTEGRATION.md`](./HUMM_TAURI_ACLSPEC_INTEGRATION.md)
> § 4 + § 5.
>
> This document remains the canonical reference for the pass-3
> shape itself (the four `AclSpec` variants, the variant-dispatch
> validators, group authority + grant-window). Pass-4 only adds; it
> does not subtract.

Short-form handoff for the humm-tauri team to integrate the pass-3
integrity-zome changes shipped on `feat-integrity-pass-3-groups`.
Pass-3 **intentionally bumps the DNA hash** (the AclSpec reshape is a
non-additive integrity change) and is the next intentional DNA bump
after pass-2. Existing pass-2 data MUST be migrated forward via the
extended `scripts/migrate-dna.ts` pipeline (new pass-3 wire-shape
classifier; same operator workflow as pass-2.5).

For the full per-change reference, see
[`HUMM_TAURI_ACLSPEC_INTEGRATION.md`](./HUMM_TAURI_ACLSPEC_INTEGRATION.md)
(per-content-type + per-modal mechanics) and
[`HUMM_TAURI_FEATURE_ENABLEMENT.md`](./HUMM_TAURI_FEATURE_ENABLEMENT.md)
(feature-by-feature implementation guide). For an ongoing
"what-changed-since-pass-2.5" delta visible to devs polling the repo,
see [`HANDOFF_UPDATED_INFO.md`](./HANDOFF_UPDATED_INFO.md). For a
per-pass concrete-task mapping to humm-tauri files + features, see
[`HUMM_TAURI_PASS_ROADMAP.md`](./HUMM_TAURI_PASS_ROADMAP.md).

## TL;DR

- **DNA hash CHANGED** from
  `uhC0kawoZqBxv3Jjvh-TlSQ5aO4U-hwiUNtZxFzXkTOBc5ijKVatw` (pass-2) to
  **the new pass-3 hash recorded in `.baseline-hashes.txt` once
  Phase F lands**. Coordinator hot-swap does NOT work for this pass;
  users see a new cell on install and require the migration flow to
  keep their data.
- **G-series — Validated group-membership infrastructure** (NEW entry
  types `GroupGenesis` + `GroupMembership` + shared `Role` enum +
  `check_group_authority` three-path helper). Closes the group / role
  / ACL poisoning class for group-scoped content. The hive owner is
  the root sovereign — hive Admin+ controls every group in their hive
  via Path B.
- **AclSpec wire reshape** — `EncryptedContentHeader` collapses four
  pass-2 fields (`hive_id`, `hive_genesis_hash`,
  `author_membership_hash`, `acl`) into a single `acl_spec: AclSpec`
  discriminated union. Four variants (`HiveGroup`, `DirectMessage`,
  `Public`, `OpenWrite`) carry the right authority contract per scope.
  Cross-hive DMs, public posts, member-requests, and hive-discovery
  remain functional — they map to the appropriate non-HiveGroup
  variants.
- **Bonus hardenings** landed alongside the reshape:
  - **M-1**: `validate_update_encrypted_content` now requires
    `action.author == original_action.author` across all four variants
    (was a pre-existing pass-1 gap that pass-3 amplified).
  - **L-1**: `EncryptedContentUpdates` link binds link author == base
    author == target author. Closes app-level update-graph poisoning.
  - **L-2**: DM rejects duplicate recipients (degenerate self-DM
    hygiene).
  - `GROUP_ACL_MAX_GROUPS = 64` cap on HiveGroup amplification
    (analogous to `DM_MAX_RECIPIENTS = 32`).
- **G-6.2 DEFERRED** — recipient-set integrity for HiveGroup
  `public_key_acl` (every pubkey must hold a matching
  `GroupMembership` in the same-or-higher group_acl bucket) is
  documented but NOT enforced this pass. `public_key_acl` on
  HiveGroup is an unauthenticated routing hint at commit time;
  decryption gating via SharedSecrets is unaffected. See "What is
  NOT enforced this commit" in
  [`HANDOFF_UPDATED_INFO.md`](./HANDOFF_UPDATED_INFO.md).

### Deploy (NOT transparent — DNA-hash bump, migration required)

The same shape as the pass-2 deploy. Pass-2 migration scaffold
(hive-identity track, V2 markers) is preserved and extended; the
pass-3 wire-shape reshape is handled invisibly by the script's
classifier (Phase D).

1. **Pre-publish** (this hApp repo, before bundling into humm-tauri):
   ```bash
   cd ~/humm-earth-core-happ
   RUSTFLAGS='--cfg getrandom_backend="custom"' \
     CARGO_TARGET_DIR=target \
     cargo build --release --target wasm32-unknown-unknown \
     -p content_integrity -p content
   hc dna pack dnas/humm_earth_core/workdir
   hc dna hash dnas/humm_earth_core/workdir/humm_earth_core.dna
   # MUST print the pass-3 hash recorded in .baseline-hashes.txt
   hc app pack workdir --recursive
   sha256sum workdir/humm-earth-core-happ.happ
   ```
2. **humm-tauri side** (after copying the new .happ into
   `src-tauri/bin/`): bump the `APP_ID` constant. Per the pass-1
   deploy handoff, the constant lives in
   `humm-tauri/src-tauri/src/holochain/install_humm_core_happ.rs`.
3. **User-facing migration** (humm-tauri-side UX work):
   - On launch with a pass-2 cell present, present "migration
     available" prompt; user opts in.
   - Run `scripts/migrate-dna.ts export` against the pass-2 cell to
     produce the data bundle.
   - Install the pass-3 hApp (new APP_ID).
   - **Owner side** (same pipeline as pass-2):
     - `migrate-hive <new-app-id> <old-hive-id> <old-anchor-ah-b64>
       <hive-bundle.json>` — creates pass-3 `HiveGenesis` entries.
     - `grant-memberships <new-app-id> <hive-bundle.json>
       <old-hive-id> <role> <pubkey-b64>...` — issues pass-3
       `HiveMembership`s to each cell agent.
     - `mark-hive-migrated <old-app-id> <hive-bundle.json>` — writes
       V2 markers on the old hive's entry pointing at the new genesis.
   - **Member side**:
     - `import <new-app-id> <bundle.json> <hive-bundle.json>
       <remap.json>` — re-stamps every entry via the new AclSpec
       classifier; the classifier handles the variant decision per
       content_type invisibly.
     - `mark-migrated <old-app-id> <remap.json>` — writes V2 markers
       on the old per-entry chain.
   - **NEW post-migration step (optional, humm-tauri side)**: for
     entries that should be restricted to a specific group, humm-tauri
     UI calls `create_group_genesis` + `create_group_membership` to
     create real pass-3 groups, then re-emits the affected entries
     via `create_encrypted_content` with
     `acl_spec: AclSpec::HiveGroup { ... }`. The migration script
     defaults to `Public` for unknown content types because pass-1/2
     had no `group_acl` field; per-entry re-stamping is a humm-tauri
     workflow concern.

## Wire-shape changes (REQUIRED humm-tauri callsite updates)

Pass-3 reshapes `CreateEncryptedContentInput`. ALL existing humm-tauri
callsites that target it MUST be updated; otherwise zome calls will
fail to deserialize.

### `CreateEncryptedContentInput` (REQUIRED reshape)

```ts
// pass-2 shape (now broken)
{
  id,
  hive_id,                        // display alias
  hive_genesis_hash,              // load-bearing ActionHash
  author_membership_hash,         // load-bearing Option<ActionHash>
  content_type,
  revision_author_signing_public_key, bytes,
  acl, public_key_acl,
  dynamic_links?
}

// pass-3 shape
{
  id,
  display_hive_id,                // renamed from hive_id (display only)
  content_type,
  revision_author_signing_public_key, bytes,
  acl_spec: AclSpec,              // NEW — variant-dispatched authority
  public_key_acl,
  dynamic_links?
}
```

The three load-bearing pass-2 fields collapse INTO the
`AclSpec::HiveGroup` variant (and into `AclSpec::Public` for the
public-post path):

```ts
type AclSpec =
  | { HiveGroup: {
        hive_genesis_hash: ActionHash;
        author_membership_hash: ActionHash | null;
        group_acl: AclByGroupGenesis;
        author_group_membership_hash: ActionHash | null;
      } }
  | { DirectMessage: { recipients: AgentPubKey[] } }
  | { Public: {
        hive_genesis_hash: ActionHash;
        author_membership_hash: ActionHash | null;
      } }
  | { OpenWrite: { target_hive_genesis_hash: ActionHash | null } };

type AclByGroupGenesis = {
  owner: ActionHash;
  admin: ActionHash[];
  writer: ActionHash[];
  reader: ActionHash[];
};
```

### Per-content-type wiring (quick reference)

Full table lives in `HUMM_TAURI_ACLSPEC_INTEGRATION.md`. The short
version:

| Path | Variant |
|---|---|
| DM sidecar (`sendDirectMessage`) | `DirectMessage { recipients: [me, peer] }` |
| Compose / public posts | `Public { hive_genesis_hash, author_membership_hash }` |
| Group / member / member-list / invite / sidecar config | `HiveGroup { hive_genesis_hash, author_membership_hash, group_acl, author_group_membership_hash }` |
| Member-request | `OpenWrite { target_hive_genesis_hash: Some(target_hive) }` |
| Hive-discovery | `OpenWrite { target_hive_genesis_hash: None }` |
| Personal vault (today) | `HiveGroup` with a singleton personal group |

### Hive-scoped query inputs — UNCHANGED from pass-2

`ListByHiveInput`, `CountByHiveInput`, `ListByContentIdInput`,
`ListByDynamicLinkInput`, `ListByAclInput`, and the C4
`FetchPairWithHiveCheckInput` continue to take
`hive_genesis_hash: ActionHash` as their input. The path construction
on the integrity side recovers the hive context from
`header.hive_context()` (the new variant accessor); the path STRUCTURE
is unchanged. Queries against pass-3 hive content work without any
TS-side change.

DM / OpenWrite-without-target entries do NOT appear in these queries
(they have no hive context). This is intentional — DMs are queried
via the per-recipient pubkey path (`ListByAuthorInput`), and
`OpenWrite { target: None }` entries surface via the
hive-discovery-specific extern.

## New externs (read surface — cap-granted)

Group authority + roster management. See
`HUMM_TAURI_ACLSPEC_INTEGRATION.md` for per-modal wiring.

- `get_latest_group_membership({agent, group_genesis_hash}) ->
  Option<GroupMembershipResponse>` — analogue of
  `get_latest_membership` one level down. Used to stamp
  `author_group_membership_hash` into the `HiveGroup` variant before
  group-scoped writes.
- `list_group_members(group_genesis_hash) ->
  Vec<GroupMembershipResponse>` — **the cryptographic roster**.
  Replaces the forgeable `GroupMemberList`-keyed roster lookups in
  humm-tauri. Dedupes by `for_agent` taking the latest unexpired
  membership.
- `list_my_groups() -> Vec<ListedGroup>` — analogue of
  `list_my_hives`. Walks `Inbox::GroupInvite` (byte 3) links on the
  local agent's pubkey.
- `list_groups_in_hive(hive_genesis_hash) -> Vec<ListedGroup>` —
  enumerates a hive's groups from the `HiveToGroups` link set.
- `get_group_genesis(action_hash) -> Option<GroupGenesisResponse>`
  — single-genesis fetch for UI consumption.

## New externs (write surface — NOT cap-granted; local-UI only)

These match the pass-2 hive-membership write pattern: local conductor
calls them via AppWebsocket auth; not remotely reachable.

- `create_group_genesis({hive_genesis_hash, display_id, hive_wide_role,
  creator_hive_membership_hash}) -> GroupGenesisResponse` —
  permissionless... almost. `hive_wide_role.is_some()` requires hive
  Owner; otherwise hive Admin+. The integrity validator enforces.
- `create_group_membership({group_genesis_hash, for_agent, role,
  grantor_membership_hash, grantor_hive_membership_hash, expiry}) ->
  GroupMembershipResponse` — no self-grant; grantor needs Admin+ via
  Path A/B/C; no escalation above grantor's own role; G-4.4
  grant-window containment for expiring Path-C grantors.
- `revoke_group_membership({membership_hash, new_expiry,
  grantor_membership_hash, grantor_hive_membership_hash}) ->
  GroupMembershipResponse` — ergonomic helper that issues a fresh
  `GroupMembership` with past expiry. **Self-revocation is NOT
  supported** — Rule 1 unconditionally rejects
  `action.author == for_agent`. Implement "leave group" as a
  remove-member request that an Admin+ holder processes.

## New `Signal` variants — none

The existing generic `Signal` enum auto-dispatches the new entries
and link types from Phase A via the existing
`Signal::EntryCreated { app_entry: EntryTypes }` and
`Signal::LinkCreated { link_type: LinkTypes }` pathways. humm-tauri
receives:

- `Signal::EntryCreated { app_entry: EntryTypes::GroupGenesis(..) }`
- `Signal::EntryCreated { app_entry: EntryTypes::GroupMembership(..) }`
- `Signal::LinkCreated { link_type: LinkTypes::HiveToGroups }`
- `Signal::LinkCreated { link_type: LinkTypes::AgentToGroupMemberships }`
- `Signal::LinkCreated { link_type: LinkTypes::GroupToGroupMemberships }`

Group revocation arrives as a fresh `EntryCreated{GroupMembership}`
with `expiry: Some(past_ts)`; humm-tauri distinguishes by checking the
`expiry` field.

## What this closes / does NOT close

### Closed by pass-3

- **Group / role / ACL poisoning** for HiveGroup content (attacks
  #1-#10 in the plan): every group/role claim is cryptographically
  attributable to the hive sovereign via the `GroupMembership` chain.
- **DM impersonation + reader-bucket forgery** (#3, #4): cardinality
  bounds, author-in-recipients, sorted-equality reader bucket binding.
- **OpenWrite fake-target** (#13): target HiveGenesis must resolve.
- **Public content without hive membership** (#14): Writer+ required.
- **Cross-hive group claim** (#9): every `group_acl` entry must
  belong to the entry's hive.
- **Update-chain hijack** (M-1, pre-existing): `action.author ==
  original_action.author` enforced across all variants.
- **EncryptedContentUpdates link poisoning** (L-1, pre-existing):
  link author binds to base + target authorship.

### Preserved unchanged

- Cross-hive DMs via `AclSpec::DirectMessage` (7 shipped + 3 planned
  patterns).
- Public posts via `AclSpec::Public`.
- Member-request + cross-network hive-discovery via
  `AclSpec::OpenWrite`.
- The pass-2 hive-authority chain (`check_hive_authority`,
  `HiveMembership` grant model) — extended for use by group Path B.

### NOT closed THIS pass — G-6.2 (closed in pass-4)

- **Recipient-set integrity on HiveGroup `public_key_acl`** — Mallory
  adding her pubkey to the reader bucket of a private group post to
  receive remote-signal notifications. Decryption gating
  (SharedSecrets) is unaffected, but signal routing can be
  manipulated. **Closed in pass-4** by the `recipient_witnesses`
  field on `AclSpec::HiveGroup`; see
  [`PASS_4_DEPLOY_HANDOFF.md`](./PASS_4_DEPLOY_HANDOFF.md). Until
  humm-tauri lands the pass-4 wire-shape change, treat
  `public_key_acl` on HiveGroup as a routing hint and use
  `list_group_members` for authority decisions.

## Hash invariants for verification

To be populated by Phase F. Build the wasm against
`feat-integrity-pass-3-groups` tip and record the:

- Integrity wasm sha256 (`content_integrity.wasm`)
- Coordinator wasm sha256 (`content.wasm`)
- DNA hash (`humm_earth_core.dna`)
- hApp sha256 (`humm-earth-core-happ.happ`)

The hashes live in `.baseline-hashes.txt` "Pass-3" section after
Phase F lands.

## Migration commands (operational)

```bash
# Owner side
npx tsx scripts/migrate-dna.ts migrate-hive \
  <new-app-id> <old-hive-id> <old-anchor-ah-b64> <hive-bundle.json>

npx tsx scripts/migrate-dna.ts grant-memberships \
  <new-app-id> <hive-bundle.json> <old-hive-id> <Owner|Admin|Writer|Reader> \
  <member-pubkey-b64> [<member-pubkey-b64>...]

npx tsx scripts/migrate-dna.ts mark-hive-migrated \
  <old-app-id> <hive-bundle.json>

# Member side
npx tsx scripts/migrate-dna.ts export \
  <old-app-id> <out.bundle.json>

npx tsx scripts/migrate-dna.ts import \
  <new-app-id> <in.bundle.json> <hive-bundle.json> <out.remap.json>

npx tsx scripts/migrate-dna.ts mark-migrated \
  <old-app-id> <in.remap.json>
```

The classifier runs invisibly during `import`. Operators do not need
to pre-classify or pre-translate bundles — `schema_version: 1`
(pass-1/2) bundles are accepted directly.

## Cross-hive smoke-test checklist for humm-tauri integrators

Run BEFORE shipping pass-3 to users (acceptance criteria):

1. **DM cross-hive**: Alice in HIVE_A sends a DM to Bob in HIVE_B.
   Both see the thread. Either party can delete.
2. **Hive owner sovereignty**: Alice creates a custom group in
   HIVE_A. As hive owner, Alice grants Bob group Writer without
   Bob holding any prior group membership.
3. **Group write authority**: Bob (group Writer) commits a group
   post. Charlie (hive member, not group member) attempts the same
   write; commit rejected.
4. **Member-request outsider**: Outsider Dave (no membership in
   HIVE_A) creates an `AclSpec::OpenWrite { target: HIVE_A }`
   member-request. Hive owner Alice sees it.
5. **Hive-discovery cross-network**: Alice publishes an
   `AclSpec::OpenWrite { target: None }` hive-discovery anchor.
   Bob (different network bootstrap, no shared hive) queries by
   Alice's pubkey and sees the anchor.
6. **Public post**: Alice publishes a text-post via
   `AclSpec::Public`. World-readable; no group_acl required.
7. **Update-chain integrity (M-1 fix)**: Mallory attempts to commit
   an Update on her own source chain pointing at Alice's
   EncryptedContent; commit rejected with
   `does not match original action author`.

## Commit + branch state

Branch: `feat-integrity-pass-3-groups` (off
`chore-pass-2.5-cleanup` tip `e82f196`).

Commits (in order):

- Phase A `00329eb` — group integrity foundation
- Phase A `65fcd7a` — `HANDOFF_UPDATED_INFO.md` living delta
- Phase B `699c093` — coordinator group externs + `InboxEvent::GroupInvite`
- Phase C `7e17b4c` — AclSpec wire reshape + variant validators
- Phase D `1a745d7` — migration classifier
- Phase E (this commit) — handoff docs
- Phase F (final commit) — verification + new `.baseline-hashes.txt`
  section + final report

**Not pushed.** The user controls when this branch reaches `origin`
and `main`.
