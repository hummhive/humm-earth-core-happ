<!-- codemap:architecture | generated:2026-06-05 | scope:full -->

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

## Security Model (4-pass evolution)

```
Pass 1: author-vs-header binding (check_author_matches_header)
Pass 2: cryptographic hive identity (HiveGenesis → HiveMembership chain)
Pass 3: group authority (GroupGenesis → GroupMembership) + AclSpec variants
Pass 4: recipient-witness integrity (RecipientWitness on HiveGroup entries)
```

## Data Flow

```
Writer UI → create_encrypted_content(input)
  ├─ commit EncryptedContent entry (DHT)
  ├─ create Hive/Dynamic/ACL/ContentId links (DHT paths)
  ├─ send_to_inbox → Inbox link on recipient pubkey (DHT)
  ├─ emit_signal (local UI)
  └─ remote_signal_acl_readers → send_remote_signal per reader (p2p)

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

All users are on pass-4 (current). Official binaries: `~/hummhive-official-happ-versions/`.

| Pass | DNA Hash (prefix) | Integrity Change? | Key Change |
|---|---|---|---|
| main-hc060 | uhC0ksx1N1sx | baseline | Initial hc 0.6 port |
| pass-1 | uhC0kb0T3Lrh | YES | Author-vs-header binding |
| pass-2 | uhC0kRHiJeJC | YES | Cryptographic hive identity |
| pass-2.5 | uhC0kRHiJeJC | no (coordinator) | Coordinator cleanup |
| pass-3 | uhC0k6pMjhrN | YES | Group authority + AclSpec |
| pass-4 | uhC0k26bYG0q | YES | Recipient-witness integrity (G-6.2) |
