# Pass-4 deploy handoff — humm-tauri integration

Short-form handoff for the humm-tauri team to integrate the pass-4
integrity-zome changes shipped on
`feat-integrity-pass-4-recipient-witnesses`.

Pass-4 **intentionally bumps the DNA hash** (the
`AclSpec::HiveGroup` variant gains a required field — non-additive
integrity change) and is the next intentional DNA bump after pass-3.
Existing pass-3 data MUST be migrated forward via the
`scripts/migrate-dna.ts` pipeline (the pass-3 classifier still
ships; the witness-populating HiveGroup branch arrives via the
separate Phase D.1 follow-up).

For the full pass-3 + pass-4 wire-shape reference, see
[`HUMM_TAURI_ACLSPEC_INTEGRATION.md`](./HUMM_TAURI_ACLSPEC_INTEGRATION.md);
for the rolling delta visible to devs polling the repo, see
[`HANDOFF_UPDATED_INFO.md`](./HANDOFF_UPDATED_INFO.md); for the
per-pass concrete-task mapping to humm-tauri files + features,
see [`HUMM_TAURI_PASS_ROADMAP.md`](./HUMM_TAURI_PASS_ROADMAP.md).

## TL;DR

- **DNA hash CHANGED** from
  `uhC0kwO11VbVMLrFlQBqeslvnZroeHUp5VetnH1tgX68lH5FebRgC` (pass-3) to
  **the new pass-4 hash recorded in `.baseline-hashes.txt` once
  Phase 4-F lands**. Coordinator hot-swap does NOT work for this
  pass; users see a new cell on install and require the migration
  flow to keep their data.
- **G-6.2 — Recipient-set integrity SHIPPED.** Every
  `AclSpec::HiveGroup` write MUST carry a
  `recipient_witnesses: Vec<RecipientWitness>` covering every pubkey
  in `public_key_acl.{owner,admin,writer,reader}` exactly once.
  Each witness names a real `GroupMembership` in a group present in
  the corresponding (or higher) bucket of `group_acl`, granting a
  role that satisfies the witness's claimed bucket, unexpired at
  the entry's `action.timestamp`. Closes attack #5 (a modified
  coordinator can no longer inject a foreign pubkey into the reader
  bucket of a private group post to receive remote-signal
  notifications).
- **G-4.4 back-ported to HiveMembership.** An expiring Path-2 hive
  grantor can no longer extend the delegation window or mint a
  permanent membership. Mirrors the pass-3 group-layer rule one
  level up.
- **Defense-in-depth** within G-6.2: cardinality bound
  (`HIVEGROUP_MAX_WITNESSES = 256`); duplicate-pubkey rejection;
  bidirectional PKA ↔ witnesses cross-check; bucket dominance
  preserves the pass-3 admin ⊆ writer ⊆ reader semantics.
- **All non-HiveGroup AclSpec variants UNCHANGED** —
  `DirectMessage`, `Public`, and `OpenWrite` continue to work exactly
  as pass-3. Cross-hive DMs, member-requests, hive-discovery, and
  public posts need no humm-tauri changes.

### Deploy (NOT transparent — DNA-hash bump, migration required)

Same shape as the pass-3 deploy. Pass-2/3 migration scaffold
(hive-identity track, V2 markers, AclSpec classifier) is preserved
and extended; the pass-4 wire-shape addition is handled invisibly
by the classifier for non-HiveGroup content. HiveGroup content
needs the D.1 group track + per-bundle
`classification-overrides.json` (separate branch) to migrate; until
D.1 lands, every legacy entry defaults to `AclSpec::Public`.

1. **Pre-publish** (this hApp repo, before bundling into humm-tauri):
   ```bash
   cd ~/humm-earth-core-happ
   RUSTFLAGS='--cfg getrandom_backend="custom"' \
     CARGO_TARGET_DIR=target \
     cargo build --release --target wasm32-unknown-unknown \
     -p content_integrity -p content
   hc dna pack dnas/humm_earth_core/workdir
   hc dna hash dnas/humm_earth_core/workdir/humm_earth_core.dna
   # MUST print the pass-4 hash recorded in .baseline-hashes.txt
   hc app pack workdir --recursive
   sha256sum workdir/humm-earth-core-happ.happ
   ```
2. **humm-tauri side** (after copying the new .happ into
   `src-tauri/bin/`): bump the `APP_ID` constant. Per the pass-1
   deploy handoff, the constant lives in
   `humm-tauri/src-tauri/src/holochain/install_humm_core_happ.rs`.
