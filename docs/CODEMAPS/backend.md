<!-- codemap:backend | generated:2026-06-05 | updated:2026-07-16 | scope:full -->

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
  └─ crud.rs → native update-root walk + update_entry + EncryptedContentUpdates link + OriginalHashPointer link
delete_encrypted_content(ActionHash) → ActionHash
  └─ crud.rs → delete_entry (I-A tombstone) + sweep author's discovery links (self-scoping local-chain CreateLink query)
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
  └─ queries.rs → Path([author_pubkey, content_type]) + since_ts + limit, oldest-first (pass-5)
get_by_content_id_link(ListByContentIdInput) → EncryptedContentResponse
  └─ queries.rs → Path([hive_genesis_hash, content_id])
fetch_pair_ss_with_hive_check(FetchPairWithHiveCheckInput) → Vec<EncryptedContentResponse>
  └─ queries.rs → intersect author-path ∩ dynamic-path (C4)
content_summary(ContentSummaryInput) → Vec<ContentTypeSummary>
  └─ queries.rs → per content_type: count + latest (action_micros + hash) (pass-5, humm-tauri)
my_pair_shared_secret_exists(PairSharedSecretExistsInput) → bool
  └─ queries.rs → LOCAL-chain check for the pair-SS dynamic link (pass-5; not cap-granted)
```

All list/get-many reads above resolve targets through `get_many_encrypted_content`,
which is **decode-tolerant** (`filter_map(.ok())`, pass-4-query-tolerance): an
unresolvable / gossip-lagged / tombstoned target is skipped, never poisoning the
batch. `list_my_hives[_local]` / `get_latest_membership[_local]` (+ the group
equivalents) discriminate the genesis target by **EntryType** via
`try_decode_hive_genesis` (`EntryTypes::deserialize_from_type` dispatch, v2.0.0):
`GroupGenesis` is a strict field-superset of `HiveGenesis`, so the old shape-decode
(`to_app_option`) silently false-positived every device-set / role-group as a
"hive"; the EntryType filter returns `None` for a non-`HiveGenesis` target (and
`warn!`-logs a corrupt recognised-type entry) instead of poisoning the list.

## Coordinator Externs — Bounded pages + exact-own (pass-6-pinned-hosts, v3.1.0)

```
list_by_hive_link_page(HiveLinkPageInput) → BoundedLinkPage
  └─ paging.rs → hive path, shared link_page engine (composite exclusive cursor)
list_by_dynamic_link_page(DynamicLinkPageInput) → BoundedLinkPage
  └─ paging.rs → dynamic path, same engine
list_by_author_page(AuthorLinkPageInput) → BoundedLinkPage
  └─ paging.rs → author path (Hive link type), same engine
get_my_content_by_id_link(MyContentByIdInput) → OwnContentRecords
  └─ paging.rs → author-scoped LinkQuery + retain, dedupe-by-target, 4096 saturation (NOT cap-granted)
send_blob_pin_signal(SendBlobPinSignalInput) → ()
  └─ signals/outbound.rs → BlobPinSignal (tag "pin") to ≤16 recipients via send_encoded_remote_signal (NOT cap-granted)
```

`BoundedLinkPage` = `{records, source_count, source_positions[{timestamp_micros,
action_hash}], truncated}` — positions are SOURCE truth (one per selected link,
targets resolved best-effort after the bound). Cursor: sort asc `(timestamp,
create_link_hash)`; `since_ts`+`source_after_action_hash` strictly exclusive;
`since_ts` alone inclusive; limit default 100 / hard cap 256.
`EncryptedContentResponse` additionally carries `latest_action_micros:
Option<i64>` (None on create responses). Contract doc:
`HUMM_TAURI_PINNED_HOSTS_INTEGRATION.md`.

## Coordinator Externs — Idempotent writes + remediation (pass-6-idempotent-writes, v3.2.0)

```
find_or_create_encrypted_content(CreateEncryptedContentInput) → FindOrCreateContentResponse{response, was_created}
  └─ crud.rs → header_from_input → hive_context gate → content_id_records_by_author(me) → canonical_lowest_hash | create_encrypted_content (NOT cap-granted)
find_or_create_group_genesis(CreateGroupGenesisInput) → FindOrCreateGroupGenesisResponse{response, was_created}
  └─ group/crud.rs → caller_matching_geneses (author-scoped HiveToGroups walk; role key, display_id for custom) | create_group_genesis (NOT cap-granted)
