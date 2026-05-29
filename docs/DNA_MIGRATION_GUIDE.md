# DNA Migration Guide

When the integrity zome ships its first non-additive change (pass 2 of
the coordinator/integrity work — see `HUMM_TAURI_COORDINATOR_INTEGRATION.md`
"second pass scope"), the DNA hash changes. A different DNA hash means a
fresh DHT, a fresh conductor source chain per cell, and fresh agent
pubkeys. Existing users keep their data ONLY if it is migrated forward.

This guide and the companion `scripts/migrate-dna.ts` are the scaffold
that makes that forward migration possible, **plus** a forward-pointer
marker mechanism (coordinator-only, no DNA-hash impact) that lets
old-DNA clients detect "this data has moved" and prompt the user to
upgrade — so a partially-migrated user base degrades gracefully instead
of silently losing access.

> **Pre-launch caveat:** with no production users today, the migration
> path is **optional and forward-looking infrastructure**. Ship it now so
> we don't scramble for it when there are users to protect.

---

## When you need this

You need to run the migration script (or wire its logic into the
humm-tauri install/upgrade flow) whenever the hApp's integrity zome
changes — concretely, whenever `hc dna hash` produces a value different
from the previous build's. After pass 1 of this work the hash is
`uhC0kT0Tkc3b6ccfa75YwWdzpSWvdkXERpdqkxIndRhfK5TJAUusY`; pass 2's
integrity validator additions (I-A / I-B / I-C / I-D) all bump it.

You do NOT need this for coordinator-only changes — those hot-swap in
place (see the deploy section of the handoff guide).

## What gets migrated

`EncryptedContent` entries — for every `header.id` on the local source
chain, the **latest live version** of that entry is migrated:

- The latest `Create`-or-`Update` content for that id is the payload
  re-published on the new DNA. User edits are preserved; the original
  Create's content does NOT get republished if it was later updated.
- If the entry was deleted at any point on the chain (an `alive` flag
  monotonically falls to `false` on the first `Delete` of any of its
  Create-or-Update actions and never re-rises), the entire id is
  **excluded** from the bundle. User deletions are honored — deleted
  entries do NOT resurrect on the new DNA, even if a later action
  re-creates the same id.
- The bundle's `old_action_hash` field for each entry is the ORIGINAL
  Create's action hash, regardless of how many Updates followed. This
  matches the host's persisted references (humm-tauri keys by the
  Create hash at first ingest), so the remap file's keys line up
  cleanly.

## What gets dropped

| Dropped | Why |
|---|---|
| Action hashes | New DNA → new action hashes. The script outputs an old→new remap. |
| Signatures | Old signatures are invalid against the new DNA hash. |
| Intermediate Update versions | Only the latest live content per `id` is preserved; intermediate edits are lost. If you need full history, snapshot separately before migration. |
| Deleted entries | Excluded from the bundle so the new DNA does not silently undo user deletions. |
| `Dynamic` links | These are link entries derived from the original create's `dynamic_links` arg, not part of the entry payload. The host (humm-tauri) must re-stamp them via `dynamic_links` on each re-import, using app-level state to determine the right group context. |
| Agent pubkey continuity | A fresh hApp install generates a fresh lair key. The migration restamps `revision_author_signing_public_key` to the new agent so the integrity author-match validator (`check_author_matches_header`) passes. The encrypted body itself is opaque to the DNA and decrypts the same with the unchanged Tauri keyring. |
| Read receipts, signal-only ephemeral state | Never persisted to the DHT — nothing to migrate. |

## Migration markers — forward pointers on the OLD chain

After the data migration is done (export from old, import to new), the
script's third phase calls the OLD hApp's `mark_migrated` coordinator
extern for each successfully-imported entry. This writes a marker
update onto the OLD entry whose:

- `content_type` is set to `_migrated/<original_content_type>` so
  old-DNA queries can cheap-filter by prefix.
- `bytes` carries a `MigrationMarkerV1 { schema_tag, schema_version,
  new_dna_hash_base64, new_action_hash_base64, new_app_id, migrated_at_microseconds }`
  msgpack-serialized payload.

