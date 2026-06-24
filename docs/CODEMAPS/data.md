<!-- codemap:data | generated:2026-06-05 | updated:2026-06-24 | scope:full -->

# Data Model

All state lives on-DHT (Holochain entries + links) or on-chain (private entries).
No external database. Source file: `integrity/content/src/`.

> **⚠ Integrity zome change gravity.** Adding/removing/reordering entry types
> or link types in the enums below changes the DNA hash and forks the chain.
> Append-only at the END of each enum preserves existing variant indices.

## Entry Types (EntryTypes enum — integrity/lib.rs)

| # | Entry | Visibility | Mutability | Source |
|---|---|---|---|---|
| 0 | `EncryptedContent` | public | update + delete | `encrypted_content.rs` |
| 1 | `HiveGenesis` | public | immutable | `hive.rs` |
| 2 | `HiveMembership` | public | immutable (revoke via expiry) | `hive.rs` |
| 3 | `DmProbeLog` | **private** | create-only | `inbox.rs` |
| 4 | `GroupGenesis` | public | immutable | `group.rs` |
| 5 | `GroupMembership` | public | immutable (revoke via expiry) | `group.rs` |
| 6 | `HiveOwnerHandoffOffer` | public | immutable | `hive.rs` |
| 7 | `HiveOwnerHandoffAccept` | public | immutable | `hive.rs` |
| 8 | `InviteRedemption` | public | immutable | `invite.rs` |

## Entry Schemas

### EncryptedContent (encrypted_content.rs)
```
{ header: EncryptedContentHeader, encrypted_content: SerializedBytes }
```
Header fields: `id`, `content_type`, `display_hive_id`,
`revision_author_signing_public_key`, `acl_spec: AclSpec`,
`public_key_acl: Acl`.

### AclSpec Variants (pass-3 authority contract)
```
HiveGroup    { hive_genesis_hash, author_membership_hash?,
               group_acl: AclByGroupGenesis, author_group_membership_hash?,
               recipient_witnesses: Vec<RecipientWitness> }
DirectMessage { recipients: Vec<AgentPubKey> }   (2..=32)
Public        { hive_genesis_hash, author_membership_hash? }
OpenWrite     { target_hive_genesis_hash? }
```

### HiveGenesis (hive.rs)
```
{ display_id: String, created_at_microseconds: i64 }
```
Identity = action hash. Immutable.

### HiveMembership (hive.rs)
```
{ hive_genesis_hash, for_agent, role: Role, grantor_membership_hash?,
  expiry?, grantor_owner_accept_hash? }
```
Role: Owner | Admin | Writer | Reader. Dominance: Owner > Admin > Writer > Reader.
`grantor_owner_accept_hash` (pass-5, `#[serde(default)]`): cited for Admin grants
to prove the grantor is a lineage owner. Owner is NOT grantable via membership.

### GroupGenesis (group.rs)
```
{ hive_genesis_hash, display_id, hive_wide_role?, creator_hive_membership_hash?,
  created_at_microseconds }
```

### GroupMembership (group.rs)
```
{ group_genesis_hash, for_agent, role: Role, grantor_membership_hash?,
  grantor_hive_membership_hash?, expiry? }
```

### DmProbeLog (inbox.rs) — private
```
{ probed_at_microseconds: i64, last_processed_inbox_link_hash?: ActionHash }
```

### HiveOwnerHandoffOffer / HiveOwnerHandoffAccept (hive.rs) — pass-5
```
Offer  { hive_genesis_hash, to_agent, offerer_owner_accept_hash?, created_at_microseconds }
Accept { offer_hash }
```
Owner-transfer handshake. `is_lineage_owner` walks accept→offer by induction.
Immutable. The coordinator folds the accept lineage to resolve the current owner.

### InviteRedemption (invite.rs) — pass-5
```
{ invite_action_hash, redeemer }
```
Advisory `max_uses` soft-cap marker (approver-authored; count is advisory, not
authority). Immutable.

## Link Types (LinkTypes enum — integrity/lib.rs)

| # | Link Type | Base | Target | Tag | Purpose |
|---|---|---|---|---|---|
| 0 | OriginalHashPointer | updated AH | original AH | — | update-chain back-pointer |
| 1 | EncryptedContentUpdates | original AH | updated AH | — | update-chain forward index |
| 2 | TimePath | path | path | — | declared integrity variant, never created |
| 3 | TimeItem | path | entry AH | — | declared integrity variant, never created |
| 4 | Hive | Path([key, content_type]) | entry AH | — | dual-shape: author OR hive discovery |
| 5 | Dynamic | Path([hive, type, label]) | entry AH | label (UTF-8) | per-group/topic index |
| 6 | HummContentId | Path([hive, id]) | entry AH | — | content-id lookup within hive |
| 7 | HummContentOwner | Path([hive, type, group]) | entry AH | group (UTF-8) | ACL owner index |
| 8 | HummContentAdmin | Path([hive, type, group]) | entry AH | group (UTF-8) | ACL admin index |
| 9 | HummContentWriter | Path([hive, type, group]) | entry AH | group (UTF-8) | ACL writer index |
| 10 | HummContentReader | Path([hive, type, group]) | entry AH | group (UTF-8) | ACL reader index |
| 11 | Inbox | AgentPubKey | ActionHash | 1-byte InboxEvent | offline DM delivery |
| 12 | AgentToGroupMemberships | AgentPubKey | GroupMembership AH | — | forward: "my memberships" |
| 13 | GroupToGroupMemberships | GroupGenesis AH | GroupMembership AH | for_agent | reverse: roster |
| 14 | HiveToGroups | HiveGenesis AH | GroupGenesis AH | — | hive → groups enumeration |
| 15 | AgentToOwnerHandoffs | AgentPubKey (to_agent) | HiveOwnerHandoffOffer AH | — | recipient's pending owner offers |
| 16 | HiveToOwnerHandoffs | HiveGenesis AH | HiveOwnerHandoffAccept AH | — | owner-lineage resolution |
| 17 | InviteToRedemptions | invite AH | InviteRedemption AH | — | advisory redemption count |

## InboxEvent Discriminators (inbox.rs)

```
DmCreate = 1, DmDelete = 2, HiveInvite = 3, GroupInvite = 4
```

## Authority Chain (trust hierarchy)

```
HiveGenesis (action hash = hive identity)
  └─ HiveMembership (chain-walked, Moss-style inductive validation)
       └─ GroupGenesis (bound to parent hive)
            └─ GroupMembership (3-path authority: group author / hive sovereign / explicit)
                 └─ EncryptedContent (AclSpec variant validates per-scope contract)
                      └─ RecipientWitness (per-pubkey membership cross-reference, pass-4)
       └─ Hive ownership (pass-5): genesis author = root owner; the
          HiveOwnerHandoffOffer→Accept lineage transfers it; coordinator
          resolve_current_owner folds to the single current owner.
```

## Constants

```
DM_MAX_RECIPIENTS = 32
GROUP_ACL_MAX_GROUPS = 64
HIVEGROUP_MAX_WITNESSES = 256
ENCRYPTED_CONTENT_TIME_INDEX = "encrypted_content_time"
MIGRATION_MARKER_CONTENT_TYPE_PREFIX = "_migrated/"
```
