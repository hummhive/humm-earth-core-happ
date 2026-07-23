# MISC security follow-up — DM live push, DHT cleanup, and SharedSecret admission

- **Status:** OPEN — documentation/reproduction task only. Do not change either repo until the proof gates below pass.
- **Scope:** pass-7 scratch M21 plus humm-tauri's cross-hive DM and pair-SharedSecret paths.
- **Release posture:** pass-7 stays parked and undistributed. M21 is not blessing-ready until this task is closed or M21 is reverted.
- **Owner contract:** an online peer-to-peer DM should arrive through the live push path without waiting for a DHT read. DHT/Inbox delivery is recovery for offline peers and networking gaps. Recovery artifacts should be consumed/tombstoned after pickup so ordinary future enumeration exposes no more contact residue than necessary.

## Decision recorded 2026-07-23

1. Stop the speculative cross-repo code changes.
2. Do not reject full `EncryptedContentSignal` payloads in `recv_remote_signal` now.
3. Do not ask humm-tauri to adopt fetch hints until an end-to-end comparison proves equal delivery, less leakage, no added DHT residue, and a concrete security improvement.
4. Keep the narrow client-side `author ∈ canonical pair parties` check as a candidate, not an approved change. First reproduce the claimed poisoning path against the active runtime code.
5. Before pass-7 blessing, either:
   - prove and land a coordinated transport change that passes every gate below; or
   - revert M21's hint-only remote content fan-out and preserve the established live push path.

## Why this is parked

The Wave-4 code review expanded from earth-core batch/DRY verification into an unrequested humm-tauri transport redesign. The source walk established enough risk to stop, not enough evidence to change the protocol:

- Pass-7 M21 sends `EncryptedContentHint { action_type, hash, original_hash }` for honest cross-host content fan-out while retaining full local author signals.
- The active humm-tauri signal ingest expects `signal.data.encrypted_content`. It can render a full signal provisionally before asynchronous DHT confirmation.
- The current humm-tauri guard accepts any object with `action_type`, so it misclassifies a hint as a full signal and then finds no `data` record. Create/update, delete, SharedSecret, and first-contact live handling are not migrated.
- A hint fetch makes the online receiver wait for DHT availability. Signal delivery may precede DHT integration, so this also requires bounded retry, deduplication, and late-gossip handling.
- `recv_remote_signal` still accepts the old full shape. Keeping honest hint fan-out while accepting attacker-supplied full bodies pays the latency/DHT cost without completing the proposed trust-boundary change.
- A source walk found automatic Inbox-link consumption, but did not prove automatic tombstoning of each fetched DM, first-contact request, pair SharedSecret, key binding, and sender-authored discovery link. Duplicate/current and older client paths coexist; only a runtime trace can settle which lifecycle is active.

## Current anchors to re-ground before work

### humm-earth-core-happ

- `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs`
  - `create_encrypted_content`
  - `emit_content_change`
  - `delete_encrypted_content`
- `dnas/humm_earth_core/zomes/coordinator/content/src/lib.rs`
  - `recv_remote_signal`
- `dnas/humm_earth_core/zomes/coordinator/content/src/inbox/crud.rs`
  - `send_to_inbox`
  - `consume_inbox`
- `crates/sweettest/tests/signal_hints.rs`

### humm-tauri

- `src/api/core/holochain/zomeSignals.ts`
  - full-signal guard and dispatch order
- `src/sidecars/direct-messages/state/dmIngestSupport.ts`
  - `encryptedRecordFromSignal`
- `src/sidecars/direct-messages/state/dmIngest.ts`
  - live provisional ingest and DHT reconciliation
- `src/sidecars/direct-messages/state/dmSweep.ts`
  - Inbox fetch, dispatch, and consume sequence
- `src/sidecars/direct-messages/state/dmSend.ts`
  - first-contact request and normal DM creation
- `src/api/content/sharedSecret/admitSharedSecretToCache.ts`
  - central cache admission
- `src/api/sidecarHost/pairSharedSecret.ts` or its current successor
  - canonical pair parsing and author membership

## Existing conductor coverage checked 2026-07-23

Both focused commands pass against humm-tauri's pinned pass-6 hApp:

```text
pnpm exec vitest run --project bdd <four focused files>        6/6 passed
pnpm exec vitest run --project bdd-swarm dm-offline-catchup   1/1 passed
```