find_or_create_group_membership(CreateGroupMembershipInput) → FindOrCreateGroupMembershipResponse{response, was_created}
  └─ group/crud.rs → get_latest_group_membership same-role unexpired | create_group_membership (NOT cap-granted)
list_my_hiveless_content(String) → Vec<EncryptedContentResponse>
  └─ remediation.rs → list_by_author(me) + retain hive_context().is_none() (NOT cap-granted)
remediate_hiveless_content(RemediateHivelessInput) → Vec<RemediationOutcome>
  └─ remediation.rs → ≤64 items; per-item probe-first recreate+tombstone; create Err aborts whole call atomically (NOT cap-granted)
content_summary_many(Vec<ContentSummaryInput>) → Vec<HiveContentSummary>
  └─ queries.rs → ≤32 hives, ≤256 aggregate content types; order-preserving map of content_summary (cap-granted — only new grant)
```

Changed shapes: `FetchPairWithHiveCheckInput.active_hive_genesis_hash` is now
`Option<ActionHash>` (`#[serde(default)]`; `None` → union over the callee's
own hives via `pair_intersection` per hive). `mark_migrated_v2` /
`get_migration_marker_v2` accept HiveGenesis action hashes (create-based
founder-only marker, content-id `hive-migration-marker-v2`, content_type
`_migrated/hive-genesis`; entry-def-index dispatch via
`try_decode_hive_genesis` — GroupGenesis is a serde field-superset of
HiveGenesis). `send_dm_delete_request` + `DmRemoteSignal::DmDeleteRequest`
doc-deprecated. Contract doc: `HUMM_TAURI_IDEMPOTENT_WRITES_INTEGRATION.md`.

## Coordinator Externs — Hive

```
create_hive_genesis(CreateHiveGenesisInput) → HiveGenesisResponse
  └─ hive/crud.rs → create_entry + Inbox::HiveInvite(self)
create_hive_membership(CreateHiveMembershipInput) → HiveMembershipResponse
  └─ hive/crud.rs → create_entry + Inbox::HiveInvite(grantee); Admin grants require resolve_current_owner==caller (pass-5)
get_latest_membership(GetLatestMembershipInput) → Option<HiveMembershipResponse>
  └─ hive/queries.rs → walk Inbox::HiveInvite links (Network), filter by hive + unexpired
get_latest_membership_local(GetLatestMembershipInput) → Option<HiveMembershipResponse>
  └─ hive/queries.rs → same shape as above, GetStrategy::Local (dormancy-proof)
list_my_hives(()) → Vec<ListedHive>
  └─ hive/queries.rs → walk own Inbox::HiveInvite links
list_my_hives_local(()) → Vec<ListedHive>   (pass-4 rescue; dormancy-proof)
  └─ hive/queries.rs → own source-chain query() (founder, role=None) + Inbox links via GetStrategy::Local (joined); EntryType-filtered
get_member_hive_role(GetMemberHiveRoleInput) → Option<HiveRole>   (pass-5)
  └─ hive/owner.rs → resolve_current_owner==agent ? Owner : latest non-Owner membership
list_member_hive_roles(ListMemberHiveRolesInput) → Vec<(AgentPubKey, Option<HiveRole>)>   (pass-5)
  └─ hive/owner.rs → resolve owner once + per-agent role (batched, no N+1)
get_hive_owner(ActionHash) → AgentPubKey   (pass-5, humm-tauri)
  └─ hive/owner.rs → resolve_current_owner
is_ownership_contested(ActionHash) → bool   (pass-5, humm-tauri)
  └─ hive/owner.rs → true if the owner lineage has a fork
initiate_owner_handoff(InitiateOwnerHandoffInput) → ActionHash   (pass-5)
  └─ hive/owner.rs → HiveOwnerHandoffOffer + AgentToOwnerHandoffs link
accept_owner_handoff(AcceptOwnerHandoffInput) → ActionHash   (pass-5)
  └─ hive/owner.rs → HiveOwnerHandoffAccept + HiveToOwnerHandoffs link
cancel_owner_handoff(ActionHash) → ()   (pass-5)
  └─ hive/owner.rs → delete the offerer's AgentToOwnerHandoffs link
list_pending_owner_handoffs(()) → Vec<PendingOwnerHandoff>   (pass-5)
  └─ hive/owner.rs → get_links(my_pubkey, AgentToOwnerHandoffs)
revoke_hive_membership(RevokeHiveMembershipInput) → HiveMembershipResponse   (pass-5)
  └─ hive/crud.rs → re-issue with past expiry; refuses the current owner's membership
changes_since(ChangesSinceInput) → ChangesSinceSummary   (pass-5; not cap-granted)
  └─ hive/queries.rs → LOCAL-chain delta count for the hive's content paths
```

