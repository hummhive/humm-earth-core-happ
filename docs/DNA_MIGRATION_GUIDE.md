# DNA Migration Guide

When the integrity zome ships a non-additive change, the DNA hash
changes. A different DNA hash means a fresh DHT, a fresh conductor
source chain per cell, and fresh agent pubkeys. Existing users keep
their data ONLY if it is migrated forward.

`scripts/migrate-dna.ts` is the orchestrator that does the migration,
plus a forward-pointer marker mechanism (coordinator-only, no DNA-hash
impact) that lets old-DNA clients detect "this data has moved" and
prompt the user to upgrade — so a partially-migrated user base degrades
gracefully instead of silently losing access.

> **Status update — pass-3 in flight.** The **pass-3** integrity work
> (group authority + `AclSpec` wire reshape) is committed on branch
> `feat-integrity-pass-3-groups` but **not yet pushed**. The pass-2
> DNA hash listed below is still the latest deployed hash; pass-3 will
> bump it again. The migration tool in `scripts/migrate-dna.ts` has
> been extended to emit the new pass-3 wire shape on `import`; see
> the new "Pass-3 wire-shape migration" section below for the
> mechanics and humm-tauri implications. For the full delta vs the
> pass-2.5 handoff, see
> [`HANDOFF_UPDATED_INFO.md`](./HANDOFF_UPDATED_INFO.md).
>
> **Status:** the **pass-2** integrity change has shipped on
> `feat-integrity-pass-2` at DNA hash
> `uhC0kawoZqBxv3Jjvh-TlSQ5aO4U-hwiUNtZxFzXkTOBc5ijKVatw`
> (was `uhC0kT0Tkc3b6ccfa75YwWdzpSWvdkXERpdqkxIndRhfK5TJAUusY` on
> pass-1). **Pass-2.5** extends the migration tool with the
> hive-identity track + V2 markers needed to actually move data from
> pass-1 to pass-2. No DNA hash change — coordinator-only.
>
> See [`PASS_2_DEPLOY_HANDOFF.md`](./PASS_2_DEPLOY_HANDOFF.md) for the
> wire-shape change list + deploy steps.

> **Pre-launch caveat:** with no production users today, this scaffold
> is **forward-looking infrastructure**. Get it right now while the
> blast radius is zero.

---

## Two tracks, one pipeline

Pass-2.5 ships two coordinated tracks; they share state via a
**hive-bundle** file:

1. **Per-entry track** — `export` → `import` → `mark-migrated`. Shuttles
   every live `EncryptedContent` onto the new DNA via
   `create_encrypted_content`; writes forward-pointer markers onto the
   old chain. The pass-1 baseline, now stamping the pass-2-required
   `hive_genesis_hash` + `author_membership_hash` fields on every import.
2. **Hive-identity track** — `migrate-hive` → `grant-memberships` →
   `mark-hive-migrated`. Owner-side, run before any member imports.
   Creates `HiveGenesis` entries on the new DNA, grants
   `HiveMembership`s to the cell agents who will re-import, and writes
   V2 markers on the old chain pointing at the new genesis hashes.

You need to run BOTH tracks when migrating from pass-1 to pass-2. The
pass-1 schema had no `hive_genesis_hash`; pass-2 requires it on every
entry. Without the hive-identity track, every `import` call fails
integrity validation.

You do NOT need this for coordinator-only changes — those hot-swap in
place (see the deploy section of the handoff guide).

---

## Pipeline at a glance

