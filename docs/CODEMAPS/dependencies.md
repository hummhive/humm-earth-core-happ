<!-- codemap:dependencies | generated:2026-06-05 | updated:2026-06-24 | scope:full -->

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
| `hdi` | =0.7.1 | integrity zome (validation, entry helpers, link types) |
| `hdk` | =0.6.1 | coordinator zome (externs, signals, DHT ops) |
| `holochain_integrity_types` | =0.6.1 | integrity zome (shared wire types) |
| `holochain_serialized_bytes` | =0.0.57 | both zomes (SerializedBytes wire type) |
| `serde` | 1.0 | both zomes (de/serialization) |
| `base64` | 0.22 | coordinator only (decode AgentPubKey from Acl::reader) |

## Nix / Build Environment

| Dependency | Source | Purpose |
|---|---|---|
| holonix | `github:holochain/holonix/main-0.6` | Holochain 0.6.1 toolchain (hc, holochain, lair, rustc 1.94) |
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

## Conductor Test Harness (crates/sweettest)

In-process holochain-0.6.1 Sweettest behavior tests for the `content` coordinator
and must-get-backed integrity paths. A **separate Cargo workspace** keeps the
conductor dep tree out of lean wasm builds; both workspaces now pin
`holochain_serialized_bytes =0.0.57`. Loads the pre-built DNA bundle, so it tests
whatever `npm run build:zomes` last produced. Run inside `nix develop` with
`LIBCLANG_PATH` set; transport is **iroh** (`transport-iroh`; tx5/datachannel
dropped in 0.6.1) so the devShell provides `openssl` + `pkg-config`. First compile
is slow (conductor + wasmer + iroh). Tryorama cannot boot on hc 0.6.x.

## External Services

None. Fully peer-to-peer via Holochain DHT. No external databases, APIs,
or cloud services. Signal server (WebRTC) is Holochain's built-in `hc
run-local-services --signal-port`.

## Disabled / Commented-Out

| Dependency | Status | Reason |
|---|---|---|
| `p2p-shipyard` (Tauri plugin) | disabled | Private repo; contact darksoil-studio |
| `zome_utils` | disabled | Waiting for hdk-4.0 update |

## Official hApp Versions

Prebuilt binaries for every DNA generation live at `~/hummhive-official-happ-versions/`
with `MANIFEST.tsv` mapping label → commit → DNA hash → SHA256 → filename.
Mirrored in `../humm-tauri/.testdata/happs/` for migration testing.
`dry-refactor` carries a **pass-6 candidate** (structural integrity refactor, hc
0.6.1): DNA `uhC0kOQX5rU8yL6CIEWAfGu1G5TaNsgMcS7yp-D0fV2eG1-2bA7iJ`, integrity wasm
`156d3ea2…`, content wasm `0d022f07…`, happ
`3dcb8827d7d45f3fabc68708862c4d379ed52d0b30f609ebed3f3b6dc8524d4e`, not released.
`main` remains **v2.0.0 = pass-5-owner-role**: DNA `uhC0k2dX…`, integrity wasm
`53d867f7…`, content wasm `48065345…`, happ `42dbf9df…`, built at `834335e` (tag
`v2.0.0` at `4e28a86`) and distributed as
`…_pass-5-owner-role_dna-uhC0k2dX_happ-42dbf9df.happ`. The current **live**
production cell is still **pass-4** (DNA uhC0k26b) until cutover/migration.

## Build Artifacts

```
target/wasm32-unknown-unknown/release/
  content_integrity.wasm    → dnas/humm_earth_core/workdir/dna.yaml (integrity)
  content.wasm              → dnas/humm_earth_core/workdir/dna.yaml (coordinator)

workdir/humm-earth-core-happ.happ   → packaged by `hc app pack workdir --recursive`
```
