<!-- codemap:data | generated:2026-06-05 | updated:2026-07-23 | scope:full -->

# Data Model

All state lives on-DHT (Holochain entries + links) or on-chain (private entries).
No external database. Source file: `integrity/content/src/`.

> **⚠ Integrity zome change gravity.** Adding/removing/reordering entry types
> or link types in the enums below changes the DNA hash and forks the chain.
> Append-only at the END of each enum preserves existing variant indices.

The shipped pass-6 v3.3.0 baseline remains DNA `uhC0ksXs…`; its dry-refactor
changed integrity source/WASM without reordering entry or link enums. The parked
pass-7 branch adds `ContentLineage`, `Lineage`, and `HiveMembershipIndex` in
earlier waves. Wave-4 changed integrity bytes once at M16 for shared validation
helpers, yielding the scratch-only DNA
`uhC0k-HAqM4zW2rCWrKSujEKDZcqybE_ATUjKxkRy2BmRjURYddxP` and integrity wasm
sha256 `ec11ba8f9518cee6aee5d9e1df4fc1f7449f42584213abb4f8636cdceb90fcdd`.
M16 added no data variant or field; coordinator-only M17–M21 held this pin. The
scratch DNA is parked and undistributed, outside the shipped baseline.

## Entry Types (EntryTypes enum — integrity/lib.rs)

| # | Entry | Visibility | Mutability | Source |
|---|---|---|---|---|
| 0 | `EncryptedContent` | public | update + delete | `encrypted_content/` |
| 1 | `HiveGenesis` | public | immutable | `hive/` |
| 2 | `HiveMembership` | public | immutable (revoke via expiry) | `hive/` |
| 3 | `DmProbeLog` | **private** | create-only | `inbox.rs` |
| 4 | `GroupGenesis` | public | immutable | `group/` |
| 5 | `GroupMembership` | public | immutable (revoke via expiry) | `group/` |
| 6 | `HiveOwnerHandoffOffer` | public | immutable | `hive/` |
| 7 | `HiveOwnerHandoffAccept` | public | immutable | `hive/` |
| 8 | `InviteRedemption` | public | immutable | `invite.rs` |

## Entry Schemas

### EncryptedContent (encrypted_content/)
```
{ header: EncryptedContentHeader, encrypted_content: SerializedBytes }
```
Header fields: `id`, `content_type`, `display_hive_id`,
`revision_author_signing_public_key`, `acl_spec: AclSpec`,
`public_key_acl: Acl`, and pass-7 `lineage?: ContentLineage`
(`#[serde(default)]`).

### ContentLineage (pass-7 scratch)
```
{ prior_dna_hash_b64: String, prior_action_hash_b64: String }
```
The claim lets migrated content cite its prior-generation action. Shape
validation rejects current-DNA self-reference, while the `Lineage` reverse index
binds the lookup path to the target entry's own claim and author.

### AclSpec Variants (pass-3 authority contract)
```
HiveGroup    { hive_genesis_hash, author_membership_hash?,
               group_acl: AclByGroupGenesis, author_group_membership_hash?,
               recipient_witnesses: Vec<RecipientWitness> }
DirectMessage { recipients: Vec<AgentPubKey> }   (2..=32)
Public        { hive_genesis_hash, author_membership_hash? }
OpenWrite     { target_hive_genesis_hash? }
```

### HiveGenesis (hive/)
```
{ display_id: String, created_at_microseconds: i64 }
```
Identity = action hash. Immutable.

### HiveMembership (hive/)
```
{ hive_genesis_hash, for_agent, role: Role, grantor_membership_hash?,
  expiry?, grantor_owner_accept_hash? }
```
Role: Owner | Admin | Writer | Reader. Dominance: Owner > Admin > Writer > Reader.
`grantor_owner_accept_hash` (pass-5, `#[serde(default)]`): cited for Admin grants
to prove the grantor is a lineage owner. Owner is NOT grantable via membership.

### GroupGenesis (group/)
```
{ hive_genesis_hash, display_id, hive_wide_role?, creator_hive_membership_hash?,
  created_at_microseconds }
```

### GroupMembership (group/)
```
{ group_genesis_hash, for_agent, role: Role, grantor_membership_hash?,
  grantor_hive_membership_hash?, expiry? }
```

### DmProbeLog (inbox.rs) — private
```
{ probed_at_microseconds: i64, last_processed_inbox_link_hash?: ActionHash }
```

### HiveOwnerHandoffOffer / HiveOwnerHandoffAccept (hive/) — pass-5
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
| 0 | OriginalHashPointer | current EncryptedContent AH | root EncryptedContent create AH | empty | update-chain back-pointer; validated against native action root |
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
| 18 | Lineage | Path([prior DNA hash, prior action hash]) | current EncryptedContent AH | prior action hash (UTF-8) | reverse lookup for cross-generation provenance |
| 19 | HiveMembershipIndex | member AgentPubKey | HiveMembership AH or founder HiveGenesis AH | empty | durable self-scoped hive discovery; author-only deletion survives Inbox sweeps |

Both pass-7 indexes expose public relationship metadata: `Lineage` correlates
records across DNA generations, and `HiveMembershipIndex` makes hive
affiliations enumerable from an AgentPubKey. The scratch ledger accepts that
tradeoff for portable provenance and retraction-safe discovery. Dynamic and ACL
link tags also remain public; sensitive dynamic labels should be opaque client
identifiers rather than low-entropy names.

## InboxEvent Discriminators (inbox.rs)

```
DmCreate = 1, DmDelete = 2, HiveInvite = 3, GroupInvite = 4
```

## Transient Signal Shapes (coordinator; pass-7 Wave-4 scratch)

```
EncryptedContentSignal {
  action_type, data: EncryptedContentResponse, from_agent?
}                                      // full payload; coordinator emits locally

EncryptedContentHint {
  action_type, hash, original_hash, from_agent?
}                                      // remote content fan-out; no ciphertext

OwnerHandoffOfferHint {
  offer_hash, hive_genesis_hash, from_agent?
}                                      // explicit handoff recipient
```

`recv_remote_signal` replaces `from_agent` with `call_info().provenance` before
re-emitting either hint locally. The legacy full-content decoder remains, so
remote signals are wake-up hints: consumers fetch and validate the referenced
DHT record instead of trusting signal-carried bytes.

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

## Wave-4 Integrity Helper Invariants

- `AclByGroupGenesis::groups()` supplies one owner-first traversal across the
  owner, admin, writer, and reader buckets. Duplicate checks and per-group
  authority walks therefore cannot silently disagree on bucket order.
- `validate_expiry_containment` supplies the common hive/group verdict for an
  expiring grantor: a child grant must expire no later than its grantor and
  cannot become permanent.

## Constants

Integrity:
```
DM_MAX_RECIPIENTS = 32
GROUP_ACL_MAX_GROUPS = 64
HIVEGROUP_MAX_WITNESSES = 256
ENCRYPTED_CONTENT_TIME_INDEX = "encrypted_content_time"
MIGRATION_MARKER_CONTENT_TYPE_PREFIX = "_migrated/"
```

Pass-7 Wave-4 coordinator read bounds:
```
DYNAMIC_LINKS_BATCH_MAX = 64
HIVE_LINKS_BATCH_MAX = 32
CONTENT_ID_BATCH_MAX = 64
AUTHOR_BATCH_MAX = 64
MEMBERSHIP_BATCH_MAX = 64
GROUP_MEMBERS_BATCH_MAX = 64
BATCH_RESOLVE_BUDGET = 4096
GROUP_MEMBERS_LINK_BUDGET = 4096
```