```
OWNER side                                MEMBER side
  ┌───────────────────────────┐
1 │ migrate-hive              │           (owner publishes new
  │  → create_hive_genesis    │            HiveGenesis on new DNA;
  │  → write hive-bundle.json │            appends mapping to bundle)
  └───────────────────────────┘
              │
              ▼
  ┌───────────────────────────┐
2 │ grant-memberships         │           (owner grants Writer+ to
  │  → create_hive_membership │            each member's NEW pubkey;
  │  → update hive-bundle.json│            appends per-member membership)
  └───────────────────────────┘
              │
              ▼ (hive-bundle shared with members out-of-band)
              │
  ┌───────────────────────────┐  ┌───────────────────────────┐
3 │ export <old-app-id>       │  │ export <old-app-id>       │
  │  → bundle.json            │  │  → bundle.json            │
  └───────────────────────────┘  └───────────────────────────┘
              │                              │
              ▼                              ▼
  ┌───────────────────────────┐  ┌───────────────────────────┐
4 │ import <new-app-id>       │  │ import <new-app-id>       │
  │  + bundle.json            │  │  + bundle.json            │
  │  + hive-bundle.json       │  │  + hive-bundle.json       │
  │  → remap.json             │  │  → remap.json             │
  │  (stamps hive_genesis_hash│  │  (lookup their member-     │
  │   + None membership_hash) │  │   ship_hash via            │
  │                           │  │   get_latest_membership)   │
  └───────────────────────────┘  └───────────────────────────┘
              │                              │
              ▼                              ▼
  ┌───────────────────────────┐  ┌───────────────────────────┐
5 │ mark-hive-migrated        │  │ mark-migrated             │
  │  → V2 markers on OLD hive │  │  → V2 markers on OLD       │
  │    setup entry, pointing  │  │   per-entry chain          │
  │    at new HiveGenesis     │  │                            │
  └───────────────────────────┘  └───────────────────────────┘
              │                              │
              └──────────────┬───────────────┘
                             ▼
                    Old-DNA clients reading
                    via get_migration_marker_v2
                    discover the new DNA + hive
                    identity; prompt user to upgrade
```

The owner MUST complete steps 1–2 before any member runs step 4 —
without a granted HiveMembership on the new DNA, the integrity
validator rejects the member's `create_encrypted_content` calls (see
[`HUMM_TAURI_COORDINATOR_INTEGRATION.md`](./HUMM_TAURI_COORDINATOR_INTEGRATION.md)
for the validator rules). The owner can run step 1 on their own
schedule; step 2 needs every member's new-DNA pubkey, which members
obtain by installing the new `.happ` (fresh pubkey per install) and
sharing it out-of-band.

---

## Hive-bundle file format

The hive-bundle is the load-bearing handoff between owner and members.
The owner runs steps 1–2 and shares the resulting JSON; members consume
it in step 4. The owner ALSO uses it in step 5.

```json
{
  "schema_version": 1,
  "generated_at_iso": "2026-05-30T15:30:00.000Z",
  "hives": [
    {
      "old_hive_id": "hive-abc123",
      "new_genesis_hash_base64": "uhCkk...",
      "new_display_id": "hive-abc123",
      "owner_pubkey_base64": "uhCAk...",
      "owner_membership_hash_base64": null,
      "old_marker_action_hash_base64": "uhCkk...",
      "granted_memberships": [
        {
          "for_agent_base64": "uhCAk...",
          "role": "Writer",
          "membership_hash_base64": "uhCkk..."
        }
      ]
    }
  ]
}
```

Field notes:

- `old_hive_id` — the squuid `hive_id` from the OLD DNA. Used by
  `import` to map `BundleEntry.encrypted_content.header.hive_id` →
  `new_genesis_hash_base64`.
- `new_genesis_hash_base64` — multibase action hash of the `HiveGenesis`
  the owner committed on the NEW DNA in step 1.
- `new_display_id` — alias stamped on the new `HiveGenesis`; defaults
  to `old_hive_id` for continuity.
- `owner_pubkey_base64` — pubkey of the agent that created the new
  HiveGenesis. The integrity zome treats this agent as the implicit
  Owner; they do NOT hold an explicit membership entry (no
  `owner_membership_hash_base64`).
- `owner_membership_hash_base64` — always `null` in this revision;
  preserved as a field for forward compatibility.
- `old_marker_action_hash_base64` — action hash of the OLD-DNA entry
  that step 5 (`mark-hive-migrated`) writes the V2 marker onto.
  Typically the OLD entry humm-tauri uses as the "hive setup" anchor.
  `null` defers the marker until the operator populates it (the field
  may be set via `migrate-hive`'s third positional arg or by editing
  the JSON directly).
- `granted_memberships` — appended-to by `grant-memberships`. Each
  member's `membership_hash_base64` is what `import` stamps as
  `author_membership_hash` when that member re-imports.

The hive-bundle is built incrementally:

- `migrate-hive` creates the bundle file (or appends to it) with one
  entry per call.