An old-DNA client that calls `get_migration_marker(old_action_hash)`
then sees the marker and can show a friendly "your data has moved,
please upgrade humm-tauri to continue" prompt (or, with explicit user
approval, auto-redirect to the new hApp).

The OLD hApp ALSO fans out an `EncryptedContentSignal{Update}` to every
agent in `public_key_acl.reader` (this is the existing
`remote_signal_acl_readers` behavior, NOT a new code path). Recipients
that are online when the marker is written get a real-time "this
contact migrated" signal with `from_agent` stamped by the C7b
dispatcher — useful for prompting the recipient to upgrade their own
humm-tauri install before they lose visibility into the thread.

### Security model (LOAD-BEARING — read this)

The integrity update validator only enforces the self-referential
`action.author == header.revision_author_signing_public_key`. It does
NOT enforce that an update's author equals the original entry's author.
Without further defense, any old-DNA peer could write a forged marker
on someone else's entry by calling `update_encrypted_content` with the
victim's original AH and their own pubkey in
`revision_author_signing_public_key`.

The **coordinator reader** `get_migration_marker` closes this gap by:
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
MUST re-query authoritatively via `get_migration_marker`.

This means humm-tauri MUST always pass the **original Create's action
hash** to `get_migration_marker`, not an arbitrary update hash. The
script's remap file preserves this invariant by storing the original
Create hash in `old_action_hash`.

**Mandatory host-side defenses** (the coordinator can only do so much):

- **(A) Author binding**: the coordinator reader enforces this — but
  humm-tauri SHOULD ALSO cross-check the marker arrived via the C1
  `from_agent` path from the trusted partner identity before treating
  it as authoritative.
- **(B) User consent before DNA crossover**: NEVER auto-follow the
  marker's `new_dna_hash_base64` / `new_app_id` without explicit human
  approval. Switching DNA crosses a trust boundary and must be a user
  decision — phishing via fake migration prompts is otherwise
  trivial.
- **(C) Cross-verify on the new DNA**: before redirecting UI to the
  new AH, confirm `get_encrypted_content(new_action_hash)` actually
  resolves on the new DNA. Catches both attacker-forged markers AND
  the legitimate uninstall/reinstall staleness case (the new AH from
  a previous install is stale after reinstall). Defense (C) is
  strongest when the marker carries a valid `new_dna_hash_base64`
  (resolvable to a specific DNA instance); if the migration script
  was run with `NEW_DNA_HASH_BASE64` unset, the marker carries an
  empty string and the host can only validate via `new_app_id`,
  weakening (C) to app-id-only resolution. Always pass
  `NEW_DNA_HASH_BASE64` for production migrations.

## humm-tauri GUI integration (recommended UX)

The standalone TS script is the reference. For the user-facing flow,
wire the same logic into humm-tauri so migration is GUI-driven:

1. **Update detection** — humm-tauri's auto-update channel detects a
   new `.happ` bundle (different DNA hash from the currently-installed
   one). Computes the new DNA hash via `hc dna hash` (or by parsing
   the bundle manifest) and compares.
2. **User prompt** — show a dialog: "humm-tauri has shipped a security
   update that requires migrating your data. This takes about N
   seconds and is reversible until you confirm. Migrate now?"
3. **Install new hApp** — install the new `.happ` under a distinct
   `installed_app_id` (e.g. `humm-earth-core@<short-hash>`). Both
   old and new are now running.
4. **Export-then-import** — run the equivalent of `export` + `import`
   phases of the script in the background (use
   `holochain_client_rust` from the Rust side, or invoke the TS script
   as a child process). Show a progress bar.
5. **Host-side rewrite** — walk the remap file in-process; update
   localStorage, IndexedDB, SS index, DmStore caches, every key that
   carries an old action hash.
6. **Write markers** — run `mark-migrated` phase. Recipients online at
   this moment get the cross-host "I migrated" signal in real time.
7. **Confirm cutover** — show "migration complete; old hApp will be
   disabled in 7 days unless you opt to keep it for cross-agent
   coordination". Default opt-in to keeping during the cutover window;
   the marker reader keeps working on the OLD hApp for SS partners
   who haven't migrated yet.
8. **Disable old hApp** — after the window closes (or on user
   confirmation), call `disableApp` on the old `installed_app_id`.