3. **User-facing migration** (humm-tauri-side UX work): identical to
   pass-3 — the migration script handles the pass-3 → pass-4
   wire-shape adjustment transparently for the four content-type
   classifications it knows about (DM / Public / OpenWrite). The
   classifier's HiveGroup branch still throws pre-D.1 (operators
   wanting HiveGroup classification wait for D.1).

## Wire-shape diff vs pass-3

**Only `AclSpec::HiveGroup` changes.** All other variants identical
to pass-3.

```ts
// pass-3 HiveGroup
type AclSpec =
  | { HiveGroup: {
        hive_genesis_hash: ActionHash;
        author_membership_hash: ActionHash | null;
        group_acl: AclByGroupGenesis;
        author_group_membership_hash: ActionHash | null;
      } }
  // ...

// pass-4 HiveGroup
type AclSpec =
  | { HiveGroup: {
        hive_genesis_hash: ActionHash;
        author_membership_hash: ActionHash | null;
        group_acl: AclByGroupGenesis;
        author_group_membership_hash: ActionHash | null;
        recipient_witnesses: RecipientWitness[]; // NEW (required)
      } }
  // ...

// NEW types
type AclBucket = 'Owner' | 'Admin' | 'Writer' | 'Reader';

type RecipientWitness = {
  pubkey: AgentPubKey;
  bucket: AclBucket;
  membership_hash: ActionHash;
};
```

`HIVEGROUP_MAX_WITNESSES = 256`. Per-witness validator cost: one
`must_get_valid_record` (the cited `GroupMembership`).

## REQUIRED humm-tauri callsite update — stampWitnessesFromGroupAcl helper

Centralise the witness-stamping logic in one helper humm-tauri
calls from every HiveGroup write site:

```ts
import { decodeHashFromBase64, encodeHashToBase64 } from '@holochain/client';
import type { ActionHash, AgentPubKey } from '@holochain/client';

type AclBucket = 'Owner' | 'Admin' | 'Writer' | 'Reader';

type RecipientWitness = {
  pubkey: AgentPubKey;
  bucket: AclBucket;
  membership_hash: ActionHash;
};

type AclByGroupGenesis = {
  owner: ActionHash;
  admin: ActionHash[];
  writer: ActionHash[];
  reader: ActionHash[];
};

type Acl = { owner: string; admin: string[]; writer: string[]; reader: string[] };

type GroupMembershipResponse = {
  membership: {
    group_genesis_hash: ActionHash;
    for_agent: AgentPubKey;
    role: 'Owner' | 'Admin' | 'Writer' | 'Reader';
    expiry: { secs: number; nanos: number } | null;
    // ...
  };
  hash: ActionHash;
};

/**
 * Walk every pubkey in `publicKeyAcl` and produce the
 * `recipient_witnesses` array required by pass-4 `AclSpec::HiveGroup`.
 *
 * For each pubkey + the highest bucket it appears in:
 *  - Find the first group_acl bucket (Owner → Admin → Writer → Reader)
 *    that dominates or equals the PKA bucket.
 *  - Call `get_latest_group_membership(agent, group_genesis_hash)`
 *    for each group in that bucket until one resolves.
 *  - Stamp { pubkey, bucket, membership_hash } using the resolved
 *    membership hash.
 *
 * Throws if any pubkey has no qualifying membership — humm-tauri
 * should surface this as a user-facing error ("this person is no
 * longer a member of the required group; remove them or re-add to
 * the group first") rather than committing a doomed entry.
 *
 * The validator's bucket-dominance rule means an Admin-bucket witness
 * covers Admin + Writer + Reader PKA buckets for the same pubkey;
 * this helper picks the highest-bucket fit so a single witness
 * suffices when the pubkey is listed across multiple PKA buckets.
 */
export async function stampWitnessesFromGroupAcl({
  callZome,           // your zome-call helper bound to the content zome
  groupAcl,           // AclByGroupGenesis (Uint8Array hashes)
  publicKeyAcl,       // Acl (string-form pubkeys)
}: {
  callZome: (fn: string, payload: unknown) => Promise<unknown>;
  groupAcl: AclByGroupGenesis;
  publicKeyAcl: Acl;
}): Promise<RecipientWitness[]> {
  // 1) Build pubkey → highest-bucket map. Owner > Admin > Writer > Reader.
  const highestBucket = new Map<string, AclBucket>();
  const recordIfHigher = (pubkeyStr: string, bucket: AclBucket) => {
    const current = highestBucket.get(pubkeyStr);
    if (!current || bucketRank(bucket) > bucketRank(current)) {
      highestBucket.set(pubkeyStr, bucket);
    }
  };
  if (publicKeyAcl.owner) recordIfHigher(publicKeyAcl.owner, 'Owner');
  for (const pk of publicKeyAcl.admin) recordIfHigher(pk, 'Admin');
  for (const pk of publicKeyAcl.writer) recordIfHigher(pk, 'Writer');
  for (const pk of publicKeyAcl.reader) recordIfHigher(pk, 'Reader');

  // 2) For each pubkey, find a backing membership in a dominating
  //    group_acl bucket.
  const witnesses: RecipientWitness[] = [];
  for (const [pubkeyStr, bucket] of highestBucket) {
    const agentPubKey = decodeHashFromBase64(pubkeyStr);
    const candidateGroups = groupsForBucket(groupAcl, bucket);
    let stamped = false;
    for (const groupHash of candidateGroups) {
      const response = (await callZome('get_latest_group_membership', {
        agent: agentPubKey,
        group_genesis_hash: groupHash,
      })) as GroupMembershipResponse | null;
      if (response) {
        witnesses.push({
          pubkey: agentPubKey,
          bucket,
          membership_hash: response.hash,
        });
        stamped = true;
        break;
      }
    }
    if (!stamped) {
      throw new Error(
        `No GroupMembership found for ${pubkeyStr} in any group_acl ` +
          `${bucket}-or-higher bucket. The pubkey cannot be stamped ` +
          `as a recipient witness; either remove them from ` +
          `public_key_acl.${bucket.toLowerCase()} or grant them a ` +
          `valid GroupMembership in the appropriate group.`,
      );
    }
  }
  return witnesses;
}

function bucketRank(bucket: AclBucket): number {
  return bucket === 'Owner' ? 4 : bucket === 'Admin' ? 3 : bucket === 'Writer' ? 2 : 1;
}

function groupsForBucket(groupAcl: AclByGroupGenesis, bucket: AclBucket): ActionHash[] {
  switch (bucket) {
    case 'Owner':  return [groupAcl.owner];
    case 'Admin':  return [groupAcl.owner, ...groupAcl.admin];
    case 'Writer': return [groupAcl.owner, ...groupAcl.admin, ...groupAcl.writer];
    case 'Reader': return [groupAcl.owner, ...groupAcl.admin, ...groupAcl.writer, ...groupAcl.reader];
  }
}
```