- `grant-memberships` appends to an existing hive entry's
  `granted_memberships`.
- `import` reads it (one bundle covering every hive the member belongs
  to).
- `mark-hive-migrated` reads it (one bundle covering every hive the
  owner founded).

---

## CLI reference

### `migrate-hive` (owner, step 1)

```bash
NEW_APP_ID=humm-earth-core@2 \
ADMIN_PORT=4444 npx tsx scripts/migrate-dna.ts migrate-hive \
  "$NEW_APP_ID" \
  hive-abc123 \
  uhCkk-old-anchor-action-hash \
  /tmp/migrate/hive-bundle.json
```

Creates a `HiveGenesis` on the new DNA with `display_id = old-hive-id`.
Appends a hive entry to the bundle with the new genesis hash + the
operator-supplied old anchor action hash (the OLD entry the V2 marker
will land on).

Pass `""` (empty string) for the third arg to defer the marker target —
the hive entry's `old_marker_action_hash_base64` stays `null` and
`mark-hive-migrated` will SKIP it with a warning.

### `grant-memberships` (owner, step 2)

```bash
ADMIN_PORT=4444 npx tsx scripts/migrate-dna.ts grant-memberships \
  humm-earth-core@2 \
  /tmp/migrate/hive-bundle.json \
  hive-abc123 \
  Writer \
  uhCAk-member-1-pubkey-b64 uhCAk-member-2-pubkey-b64
```

Calls `create_hive_membership` per pubkey at the given role
(`Owner`|`Admin`|`Writer`|`Reader`); appends `{for_agent_base64, role,
membership_hash_base64}` entries to the named hive's
`granted_memberships`.

Re-run with additional pubkeys to grant more memberships later. Each
invocation appends; existing memberships are NOT deduplicated.

### `export` (either side, step 3)

```bash
ADMIN_PORT=4444 npx tsx scripts/migrate-dna.ts export \
  humm-earth-core@1 \
  /tmp/migrate/bundle.json
```

Walks the local source chain, emits the deduped-by-id bundle. Identical
to the pass-1 export. Bundles from a pass-2 DNA include the
`hive_genesis_hash` / `author_membership_hash` header fields (as
base64-encoded multibase strings); pass-1 bundles omit them.

### `import` (either side, step 4)

```bash
ADMIN_PORT=4444 npx tsx scripts/migrate-dna.ts import \
  humm-earth-core@2 \
  /tmp/migrate/bundle.json \
  /tmp/migrate/hive-bundle.json \
  /tmp/migrate/remap.json
```

For each bundle entry:

1. Look up `header.hive_id` in the hive-bundle to find the
   `new_genesis_hash_base64`.
2. Determine the caller's `author_membership_hash`:
   - If the caller IS the hive's `owner_pubkey_base64`, stamp `null`
     (implicit Owner).
   - Else, call `get_latest_membership({agent: me, hive_genesis_hash})`
     on the NEW DNA. Cache per-hive so the lookup happens once per
     hive. If `None`, every entry in that hive is pre-failed with
     `no_membership_in_new_hive`.
3. Call `create_encrypted_content` with the new fields stamped + the
   caller's NEW agent pubkey on `revision_author_signing_public_key`.
4. Record `{old_action_hash → new_action_hash, new_hive_genesis_hash_base64}`
   in the remap.

`remap.json` shape (schema_version 1; pass-2 fields optional):

```json
{
  "schema_version": 1,
  "source_app_id": "humm-earth-core@1",
  "source_agent_pubkey_base64": "uhCAk...",
  "target_app_id": "humm-earth-core@2",
  "target_agent_pubkey_base64": "uhCAk...",
  "imported_at_iso": "2026-05-30T15:30:00.000Z",
  "entries": [
    {
      "id": "msg-abc123",
      "old_action_hash": "uhCkk...",
      "new_action_hash": "uhCkk...",
      "content_type": "dm",
      "hive_id": "hive-abc123",
      "new_hive_genesis_hash_base64": "uhCkk..."
    }
  ],
  "failures": []
}
```

Failures land in the `failures` array with one of:

- `hive_not_in_hive_bundle: <hive_id>` — the entry's `hive_id` has no
  corresponding entry in the hive-bundle. Run `migrate-hive` for that
  hive or remove the entries from the bundle.
