<!-- codemap:architecture | generated:2026-06-05 | updated:2026-07-23 | scope:full -->

# Architecture

Core hApp for HummHive — a sovereign, encrypted content platform built on
Holochain. This repo produces the DNA (the "smart contract" layer); the
desktop/headless application lives in `../humm-tauri/`.

## Domain (see ../humm-tauri/GLOSSARY.md)

- **Hive** — a content namespace with Members, Groups, Sidecars, Content.
  Backed by one DNA. Cryptographic identity = `HiveGenesis` action hash.
- **Member** — a User admitted to a Hive, represented by one Agent per Node.
- **Group** — a named subset of Members scoping Content visibility.
- **Content** — anything a Member publishes (posts, DMs, sidecar records),
  typed by ContentType, scoped to Groups, encrypted at rest.
- **Sidecar** — pluggable feature module attached to a Hive (formerly "Connection").
- **Node** — a humm-tauri process on one device, with one Cell per Hive.

## System Diagram

```
humm-tauri (desktop + headless Node)
    │ AppWebsocket (call_zome)
    ▼
┌─────────────────────────────────────────────────────┐
│  humm-earth-core-happ  (1 DNA role: humm_earth_core)│
│                                                     │
│  ┌───────────────────┐  ┌─────────────────────────┐ │
│  │ content_integrity  │  │ content (coordinator)   │ │
│  │  INTEGRITY ZOME    │◄─│  COORDINATOR ZOME       │ │
│  │  ⚠ CHANGES FORK   │  │  hot-swappable          │ │
│  │    THE CHAIN       │  │  backwards-compat pref  │ │
│  └───────────────────┘  └─────────────────────────┘ │
│                                                     │
│  DHT (shared across all Peers in a Hive)            │
│  Source Chain (per-Agent, private entries)           │
└─────────────────────────────────────────────────────┘
```

## Change Gravity

| Layer | Impact | Policy |
|---|---|---|
| **Integrity zome** | Changes DNA hash → forks/splits the chain. All Nodes must migrate. | MUST NOT modify without significant cause + multi-user validation. |
| **Coordinator zome** | Hot-swappable. Does NOT change DNA hash. | Backwards-compatible preferred. humm-tauri must update hApp bundle to use new externs. |
| **Wire shapes** (input/output structs) | Breaking if fields removed/renamed. | Add with `#[serde(default)]`; remove only via versioned migration. |

## Project Type

Single Holochain hApp (not a monorepo). Two Cargo workspace members produce
two WASM zomes packaged into one DNA → one hApp bundle. The hApp is consumed
by humm-tauri (both GUI and headless modes) via `@holochain/client` AppWebsocket.

## Entry Points

| Layer | Entry Point | Purpose |
|---|---|---|
| WASM (integrity) | `integrity/.../lib.rs` | `validate()`, `genesis_self_check()` |
| WASM (coordinator) | `coordinator/.../lib.rs` | `init()`, `recv_remote_signal()`, `post_commit()`, all `#[hdk_extern]` |
| Conductor tests | `crates/sweettest/tests/*.rs` | In-process Holochain behavior tests; includes Wave-4 batch reads and signal hints |
| Legacy tests | `tests/src/**/**.test.ts` | Tryorama/Vitest sources; Tryorama cannot boot against hc 0.6.x |
| Migration | `scripts/migrate-dna.ts` | CLI: export → migrate-hive → import → mark-migrated |
| Build | `scripts/build-zomes.sh` | Reproducible WASM build + `strip-wasms.sh` |
| Dev env | `flake.nix` | Holonix 0.6 devShell |

## Security Model (pass lineage)

