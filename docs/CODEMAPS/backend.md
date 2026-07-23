<!-- codemap:backend | generated:2026-06-05 | updated:2026-07-23 | scope:full -->

# Backend (Zome Externs)

All externs live in coordinator zome `content`. Integrity zome `content_integrity`
has no callable externs (only `validate` + `genesis_self_check`).

> **Branch scope:** the pass-7 Wave-4 sections describe parked work on
> `feat-integrity-pass-7`. M16 moved the scratch DNA once to
> `uhC0k-HAqM4zW2rCWrKSujEKDZcqybE_ATUjKxkRy2BmRjURYddxP`
> (`content_integrity.wasm`
> `ec11ba8f9518cee6aee5d9e1df4fc1f7449f42584213abb4f8636cdceb90fcdd`);
> coordinator-only M17–M21 held it. This hash has never shipped. The shipped
> `.baseline-hashes.txt` line remains pass-6 v3.3.0 (`uhC0ksXs…`).

## Coordinator Externs — EncryptedContent CRUD

```
create_encrypted_content(CreateEncryptedContentInput) → EncryptedContentResponse
  └─ crud.rs → links: Hive(author) + Hive(hive) + Dynamic + ACL + ContentId + Inbox
get_encrypted_content(ActionHash) → EncryptedContentResponse
  └─ crud.rs → resolve_encrypted_content(Network) → get_eh → get_latest_typed_from_eh
get_many_encrypted_content(Vec<ActionHash>) → Vec<EncryptedContentResponse>
  └─ crud.rs → resolve_many_encrypted_content(Network), memoized by input hash; duplicate request rows remain
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

Legacy list reads collect link targets in `resolve_content_link_targets`, then
share `resolve_action_targets` → `resolve_many_encrypted_content` →
`resolve_encrypted_content`. `GetOptions` stays explicit along the lower chain,
which lets the Wave-4 local page twin read only the local store while every older
caller keeps `GetOptions::network()`. An unresolvable, gossip-lagged, or
tombstoned target is logged and skipped without aborting the remaining rows.

Membership and group walks share `get_typed_entry_with_timestamp`; absence or a
wrong entry shape returns `None` so a forged or lagging discovery link cannot
poison the whole list. `list_my_hives[_local]` and membership-index readers still
discriminate genesis targets by **EntryType** with `try_decode_hive_genesis`.
This prevents a `GroupGenesis`—a strict field superset—from decoding as a hive.

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

## Coordinator Externs — Service records (pass-6-service-meter, v3.3.0)

```
upsert_service_meter(UpsertServiceMeterInput) → UpsertContentResponse{response, was_created, was_updated}
  └─ service_records.rs → validate period/counters (canonical u128, ≤16 dims) → content_id_records_by_author(me, "service-meter-v1:<period>") → canonical_lowest_hash → create (dynamic link = period) | max-merge union + header convergence → update | no-op (NOT cap-granted)
publish_node_spec(PublishNodeSpecInput) → UpsertContentResponse{response, was_created, was_updated}
  └─ service_records.rs → validate spec (≤32 entries, no control chars) → optional app attestation (verify_signature_raw vs ACCEPTED_APP_SIGNING_KEYS_B64 — SHIPS EMPTY, all attestations reject) → find "node-spec-v1" → create | REPLACE + header convergence → update | no-op (NOT cap-granted)
```

Snapshots `ServiceMeterSnapshot{schema, period, counters}` /
`NodeSpecSnapshot{schema, spec, declared_at_micros, verified_by_app_key}` are
zome-built msgpack; reads ride the existing granted list/page externs.
Contract doc: `HUMM_TAURI_SERVICE_METER_INTEGRATION.md`.

## Coordinator Externs — Pass-7 Wave-4 reads (scratch, parked)

```
list_encrypted_content_by_dynamic_links(ListByDynamicLinksInput) → Vec<DynamicLinkBucket>
  └─ ≤64 labels; bounded first page per label; request-order buckets
list_by_hive_links_many(HiveLinksBatchInput) → Vec<HiveLinksBatchBucket>
  └─ ≤32 content-type requests under one hive; bounded first pages
get_many_by_content_id_link(Vec<ContentIdLookup>) → Vec<ContentIdResult>
  └─ ≤64 lookups; mirrors singleton first-target selection; unresolved record=None
list_by_author_many(Vec<AuthorContentLookup>) → Vec<AuthorBatchBucket>
  └─ ≤64 lookups; bounded oldest-first page per author
content_id_exists(ListByContentIdInput) → bool
  └─ link-set probe; resolves and returns no ciphertext record
get_latest_memberships_local_many(GetLatestMembershipsLocalManyInput) → Vec<LatestMembershipBucket>
  └─ ≤64 hives; caller derived from agent_info(); one Local membership-index walk
list_group_members_many(Vec<ActionHash>) → Vec<GroupMembersBucket>
  └─ ≤64 groups; complete rosters or a fail-closed aggregate-budget rejection
list_my_groups_local(()) → Vec<ListedGroup>
  └─ Local twin of list_my_groups; founded and granted rows keep singleton policy
list_by_hive_link_local_page(HiveLinkPageInput) → BoundedLinkPage
  └─ Local twin of list_by_hive_link_page for self-authored recovery