- `no_membership_in_new_hive: <old_hive_id>` — the owner has not granted
  the caller a membership on the new DNA. Ask the owner to run
  `grant-memberships` for the caller's pubkey.
- (any other Rust integrity-validator error) — surfaced verbatim from
  the zome call.

### `mark-hive-migrated` (owner, step 5)

```bash
NEW_DNA_HASH_BASE64=$(hc dna hash dnas/humm_earth_core/workdir/humm_earth_core.dna) \
NEW_APP_ID=humm-earth-core@2 \
ADMIN_PORT=4444 npx tsx scripts/migrate-dna.ts mark-hive-migrated \
  humm-earth-core@1 \
  /tmp/migrate/hive-bundle.json
```

For each hive in the bundle with `old_marker_action_hash_base64` set,
calls `mark_migrated_v2` on the OLD app. The V2 marker carries
`new_hive_genesis_hash_base64` and `new_hive_genesis_display_id` so
members reading via `get_migration_marker_v2(old_anchor_ah)` discover
the new genesis.

Hives without `old_marker_action_hash_base64` are skipped with a
warning. Populate it by re-running `migrate-hive` against a fresh
bundle OR by editing the JSON directly.

### `mark-migrated` (either side, step 5; per-entry)

```bash
NEW_DNA_HASH_BASE64=$(hc dna hash dnas/humm_earth_core/workdir/humm_earth_core.dna) \
ADMIN_PORT=4444 npx tsx scripts/migrate-dna.ts mark-migrated \
  humm-earth-core@1 \
  /tmp/migrate/remap.json
```

Default writes V2 markers (`mark_migrated_v2`). Add `--v1-only` when
the OLD app's coordinator predates the pass-2.5 hot-swap and lacks the
`mark_migrated_v2` extern — the OLD chain still receives the redirect
under the V1 shape.

The marker is an update to the original entry; only the original author
can write a valid one (the readers' author-binding filter discards
sibling-author updates). The `mark_migrated_v2` extern is NOT in the
cap grant, so only the local UI / this script can invoke it.

---

## Marker versions

Two marker schemas coexist on the wire:

| Schema | Reader extern | What V1-only hosts see |
|---|---|---|
| **`MigrationMarkerV1`** | `get_migration_marker` → `Option<MigrationMarkerV1>` | Decodes normally. |
| **`MigrationMarkerV2`** (adds `new_hive_genesis_hash_base64`, `new_hive_genesis_display_id`) | `get_migration_marker_v2` → `Option<MigrationMarker>` (tagged enum over `V1` + `V2`) | `Ok(None)`. V2 bytes decode into the V1 struct (msgpack `with_struct_map` ignores unknown fields), but the V1 reader's `is_well_formed()` checks `schema_version == 1` and rejects. |

V2 readers handle BOTH shapes:

- V2 bytes → decoded as `MigrationMarker::V2(MigrationMarkerV2 { ... })`.
- V1 bytes → decoded as `MigrationMarker::V1(MigrationMarkerV1 { ... })`.
  V2 decode of V1 bytes succeeds via `#[serde(default)]` on the V2-only
  fields, but V2's `is_well_formed()` rejects (`schema_version == 1`);
  the reader falls back to V1 decode.

**Rollout implication:** V1-only hosts must be upgraded to a
pass-2.5-aware coordinator before they can discover hive-identity
migrations. This is acceptable for the current pre-launch user base
(humm-tauri is the only host; humm-tauri upgrades alongside this
repo). For per-entry markers, pass `--v1-only` to `mark-migrated` if
you specifically need to communicate with V1-only readers.

---

## Pass-3 wire-shape migration

Pass-3 reshapes `EncryptedContentHeader`: the four pass-2 fields
`hive_id` / `hive_genesis_hash` / `author_membership_hash` / `acl`
collapse into a single `acl_spec: AclSpec` discriminated-union field
with four variants (`HiveGroup`, `DirectMessage`, `Public`,
`OpenWrite`). The migration script handles the reshape on `import`
via a **content-type → `AclSpec` classification table** —
`CONTENT_TYPE_ACL_SPEC` near the top of `scripts/migrate-dna.ts`.
No operator action is needed to translate the bundle format; the
classifier runs per-entry during `import`.