9. **Receiver side** — humm-tauri on a NOT-YET-migrated peer's
   conductor sees the incoming `_migrated/*` signal (from step 6
   above) and proactively prompts THEM to upgrade. Apply ALL of
   defenses A/B/C from the Security model above before treating the
   signal as authoritative: verify the signal's `from_agent`
   **equals** the trusted partner identity (defense A); require
   explicit user consent before any DNA crossover (defense B); and
   cross-verify the new AH on the new DNA (defense C). The signal
   payload is a HINT — re-query through the author-bound
   `get_migration_marker(old_ah)` for the authoritative marker, then
   confirm with the user before installing the new hApp.

This is the user-transparent path. The mandatory human-in-the-loop
steps are: (2) initial approval to migrate own data, and (B) explicit
approval before any DNA crossover. Everything else can be background.

## Stages

```
┌──────────────────────────────┐   ┌──────────────────────────────┐
│      OLD hApp (installed)    │   │      NEW hApp (installed)    │
│  app-id = humm-earth-core@1  │   │  app-id = humm-earth-core@2  │
│      DNA hash = uhC0k……      │   │     DNA hash = uhC0k… (≠1)   │
└──────────────────────────────┘   └──────────────────────────────┘
                │                                  ▲
                │  ① export                        │  ③ import
                ▼                                  │
            bundle.json   ───────────────────────  │
                                                   │
                                              remap.json
                                                   │
                                                   ▼
                                       host-side rewrite
                                  (humm-tauri reads remap.json,
                                   updates localStorage / SS index)
```

## Step-by-step

### 0. Prereqs

- Both old and new `.happ` bundles available.
- Conductor admin port reachable (default `4444`; override via `ADMIN_PORT`).
- Tauri keyring (or whatever holds your symmetric / Ed25519 keys) is
  **unchanged** across the install. The script does not touch keys.
- Node + `npx tsx`, plus `@holochain/client` and `@msgpack/msgpack`
  installed in the repo (already there via `tests/`).

### 1. Install the new hApp alongside the old one

Different app-id, same conductor. humm-tauri's install flow already
supports parallel installs (relays / dev environments do this); for the
ad-hoc case use `hc app install` with `--installed-app-id` set to
something distinct.

The two hApps must both be running and have App websocket interfaces
attached. The script issues its own short-lived auth tokens via
`issueAppAuthenticationToken`.

### 2. Export from the old hApp

```bash
ADMIN_PORT=4444 npx tsx scripts/migrate-dna.ts \
  export humm-earth-core@1 /tmp/migrate/bundle.json
```

The bundle is a JSON document containing every `EncryptedContent` entry
on the local source chain (latest version per `id`, with encrypted
bytes base64-encoded for JSON-stability). Size is ~1.5× the encrypted
body bytes due to JSON + base64 overhead.

Inspect it; back it up. The bundle is the source of truth for the
import step and can be regenerated as long as the old hApp installation
exists.

### 3. Import into the new hApp

```bash
ADMIN_PORT=4444 npx tsx scripts/migrate-dna.ts \
  import humm-earth-core@2 /tmp/migrate/bundle.json /tmp/migrate/remap.json
```

For each entry: calls `create_encrypted_content` on the new hApp,
restamps `revision_author_signing_public_key` to the new agent's
pubkey, records the old→new action hash in `remap.json`.

`remap.json` shape (schema_version 1):

```json
{
  "schema_version": 1,
  "source_app_id": "humm-earth-core@1",
  "source_agent_pubkey_base64": "uhCAk...",
  "target_app_id": "humm-earth-core@2",
  "target_agent_pubkey_base64": "uhCAk...",
  "imported_at_iso": "2026-05-29T15:30:00.000Z",
  "entries": [
    {
      "id": "msg-abc123",
      "old_action_hash": "uhCkk...",
      "new_action_hash": "uhCkk...",
      "content_type": "dm",
      "hive_id": "hive-xyz"
    }
  ],
  "failures": []
}
```

### 4. Host-side rewrite (humm-tauri's responsibility)

Walk the remap and rewrite every persisted reference that carries an
old action hash:

- localStorage / IndexedDB keys keyed by action hash → swap to new AH.
- Direct-message store thread anchors that reference message AHs.
- Shared-secret lookup tables (the actual SS payload is in the
  migrated `bytes` and is unchanged after decryption, but the index by
  AH is stale).
- Any sidecar cache (`DmStore`, `HummContentStore`) that keys by AH.

Keys that survive unchanged (the `id` field, the `hive_id`, the
`content_type`) keep working — that's the migration's stability
contract.

### 5. Disable the old hApp

Once the rewrite is verified — content visible in the new app, no
broken references — disable the old app-id via the admin API
(`disableApp`). Keep its data on disk until you're confident no recovery
is needed.

## Cross-agent coordination (the hard case)

A shared secret between Alice and Bob lives in BOTH their source chains
(each created their own copy under their own pubkey when the pair was
established). Both must migrate before the shared SS works in the new
DNA. If only Alice migrates:

- She can still decrypt her own SS copy locally.
- Calls to `fetch_pair_ss_with_hive_check` on the new DNA return only
  her own entries (Bob hasn't seeded the new DHT yet) — looks like "no
  pair-SS exists" to her UI.
- Bob's old conductor keeps working under the old DNA in isolation.

Sequence migrations explicitly: announce a cutover window, both peers
migrate, both confirm visibility, then disable old hApp.

## Idempotency and re-runs

- **Export is fully idempotent.** Re-running `export` against the same
  source chain produces the same bundle (modulo the `exported_at_iso`
  timestamp).
- **Import is NOT idempotent at the action-hash level.** Re-running
  `import` against the new hApp creates a SECOND set of fresh action
  hashes for the same `id` values. If you re-import, dedupe by `id` on
  the host side, or delete the prior import first via the admin API
  (uninstall+reinstall the new app-id).
- For partial failures: the `remap.json` has a `failures` array with
  the IDs that didn't import. Address the root cause (most likely an
  integrity validator rejecting a payload) and re-run; the second
  attempt will create new action hashes for the still-failing entries
  rather than overwriting the successful ones.

## Failure modes worth pre-empting

| Symptom | Cause | Fix |
|---|---|---|
| `App "X" not found on conductor` | Old hApp uninstalled before export | Reinstall old `.happ`, re-export, then proceed |
| `check_author_matches_header` rejection on import | Importer not restamping the pubkey (manual replay outside the script) | Use the script — it restamps; or restamp `revision_author_signing_public_key = encodeHashToBase64(new_agent_pubkey)` manually |
| `create_encrypted_content` errors with link-validator failure (pass 2+) | The new DNA's I-D integrity validator rejects a hive/dynamic link the host requested | Host (humm-tauri) needs to provide correct dynamic_links / hive_id matching the new validator's constraints — surfaced in the `failures` array |
| Encrypted body decrypts to garbage after migration | Tauri keyring changed between old and new install | Restore the old keyring file; never delete it during a migration window |
| `fetch_pair_ss_with_hive_check` returns `[]` after migration | Counterparty has not migrated yet | Wait for the counterparty; this is expected during the cutover window |

## Wire-script vs in-app integration

The standalone TS script is the **reference implementation**. For
production use, humm-tauri's auto-update flow should integrate the
same logic in Rust (via the `holochain_client_rust` crate) so the
migration runs transparently when the user installs an integrity-
breaking version. The script's bundle / remap JSON schemas are versioned
(`schema_version: 1`) so the in-app implementation and the standalone
script stay interoperable.

## Pass-2 readiness checklist

Before shipping pass 2's integrity changes (I-A / I-B / I-C / I-D),
verify the migration scaffold works end-to-end against a representative
data shape:

- [ ] Build pass 1's coordinator with the migration script in tree
      (this is the current state).
- [ ] Smoke-test export+import against a small synthetic source chain
      (5–10 entries across a couple of `hive_id`s).
- [ ] Build pass 2's coordinator + new integrity validators on a
      branch.
- [ ] Run the full migration against a realistic dev dataset. Confirm
      `remap.json` is empty-failures and the new DNA's queries return
      the migrated content.
- [ ] Document the cutover window protocol for cross-agent SS pairs
      in the user-facing release notes.

Only after all four pass before tagging pass 2 as ready to deploy.