```
Pass 1: author-vs-header binding (check_author_matches_header)
Pass 2: cryptographic hive identity (HiveGenesis → HiveMembership chain)
Pass 3: group authority (GroupGenesis → GroupMembership) + AclSpec variants
Pass 4: recipient-witness integrity (RecipientWitness on HiveGroup entries)
Pass 5: hive Owner role (offer/accept handshake) + reader read-only + role-grant hardening
v2.0.0:  GroupGenesis EntryType filter (try_decode_hive_genesis) — closes the
         HiveGenesis false-positive; pass-4 rescue _local twins ride along
Pass 6:  directory-module split + OriginalHashPointer link validation (native
         update-chain root binding) + cross-entry-type update gate
Pass 7:  SCRATCH ONLY — bounded headers/ACLs, cross-generation Lineage,
         durable HiveMembershipIndex, disjoint group ACL buckets, and
         ciphertext-free remote content hints
```

## Data Flow

```
Writer UI → create_encrypted_content(input)
  ├─ commit EncryptedContent entry (DHT)
  ├─ create Hive/Dynamic/ACL/ContentId links (DHT paths)
  ├─ send_to_inbox → Inbox link on recipient pubkey (DHT)
  ├─ emit_signal → full EncryptedContentSignal (local author only)
  └─ remote_signal_acl_readers → EncryptedContentHint per reader (identifiers only)

Owner UI → initiate_owner_handoff → commit offer + OwnerHandoffOfferHint to recipient
Reader UI (online)  ← recv_remote_signal stamps call provenance → re-query DHT
Reader UI (offline) ← probe_inbox → resolve target → get entry
```

## Pass-7 Wave-4 Read Surface (scratch only)

Wave-4 collapses repeated client calls into nine read-only coordinator externs:

- Content reads: `list_encrypted_content_by_dynamic_links`,
  `list_by_hive_links_many`, `get_many_by_content_id_link`,
  `list_by_author_many`, and `content_id_exists`.
- Membership, roster, and local reads: `get_latest_memberships_local_many`,
  `list_group_members_many`, `list_my_groups_local`, and
  `list_by_hive_link_local_page`.

Each extern has the same `Unrestricted` grant class as its singleton twin.
Page-based batches return only a bounded first page per item and share a 4096
aggregate resolution budget; `list_group_members_many` instead rejects when its
complete rosters exceed 4096 source links. Item caps are 64, except the
32-request hive-link batch.

Network and local reads share one options-threaded resolution chain:
`resolve_content_link_targets` / `resolve_action_targets` →
`resolve_many_encrypted_content` → `resolve_encrypted_content`. Existing reads
select network options; the two new local twins select local options, so adding
the shared chain does not change an older extern's consistency mode.

## Integration with humm-tauri

Build output: `workdir/humm-earth-core-happ.happ` (gitignored).
Deployment: copy to `../humm-tauri/src-tauri/bin/humm-earth-core-happ.happ`.
humm-tauri embeds this in the Tauri binary for both GUI and headless Node modes.

humm-tauri's `scripts/wsl-relay/` provides headless conductor orchestration
for cross-repo integration testing against this hApp.

## hApp Version Lineage