### Classification table (defaults)

| Content type                                           | AclSpec variant                                   |
|--------------------------------------------------------|---------------------------------------------------|
| `direct_message`, `hummhive-core-peer-identity-claim-v1` | `DirectMessage { recipients }`                 |
| `hummhive-core-member-request-v1`, `hummhive-core-hive-discovery-v1`, `hummhive-core-agent-directory-v1` | `OpenWrite { target_hive_genesis_hash }` |
| `humm-addon-text-post-v1`, `hummhive-core-hive-v1`     | `Public { hive_genesis_hash, author_membership_hash }` |
| Everything else (default)                              | `Public { hive_genesis_hash, author_membership_hash }` |

The default is `Public` (not `HiveGroup`) — see the rationale in the
script comment block. pass-3 `HiveGroup` requires the author to hold
Writer+ in every group listed in `group_acl.*`, but pass-1/pass-2
had no `group_acl` field, so the migration cannot populate it
without operator input. `Public` keeps the entry readable by every
member of the hive (via hive Writer+ on the author), matching the
most common humm-tauri "everyone in the hive sees this" pattern.
humm-tauri can re-stamp specific entries to `HiveGroup` post-migration
once real groups exist.

### Per-variant restamp mechanics

- **`DirectMessage`** — the classifier reads the legacy
  `public_key_acl.reader` list as the recipient set, splices the
  new agent's pubkey in if absent (the integrity validator requires
  `author ∈ recipients`), and pins `public_key_acl.reader ==
  recipients` (validator binds these for I-A delete authority
  symmetry). Cardinality bounds are checked at commit time
  (`2 <= recipients.len() <= DM_MAX_RECIPIENTS = 32`).
- **`OpenWrite`** — the classifier inlines the resolved
  `hive_genesis_hash` as `target_hive_genesis_hash` so the entry
  keeps appearing in the target hive's discovery index. The
  integrity validator only enforces author-identity + target
  existence.
- **`Public`** — the classifier inlines `hive_genesis_hash` +
  `author_membership_hash`. The integrity validator requires
  Writer+ in the named hive (mirrors the pass-2 hive-authority
  chain — the membership the operator established via
  `grant-memberships` in step 2 of the hive-identity track).
- **`HiveGroup`** — NOT auto-generated by the migration this pass.
  If `CONTENT_TYPE_ACL_SPEC` maps a content type to `HiveGroup`,
  the classifier throws a clear error: HiveGroup classification
  requires the group-migration track (Phase D.1, deferred).

### Deferred — Phase D.1 (group track + classification overrides)

The pass-3 plan calls for a `migrate-group` / `grant-group-memberships`
command pair that materialises legacy humm-tauri group squuids as
`GroupGenesis` entries on the new DNA, plus a per-bundle
`classification-overrides.json` mechanism so operators can re-route
specific entries to `HiveGroup` (with the right `group_acl`) without
editing the script. **Both are deferred to a Phase D.1 follow-up.**
Until they land, the migration runs without the group track — the
default-to-`Public` classifier keeps the migration functional and
the resulting entries readable across the hive; humm-tauri post-
migration is responsible for creating real `GroupGenesis` entries
(via the new `create_group_genesis` extern) and re-stamping selected
entries with `HiveGroup` `acl_spec` when needed.

### Bundle schema versioning

- `schema_version: 1` — pass-1 / pass-2 bundles (every legacy
  export). The `import` command auto-classifies these into pass-3
  `AclSpec` variants on the fly.
- `schema_version: 2` — pass-3-aware bundles (hypothetical; not
  currently produced by `export`). `import` accepts these too; the
  classifier still runs to handle cross-DNA migrations.
- Any other `schema_version` is rejected with a clear error.

### Operational ordering

The pass-3 migration uses the SAME pipeline as pass-2:

```
OWNER:      migrate-hive  →  grant-memberships  →  mark-hive-migrated
MEMBER:     export        →  import             →  mark-migrated
```

No additional steps; the classifier is invisible to the operator.
The hive-identity track (created in pass-2.5) is unchanged. After
pass-3 ships, humm-tauri's auto-update flow integrates the same
orchestration with the new DNA hash.

## Security model (LOAD-BEARING — read this)

