> **Forward-pointer to pass-3 (`feat-integrity-pass-3-groups`, in
> flight, not yet pushed):** pass-3 bumps the DNA hash again and
> reshapes `EncryptedContentHeader` via the new
> `acl_spec: AclSpec` discriminated union; pass-4 then adds the
> required `recipient_witnesses` field on `AclSpec::HiveGroup`.
> humm-tauri is doing a pass-2.5 → pass-4 leapfrog (skipping pass-3):
> its live content path is on the pass-2 wire shape, so it adopts
> pass-4 directly. If you arrived here from a pass-2.5 integration
> ticket, the canonical pass-3/4 docs are:
>
> - [`HUMM_TAURI_ACLSPEC_INTEGRATION.md`](./HUMM_TAURI_ACLSPEC_INTEGRATION.md) — wire shape + per-modal wiring
> - [`PASS_3_DEPLOY_HANDOFF.md`](./PASS_3_DEPLOY_HANDOFF.md) — deploy runbook
> - [`PASS_4_DEPLOY_HANDOFF.md`](./PASS_4_DEPLOY_HANDOFF.md) — pass-4 deploy + `recipient_witnesses`
> - [`HUMM_TAURI_FEATURE_ENABLEMENT.md`](./HUMM_TAURI_FEATURE_ENABLEMENT.md) — feature-by-feature implementation
> - [`HANDOFF_UPDATED_INFO.md`](./HANDOFF_UPDATED_INFO.md) — living delta from pass-2.5 to pass-4
>
> The pass-2 content below remains the historical reference for the
> intermediate integrity bump (hive-identity track, V2 markers,
> pass-2 wire shape). All of it is preserved by pass-3 and pass-4.
>
# Pass-2 deploy handoff — humm-tauri integration

Short-form handoff for the humm-tauri team to integrate the pass-2
integrity-zome changes shipped on `feat-integrity-pass-2`. Pass-2
**intentionally bumps the DNA hash** and is the first non-additive
integrity change since pass-1 froze the baseline. Existing data MUST be
migrated forward via the pass-2.5 migration tooling
(`scripts/migrate-dna.ts`'s four-phase hive-identity + per-entry flow,
+ the `MigrationMarker{V1,V2}` readers) before users can keep using
their hives.

For the full per-change reference, see
[`HUMM_TAURI_COORDINATOR_INTEGRATION.md`](./HUMM_TAURI_COORDINATOR_INTEGRATION.md)
(pass-1 historical context, with a pass-2 banner at the top that
redirects here) and
[`DNA_MIGRATION_GUIDE.md`](./DNA_MIGRATION_GUIDE.md) (pass-2.5 migration
mechanics).

## TL;DR

- **DNA hash CHANGED** from `uhC0kT0Tkc3b6ccfa75YwWdzpSWvdkXERpdqkxIndRhfK5TJAUusY`
  (pass-1) to `uhC0kawoZqBxv3Jjvh-TlSQ5aO4U-hwiUNtZxFzXkTOBc5ijKVatw` (pass-2).
  Coordinator hot-swap does NOT work for this pass; users see a new cell
  on install and require the migration flow to keep their data.
- **I-H — Validated hive-membership infrastructure** (NEW entry types
  `HiveGenesis` + `HiveMembership` + role enum `HiveRole`). Closes H-1
  cryptographically. The action_hash of a `HiveGenesis` entry is the
  per-hive root-of-trust — the shared-DNA equivalent of Moss's
  progenitor-in-DNA-properties pattern.
- **I-A — Receiver-initiated tombstone** (NEW
  `validate_delete_encrypted_content` logic). For DMs: either party
  (anyone listed in `public_key_acl.reader`) can delete. For non-DM
  content: any agent listed in `public_key_acl.{owner|admin|writer|reader}`
  can delete.
- **I-C — Offline DM inbox** (NEW `Inbox` link type +
  `DmProbeLog` private entry + `InboxEvent` discriminator). Adapted from
  the vines `notify_peer.rs` pattern.
- **I-B — Closed without code change.** humm-tauri's
  conductor-keyring binding (`agentPubkey.ts:13-26`) already binds the
  AgentPubKey's inner 32 bytes to the Ed25519 saltpack signing key. A
  separate `sender_signing_pubkey` header field would be redundant AND
  non-validatable by the integrity zome. Documented in
  [`T_SECURITY_SENDER_IDENTITY_UNATTESTED`](../../humm-tauri/.newTasks/T_SECURITY_SENDER_IDENTITY_UNATTESTED.md)
  as resolved by binding.