```

All nine externs are read-only and `Unrestricted`, beside the same-class
singleton reads in `set_cap_tokens`. Dynamic-link, hive-link, and author batches
normalize each item's limit (default 100, hard 256) and reject when the sum
exceeds `BATCH_RESOLVE_BUDGET = 4096`. `list_group_members_many` cannot truncate
ACL rosters, so it applies a separate 4096 roster-link budget before resolution.

Wave-4 helper spine:

| Helpers | Reason for the shared path |
|---|---|
| `resolve_content_link_targets` / `resolve_action_targets` / `resolve_many_encrypted_content` / `resolve_encrypted_content` | One `GetOptions`-threaded resolution policy prevents network and local twins from drifting; repeated input hashes resolve once per call. |
| `enforce_batch_resolve_budget` | Page batches cannot multiply the singleton's 4096-record resolution ceiling. |
| `get_typed_entry_with_timestamp`, `membership_index_links`, `my_hive_ids_network`, `cached_hive_display` | Hive reads share tolerant typed fetches, one membership-index shape, and immutable display lookup results while preserving row multiplicity. |
| `cached_group_genesis`, `group_roster_links`, `resolve_roster` | Group listings reuse immutable genesis values; singleton and batch rosters keep one strict newest-membership policy. |
| `create_acl_link_at` | Create and reindex paths emit the same validator-pinned base, tag, target, and link type. |
| integrity `AclByGroupGenesis::groups()` / `validate_expiry_containment` | Bucket walks keep owner-first ordering, and hive/group grant validators share one expiry-containment verdict. |

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
send_dm_delete_request(SendDmDeleteRequestInput) → ()       (C6, ephemeral)
send_dm_call_init_request(SendDmCallInitRequestInput) → ()  (C7, WebRTC)
send_dm_call_init_accept(SendDmCallInitAcceptInput) → ()    (C7, WebRTC)
send_dm_call_sdp_data(SendDmCallSdpDataInput) → ()          (C7, WebRTC)

emit_content_change(...) [internal]
  ├─ local emit_signal(EncryptedContentSignal{action_type, data, from_agent=None})
  └─ remote_signal_acl_readers(EncryptedContentHint{action_type, hash, original_hash})
initiate_owner_handoff(...) [existing mutator]
  └─ best-effort OwnerHandoffOfferHint{offer_hash, hive_genesis_hash} to recipient
```

Every remote sender passes an `ExternIO`-encoded payload through the shared
signal funnel before `send_remote_signal`; a typed map would fail the recipient
extern's second decode. Wave-4 removes ciphertext from the coordinator's remote
content fan-out while retaining the full `EncryptedContentSignal` for the
author's local UI. `recv_remote_signal` overwrites each hint's `from_agent` with
`call_info().provenance` before local emission. Its legacy full-content arm
remains decodeable, so any delivered signal with `from_agent: Some(_)` is an
untrusted fetch trigger and clients must resolve and verify the cited DHT entry.

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

Granted `Unrestricted`: public-DHT reads, self-scoped local twins, and
`recv_remote_signal`. Wave-4 adds all nine read externs:
`list_encrypted_content_by_dynamic_links`, `list_by_hive_links_many`,
`get_many_by_content_id_link`, `list_by_author_many`, `content_id_exists`,
`get_latest_memberships_local_many`, `list_group_members_many`,
`list_my_groups_local`, and `list_by_hive_link_local_page`.

NOT granted (local AppWebsocket only): source-chain mutators; owner-handoff,
revoke, and redeem mutators; private/local-chain readers
`get_messages_since`, `get_last_probe`, `my_pair_shared_secret_exists`, and
`changes_since`; caller-chosen-recipient `send_dm_*` / `send_blob_pin_signal`;
and `mark_migrated*`. The owner-offer hint adds no sender extern: it runs inside
the existing ungranted `initiate_owner_handoff`.

## Key Files

```
coordinator/content/src/
  lib.rs                          (init, provenance-stamping signal dispatcher, cap grants, get_typed_entry[_with_timestamp])
  encrypted_content/
    mod.rs                        (wire types: EncryptedContentResponse (+latest_action_micros), CreateInput, UpdateInput)
    crud.rs                       (create/get/update/delete externs)
    queries.rs                    (legacy lists + dynamic-link/content-id Wave-4 batches)
    paging.rs                     (bounded pages + hive/author batches + resolution/budget helpers)
    signals/                      (full local EncryptedContentSignal; remote EncryptedContentHint; DM + BlobPin families; ExternIO funnel)
    get_helpers.rs                (get_eh, record reuse, get_latest_typed_from_eh)
    migration/                   (MigrationMarkerV1/V2, mark_migrated*, get_migration_marker*)
  linking/
    hive_link.rs                  (create_hive_link — hive-shape Hive link)
    dynamic_links.rs              (create_dynamic_links — Dynamic links)
    acl_links.rs                  (create_acl_links/create_acl_link_at — Owner/Admin/Writer/Reader fan-out)
    humm_content_id_link.rs       (create_humm_content_id_link)
  hive/
    crud.rs                       (create_hive_genesis, create_hive_membership)
    queries.rs                    (membership-index reads; latest-membership batch; local/network hive lists; caches)
    owner.rs                      (owner handshake + OwnerHandoffOfferHint, resolve_current_owner, role reads, contest probe)
  group/
    crud.rs                       (create_group_genesis, create_group_membership, revoke)
    queries.rs                    (membership reads, roster batch, local/network group lists, genesis cache)
  inbox/
    crud.rs                       (send_to_inbox, consume_inbox_item, record_probe)
    queries.rs                    (probe_inbox, get_last_probe)
  invite.rs                       (redeem_invite_grant — advisory max_uses soft-cap)
```