The integrity update validator only enforces the self-referential
`action.author == header.revision_author_signing_public_key`. It does
NOT enforce that an update's author equals the original entry's
author. Without further defense, any old-DNA peer could write a forged
marker on someone else's entry by calling `update_encrypted_content`
with the victim's original AH and their own pubkey in
`revision_author_signing_public_key`.

The **coordinator readers** `get_migration_marker` and
`get_migration_marker_v2` close this gap by:

1. Fetching the action passed in (typically the original Create).
2. Taking that action's `author` as the trusted author for marker
   updates.
3. Filtering `details.updates` to only those whose `action.author`
   matches the trusted author — sibling-author updates are silently
   ignored regardless of timestamp.

The filter binds to cryptographically-attested action authorship
(signed by the author's lair key). A modified coordinator CANNOT forge
an Update claiming `author == victim` without the victim's private
key, so the marker write-path forge is closed even against the
standard Holochain "modified-coordinator" adversary — strictly
stronger than C4's link-placement defense, which only narrows under
the unmodified-client assumption. The only residual modified-
coordinator angle is the SIGNAL path (a forged marker-shaped signal
arriving at the recipient), which the threat-model doc-comment on
`recv_remote_signal` already covers — signals are hints, the receiver
MUST re-query authoritatively via `get_migration_marker_v2`.

This means humm-tauri MUST always pass the **original Create's action
hash** (or, for hive-identity markers, the **owner-supplied anchor
hash**) to the reader, not an arbitrary update hash. The script's
remap file preserves this invariant by storing the original Create
hash in `old_action_hash`; the hive-bundle stores
`old_marker_action_hash_base64` for the same reason on the hive track.

**Mandatory host-side defenses** (the coordinator can only do so much):

- **(A) Author binding**: the coordinator readers enforce this — but
  humm-tauri SHOULD ALSO cross-check the marker arrived via the C1
  `from_agent` path from the trusted partner identity before treating
  it as authoritative.
- **(B) User consent before DNA / hive crossover**: NEVER auto-follow
  the marker's `new_dna_hash_base64` / `new_app_id` /
  `new_hive_genesis_hash_base64` without explicit human approval.
  Switching DNA or joining a new HiveGenesis crosses a trust boundary
  and must be a user decision — phishing via fake migration prompts is
  otherwise trivial.
- **(C) Cross-verify on the new DNA**: before redirecting UI to the
  new AH, confirm `get_encrypted_content(new_action_hash)` actually
  resolves on the new DNA. For hive-identity markers, also confirm the
  named `HiveGenesis` resolves and that the caller has been granted a
  membership in it. Defense (C) is strongest when the marker carries a
  valid `new_dna_hash_base64`; if the migration script was run with
  `NEW_DNA_HASH_BASE64` unset, the marker carries an empty string and
  the host can only validate via `new_app_id`. Always pass
  `NEW_DNA_HASH_BASE64` for production migrations.

---

## humm-tauri GUI integration — transparent by default, prompt only at trust boundaries

The goal is the same as humm-tauri's `WS-L` install-guard pattern for
coordinator-only changes (see
`.extraResearch/decentralizedStartupSync/EXECUTION_PLAN.md` §WS-L in
the humm-tauri repo — Tier-0 planned). When WS-L ships, users see
nothing during routine coordinator upgrades. For DNA-hash-bumping
integrity changes the cryptographic trust boundary forces ONE user
prompt (DNA crossover requires explicit consent — defense (B) above),
but everything else — install, hive-genesis publish, grant, export,
import, remap-rewrite, marker write — runs silently in the background.

### Tiered transparency model

| Change tier | What's transparent | What needs a user prompt |
|---|---|---|
| **Coordinator-only** (this pass-2.5 follow-up) | Everything (once WS-L lands). `update_coordinators` hot-swap fires on next launch when the coordinator wasm changes; no UI, no wipe, no migration. | None. |
| **Integrity / DNA-hash bump** (pass-2 itself, and later) | Install of the new `.happ` (background); hive-identity track ALL phases (background; owners are notified per-hive that their hives are migrating, but the per-step calls are background); per-entry track ALL phases (background); host-side remap rewrite (background); marker writes back to old DNA (background); cutover countdown (informational notification, not a blocking prompt). | **Exactly one mandatory prompt per side**: (1) owner consents to creating the new HiveGenesis (joining the new DNA), (2) each member consents to joining the new HiveGenesis they were invited into. Both are defense-B trust boundaries. |

The migration script's commands are the building blocks; the
recommended UX wires them as silent background work behind a single
consent dialog per side.

### Recommended flow (owner side)

1. **Update detection.** humm-tauri's auto-update channel detects a new
   `.happ` bundle (different DNA hash). Computes the new DNA hash via
   `hc dna hash` and compares.
2. **User prompt — the one mandatory dialog for owners (defense B).**
   "humm-tauri has shipped a security update that requires migrating
   your hives to a new DNA. This takes about N seconds and runs in the
   background. Migrate now?"
3. **Install new hApp** under a distinct `installed_app_id`.
4. **Run `migrate-hive` per owned hive.** Background.
5. **Run `grant-memberships` per (hive, member-list).** humm-tauri's
   address book provides the member pubkeys. Background.
6. **Export + import (per-entry track) for the owner's own entries.**
   Background.
7. **Run `mark-hive-migrated` + `mark-migrated`.** Background.
8. **Send the hive-bundle to each member via an ENCRYPTED out-of-band
   channel** (Signal, age-encrypted email, password-protected
   download, etc.). The bundle contains no private keys, but it
   enumerates the hive's full member roster (every grantee's
   AgentPubKey + role + membership hash). Treat it as operationally
   sensitive — a plaintext leak reveals "who is in this private hive"
   to anyone who intercepts the channel.