## Coordinator Externs — Group

```
create_group_genesis(CreateGroupGenesisInput) → GroupGenesisResponse
  └─ group/crud.rs → create_entry + HiveToGroups link + Inbox::GroupInvite(self)
create_group_membership(CreateGroupMembershipInput) → GroupMembershipResponse
  └─ group/crud.rs → create_entry + 3 discovery links + Inbox::GroupInvite(grantee)
revoke_group_membership(RevokeGroupMembershipInput) → GroupMembershipResponse
  └─ group/crud.rs → issues new membership with past expiry
delete_group_genesis(ActionHash) → ActionHash   (pass-5)
  └─ group/crud.rs → author-gated tombstone; refuses if live members; sweeps own links
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

## Coordinator Externs — Invite (pass-5)

```
redeem_invite_grant(RedeemInviteGrantInput) → HiveMembershipResponse
  └─ invite.rs → count InviteToRedemptions; advisory max_uses soft-cap; then create_hive_membership
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
get_messages_since(GetMessagesSinceInput) → Vec<Record>  (source-chain replay; since_seq=0 = full chain)
```

## Cap Grant Policy (set_cap_tokens)

Granted `Unrestricted`: all read-only DHT queries + `recv_remote_signal` + the
pass-5 public-DHT reads (`get_member_hive_role`, `list_member_hive_roles`,
`get_hive_owner`, `is_ownership_contested`, `content_summary`,
`list_pending_owner_handoffs`) + the pass-4 rescue's `_local` read twins
(`list_my_hives_local`, `get_latest_membership_local`).
NOT granted (local-only): all `create_*/update_*/delete_*` + the owner-handoff /
revoke / redeem mutators, `get_messages_since`, `get_last_probe`,
`my_pair_shared_secret_exists`, `changes_since`, `send_dm_*`, `mark_migrated*`.

## Key Files

```
coordinator/content/src/
  lib.rs                          (init, recv_remote_signal, post_commit, cap grants, get_typed_entry + delete_own_links_targeting helpers)
  encrypted_content/
    mod.rs                        (wire types: EncryptedContentResponse (+latest_action_micros), CreateInput, UpdateInput)
    crud.rs                       (create/get/update/delete externs)
    queries.rs                    (list_by_*, count, fetch_pair — legacy, wire-stable)
    paging.rs                     (bounded page externs + link_page/page_links engine + get_my_content_by_id_link)
    signals/                     (EncryptedContentSignal, DmRemoteSignal, BlobPinSignal (blob_pin.rs), send_dm_* + send_blob_pin_signal externs, ExternIO funnel)
    get_helpers.rs                (get_eh, get_record, get_latest_typed_from_eh)
    migration/                   (MigrationMarkerV1/V2, mark_migrated*, get_migration_marker*)
  linking/
    hive_link.rs                  (create_hive_link — hive-shape Hive link)
    dynamic_links.rs              (create_dynamic_links — Dynamic links)
    acl_links.rs                  (create_acl_links — Owner/Admin/Writer/Reader fan-out)
    humm_content_id_link.rs       (create_humm_content_id_link)
  hive/
    crud.rs                       (create_hive_genesis, create_hive_membership)
    queries.rs                    (get_latest_membership[_local], list_my_hives[_local], try_decode_hive_genesis EntryType discriminator)
    owner.rs                      (owner handshake, resolve_current_owner, role reads, is_ownership_contested)
  group/
    crud.rs                       (create_group_genesis, create_group_membership, revoke)
    queries.rs                    (get_latest_group_membership, list_group_members, list_my_groups)
  inbox/
    crud.rs                       (send_to_inbox, consume_inbox_item, record_probe)
    queries.rs                    (probe_inbox, get_last_probe)
  invite.rs                       (redeem_invite_grant — advisory max_uses soft-cap)
```
