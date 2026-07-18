<!-- codemap:architecture | generated:2026-06-05 | updated:2026-07-17 | scope:full -->

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
| Tests | `tests/src/**/**.test.ts` | Tryorama integration tests via Vitest |
| Migration | `scripts/migrate-dna.ts` | CLI: export → migrate-hive → import → mark-migrated |
| Build | `scripts/build-zomes.sh` | Reproducible WASM build + `strip-wasms.sh` |
| Dev env | `flake.nix` | Holonix 0.6 devShell |

## Security Model (6-pass evolution)

```
Pass 1: author-vs-header binding (check_author_matches_header)
Pass 2: cryptographic hive identity (HiveGenesis → HiveMembership chain)
Pass 3: group authority (GroupGenesis → GroupMembership) + AclSpec variants
Pass 4: recipient-witness integrity (RecipientWitness on HiveGroup entries)
Pass 5: hive Owner role (offer/accept handshake) + reader read-only + role-grant hardening
v2.0.0:  GroupGenesis EntryType filter (try_decode_hive_genesis) — closes the
         HiveGenesis false-positive; pass-4 rescue _local twins ride along
Pass 6:  directory-module split + OriginalHashPointer link validation (native
         update-chain root binding) + cross-entry-type update gate; no
         EntryTypes, LinkTypes, serde tags, entry fields, or wire-shape changes
```

## Data Flow

```
Writer UI → create_encrypted_content(input)
  ├─ commit EncryptedContent entry (DHT)
  ├─ create Hive/Dynamic/ACL/ContentId links (DHT paths)
  ├─ send_to_inbox → Inbox link on recipient pubkey (DHT)
  ├─ emit_signal (local UI)
  └─ remote_signal_acl_readers → send_encoded_remote_signal per reader (p2p, ExternIO pre-encode)

Reader UI (online)  ← recv_remote_signal → re-query DHT
Reader UI (offline) ← probe_inbox → resolve target → get entry
```

## Integration with humm-tauri

Build output: `workdir/humm-earth-core-happ.happ` (gitignored).
Deployment: copy to `../humm-tauri/src-tauri/bin/humm-earth-core-happ.happ`.
humm-tauri embeds this in the Tauri binary for both GUI and headless Node modes.

humm-tauri's `scripts/wsl-relay/` provides headless conductor orchestration
for cross-repo integration testing against this hApp.

## hApp Version Lineage

Current: **`main` = v3.2.0 = pass-6-idempotent-writes**, DNA `uhC0ksXs` (HELD
from pass-6/v3.0.0 — coordinator-only generation), happ `bfe357aa`, merged
2026-07-16 (merge `b5c830f`, tag `v3.2.0`). Prior coordinator generation:
**v3.1.0 = pass-6-pinned-hosts**, same DNA, happ `1c7d981b` (merge `e16b793`).
Integrity baseline: **v3.0.0 = pass-6-dry-refactor**, happ `3062de38`, blessed
2026-07-02 (merge `2de8923`, tag `v3.0.0`; zome-source tip `a07dc99`).
Prior: **v2.0.0 = pass-5-owner-role**, DNA `uhC0k2dX`, happ `42dbf9df`, built at
`834335e` (tag `v2.0.0` @ `4e28a86`) — the migration source generation. Official
released binaries live in
`~/hummhive-official-happ-versions/`. Conductor behavior is proven in-process via
`crates/sweettest` (tryorama can't boot on hc 0.6.x).

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
