# humm-tauri × pass-3 AclSpec integration

The canonical reference for humm-tauri devs implementing the pass-3
wire-shape migration. Where `PASS_3_DEPLOY_HANDOFF.md` covers the
deploy mechanics, this doc covers **what each call site looks like
on the humm-tauri side after pass-3**.

> **Status note.** This doc is the wire-shape contract. For the
> living "what changed since pass-2.5" view, see
> [`HANDOFF_UPDATED_INFO.md`](./HANDOFF_UPDATED_INFO.md). For the
> feature-by-feature implementation guide (which TS files change,
> what new files are needed, smoke tests), see
> [`HUMM_TAURI_FEATURE_ENABLEMENT.md`](./HUMM_TAURI_FEATURE_ENABLEMENT.md).
>
> humm-tauri is doing a **pass-1 → pass-3 leapfrog**: the pass-2
> wire shape (`hive_genesis_hash` / `author_membership_hash` /
> `acl` at the top of the header) was never integrated downstream,
> so pass-3 lands directly. The pass-2.5 handoff docs are still
> useful for the hive-identity track (`HiveGenesis`,
> `HiveMembership`, `migrate-hive`, `grant-memberships`); ALL of
> those are preserved unchanged by pass-3.

## 1. The new header shape

```ts
// pass-3 EncryptedContentHeader (what arrives back from
// get_encrypted_content + what you stamp on create_encrypted_content)
type EncryptedContentHeader = {
  id: string;
  display_hive_id: string;          // was `hive_id` in pass-1/2
  content_type: string;
  acl_spec: AclSpec;                // variant-dispatched authority
  public_key_acl: Acl;
  revision_author_signing_public_key: string;
};

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
  owner: ActionHash;                // GroupGenesis action hash
  admin: ActionHash[];
  writer: ActionHash[];
  reader: ActionHash[];
};

type Acl = {
  owner: string;
  admin: string[];
  writer: string[];
  reader: string[];                 // pubkey strings (multibase)
};
```

`CreateEncryptedContentInput` (the wire shape you POST) is the same
structure minus `revision_author_signing_public_key` (which is moved
to a top-level field on the input — see below) plus an optional
`dynamic_links`.

```ts
type CreateEncryptedContentInput = {
  id: string;
  display_hive_id: string;
  content_type: string;
  revision_author_signing_public_key: string;
  bytes: Uint8Array;
  acl_spec: AclSpec;
  public_key_acl: Acl;
  dynamic_links?: string[] | null;
};
```

## 2. AclSpec variant per content type

Exhaustive mapping over every shipped + planned content type. The
**Notes** column flags any non-default wiring on the humm-tauri side.

