<!-- codemap:backend | generated:2026-06-05 | updated:2026-06-23 | scope:full -->

# Backend (Zome Externs)

All externs live in coordinator zome `content`. Integrity zome `content_integrity`
has no callable externs (only `validate` + `genesis_self_check`).

## Coordinator Externs — EncryptedContent CRUD

```
create_encrypted_content(CreateEncryptedContentInput) → EncryptedContentResponse
  └─ crud.rs → links: Hive(author) + Hive(hive) + Dynamic + ACL + ContentId + Inbox
get_encrypted_content(ActionHash) → EncryptedContentResponse
  └─ crud.rs → get_helpers::get_eh → get_latest_typed_from_eh
get_many_encrypted_content(Vec<ActionHash>) → Vec<EncryptedContentResponse>
  └─ crud.rs → maps get_encrypted_content
update_encrypted_content(UpdateEncryptedContentInput) → EncryptedContentResponse
  └─ crud.rs → update_entry + EncryptedContentUpdates link + OriginalHashPointer link
delete_encrypted_content(ActionHash) → ActionHash
  └─ crud.rs → delete_entry (I-A receiver-initiated tombstone)
```

## Coordinator Externs — Queries

```
list_by_hive_link(ListByHiveInput) → Vec<EncryptedContentResponse>
  └─ queries.rs → Path([hive_genesis_hash, content_type]) + since_ts + limit (C2)
count_links_by_hive(CountByHiveInput) → usize
  └─ queries.rs → count only, no entry resolution (C3)
list_by_dynamic_link(ListByDynamicLinkInput) → Vec<EncryptedContentResponse>
  └─ queries.rs → Path([hive_genesis_hash, content_type, dynamic_label])
list_by_acl_link(ListByAclInput) → Vec<EncryptedContentResponse>
  └─ queries.rs → Path([hive_genesis_hash, content_type, entity_id]) by ACL class
list_by_author(ListByAuthorInput) → Vec<EncryptedContentResponse>
  └─ queries.rs → Path([author_pubkey, content_type])
get_by_content_id_link(ListByContentIdInput) → EncryptedContentResponse
  └─ queries.rs → Path([hive_genesis_hash, content_id])
fetch_pair_ss_with_hive_check(FetchPairWithHiveCheckInput) → Vec<EncryptedContentResponse>
  └─ queries.rs → intersect author-path ∩ dynamic-path (C4)
get_encrypted_content_by_time_and_author(_) → [] (stub)
```

All list/get-many reads above resolve targets through `get_many_encrypted_content`,
which is **decode-tolerant** (`filter_map(.ok())`, pass-4-query-tolerance): an
unresolvable / gossip-lagged / tombstoned target is skipped, never poisoning the
batch. Likewise `list_my_hives` / `get_latest_membership` (+ the group equivalents)
`.ok().flatten()` a wrong-type Inbox target instead of `?`-propagating a decode error.

## Coordinator Externs — Hive

```
create_hive_genesis(CreateHiveGenesisInput) → HiveGenesisResponse
  └─ hive/crud.rs → create_entry + Inbox::HiveInvite(self)
create_hive_membership(CreateHiveMembershipInput) → HiveMembershipResponse
  └─ hive/crud.rs → create_entry + Inbox::HiveInvite(grantee)
get_latest_membership(GetLatestMembershipInput) → Option<HiveMembershipResponse>
  └─ hive/queries.rs → walk Inbox::HiveInvite links (Network), filter by hive + unexpired
get_latest_membership_local(GetLatestMembershipInput) → Option<HiveMembershipResponse>
  └─ hive/queries.rs → same shape as above, GetStrategy::Local (dormancy-proof)
list_my_hives(()) → Vec<ListedHive>
  └─ hive/queries.rs → walk own Inbox::HiveInvite links (Network)
list_my_hives_local(()) → Vec<ListedHive>
  └─ hive/queries.rs → source-chain query() (founder) + local-store get_links (joiner); dormancy-proof
```

## Coordinator Externs — Group

```
create_group_genesis(CreateGroupGenesisInput) → GroupGenesisResponse
  └─ group/crud.rs → create_entry + HiveToGroups link + Inbox::GroupInvite(self)
create_group_membership(CreateGroupMembershipInput) → GroupMembershipResponse
  └─ group/crud.rs → create_entry + 3 discovery links + Inbox::GroupInvite(grantee)
revoke_group_membership(RevokeGroupMembershipInput) → GroupMembershipResponse
  └─ group/crud.rs → issues new membership with past expiry
get_group_genesis(ActionHash) → Option<GroupGenesisResponse>
get_latest_group_membership(GetLatestGroupMembershipInput) → Option<GroupMembershipResponse>
list_group_members(ActionHash) → Vec<GroupMembershipResponse>
  └─ group/queries.rs → GroupToGroupMemberships reverse index (cryptographic roster)
list_my_groups(()) → Vec<ListedGroup>
list_groups_in_hive(ActionHash) → Vec<ListedGroup>
```