Wire it into every HiveGroup write site (Compose-with-group-scope,
Manage* modals, Invites, group SS provisioning, group-message sidecar)
**before** calling `create_encrypted_content`:

```ts
const recipient_witnesses = await stampWitnessesFromGroupAcl({
  callZome: (fn, payload) => appWebsocket.callZome({
    cell_id,
    zome_name: 'content',
    fn_name: fn,
    payload,
  }),
  groupAcl,
  publicKeyAcl: derrivedPublicKeyAcl,
});

await appWebsocket.callZome({
  cell_id,
  zome_name: 'content',
  fn_name: 'create_encrypted_content',
  payload: {
    id,
    display_hive_id,
    content_type,
    revision_author_signing_public_key,
    bytes,
    acl_spec: {
      HiveGroup: {
        hive_genesis_hash,
        author_membership_hash,
        group_acl,
        author_group_membership_hash,
        recipient_witnesses, // <-- NEW
      },
    },
    public_key_acl,
    dynamic_links,
  },
});
```

## Cross-hive smoke-test checklist (pass-4 additions)

Run BEFORE shipping pass-4 to users (in addition to the pass-3
checklist in `PASS_3_DEPLOY_HANDOFF.md`):

1. **G-6.2 recipient-witness happy path**: Alice (group Admin)
   writes a HiveGroup entry with bob + carol in `public_key_acl.reader`
   and the matching `recipient_witnesses`. Commit succeeds; both bob
   and carol see the entry via their `Inbox::HiveInvite` /
   per-author indices.

2. **G-6.2 modified-coordinator forges recipient list**: Alice's
   modified coordinator inserts mallory's pubkey into
   `public_key_acl.reader` without adding a witness. Commit
   REJECTED at validator with
   `public_key_acl.Reader entry … is not backed by any dominating
   recipient_witness`.

3. **G-6.2 over-claim**: A modified coordinator stamps a witness
   for mallory with `bucket = Reader` but never lists mallory in
   `public_key_acl.reader`. Commit REJECTED with
   `recipient_witness for … claims bucket Reader but pubkey is not
   in public_key_acl.Reader`.

4. **G-6.2 stale roster**: bob was a group member when the inviter
   stamped the witness, but bob's `GroupMembership` has since
   expired. The validator re-fetches the membership; expiry check
   fires; commit REJECTED with `membership … expired at …`.