- **All hive-scoped link validators ARE LIVE.** Pre-pass-2 every
  `Hive`/`Dynamic`/`HummContent*`/`HummContentId` validator was a no-op
  `Ok(Valid)` stub. Post-pass-2 they recompute the expected base path
  from the target entry's validated header fields and reject any link
  whose claimed base does not match. This is the **load-bearing
  control** that the C4 docstring previously warned was missing.

### Deploy (NOT transparent — DNA-hash bump, migration required)

This is the inverse of pass-1's coordinator hot-swap path. The DNA hash
changes; existing data does not survive without migration. The pass-1
migration scaffold (shipped at commit `520bfc6`) activates here:

1. **Pre-publish** (this hApp repo, before bundling into humm-tauri):
   ```bash
   cd ~/humm-earth-core-happ
   RUSTFLAGS='--cfg getrandom_backend="custom"' \
     CARGO_TARGET_DIR=target \
     cargo build --release --target wasm32-unknown-unknown
   hc dna pack dnas/humm_earth_core/workdir
   hc dna hash dnas/humm_earth_core/workdir/humm_earth_core.dna
   # MUST print: uhC0kawoZqBxv3Jjvh-TlSQ5aO4U-hwiUNtZxFzXkTOBc5ijKVatw
   hc app pack workdir --recursive
   sha256sum workdir/humm-earth-core-happ.happ
   ```
2. **humm-tauri side** (after copying the new .happ into
   `src-tauri/bin/`): bump the `APP_ID` constant so the conductor
   installs the new hApp under a fresh cell rather than colliding with
   the pass-1 cell. Per the pass-1 deploy handoff, the constant lives in
   `humm-tauri/src-tauri/src/holochain/install_humm_core_happ.rs`.
3. **User-facing flow** (still needs UI work on the humm-tauri side):
   - On launch with a pass-1 cell present, present "migration available"
     prompt; user opts in.
   - Run `scripts/migrate-dna.ts` against the pass-1 cell to export
     the user's data bundle.
   - Install the pass-2 hApp (new APP_ID).
   - For each hive the user OWNS, call `create_hive_genesis` (with
     `display_id = <old squuid hive_id>` for continuity).
   - For each hive the user is a MEMBER of, wait for the owner's
     pass-2 migration to complete and publish the new genesis hash
     (via a `MigrationMarkerV1` on the old hive's Hive entry, or
     out-of-band — TBD per integration discussion).
   - For each entry: re-stamp with the new `hive_genesis_hash`
     (and `author_membership_hash` if not the genesis author) and
     re-import.
   - Optionally: write `mark_migrated` markers on the pass-1 cell so
     pass-1 clients still online discover the move.

The user-facing migration UX is humm-tauri-side work; this repo ships
the integrity contract + coordinator externs + scaffold.

## Wire-shape changes (REQUIRED humm-tauri callsite updates)

Pass-2 changes the input shape of every hive-scoped extern. ALL
existing humm-tauri callsites that target these MUST be updated;
otherwise zome calls will fail to deserialize.

### `CreateEncryptedContentInput` (REQUIRED schema additions)

```ts
// pass-1 shape (now broken)
{
  id, hive_id, content_type,
  revision_author_signing_public_key, bytes, acl, public_key_acl,
  dynamic_links?
}

// pass-2 shape
{
  id,
  hive_id,                        // kept as display alias
  hive_genesis_hash,              // NEW — required ActionHash (msgpack bytes)
  author_membership_hash,         // NEW — required Option<ActionHash>
  content_type,
  revision_author_signing_public_key, bytes, acl, public_key_acl,
  dynamic_links?
}
```

humm-tauri must call `get_latest_membership({agent: me, hive_genesis_hash})`
before each write to fetch the right `author_membership_hash` to stamp.
If the local agent IS the genesis author, the call returns `None` and
you pass `null` for `author_membership_hash` (the integrity validator
picks up the implicit-Owner path).

### Hive-scoped query inputs (`hive_id` → `hive_genesis_hash`)

Every `hive_id: String` field on the following inputs is replaced with
`hive_genesis_hash: ActionHash`:

- `ListByHiveInput` (`list_by_hive_link`)
- `CountByHiveInput` (`count_links_by_hive`)
- `ListByContentIdInput` (`get_by_content_id_link`)
- `ListByDynamicLinkInput` (`list_by_dynamic_link`)
- `ListByAclInput` (`list_by_acl_link`)
- `FetchPairWithHiveCheckInput` — `active_hive_id` renamed to
  `active_hive_genesis_hash` (`fetch_pair_ss_with_hive_check`)

The author leg (`author: String`) on `ListByAuthorInput` and
`FetchPairWithHiveCheckInput` stays unchanged — that one is keyed by
pubkey, not by hive.

### New externs (read surface — cap-granted)

- `get_latest_membership({agent, hive_genesis_hash}) -> Option<HiveMembershipResponse>`
  — the read-side helper humm-tauri calls before each content write to
  resolve `author_membership_hash`.
- `list_my_hives() -> Vec<ListedHive>` — derives "hives I'm part of"
  from the local agent's Inbox HiveInvite link set. Each `ListedHive`
  carries `hive_genesis_hash`, `display_id`, and `role: Option<HiveRole>`
  (`None` when the local agent is the hive's genesis author).
- `probe_inbox({event_filter?}) -> Vec<InboxItem>` — surfaces every
  Inbox pointer addressed to the local agent, optionally filtered by
  event type. Each item carries the link's action hash (pass to
  `consume_inbox_item`), target action hash, decoded event, timestamp,
  and sender pubkey (cryptographically attested as the link author).
- `get_last_probe() -> Option<DmProbeLog>` — most-recent `DmProbeLog`
  private entry the caller has committed. Source-chain-local; cheap to
  call on every UI tick.

### New externs (write surface — NOT cap-granted, local-UI only)

- `create_hive_genesis({display_id}) -> HiveGenesisResponse` — commits
  a `HiveGenesis` entry. Permissionless (any agent may found a hive).
  Also publishes a self-tagged `Inbox` HiveInvite link so
  `list_my_hives` surfaces the hive without a chain replay.
- `create_hive_membership({hive_genesis_hash, for_agent, role,
  grantor_membership_hash, expiry}) -> HiveMembershipResponse` — grants
  a role. Validation in the integrity zome: no self-grants; grantor
  must hold Admin+ in the hive (via `grantor_membership_hash` chain
  walk, or implicit if grantor IS genesis author); only Owner may
  grant Owner; expiry honoured.
- `send_to_inbox({recipient, target, event}) -> ActionHash` — publish
  an Inbox pointer for `recipient`. Sender = call_info().provenance
  (link author = current agent, cryptographically attested).
- `consume_inbox_item(link_action_hash) -> ActionHash` — recipient
  delete (or sender retract) of an Inbox pointer.
- `record_probe({last_processed_inbox_link_hash?}) -> ActionHash` —
  commits a `DmProbeLog` private entry (source-chain only). Use after a
  successful inbox sweep to advance the unread-count cursor.

The write externs stay local-only by design — granting them
`Unrestricted` would let a peer use the local agent as a write proxy
(amplification + spoofing the link author). See the security audit in
`coordinator/.../lib.rs::set_cap_tokens`'s "NOT GRANTED" block.

## What this closes

- **H-1 (pair-SS poisoning via author rescue):** Mallory cannot write a
  poisoned SS that binds to Bob's hive because she lacks a Bob-issued
  `HiveMembership(Writer+)` for that hive. The integrity validator
  rejects the content commit AND every hive-scoped link Mallory might
  try to publish (recompute mismatch + author-target-binding check).
- **Self-asserted hive_id pollution:** `hive_id: String` survives as a
  display alias but the integrity layer ignores it entirely; security
  flows through `hive_genesis_hash`. A peer cannot get content into
  another hive's discovery paths.
- **Forged ACL elevation:** `acl` (group squuids) stays as a routing
  hint, not an authorization source. Authority flows through
  `HiveMembership`; the per-entry ACL is informational. (Future: if/when
  per-Group authority becomes load-bearing, model it as a HiveMembership
  variant pointing at a Group entry — but pass-2 does not require this.)
- **DM delete authority:** I-A's `public_key_acl.{owner|admin|writer|reader}`
  check unlocks the receiver-initiated tombstone driver task
  (`T_DM_DELETE_IMPL.md` Tier B).
- **Offline DM delivery:** I-C's Inbox link surface unblocks the
  driver task for cross-host DM delivery when one party is offline at
  send time (the link sits in the DHT until the recipient probes).