| `content_type`                                  | AclSpec variant                                     | Notes |
|-------------------------------------------------|-----------------------------------------------------|-------|
| `hummhive-core-hive-v1`                         | `HiveGroup` (personal-group anchor) or `Public`     | The "hive setup" entry; on creation by the hive owner, treat as `HiveGroup` with a singleton personal group (the owner is the group author = implicit Owner of Path A). Migration default classifies as `Public`. |
| `hummhive-core-group-v1`                        | `HiveGroup`                                         | The group's own content entry (display fields, rename). The `GroupGenesis` action hash is the cryptographic identity; this entry is for display. |
| `hummhive-core-group-member-list-v1`            | `HiveGroup` (DEMOTED to display cache)              | Authority moves to `list_group_members(group_genesis_hash)`. Coordinator paths still accept writes for backward-compat; humm-tauri SHOULD switch reads. |
| `hummhive-core-member-v1`                       | `HiveGroup`                                         | Per-hive member metadata. Hive admin authority. |
| `hummhive-core-invite-v1`                       | `HiveGroup`                                         | Admin-issued invite. The invite payload now carries `inviter_group_authority_hashes` so accept can re-walk the inviter's authority. |
| `hummhive-core-invite-accept-v1`                | `HiveGroup`                                         | Accepter writes after token gate; emits one `create_group_membership` per group_id in the invite. |
| `hummhive-core-invite-purge-v1`                 | `HiveGroup`                                         | Admin authority. |
| `hummhive-core-member-request-v1`               | **`OpenWrite { target: Some(target_hive) }`**       | Outsider knock. Requester does NOT need hive membership; validator checks author identity + target existence. |
| `hummhive-core-hive-discovery-v1`               | **`OpenWrite { target: None }`**                    | Cross-network publishing. |
| `hummhive-core-group-discovery-v1`              | `HiveGroup`                                         | Hive admin writes; humm-tauri's existing flow. |
| `hummhive-core-shared-secrets-v1` (pair)        | **`DirectMessage`**                                 | Pair-SS provisioning. `recipients: [sender, recipient]`. Validator pins `public_key_acl.reader == recipients`. |
| `hummhive-core-shared-secrets-v1` (group)       | `HiveGroup`                                         | Group-SS provisioning. |
| `hummhive-core-shared-secrets-v1` (personal)    | `HiveGroup` (singleton personal group)              | Per-user; only author needs Writer+. |
| `hummhive-core-peer-identity-claim-v1`          | **`DirectMessage`**                                 | Cross-hive identity-rotation push. Survives identity changes. |
| `hummhive-core-blob-metadata-v1`                | Polymorphic                                          | Pick per-blob: `HiveGroup` (group-scoped media), `DirectMessage` (per-peer share), `Public` (public broadcast). See `HUMM_TAURI_FEATURE_ENABLEMENT.md` § E.4.e. |
| `hummhive-core-ui-shared-state-v1`              | `HiveGroup` (singleton personal group)              | Per-user device state; only the author needs to read/write. |
| `hummhive-core-sidecar-config-v1`               | `HiveGroup`                                         | Hive admin authority. |
| `hummhive-core-sidecar-install-v1`              | `HiveGroup`                                         | Hive admin authority. |
| `hummhive-core-sidecar-provider-v1`             | `HiveGroup`                                         | Hive admin authority. |
| `direct_message` (DM sidecar)                   | **`DirectMessage`**                                 | THE canonical DM. `recipients: [me, peer]`. Cross-hive viable. |
| `humm-addon-text-post-v1`                       | **`Public`**                                        | World-readable. `public_key_acl.reader = ['*']` is a routing hint; validator doesn't require it. |
| Planned `humm-sidecar-group-message-v1`         | `HiveGroup`                                         | Cross-hive group chat. Members in different hives hold `GroupMembership` granted by the hive owner. |
| Planned `hummhive-core-agent-directory-v1`      | **`OpenWrite { target: None }`**                    | Cross-network agent discovery. |
| Planned `hummhive-core-sidecar-manifest-v1`     | **`OpenWrite { target: None }`**                    | Sidecar marketplace. |
| Planned streaming manifests                     | Polymorphic (`Public` / `HiveGroup` / `DirectMessage`) | See § E.4.h in `HUMM_TAURI_FEATURE_ENABLEMENT.md`. |

## 3. Per-modal wiring (what changes in your TS)

### `ManageGroup` (Add Group)

```ts
// pass-3
const { hash: groupGenesisHash } = await callZome({
  zome_name: 'content',
  fn_name: 'create_group_genesis',
  payload: {
    hive_genesis_hash: activeHiveGenesisHash,
    display_id: form.name,
    hive_wide_role: form.isHiveWideRoleGroup ? form.role : null,
    creator_hive_membership_hash: myHiveMembershipHash ?? null,
  },
});
// Then issue create_group_membership for each selected member.
for (const memberPubKey of form.members) {
  await callZome({
    zome_name: 'content',
    fn_name: 'create_group_membership',
    payload: {
      group_genesis_hash: groupGenesisHash,
      for_agent: memberPubKey,
      role: form.memberRoles[memberPubKey],
      grantor_membership_hash: null, // Path A: I'm the group author
      grantor_hive_membership_hash: myHiveMembershipHash ?? null,
      expiry: null,
    },
  });
}
```

