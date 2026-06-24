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

In-process holochain-0.6.1 Sweettest behavior tests for the `content` coordinator.
A **separate Cargo workspace** (its own `Cargo.lock`): the conductor crate drags a
large dep tree with no place in the lean wasm zome workspace; both now pin
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
`main` carries **v2.0.0 = pass-5-owner-role** (integrity bump, hc 0.6.1):
DNA `uhC0k2dXMIa1yI-V4ibCWMiTY5G6-p0laq6IOAVQ2F8XXReDHSxyS`, integrity wasm
`53d867f7…` (HELD), content wasm `48065345…`, happ
`42dbf9df56d88269f629651c1253d31bd2e5a664f3bdf44fe66256345034d361` (929643 bytes),
built at commit `834335e` (tag `v2.0.0` at `4e28a86`), distributed as
`…_pass-5-owner-role_dna-uhC0k2dX_happ-42dbf9df.happ` (the prior latent
`8f284777` build is DELETED). The current **live** production cell is still
**pass-4** (DNA uhC0k26b): v1.0.0 (`pass-4-query-tolerance`, hApp 2205337c) +
the v1.0.1 (`pass-4-migration-rescue`, hApp ca1b4225) coordinator hot-swap;
humm-tauri bundles the v2.0.0 pass-5 happ on cutover (integrity change → DNA migration).

## Build Artifacts

```
target/wasm32-unknown-unknown/release/
  content_integrity.wasm    → dnas/humm_earth_core/workdir/dna.yaml (integrity)
  content.wasm              → dnas/humm_earth_core/workdir/dna.yaml (coordinator)

workdir/humm-earth-core-happ.happ   → packaged by `hc app pack workdir --recursive`
```
