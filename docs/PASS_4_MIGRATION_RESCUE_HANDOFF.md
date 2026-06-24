# pass-4 migration-rescue — Fix Handoff

**Date:** 2026-06-23
**Audience:** humm-tauri developers (responding to the
`2026-06-23T19-45-52-pass5-cutover-go-build-rescue-coordinator-and-tests` mbox GO)
**DNA version:** pass-4 (`uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`) — **UNCHANGED**
**Change class:** coordinator-only hot-swap (no chain fork)
**Branch:** `feat-coordinator-pass4-migration-rescue` (off `main` HEAD `e60ed48`)

---

## TL;DR

The post-cutover pass-4 `@4` cell goes peerless ("dormant") as every co-member
migrates to `@5`. Its enumeration externs — `list_my_hives`,
`get_latest_membership` — issue `GetStrategy::Network` reads with no responding
authorities and silently return `[]`/`None` even though the migrating user's
data is locally present. Measured on a live dormant cell:
`list_my_hives → []`, `get_messages_since → 468 local records`, `peer_count → 0`.

The rescue coordinator adds **dormancy-proof local-read twins** of the two
enumeration externs and softens `mark_migrated_v2` so an unresolvable original
entry is a skip (`Ok(None)`) rather than a hard error. No new entry types, no
link-type changes, no integrity edits — the DNA hash is byte-identical to
pass-4.