Current: **`main` = v3.3.0 = pass-6-service-meter**, DNA `uhC0ksXs` (HELD
from pass-6/v3.0.0 — coordinator-only generation), happ `b98916f1`, merged
2026-07-17 (merge `311e10c`, tag `v3.3.0`). Prior coordinator generations:
**v3.2.0 = pass-6-idempotent-writes**, same DNA, happ `bfe357aa` (merge
`b5c830f`); **v3.1.0 = pass-6-pinned-hosts**, same DNA, happ `1c7d981b`
(merge `e16b793`).
Integrity baseline: **v3.0.0 = pass-6-dry-refactor**, happ `3062de38`, blessed
2026-07-02 (merge `2de8923`, tag `v3.0.0`; zome-source tip `a07dc99`).
Prior: **v2.0.0 = pass-5-owner-role**, DNA `uhC0k2dX`, happ `42dbf9df`, built at
`834335e` (tag `v2.0.0` @ `4e28a86`) — the migration source generation. Official
released binaries live in
`~/hummhive-official-happ-versions/`. Conductor behavior is proven in-process via
`crates/sweettest` (tryorama can't boot on hc 0.6.x).

The pass-7 lineage below is a **scratch, parked branch**, not a release.
M16 changed the DNA once to
`uhC0k-HAqM4zW2rCWrKSujEKDZcqybE_ATUjKxkRy2BmRjURYddxP`
(`content_integrity.wasm` sha256
`ec11ba8f9518cee6aee5d9e1df4fc1f7449f42584213abb4f8636cdceb90fcdd`).
M17–M21 changed only the coordinator and held that pin. No pass-7 artifact was
distributed or added to the official version store. The shipped baseline in
`.baseline-hashes.txt` remains pass-6 v3.3.0 (`uhC0ksXs…`, hApp
`b98916f1…`).

| Pass | DNA Hash (prefix) | Integrity Change? | Key Change |
|---|---|---|---|
| main-hc060 | uhC0ksx1N1sx | baseline | Initial hc 0.6 port |
| pass-1 | uhC0kb0T3Lrh | YES | Author-vs-header binding |
| pass-2 | uhC0kRHiJeJC | YES | Cryptographic hive identity |
| pass-2.5 | uhC0kRHiJeJC | no (coordinator) | Coordinator cleanup |
| pass-3 | uhC0k6pMjhrN | YES | Group authority + AclSpec |
| pass-4 | uhC0k26bYG0q | YES | Recipient-witness integrity (G-6.2) |
| pass-4-recv-signal-fix | uhC0k26bYG0q | no (coordinator) | recv_remote_signal ExternIO pre-encode (DNA held) |
| pass-4-query-tolerance (v1.0.0) | uhC0k26bYG0q | no (coordinator) | decode-tolerant queries: `get_many` filter_map + `list_my_hives`/`_groups` `.ok().flatten()` (DNA held) |
| pass-4-migration-rescue (v1.0.1) | uhC0k26bYG0q | no (coordinator) | dormancy rescue: `_local` read twins (`list_my_hives_local`, `get_latest_membership_local`) + `mark_migrated_v2` fail-soft + EntryType GroupGenesis filter (`try_decode_hive_genesis`); DNA held |
| pass-5-owner-role (v2.0.0) | uhC0k2dXMIa1 | YES | Hive Owner role (offer/accept handshake) + reader read-only + role-grant hardening + GroupGenesis EntryType filter + pass-4 rescue `_local` externs merged onto main; hc 0.6.1 |
| pass-6-dry-refactor (v3.0.0) | uhC0ksXsJOT | YES | Structural module split (`encrypted_content`, `hive`, `group`, `validation_dispatch`; coordinator `migration`/`signals` dirs) plus `OriginalHashPointer` and cross-entry-update validation hardening; no EntryTypes/LinkTypes/wire changes |
| pass-6-pinned-hosts (v3.1.0) | uhC0ksXsJOT | no (coordinator) | `latest_action_micros`, `BlobPinSignal` + `send_blob_pin_signal`, bounded source-cursor page externs (`list_by_{hive_link,dynamic_link,author}_page`), exact-own `get_my_content_by_id_link`; DNA held |
| pass-6-idempotent-writes (v3.2.0) | uhC0ksXsJOT | no (coordinator) | find-or-create family (content/group-genesis/membership), hiveless remediation pair, optional-hive `fetch_pair_ss_with_hive_check`, HiveGenesis CREATE-based migration markers, `content_summary_many`; DNA held |
| pass-6-service-meter (v3.3.0) | uhC0ksXsJOT | no (coordinator) | `upsert_service_meter` (cumulative max-merge day buckets) + `publish_node_spec` (opt-in singleton, REPLACE, dormant app-attestation behind empty accepted-keys), header convergence on upsert, CI cutover to host tests + build + sweettest; DNA held |
| pass-7 Wave-4 scratch (PARKED) | uhC0k-HAqM4z | YES at M16 only | M16 integrity DRY moved the scratch pin once; M17–M21 added bounded reads and hint-only remote signals as coordinator-only changes; undistributed |