5. **G-6.2 bucket dominance**: Alice holds a group-Admin
   `GroupMembership` and writes a HiveGroup entry where she stamps
   herself as a Writer-bucket witness. Commit succeeds because
   Admin dominates Writer. (Confirms the bucket-dominance rule
   matches the link validator's admin ⊆ writer ⊆ reader semantics.)

6. **G-4.4 hive grant-window — happy path**: hive Owner alice (Path 1,
   genesis author) grants bob an expiring HiveMembership. Bob (Path
   2, expiring) tries to grant carol a permanent membership. Commit
   REJECTED with `an expiring grantor may not mint a permanent
   (no-expiry) membership`.

7. **G-4.4 hive grant-window — extension attempt**: bob (Path 2,
   expiring at T+1000) tries to grant carol a membership expiring at
   T+5000. Commit REJECTED with `granted expiry … exceeds the
   grantor membership's expiry … an expiring grantor may not extend
   the delegation window`.

8. **G-4.4 hive grant-window — Path 1 dominates Path 2**: hive Owner
   alice (also genesis author) ALSO holds an expiring HiveMembership
   for herself (unusual but legal). She grants bob a permanent
   membership using the Path-2 witness. Commit succeeds — the Path-1
   re-verification via `fetch_genesis` detects she is the genesis
   author; Owner role dominates the expiring Path-2 witness.

## What this closes / does NOT close (delta from pass-3)

### Newly closed by pass-4

- **Attack #5 — recipient-list forgery on `AclSpec::HiveGroup`
  `public_key_acl`** (was the residual attack flagged in pass-3
  docs). A modified coordinator can no longer inject pubkeys into
  the reader/writer/admin/owner buckets to receive remote-signal
  notifications. Decryption gating via SharedSecrets was always
  intact; this hardens routing fan-out attribution.
- **G-4.4 at the hive layer** (matrix row #10 hive-layer analogue).
  Pass-3 closed it at the group layer; pass-4 mirrors the same fix
  at the hive layer.

### Preserved unchanged from pass-3

- All four `AclSpec` variants (`HiveGroup`, `DirectMessage`,
  `Public`, `OpenWrite`).
- All cross-hive patterns (DMs, member-request, hive-discovery,
  public posts, cross-hive group memberships, agent directory).
- Every coordinator extern, link type, and signal channel.
- `GroupGenesis`, `GroupMembership`, `HiveGenesis`, `HiveMembership`
  entry shapes.

### Still NOT closed this pass

- **Re-encryption on membership change.** When a group member is
  revoked, the SharedSecret used for that group's content was
  derived assuming the revoked member was a recipient; the revoked
  member can still decrypt past content they already fetched. This
  is the standard forward-secrecy gap; future work is a key-rotation
  scheme.
- **`AclSpec::PersonalVault` / `AclSpec::SubscriptionGated`**
  (future variants per D12/D13 in the pass-3 plan).

## Hash invariants for verification

Recorded by Phase 4-F (commit `9e1f842` source-of-record; wasms +
packed bundle generated against that tip):

- DNA hash:                `uhC0kNS2JM6lqmdxr3Q8VK2uhDJFF-wRBz-W73JjJKZnTTMyT8_JS`
- `content_integrity.wasm`: `1f4534b24332d9fdf089b66e80c04b4eb370994841b554bfcb455524b6f0c3c4`
- `content.wasm`:           `7a9e7a9800053a916b50141fd6cc72265d0090e3965a44eb90ae4ec298ca8370`
- hApp bundle sha256:       `50d409602fa8d9eeacf553a497aff39191bc9cc4f1c9ffd8080d3c8e0e844abd`

These hashes are the **new pass-4 invariant** — every subsequent
pass-4 commit MUST hold the DNA hash + integrity wasm sha256
byte-identical until pass-5+ does its own intentional integrity-
zome bump. The full lineage + invariant statement lives in
`.baseline-hashes.txt` "Pass-4" section.

## Commit + branch state

Branch: `feat-integrity-pass-4-recipient-witnesses` (off
`feat-integrity-pass-3-groups` tip `b1e72aa`).

Commits (in order):

- Phase 4-A/B/C — G-6.2 + G-4.4 integrity changes + coordinator
  fixture updates + 11 new host-side tests.
- Phase 4-D — `migrate-dna.ts` classifier comment + error-message
  update reflecting the pass-4 wire shape.
- Phase 4-E (this commit) — handoff docs (this file + updates to
  `HUMM_TAURI_ACLSPEC_INTEGRATION.md` + `HANDOFF_UPDATED_INFO.md` +
  banner on `PASS_3_DEPLOY_HANDOFF.md` + new E.4.l section in
  `HUMM_TAURI_FEATURE_ENABLEMENT.md`).
- Phase 4-F — verification + new `.baseline-hashes.txt` section +
  final report.

**Not pushed.** The user controls when this branch reaches `origin`
and `main`.