| Test | Real topology | What it proves | Missing assertion |
|---|---|---|---|
| `dm-remote-signal-delivery` RS-1 | Alice/Bob cells in one conductor | Bob receives some App signal after the DM is DHT-readable | remote payload shape/body/hash; push before or without DHT |
| `dm-first-contact-handshake` DM-12 | Alice/Bob cells in one conductor | OpenWrite key binding is cross-agent readable | request, Accept/Block, pair key, held-message flush, cleanup |
| `reader-read-only-delete` DM case | Alice/Bob cells in one conductor | a DM recipient may call `delete_encrypted_content` | automatic delete, post-delete liveness, link cleanup, restart |
| `delete-cleanup` | Alice only | author self-delete removes Hive/Dynamic discovery for HiveGroup content | recipient-side DM/request/SharedSecret cleanup |
| `swarm/dm-offline-catchup` OC-1 | two separate conductor processes | after Bob restarts, DHT `list_by_author` finds all offline-authored hashes and replays no App signal | Inbox/sweep/decrypt path, consumption, delete, tombstone, cross-hive setup |

`DM-L4` in `docs/earth-core-handoff/HUMM_TAURI_DM_MESSAGING_INTEGRATION.md`
documents bilateral delete **authority** as an explicit real DHT tombstone. It does
not specify automatic deletion after pickup.

No existing conductor test proves that pickup automatically tombstones the DM,
request, pair SharedSecret, or key binding. The smallest missing proof is one
two-conductor swarm lifecycle test that runs the production Inbox/DM ingest path,
checks live queries before and after pickup, restarts the recipient, and checks
again.

## Claims that require proof

### C1. Live push behavior

Prove which payload reaches humm-tauri for an online established-pair DM and whether the first render requires `get_encrypted_content`.

Expected owner contract:

- full encrypted body arrives by direct remote signal;
- receiver renders/decrypts without waiting for DHT integration;
- DHT confirmation may follow asynchronously;
- Inbox remains recovery, not the live critical path.

### C2. Cleanup behavior

For each artifact, identify the exact creator, consumer, delete author, trigger, and surviving live indexes:

| Artifact | Required observation |
|---|---|
| first-contact request | lifecycle after Accept and after Block |
| request Inbox link | `CreateLink` and `DeleteLink` behavior |
| key binding | retention and discovery path |
| pair SharedSecret | retention, rotation, and dynamic/author links |
| actual DM | recipient pickup, confirmation, and tombstone timing |
| DM Inbox link | consumption after successful dispatch only |
| discovery links | whether sender-authored links remain live after recipient deletion |
| private probe cursor | source-chain-only behavior |

Distinguish these three statements:

1. ordinary `get`/`get_links` no longer enumerates a dead item;
2. historical metadata remains addressable when a basis/hash is known;
3. physical bytes have been erased from every authority.

Only statement 1 is a realistic tombstone acceptance criterion. Do not claim 2 or 3 from a Holochain `Delete` action.

### C3. Pair-SharedSecret poisoning

Use Alice, Bob, and Mallory. Prove or falsify all of the following against the active cache-admission path:

- Mallory can author a syntactically valid `Alice ↔ Bob` pair wrapper.
- Alice or Bob can decrypt that wrapper.
- conductor-stamped signal provenance and the record header still allow it through.
- it replaces or wins the cache slot for the canonical pair.
- a later Alice/Bob DM uses Mallory's known key.
- the behavior is not limited to a narrow first-contact race.

If any existing author, expected-author, action-hash, ACL, or DHT-authority check blocks the attempt, record the blocker and close the proposed author-in-pair change as unnecessary unless another bypass is demonstrated.

## Required experiment matrix

Run the matrix with two fresh conductors for delivery tests and a third for the attack test. Capture zome calls, app-signal payload shape, timestamps, live links, entry liveness, and cache decisions.

| Case | Sender/recipient state | Required assertions |
|---|---|---|
| E1 | established pair; both online | one live delivery; no Inbox duplicate; whether a DHT read precedes render |
| E2 | first contact; both online | request, Accept/Block, key-binding wake-up, held-message flush |
| E3 | established pair; recipient offline | Inbox recovery, retry, deduplication, and cleanup |
| E4 | first contact; recipient offline | request recovery, acceptance wake-up, held-message flush, cleanup |
| E5 | signal before DHT integration | bounded retry; no silent drop; no unbounded polling |
| E6 | delete while peer offline | durable delete notification and eventual local removal |
| E7 | Alice/Bob established; Mallory online | forged pair-SharedSecret reproduction or definitive rejection |
| E8 | restart after each lifecycle | no lost pending request; no resurrected dead DM; no stale poisoned cache |