9. **Confirm cutover.** "Migration complete; old hApp will be disabled
   in N days unless you opt to keep it for cross-agent coordination."

### Recommended flow (member side)

1. **Receive hive-bundle from owner** (via the chosen out-of-band
   channel). humm-tauri imports it into local app state.
2. **User prompt — the one mandatory dialog for members (defense B).**
   "You've been invited to join the migrated <hive-name> on the new
   DNA. Switch?"
3. **Install new hApp** under a distinct `installed_app_id`.
4. **Export + import (per-entry track) for the member's own entries.**
   Background.
5. **Run `mark-migrated`** for the member's own per-entry chain.
   Background.

### Receiver-side prompt on incoming markers

humm-tauri on a NOT-YET-migrated peer's conductor sees the incoming
`_migrated/*` signal (from the owner's step 7) and proactively prompts
THEM to upgrade. Apply ALL of defenses A/B/C from the Security model
above before treating the signal as authoritative: verify the signal's
`from_agent` **equals** the trusted partner identity (defense A);
require explicit user consent before any DNA crossover (defense B);
and cross-verify the new AH on the new DNA (defense C). The signal
payload is a HINT — re-query through the author-bound
`get_migration_marker_v2(old_ah)` for the authoritative marker, then
confirm with the user before installing the new hApp.

---

## Cross-agent coordination

The hive-identity track makes the owner-first ordering explicit. The
remaining hard case is a pair-shared secret between Alice and Bob:
both their source chains hold a copy of the SS (each created their own
under their own pubkey when the pair was established). Both must
migrate before the shared SS works in the new DNA. If only Alice
migrates:

- She can still decrypt her own SS copy locally.
- Calls to `fetch_pair_ss_with_hive_check` on the new DNA return only
  her own entries (Bob hasn't seeded the new DHT yet) — looks like
  "no pair-SS exists" to her UI.
- Bob's old conductor keeps working under the old DNA in isolation.

Sequence migrations explicitly: announce a cutover window, both peers
migrate, both confirm visibility, then disable old hApp.

---

## Idempotency and re-runs

- **Export is fully idempotent.** Re-running `export` against the same
  source chain produces the same bundle (modulo the `exported_at_iso`
  timestamp).
- **`migrate-hive` is NOT idempotent for an existing entry.** Re-running
  against a hive-bundle that already contains the named `old_hive_id`
  throws — it would silently fork the genesis hash. Delete the bundle
  entry first if you really want a fresh genesis.
- **`grant-memberships` IS idempotent.** Re-running with the same
  pubkey appends a fresh membership entry (the integrity zome accepts
  multiple memberships for the same agent; `get_latest_membership`
  picks the latest). Spurious but harmless.
- **`import` is NOT idempotent at the action-hash level.** Re-running
  against the new hApp creates a SECOND set of fresh action hashes for
  the same `id` values. If you re-import, dedupe by `id` on the host
  side, or delete the prior import first via the admin API
  (uninstall + reinstall the new app-id).
- For partial failures: the `remap.json` has a `failures` array with
  the IDs that didn't import. Address the root cause (most likely a
  missing hive-bundle entry or a missing membership) and re-run.

---

## Failure modes worth pre-empting

| Symptom | Cause | Fix |
|---|---|---|
| `App "X" not found on conductor` | Old hApp uninstalled before export | Reinstall old `.happ`, re-export, then proceed |
| `check_author_matches_header` rejection on import | Importer not restamping the pubkey (manual replay outside the script) | Use the script — it restamps |
| `hive_not_in_hive_bundle: <id>` | Entry references a hive the operator did not migrate | Run `migrate-hive` for that hive, or accept losing the entry |
| `no_membership_in_new_hive: <id>` | Owner has not granted this caller a membership on the new DNA | Owner runs `grant-memberships` with the caller's NEW pubkey |
| `mark_migrated_v2` errors "function not found" | OLD app's coordinator predates pass-2.5 | Add `--v1-only` to `mark-migrated`; for `mark-hive-migrated` the hive-identity marker requires the V2 extern and a coordinator hot-swap |
| Encrypted body decrypts to garbage after migration | Tauri keyring changed between old and new install | Restore the old keyring file; never delete it during a migration window |
| `fetch_pair_ss_with_hive_check` returns `[]` after migration | Counterparty has not migrated yet | Wait for the counterparty; this is expected during the cutover window |

---

## Wire-script vs in-app integration

The standalone TS script is the **reference implementation**. For
production use, humm-tauri's auto-update flow should integrate the
same logic in Rust (via the `holochain_client_rust` crate) so the
migration runs transparently when the user installs an integrity-
breaking version. The script's bundle / remap / hive-bundle JSON
schemas are versioned (`schema_version: 1`) so the in-app
implementation and the standalone script stay interoperable.

---

## Pass-1 migration (historical reference)

The pass-1 baseline shipped before pass-2 required `hive_genesis_hash`.
For migrations BETWEEN pass-1 DNAs (e.g. an integrity tweak that
predates pass-2), the pass-1 commands suffice:

```bash
# Pass-1 export → import → mark-migrated (no hive-bundle, no V2)
npx tsx scripts/migrate-dna.ts export humm-earth-core@1 /tmp/bundle.json
npx tsx scripts/migrate-dna.ts import humm-earth-core@2 /tmp/bundle.json /tmp/hive-bundle.json /tmp/remap.json
npx tsx scripts/migrate-dna.ts mark-migrated humm-earth-core@1 /tmp/remap.json --v1-only
```

Pass-1 imports into a pass-1 target need a hive-bundle that maps every
pass-1 hive_id 1:1 to itself (no new HiveGenesis to publish). In
practice this is what the four-phase pass-2 flow degenerates to when
no integrity changes ship — write a hive-bundle by hand or skip
mark-hive-migrated entirely.

The pre-pass-2 narrative is preserved in the `docs/DNA_MIGRATION_GUIDE.md`
history (see `git log --follow -- docs/DNA_MIGRATION_GUIDE.md` from a
local checkout) if you need it.

---

## Pass-2 readiness checklist

Before broadcasting pass-2 to users, verify the migration scaffold
works end-to-end against a representative data shape:

- [x] Build pass-1's coordinator with the migration script in tree
      (shipped on `feat-optional-recipient-id`).
- [x] Build pass-2's coordinator + new integrity validators
      (shipped on `feat-integrity-pass-2`).
- [x] Build pass-2.5's coordinator-only extension (V2 markers + script
      hive-identity track + this doc).
- [ ] Smoke-test the full four-phase flow against a small synthetic
      source chain (5–10 entries across 2–3 hives, with at least one
      member per hive).
- [ ] Run the full migration against a realistic dev dataset. Confirm
      `remap.json` is empty-failures and the new DNA's queries return
      the migrated content.
- [ ] Document the cutover window protocol for cross-agent SS pairs
      in the user-facing release notes.

Only after all five pass before broadcasting.