**Use this happ:** `pass-4-migration-rescue` — DNA `uhC0k26b…` (same as pass-4),
artefact + hashes in [Artifact and hashes](#artifact-and-hashes) below.

Install path: `AdminWebsocket.updateCoordinators` on the live `@4` cell. No
re-install, no migration, existing chains keep working. The local
migration-recovery UI calls the two new read externs through the
AppWebsocket (which is cap-exempt for the locally-installed app), so no
cap-grant refresh is required for the rescue to function.

---

## What changed (3 things)

1. **`list_my_hives_local()` — new extern** in `hive/queries.rs`. Dormancy-proof
   twin of `list_my_hives`. Founder branch reads the caller's source chain via
   `query(ChainQueryFilter)` (NO network); joiner branch walks the caller's
   local Inbox link store via `get_links(LinkQuery, GetStrategy::Local)` +
   `get(target, GetOptions::local())`. Both branches discriminate
   `HiveGenesis` by `EntryType::App` entry index (not msgpack shape) so a
   `GroupGenesis` entry — whose fields are a strict superset of
   `HiveGenesis`'s — is NOT false-positively surfaced as a hive;
   `list_my_hives` carries the same filter. Returns the same
   `Vec<ListedHive>` shape as `list_my_hives`.
2. **`get_latest_membership_local(GetLatestMembershipInput) -> Option<HiveMembershipResponse>` — new extern**
   in `hive/queries.rs`. Body-identical to `get_latest_membership` except both
   reads use `GetStrategy::Local` / `GetOptions::local()`. Drop-in for the
   non-owner content-stamping path.
3. **`mark_migrated_v2` — return type WIRE CHANGE** in
   `encrypted_content/migration.rs`. Now
   `ExternResult<Option<EncryptedContentResponse>>` (was
   `ExternResult<EncryptedContentResponse>`). On an unresolvable
   `original_action_hash` it logs `warn!` and returns `Ok(None)` (skip the
   courtesy marker — irrelevant on a dormant cell); on success returns
   `Ok(Some(resp))`.

Both new reads are `Unrestricted` cap-granted (same surface as their network
twins — neither widens read access; both filter to the caller's own data).
`mark_migrated_v2` stays UN-granted (Rule 1 — source-chain mutator).

`list_my_hives` and `get_latest_membership` (the network variants) are
**unchanged** and remain the correct call when the cell has peers.

---

## Wire shapes (publish to humm-tauri)

```ts
// NEW — local-read twins

list_my_hives_local(): Promise<ListedHive[]>;
// Same shape as list_my_hives:
type ListedHive = {
  hive_genesis_hash: ActionHash;   // Uint8Array (multibase decoded)
  display_id: string;
  role: HiveRole | null;            // null = founder (implicit owner)
};
type HiveRole = 'Owner' | 'Admin' | 'Writer' | 'Reader';

get_latest_membership_local(input: GetLatestMembershipInput):
  Promise<HiveMembershipResponse | null>;
// Same input + return as get_latest_membership:
type GetLatestMembershipInput = {
  agent: AgentPubKey;
  hive_genesis_hash: ActionHash;
};
type HiveMembershipResponse = {
  membership: HiveMembership;
  hash: ActionHash;
};

// CHANGED — was EncryptedContentResponse, now Option<EncryptedContentResponse>
mark_migrated_v2(input: MarkMigratedV2Input):
  Promise<EncryptedContentResponse | null>;
// null = original entry unresolvable on this cell (dormant / absent);
// callers MUST treat this as "skip the marker write and keep going",
// NOT as a failure. The skip is auditable: a warn! line lands in the
// conductor log carrying the original_action_hash + the underlying
// host error.
```

In-tree TypeScript callers of `mark_migrated_v2` (`scripts/migrate-dna.ts:965`
+ `scripts/migrate-dna.ts:1354`) `await appWebsocket.callZome(...)` and
discard the response — the wire-shape change is safe through that surface.
If anything in humm-tauri unwraps the response, it must accept `null`.

---

## When to use the local twins

Call the `*_local` twins **only when the cell is dormant / peerless**. On a
healthy cell with live peers, prefer the network variants (`list_my_hives`,
`get_latest_membership`) — they still return the same data plus any data the
host has not yet integrated locally.

The recommended pattern for the post-cutover migration recovery screen on the
`@4` cell:

```ts
// 1. Try the network read first.
const networkHives = await callZome('list_my_hives');
// 2. If empty, the cell may be dormant. Fall back to the local twin.
const hives = networkHives.length > 0
  ? networkHives
  : await callZome('list_my_hives_local');
```

The joiner branch of `list_my_hives_local` is **best-effort**: it surfaces a
joined hive only if its Inbox link + `HiveMembership` entry integrated locally
**before** the cell went dormant. A grant that never reached the joiner's
local store is invisible — there is no peer to ask for it. Document this in
the UI: "If you joined hives close to the cutover and never came online with
peers afterward, those grants may not appear; founder hives always appear."
This is a deliberate dormancy-rescue limitation, not a bug — there is no way
to recover data that never reached the local cell.

---

## How to install (humm-tauri side)

`AdminWebsocket.updateCoordinators` against the live `@4` cell:

```ts
const newCoordinators = await readFile(rescueHappPath);
await adminWebsocket.updateCoordinators({
  cell_id: pass4CellId,
  source: { Bundle: newCoordinators },
});
```

The conductor replaces the coordinator wasm in-place on the live `@4` cell;
the chain and existing cap grants are untouched.

**Cap-grant note** (relevant only if you intend a REMOTE peer to call
these externs, which the rescue does not): Holochain's `init` is gated by
`InitZomesComplete` and does NOT auto-re-run after `updateCoordinators` on
an already-initialised cell, so the two new `Unrestricted` grants do NOT
materialize on the live `@4` cell via hot-swap alone. The local UI is
unaffected — it reaches every coordinator extern through the AppWebsocket,
which bypasses cap grants. If you ever need a remote cap-grant refresh on
a hot-swapped cell, ship an explicit `set_cap_tokens()` re-call extern;
the rescue does not need it.

---

## Testing posture

- **Founder local enumeration** (`founder_lists_own_hives_via_local_path`):
  deterministic anchor on a single Sweettest conductor — founds two hives,
  asserts `list_my_hives_local` returns both with `role: null`.
- **`mark_migrated_v2` fail-soft**
  (`mark_migrated_v2_returns_none_on_unresolvable_original`): deterministic
  on a single Sweettest conductor — calls with an unresolvable
  `original_action_hash` (all-`0xdb` fixture), asserts `Ok(None)`.
- **Joiner local enumeration** (`joiner_local_lists_granted_membership`):
  2-conductor rendezvous — Alice founds a hive + grants Bob `Reader`,
  asserts Bob's `list_my_hives_local` returns the granted hive via
  `GetStrategy::Local`.
- **Network-vs-local differential**
  (`network_list_returns_empty_on_dormant_cell`): `#[ignore]`d with a
  documented reason. **Sweettest cannot reproduce live-iroh `@4` dormancy**:
  the in-process conductor is its own authority for its own basis, so the
  Network strategy resolves the agent's own data in-process too. The live
  `@4` dormancy regression is **e2e-only** and lives in humm-tauri's
  tryorama suite. For your tryorama mirror to actually differentiate
  Network vs Local, you'll need a deliberate peer-isolation step (drop the
  conductor's network bindings, or migrate every peer to `@5` and then
  re-call the externs on a still-alive `@4` cell).

A cross-member migration-marker enumeration on a dormant `@4` is a documented
**known limitation** of this rescue, not a tested case — without a peer
authority for the OTHER agent's pubkey basis, the marker is not enumerable.
Recovery for cross-member content on a dormant cell relies on the
`@5`-side migration import, not on `@4` reads.

---

## Artifact and hashes

```
File:             humm-earth-core-happ_pass-4-migration-rescue_dna-uhC0k26b_happ-ca1b4225.happ
DNA hash:         uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV
integrity_sha256: 06b01fb3527e266a5cb1b5ffbf01b83541d7a572c4b4a252521154c3e0c2cd83
content_sha256:   89444059a7e7cfcd4c85ef81925a8d6c52865ade1e647980fe43869b3a841d3a
happ_sha256:      ca1b422506c489f21cb32618951df304dc9c320596a4ddb6317d5f5ed8a2cbcf
size:             943375 bytes
```

(See `.baseline-hashes.txt` "pass-4 coordinator follow-up — migration-rescue"
for the authoritative copy and the reproducible-build invocation.
The prior `e28fc9ae` build for this same branch is SUPERSEDED — it
silently false-positived every `GroupGenesis` entry as a hive; the
filter fix is in this `ca1b4225` build and the bad happ must not be
redistributed.)

Available in `~/hummhive-official-happ-versions/` (the rescue is NOT mirrored
into `humm-tauri/.testdata/happs/` from this side — pull it from the
official-versions tree). Verify locally with:

```bash
cd ~/humm-earth-core-happ
nix develop --command bash -c '
  npm run build:zomes \
    && hc dna pack dnas/humm_earth_core/workdir \
    && hc app pack workdir --recursive \
    && hc dna hash dnas/humm_earth_core/workdir/humm_earth_core.dna
'
# MUST print: uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV
sha256sum target/wasm32-unknown-unknown/release/content_integrity.wasm
# MUST match the integrity_sha256 above (held byte-identical to pass-4).
```