## Coordinator Externs — Inbox

```
send_to_inbox(SendToInboxInput) → ActionHash
  └─ inbox/crud.rs → create_link(recipient, target, Inbox, event_byte)
consume_inbox_item(ActionHash) → ActionHash
  └─ inbox/crud.rs → delete_link
record_probe(RecordProbeInput) → ActionHash
  └─ inbox/crud.rs → private DmProbeLog entry
probe_inbox(ProbeInboxInput) → Vec<InboxItem>
  └─ inbox/queries.rs → get_links(my_pubkey, Inbox) + optional event filter
get_last_probe(()) → Option<DmProbeLog>
  └─ inbox/queries.rs → source-chain query for latest DmProbeLog
```

## Coordinator Externs — Signals

```
send_dm_delete_request(SendDmDeleteRequestInput) → ()    (C6, ephemeral)
send_dm_call_init_request(SendDmCallInitRequestInput) → ()  (C7, WebRTC)
send_dm_call_init_accept(SendDmCallInitAcceptInput) → ()    (C7, WebRTC)
send_dm_call_sdp_data(SendDmCallSdpDataInput) → ()         (C7, WebRTC)
```

All five remote-signal sends — the four `send_dm_*` above plus the
`remote_signal_acl_readers` content fan-out — funnel through
`send_encoded_remote_signal` → `remote_signal_payload`, which pre-encodes the
typed signal with `ExternIO::encode` so the recipient's
`recv_remote_signal(signal: ExternIO)` param decodes (the `#[hdk_extern]`
double-decode needs a BIN, not a typed MAP). Never call `send_remote_signal`
with a typed payload directly.

## Coordinator Externs — Migration

```
mark_migrated(MarkMigratedInput) → EncryptedContentResponse       (V1)
get_migration_marker(ActionHash) → Option<MigrationMarkerV1>       (V1)
mark_migrated_v2(MarkMigratedV2Input) → Option<EncryptedContentResponse>   (V2, fail-soft None on unresolvable original)
get_migration_marker_v2(ActionHash) → Option<MigrationMarker>      (V2, reads V1+V2)
```

## Coordinator Externs — Lifecycle

```
init(()) → InitCallbackResult            → set_cap_tokens()
recv_remote_signal(ExternIO)             → try-decode dispatcher (C1 anti-spoof)
post_commit(Vec<SignedActionHashed>)      → emit local Signal per committed action
get_messages_since(GetMessagesSinceInput) → Vec<Record>  (source-chain replay)
```

## Cap Grant Policy (set_cap_tokens)

Granted `Unrestricted`: all read-only queries + `recv_remote_signal`.
NOT granted (local-only): all `create_*/update_*/delete_*` mutators,
`get_messages_since`, `get_last_probe`, `send_dm_*` signal senders,
`mark_migrated*`.

## Key Files

```
coordinator/content/src/
  lib.rs                          (init, recv_remote_signal, post_commit, cap grants)
  encrypted_content/
    mod.rs                        (wire types: EncryptedContentResponse, CreateInput, UpdateInput)
    crud.rs                       (create/get/update/delete externs)
    queries.rs                    (list_by_*, count, fetch_pair)
    signals.rs                    (EncryptedContentSignal, DmRemoteSignal, send_dm_* externs, send_encoded_remote_signal funnel)
    get_helpers.rs                (get_eh, get_record, get_latest_typed_from_eh)
    migration.rs                  (MigrationMarkerV1/V2, mark_migrated*, get_migration_marker*)
  linking/
    hive_link.rs                  (create_hive_link — hive-shape Hive link)
    dynamic_links.rs              (create_dynamic_links — Dynamic links)
    acl_links.rs                  (create_acl_links — Owner/Admin/Writer/Reader fan-out)
    humm_content_id_link.rs       (create_humm_content_id_link)
  hive/
    crud.rs                       (create_hive_genesis, create_hive_membership)
    queries.rs                    (get_latest_membership[_local], list_my_hives[_local])
  group/
    crud.rs                       (create_group_genesis, create_group_membership, revoke)
    queries.rs                    (get_latest_group_membership, list_group_members, list_my_groups)
  inbox/
    crud.rs                       (send_to_inbox, consume_inbox_item, record_probe)
    queries.rs                    (probe_inbox, get_last_probe)
```