## What this does NOT close

- **Hive owner / member migration coordination.** A hive owner MUST
  migrate to pass-2 first and publish their new genesis hash before
  members can migrate their entries (members need the
  `hive_genesis_hash` to stamp on re-imported entries). The pass-1
  migration scaffold provides the per-entry forward-pointer mechanism
  but does NOT include the "wait for hive owner's new genesis"
  coordination step. App-level state in humm-tauri owns this.
- **Per-Group cryptographic membership.** humm-tauri's `Group` /
  `GroupMemberList` entries (separate from this DNA's `HiveMembership`)
  remain author-asserted. Treating them as load-bearing would require
  a third integrity entry type (`GroupMembership`); deferred.
- **Tryorama coverage.** Host-side cargo tests (26 integrity + 22
  coordinator = 48 total, all green at pass-2.5 HEAD `2cde900`) are
  the load-bearing proof. Tryorama tests for I-H, I-A, I-C are
  scaffolded but the harness is not yet paired in this repo. When it
  lands, the existing test fixtures port directly.

## Hash invariants for verification

After packing the pass-2 .happ, ALL of:

```
DNA hash:                uhC0kawoZqBxv3Jjvh-TlSQ5aO4U-hwiUNtZxFzXkTOBc5ijKVatw
content_integrity.wasm:  cd137fcfde8632c7236497592014b3b1e80548691ee71e5ebbad12cb373275dc
content.wasm:            b2df2efc6790b20467043cdd5519945f62c2c14144e9f4d9ebd5fb7d5d43bffd
```

Mismatch on any of these means the build diverged from this commit.
Recorded in `.baseline-hashes.txt` at the repo root.

## Commit + branch state

- **Branch:** `feat-integrity-pass-2` (from `a10a4ba` on
  `feat-optional-recipient-id`).
- **Pass-2 integrity commit:** `1fa4d37` — `feat(integrity): pass-2
  validated hive membership + delete authority + offline inbox`
  (21 files changed, 2809 insertions, 532 deletions).
- **Pass-2 FINAL (after coding-standards cleanup):** `891acc9` —
  `style(pass-2): strip task-ID labels per CODING_STANDARDS standard 3`.
  Sets the DNA hash invariant for the pass-2 release.
- **Pass-2.5 coordinator-only extension:** `2cde900` — `feat(coordinator):
  pass-2.5 migration tooling — MigrationMarkerV2 + hive-identity track`.
  HEAD as of this writing; DNA hash byte-identical to pass-2 FINAL.
- **Authoritative mirror:** `~/humm-earth-core-happ` (Linux). Use
  `git -C /mnt/c/proj/github/hummhive/humm-earth-core-happ merge --ff-only
  wsl/feat-integrity-pass-2` (or equivalent fetch path) to sync the
  Windows mirror after each commit lands.

## Migration commands (operational)

Pass-2.5 ships the migration tooling that actually moves data forward.
The full mechanics + per-command reference live in
[`DNA_MIGRATION_GUIDE.md`](./DNA_MIGRATION_GUIDE.md); below is the
minimum-viable runbook.

### Owner side (run first)

```bash
# Env once
export ADMIN_PORT=4444
export NEW_APP_ID=humm-earth-core@2
export NEW_DNA_HASH_BASE64=$(hc dna hash dnas/humm_earth_core/workdir/humm_earth_core.dna)
# Should print: uhC0kawoZqBxv3Jjvh-TlSQ5aO4U-hwiUNtZxFzXkTOBc5ijKVatw

# Step 1 — create HiveGenesis on the new DNA per hive you own.
# Third arg = OLD-DNA action hash of the entry the V2 hive-identity
# marker will land on (typically humm-tauri's "hive setup" anchor for
# the old squuid hive_id). Pass "" to defer.
npx tsx scripts/migrate-dna.ts migrate-hive \
  "$NEW_APP_ID" \
  hive-abc123 \
  uhCkk-old-hive-anchor-ah \
  /tmp/migrate/hive-bundle.json

# Step 2 — grant Writer (or Admin/Reader/Owner) to each member's NEW pubkey.
npx tsx scripts/migrate-dna.ts grant-memberships \
  "$NEW_APP_ID" \
  /tmp/migrate/hive-bundle.json \
  hive-abc123 \
  Writer \
  uhCAk-member-1-new-pubkey uhCAk-member-2-new-pubkey

# Step 3 — export the owner's own pass-1 chain.
npx tsx scripts/migrate-dna.ts export humm-earth-core@1 /tmp/migrate/bundle.json

# Step 4 — import the owner's entries onto the new DNA, stamping the
# new fields. Reads hive-bundle to resolve hive_genesis_hash per entry.
npx tsx scripts/migrate-dna.ts import \
  "$NEW_APP_ID" \
  /tmp/migrate/bundle.json \
  /tmp/migrate/hive-bundle.json \
  /tmp/migrate/remap.json

# Step 5a — write V2 hive-identity markers on the OLD chain pointing
# at the new HiveGenesis. Members discover via get_migration_marker_v2
# against the old anchor.
npx tsx scripts/migrate-dna.ts mark-hive-migrated \
  humm-earth-core@1 \
  /tmp/migrate/hive-bundle.json

# Step 5b — write V2 per-entry markers on the OLD chain for the
# owner's own entries. Use --v1-only when the OLD app's coordinator
# predates the pass-2.5 hot-swap (no mark_migrated_v2 extern).
npx tsx scripts/migrate-dna.ts mark-migrated \
  humm-earth-core@1 \
  /tmp/migrate/remap.json

# Share /tmp/migrate/hive-bundle.json with each member via an
# ENCRYPTED out-of-band channel (Signal, age, password-protected
# download). The bundle enumerates the hive's full member roster
# (AgentPubKey + role + membership hash per grantee); treat as
# operationally sensitive. On multi-user hosts move it out of
# /tmp into a chmod-700 dir (`mkdir -m 700 ~/.migrate && mv
# /tmp/migrate/hive-bundle.json ~/.migrate/`).
```

### Member side (run after owner has finished steps 1-2)

```bash
export ADMIN_PORT=4444
export NEW_APP_ID=humm-earth-core@2
export NEW_DNA_HASH_BASE64=$(hc dna hash dnas/humm_earth_core/workdir/humm_earth_core.dna)

# Receive /tmp/migrate/hive-bundle.json from the owner via an
# ENCRYPTED out-of-band channel (Signal, age-encrypted email, etc.).
# Treat as operationally sensitive: the bundle reveals the hive's
# full member roster. Store under a chmod-700 dir on multi-user hosts.

# Export the member's own pass-1 chain.
npx tsx scripts/migrate-dna.ts export humm-earth-core@1 /tmp/migrate/bundle.json

# Import — looks up the caller's membership_hash on each hive via
# get_latest_membership against the new DNA.
npx tsx scripts/migrate-dna.ts import \
  "$NEW_APP_ID" \
  /tmp/migrate/bundle.json \
  /tmp/migrate/hive-bundle.json \
  /tmp/migrate/remap.json

# Mark the member's old per-entry chain as migrated.
npx tsx scripts/migrate-dna.ts mark-migrated \
  humm-earth-core@1 \
  /tmp/migrate/remap.json
```

### Verifying success

- `remap.json.failures` is empty.
- `hive-bundle.json` has one `hives[*]` entry per migrated hive, each
  with a non-null `new_genesis_hash_base64`.
- Querying the new app via `list_my_hives` returns one entry per
  migrated hive (Owner side: `role: None`; Member side:
  `role: Some(Writer|...)`).
- Calling `get_migration_marker_v2(<old_anchor_ah>)` against the OLD
  app returns `Some(MigrationMarker::V2 { new_hive_genesis_hash_base64:
  Some(_), ... })`.

### Marker version selection cheat sheet

| Scenario | Use |
|---|---|
| OLD app already has pass-2.5 coordinator (V1+V2 readers + `mark_migrated_v2` extern) | Default (V2 markers everywhere) |
| OLD app has pass-1 coordinator only (no V2 extern) | `mark-migrated --v1-only`; SKIP `mark-hive-migrated` (no V2 extern available — fall back to out-of-band genesis-hash distribution) |
| Mixed: OLD app has the pass-2.5 hot-swap, but you specifically want pre-pass-2 hosts to also see per-entry markers | `mark-migrated --v1-only` |

### Hash invariants — pass-2.5 follow-up

The pass-2.5 follow-up is coordinator-only and MUST hold the DNA hash
from pass-2 byte-identical. The coordinator wasm sha256 changes
because new functions ship; the integrity wasm + DNA hash do not.
See [`.baseline-hashes.txt`](../.baseline-hashes.txt) for the current
invariants per pass.