Record baseline and candidate results in one table. Source inspection alone does not pass this gate.

## Candidate A — client-only author-in-pair admission

Proposed invariant:

```text
pairGroupIdParties(groupId) = [A, B]
record.header.revisionAuthorSigningPublicKey ∈ {A, B}
```

Placement: the central SharedSecret cache-admission function, so signal, DHT, author-scan, and locally created candidates follow one rule.

Required regression cases:

- A-authored and B-authored pair records accepted;
- third-party author rejected;
- party order does not matter;
- malformed pair ID follows the existing non-pair policy;
- ordinary hive/group SharedSecrets are unchanged;
- first contact across unrelated hives still works online and offline.

**Rough size:** earth-core `0`; humm-tauri production `15–30` changed lines in one file, tests `45–90` lines in one or two files, docs `5–15` lines. Expected total: roughly `65–135` humm-tauri lines.

Do not land this merely because it is small. E7 must first show a real missing invariant or a test that fails on current code.

## Candidate B — hint-only remote content transport

A complete candidate includes all of the following; M21 alone is incomplete:

- disjoint full-signal and hint guards;
- fetch of create/update records by action hash;
- fetched action/header author checked against conductor-stamped provenance;
- bounded retry when a signal outruns DHT integration;
- SharedSecret, first-contact request, key-binding, and DM dispatch parity;
- delete handling without fetching a dead target;
- signal/Inbox deduplication;
- offline recovery unchanged;
- rejection of remote full content only after every supported client uses the new path.

**Rough size:**

| Repository | Production | Tests | Docs |
|---|---:|---:|---:|
| humm-earth-core-happ | M21 footprint about `40–70` | `160–280` | `25–60` |
| humm-tauri | `90–180` | `140–280` | `10–30` |

A later core-side rejection of remote full content adds roughly `10–25` production lines, `50–110` test lines, and `15–35` doc lines. It cannot land independently of the client migration.

## Proof bar for any transport replacement

All conditions are mandatory:

### Delivery parity

- Online established-pair and first-contact messages arrive with no extra user-visible wait.
- No additional DHT read is added to the online critical path unless the owner explicitly changes that product contract.
- Offline delivery, retries, deletes, deduplication, and restart recovery are at least as reliable as baseline.
- No new polling loop, unbounded queue, or timing-dependent acceptance window.

### Leakage and residue

- Remote signal reveals fewer bytes than baseline.
- DHT creates, links, tags, tombstones, and live enumeration routes are counted before and after.
- The candidate leaves no additional live entry/link and does not lengthen the exposure window.
- Pair/contact labels are not moved to another public basis and called a privacy improvement.
- Request, SharedSecret, key-binding, DM, and Inbox artifacts are assessed separately.

### Security

- E7 succeeds against baseline and fails against the candidate, or the task closes as not reproduced.
- Signal provenance, DHT action author, header author, pair parties, ACL readers, and requested action hash are bound at one documented admission point.
- A forged full payload cannot bypass the candidate through `recv_remote_signal`.
- A forged hint cannot select a different action or author.
- Confidentiality, availability, replay, cache replacement, and offline-recovery effects are each tested.

### Go/no-go

- **GO:** every delivery, leakage/residue, and security condition passes with captured evidence.
- **NO-GO:** any live-path regression, extra residue, longer DHT exposure, unreproduced threat, or hybrid full/hint bypass. Revert M21's remote hint cutover and retain only independently proved fixes.

## Explicit non-goals

- No broad humm-tauri security audit under this earth-core batch/DRY work.
- No integrity-zome change for a client payload convention.
- No claim that Holochain tombstones physically erase historical bytes.
- No legacy-client compatibility work; there are no deployed users. The concern is current developer-client correctness and the intended protocol.
- No code until the experiment matrix and comparison report exist.

## Definition of done

- [ ] Active runtime call paths identified; duplicate/dead paths excluded.
- [ ] E1–E8 results captured for baseline.
- [ ] Pair-SharedSecret attack reproduced or falsified.
- [ ] Per-artifact DHT/link residue table completed.
- [ ] M21 candidate compared against baseline.
- [ ] Owner chooses GO or NO-GO from evidence.
- [ ] If NO-GO, M21 remote hint fan-out is reverted before pass-7 blessing.
- [ ] If GO, coordinated earth-core + humm-tauri changes land atomically and pass both repos' full gates.