### `ManageGroup` (Edit Group — rename)

`GroupGenesis` is **immutable**. The group's cryptographic identity
(action hash) is permanent. Display rename happens via a new `Group`
content entry (`hummhive-core-group-v1`, `AclSpec::HiveGroup`) with
the updated name; humm-tauri's UI surfaces the latest one.

### `ManageGroup` (Add / Remove member)

```ts
// Add: issue create_group_membership
await callZome({
  zome_name: 'content',
  fn_name: 'create_group_membership',
  payload: {
    group_genesis_hash,
    for_agent: newMemberPubKey,
    role: 'Writer',
    grantor_membership_hash: myGroupMembershipHash ?? null,
    grantor_hive_membership_hash: myHiveMembershipHash ?? null,
    expiry: null,
  },
});

// Remove: revoke_group_membership
await callZome({
  zome_name: 'content',
  fn_name: 'revoke_group_membership',
  payload: {
    membership_hash: targetMembershipHash,
    new_expiry: { secs: Math.floor(Date.now() / 1000) - 1, nanos: 0 },
    grantor_membership_hash: myGroupMembershipHash ?? null,
    grantor_hive_membership_hash: myHiveMembershipHash ?? null,
  },
});
```

**Self-revocation is NOT supported.** Rule 1 of
`validate_create_group_membership` rejects
`action.author == for_agent`. Implement leave-group as a remove-member
request that an Admin+ holder processes.

### `ManageMember` (role dropdown)

Role change = `revoke_group_membership` on the old role's
membership + `create_group_membership` for the new role. Consumers
read the latest valid membership via `get_latest_group_membership`.

### `Invites` / `ManageInvite`

The `Invite` content payload (`hummhive-core-invite-v1`) gains an
`inviter_group_authority_hashes: Record<string, ActionHash>` field
— a map from `group_genesis_hash` to the inviter's own
`GroupMembership` hash for each group the invite pre-authorizes. The
invite-accept flow iterates this map and calls
`create_group_membership` once per group. If the inviter's authority
expired since invite issue, the per-group create fails gracefully;
the rest succeed.

### `Compose` (public post)

```ts
// Swap the hardcoded buildPublicAcl(ownerSigningKey) for:
await callZome({
  zome_name: 'content',
  fn_name: 'create_encrypted_content',
  payload: {
    id: postId,
    display_hive_id: activeHive.displayId,
    content_type: 'humm-addon-text-post-v1',
    revision_author_signing_public_key: encodeHashToBase64(myPubKey),
    bytes: encryptedBytes,
    acl_spec: {
      Public: {
        hive_genesis_hash: activeHiveGenesisHash,
        author_membership_hash: myHiveMembershipHash ?? null,
      },
    },
    public_key_acl: {
      owner: '',
      admin: [],
      writer: [],
      reader: ['*'], // routing hint; validator ignores for Public
    },
    dynamic_links: null,
  },
});
```

For the upcoming per-content ACL picker (Compose's "ACL UI is a
later slice" comment), surface the four variants as a chooser. See
§ E.4.f in `HUMM_TAURI_FEATURE_ENABLEMENT.md`.

### DM sidecar (`sendDirectMessage`)

```ts
const recipients = [myPubKey, peerPubKey];
const recipientsB64 = recipients.map(encodeHashToBase64);
await callZome({
  zome_name: 'content',
  fn_name: 'create_encrypted_content',
  payload: {
    id: dmId,
    display_hive_id: '',
    content_type: 'direct_message',
    revision_author_signing_public_key: encodeHashToBase64(myPubKey),
    bytes: encryptedBytes,
    acl_spec: {
      DirectMessage: { recipients },
    },
    public_key_acl: {
      owner: '',
      admin: [],
      writer: [],
      reader: recipientsB64, // MUST equal recipients (sorted)
    },
    dynamic_links: null,
  },
});
```

The integrity validator pins `public_key_acl.reader == recipients`
(sorted-equality) so either party retains delete authority. The
sidecar's UX (thread view, read-state, soft-delete via
`DmRemoteSignal::DmDeleteRequest`) is unaffected.

