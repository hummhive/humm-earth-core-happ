<!-- codemap:dependencies | generated:2026-06-05 | updated:2026-06-08 | scope:full -->

# Dependencies

## Downstream Consumer

**humm-tauri** (`../humm-tauri/`) — desktop + headless Node application.
Consumes the built `.happ` bundle. Coordinator extern additions require
humm-tauri to update its bundled hApp to access new features. Integrity
changes require a full DNA migration (see `scripts/migrate-dna.ts`).

Integration path: `workdir/humm-earth-core-happ.happ` → `../humm-tauri/src-tauri/bin/humm-earth-core-happ.happ`.

## Holochain SDK (pinned versions)

| Crate | Version | Used By |
|---|---|---|
| `hdi` | =0.7.0 | integrity zome (validation, entry helpers, link types) |
| `hdk` | =0.6.0 | coordinator zome (externs, signals, DHT ops) |
| `holochain_serialized_bytes` | =0.0.56 | both zomes (SerializedBytes wire type) |
| `serde` | 1.0 | both zomes (de/serialization) |
| `base64` | 0.22 | coordinator only (decode AgentPubKey from Acl::reader) |

## Nix / Build Environment

| Dependency | Source | Purpose |
|---|---|---|
| holonix | `github:holochain/holonix/main-0.6` | Holochain toolchain (hc, holochain, lair) |
| hds-releases | `github:holo-host/hds-releases` | `holo-dev-server-bin` for Holo hosting dev |
| binaryen | nixpkgs | `wasm-opt` for `strip-wasms.sh` (DNA hash reproducibility) |

## Test Dependencies (tests/package.json)

| Package | Version | Purpose |
|---|---|---|
| `@holochain/client` | ^0.20.4 | Holochain client API (AppWebsocket, types) |
| `@holochain/tryorama` | ^0.19.2 | Multi-conductor test orchestration |
| `@msgpack/msgpack` | ^2.8.0 | Binary serialization (Holochain wire format) |
| `vitest` | ^0.28.4 | Test runner |
| `typescript` | ^4.9.4 | Type checking |

## Dev Dependencies (root package.json)

| Package | Purpose |
|---|---|
| `concurrently` | Run conductor + UI + playground in parallel |
| `@holochain-playground/cli` | DHT state visualizer |
| `new-port-cli` / `get-port-cli` | Dynamic port allocation for dev network |

## Migration Script (scripts/migrate-dna.ts)

Runtime: `npx tsx`. Uses `@holochain/client` (AdminWebsocket + AppWebsocket),
`node:fs/promises`, `node:path`. No additional npm deps beyond the test workspace.

## External Services

None. Fully peer-to-peer via Holochain DHT. No external databases, APIs,
or cloud services. Signal server (WebRTC) is Holochain's built-in `hc
run-local-services --signal-port`.

## Disabled / Commented-Out

| Dependency | Status | Reason |
|---|---|---|
| `p2p-shipyard` (Tauri plugin) | disabled | Private repo; contact darksoil-studio |
| `time_indexing` | disabled | Waiting for hdk-4.0 update |
| `zome_utils` | disabled | Waiting for hdk-4.0 update |

## Official hApp Versions

Prebuilt binaries for every DNA generation live at `~/hummhive-official-happ-versions/`
with `MANIFEST.tsv` mapping label → commit → DNA hash → SHA256 → filename.
Mirrored in `../humm-tauri/.testdata/happs/` for migration testing.
Current production: **pass-4-repro** (commit 8503b48, DNA uhC0k26b, hApp d74e5f2f).

## Build Artifacts

```
target/wasm32-unknown-unknown/release/
  content_integrity.wasm    → dnas/humm_earth_core/workdir/dna.yaml (integrity)
  content.wasm              → dnas/humm_earth_core/workdir/dna.yaml (coordinator)

workdir/humm-earth-core-happ.happ   → packaged by `hc app pack workdir --recursive`
```
