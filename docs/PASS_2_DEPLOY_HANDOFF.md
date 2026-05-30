# Pass-2 deploy handoff — humm-tauri integration

Short-form handoff for the humm-tauri team to integrate the pass-2
integrity-zome changes shipped on `feat-integrity-pass-2`. Pass-2
**intentionally bumps the DNA hash** and is the first non-additive
integrity change since pass-1 froze the baseline. Existing data MUST be
migrated forward via the pass-1 migration scaffold (`scripts/migrate-dna.ts`
+ `MigrationMarkerV1` reader) before users can keep using their hives.

For the full per-change reference, see
[`HUMM_TAURI_COORDINATOR_INTEGRATION.md`](./HUMM_TAURI_COORDINATOR_INTEGRATION.md)
(pass-2 section appended) and
[`DNA_MIGRATION_GUIDE.md`](./DNA_MIGRATION_GUIDE.md) (pass-2 migration
flow section appended).

## TL;DR

- **DNA hash CHANGED** from `uhC0kT0Tkc3b6ccfa75YwWdzpSWvdkXERpdqkxIndRhfK5TJAUusY`
  (pass-1) to `uhC0kzl0W9BBITBGu-NeUaXPxqxSPj0yTGfDD3UH3EjhfLDQZfZxe` (pass-2).
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
   # MUST print: uhC0kzl0W9BBITBGu-NeUaXPxqxSPj0yTGfDD3UH3EjhfLDQZfZxe
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
- **Tryorama coverage.** Host-side cargo tests (26 integrity + 11
  coordinator = 37 total, all green) are the load-bearing proof.
  Tryorama tests for I-H, I-A, I-C are scaffolded but the harness is
  not yet paired in this repo. When it lands, the existing test
  fixtures port directly.

## Hash invariants for verification

After packing the pass-2 .happ, ALL of:

```
DNA hash:                uhC0kzl0W9BBITBGu-NeUaXPxqxSPj0yTGfDD3UH3EjhfLDQZfZxe
content_integrity.wasm:  82639e455c004d04a1f6fbf25f01abf26241e2fbe1733cbe39db7882a4fb2402
content.wasm:            76ca7f5bb8ab4752ace71549800704f548d2a1d291cbecec3e324e083c236b99
```

Mismatch on any of these means the build diverged from this commit.
Recorded in `.baseline-hashes.txt` at the repo root.

## Commit + branch state

- **Branch:** `feat-integrity-pass-2` (from `a10a4ba` on
  `feat-optional-recipient-id`).
- **Commit:** `1fa4d37` — `feat(integrity): pass-2 validated hive
  membership + delete authority + offline inbox`.
- **21 files changed**, 2809 insertions, 532 deletions (3 new
  integrity-zome modules + 6 new coordinator modules + 6 updated
  coordinator files + 2 updated integrity files + baseline hash record).
- **Authoritative mirror:** `~/humm-earth-core-happ` (Linux). Use
  `git -C /mnt/c/proj/github/hummhive/humm-earth-core-happ merge --ff-only
  wsl/feat-integrity-pass-2` (or equivalent fetch path) to sync the
  Windows mirror after this commit lands.