### `MemberRequest` flow

```ts
// requester (no hive membership required)
await callZome({
  zome_name: 'content',
  fn_name: 'create_encrypted_content',
  payload: {
    id: requestId,
    display_hive_id: targetHive.displayId,
    content_type: 'hummhive-core-member-request-v1',
    revision_author_signing_public_key: encodeHashToBase64(myPubKey),
    bytes: encryptedBytes,
    acl_spec: {
      OpenWrite: { target_hive_genesis_hash: targetHiveGenesisHash },
    },
    public_key_acl: { owner: '', admin: [], writer: [], reader: [] },
    dynamic_links: null,
  },
});
```

The target hive's owner consumes the verifiable inbox of requests
(humm-tauri UI surface for the "outsider knocking" pattern; currently
stubbed in `MemberRequests` pane).

### `HiveDiscovery` publish

```ts
// cross-network discovery anchor — target: None
await callZome({
  zome_name: 'content',
  fn_name: 'create_encrypted_content',
  payload: {
    id: discoveryId,
    display_hive_id: '',
    content_type: 'hummhive-core-hive-discovery-v1',
    revision_author_signing_public_key: encodeHashToBase64(myPubKey),
    bytes: encryptedBytes,
    acl_spec: { OpenWrite: { target_hive_genesis_hash: null } },
    public_key_acl: { owner: '', admin: [], writer: [], reader: [] },
    dynamic_links: null,
  },
});
```

## 4. New TS types (canonical wire shape)

Drop into `humm-tauri/src/types/contentSchema.ts` (or wherever your
zome-call types live):

```ts
import { type ActionHash, type AgentPubKey } from '@holochain/client';

export type Role = 'Owner' | 'Admin' | 'Writer' | 'Reader';
// Pass-2 compat alias preserved on the Rust side; same TS shape.
export type HiveRole = Role;

export type Acl = {
  owner: string;
  admin: string[];
  writer: string[];
  reader: string[];
};

export type AclByGroupGenesis = {
  owner: ActionHash;
  admin: ActionHash[];
  writer: ActionHash[];
  reader: ActionHash[];
};

export type AclSpec =
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

export type EncryptedContentHeader = {
  id: string;
  display_hive_id: string;
  content_type: string;
  acl_spec: AclSpec;
  public_key_acl: Acl;
  revision_author_signing_public_key: string;
};

export type GroupGenesis = {
  hive_genesis_hash: ActionHash;
  display_id: string;
  hive_wide_role: Role | null;
  creator_hive_membership_hash: ActionHash | null;
  created_at_microseconds: number;
};

export type GroupMembership = {
  group_genesis_hash: ActionHash;
  for_agent: AgentPubKey;
  role: Role;
  grantor_membership_hash: ActionHash | null;
  grantor_hive_membership_hash: ActionHash | null;
  expiry: { secs: number; nanos: number } | null;
};

export type ListedGroup = {
  group_genesis_hash: ActionHash;
  hive_genesis_hash: ActionHash;
  display_id: string;
  hive_wide_role: Role | null;
  role: Role | null; // None = founded; Some = granted
};

export type GroupGenesisResponse = { genesis: GroupGenesis; hash: ActionHash };
export type GroupMembershipResponse = { membership: GroupMembership; hash: ActionHash };
```

## 5. `derrivePublicKeyAcl` migration

For `HiveGroup` content, humm-tauri's `derrivePublicKeyAcl` helper
(currently using `groupApi.listHolochainPublicKeys`) should switch to
`list_group_members(group_genesis_hash)`:

```ts
// pass-3 derrivePublicKeyAcl for HiveGroup
async function deriveHiveGroupPublicKeyAcl(
  groupGenesisHashes: { owner: ActionHash; admin: ActionHash[]; writer: ActionHash[]; reader: ActionHash[] },
): Promise<Acl> {
  const admin = await listMembersOfAll(groupGenesisHashes.admin);
  const writer = await listMembersOfAll([...groupGenesisHashes.admin, ...groupGenesisHashes.writer]);
  const reader = await listMembersOfAll([
    ...groupGenesisHashes.admin,
    ...groupGenesisHashes.writer,
    ...groupGenesisHashes.reader,
  ]);
  const ownerMembers = await listMembersOfAll([groupGenesisHashes.owner]);
  return {
    owner: ownerMembers[0]?.for_agent ?? '',
    admin: admin.map(m => encodeHashToBase64(m.for_agent)),
    writer: writer.map(m => encodeHashToBase64(m.for_agent)),
    reader: reader.map(m => encodeHashToBase64(m.for_agent)),
  };
}
```

For other variants:
- **DirectMessage**: `public_key_acl.reader = recipients` (pinned by
  validator); other buckets empty.
- **Public**: `public_key_acl.reader = ['*']` (routing hint) or
  empty; other buckets empty.
- **OpenWrite**: `public_key_acl` empty by convention.

## 6. Threat-model deltas (what each attack used to look like)

| # | Attack | Pre-pass-3 | Closed by |
|---|---|---|---|
| 1 | Forge `GroupMemberList` for any group | Any writer commits; readers trust first match | G-5 (links validated) + `list_group_members` |
| 2 | Self-mint admin group via `hiveWideRole: admin` | Any writer | `validate_create_group_genesis` requires hive Owner for system role groups |
| 3 | Self-promote via ManageMember | `GroupMemberListApi.add` succeeds | `validate_create_group_membership` rule 1 (no self-grant) + rule 3 (no escalation) |
| 4 | Mint privileged invite | `InviteApi.add` succeeds for any writer | Invite-accept calls `create_group_membership` → validators catch |
| 5 | Forge `public_key_acl` on group content | acl unvalidated | **DEFERRED (G-6.2)** — `public_key_acl` is a routing hint this pass |
| 6 | Forge `acl` group squuid | acl unvalidated | `AclByGroupGenesis` is ActionHash-keyed; validators require real `GroupGenesis` |
| 7 | Author group content without group-write authority | Hive-level check only | `validate_hivegroup_acl` per-group `check_group_authority` |
| 8 | Revoked / expired member writes | Revocation client-side | G-4 expiry + read-time expiry check |
| 9 | Cross-hive group claim | acl unvalidated | `group.hive_genesis_hash == header hive_genesis_hash` check |
| 10 | Delegation-window extension | Pre-existing pass-2 gap | G-4.4 enforce_grant_window |
| 11 | Forge `Invite` revoke | InvitePurge unvalidated | Same as #4 |
| 12 | Spoof a DM | DM unvalidated | DM validator: author ∈ recipients + reader pin |
| 13 | Cross-network fake hive-discovery target | N/A | OpenWrite target HiveGenesis check |
| 14 | Outsider posts under `Public` without hive membership | N/A | `Public` validator: hive Writer+ required |
| M-1 | Update-chain hijack | Pre-existing pass-1 gap | `validate_update_encrypted_content` original-author check |
| L-1 | EncryptedContentUpdates link poisoning | Pre-existing pass-1 gap | Link author == base author == target author |
| L-2 | Degenerate self-DM with duplicate recipients | N/A | DM uniqueness HashSet |

## 7. Cross-hive preservation guarantees

EVERY shipped + planned cross-hive pattern keeps working after
pass-3:

| Pattern | Variant | How |
|---|---|---|
| Cross-hive DMs (sidecar) | `DirectMessage` | `recipients` includes peer's pubkey regardless of hive; no membership check |
| Pair shared-secrets | `DirectMessage` | Recipients = [sender, recipient] |
| Cross-hive identity-claim push | `DirectMessage` | Recipient pubkey lives anywhere |
| Member-request (outsider knock) | `OpenWrite { target: Some }` | Requester needs zero memberships |
| Hive-discovery (cross-network) | `OpenWrite { target: None }` | Anyone publishes |
| Public posts (anyone reads) | `Public` | Reader bucket unconstrained |
| Cross-hive group chat | `HiveGroup` | Hive owner grants GroupMembership to peer's pubkey (just a holohash) |
| Cross-hive group SS | `HiveGroup` | Same as above |
| Agent directory (planned) | `OpenWrite { target: None }` | Cross-network |
| Sidecar marketplace (planned) | `OpenWrite { target: None }` | Cross-network |

## 8. Failure modes & UX hints

| Symptom | Cause | UX |
|---|---|---|
| `does not match action.author` (header pubkey) | `revision_author_signing_public_key` is stale | Re-derive from `agent_info()` before each write |
| `recipients.len() = 1` | DM with single recipient (the author) | Block at UI; DM needs ≥ 2 |
| `recipients.len() = X exceeds DM_MAX_RECIPIENTS` | Group DM > 32 | Surface "use a group chat instead" prompt |
| `not in recipients` | Author missing from recipient list | Splice author in client-side before submit |
| `does not match recipients` (reader bucket) | `public_key_acl.reader != recipients` | Always derive reader from recipients |
| `acl references group X in hive Y but entry claims hive Z` | Cross-hive forgery attempt | Surface "group not in this hive" to user |
| `does not match original action author` | Update on someone else's entry | Block at UI; only the original author may update |
| `link author X does not match base entry author Y` | Link author mismatch | Don't publish updates links for entries you don't own |
| `granting Owner role requires group Owner or hive Admin+` | Privilege escalation attempt | Surface "you don't have authority" |
| `granted expiry exceeds grantor membership's expiry` | G-4.4 grant-window-containment violation | Surface "your role expires earlier than the grant" |

## 9. Inbox discriminator bump

`InboxEvent::GroupInvite = 3` is added to the integrity zome's
inbox enum (the existing `DmCreate=0`, `DmDelete=1`, `HiveInvite=2`
are unchanged). humm-tauri's inbox poller should filter by both
`HiveInvite` (existing hive list) AND `GroupInvite` (new group
list) — `list_my_hives` and `list_my_groups` already do this
internally, so the change is invisible to consumers of those
externs. If you walk inbox links directly (e.g. for a unified
notification feed), match on tag bytes `[0, 1, 2, 3]`.

## 10. Quick start checklist

When you start the pass-3 migration:

1. Update `humm-tauri/src/types/contentSchema.ts` to the new wire
   shape (§ 4).
2. Update `humm-tauri/src/api/core/hummContent/hummContentWrites.ts`
   `addEntry` to take `AclSpec` instead of `acl: Acl`.
3. Replace each call site's `acl: { ... }` with the right
   `acl_spec: { Variant: { ... } }` per § 2.
4. Wire `create_group_genesis` / `create_group_membership` /
   `revoke_group_membership` into the existing
   `MembersAndGroups` UI flows (§ 3).
5. Update `derrivePublicKeyAcl` to use `list_group_members` for
   `HiveGroup` content (§ 5).
6. Run cross-hive smoke tests from
   [`PASS_3_DEPLOY_HANDOFF.md`](./PASS_3_DEPLOY_HANDOFF.md) § "Cross-hive
   smoke-test checklist".

For the feature-by-feature implementation guide (which files change,
new sidecars/components needed, smoke tests per feature), see
[`HUMM_TAURI_FEATURE_ENABLEMENT.md`](./HUMM_TAURI_FEATURE_ENABLEMENT.md).
