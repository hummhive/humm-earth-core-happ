<!-- codemap:dependencies | generated:2026-06-05 | updated:2026-07-23 | scope:full -->

# Dependencies

## Downstream Consumer

**humm-tauri** (`../humm-tauri/`) — desktop + headless Node application.
It consumes the packaged `.happ` and calls coordinator externs over
AppWebsocket. Pass-7 Wave-4 adds nine read-only calls for content, membership,
group-roster, and local-store batching. It also changes the coordinator's
cross-host content fan-out from the full `EncryptedContentSignal` to
`EncryptedContentHint` and adds `OwnerHandoffOfferHint`; humm-tauri must add both
hint shapes and fetch-verify any remote content reference at blessing.

Integration path: `workdir/humm-earth-core-happ.happ` →
`../humm-tauri/src-tauri/bin/humm-earth-core-happ.happ`. Wire contract:
`docs/HUMM_TAURI_WAVE4_INTEGRATION.md`.

## Holochain SDK (pinned versions)

| Crate | Version | Used By |
|---|---|---|
| `hdi` | =0.7.1 | integrity zome (validation, entry helpers, link types) |
| `hdk` | =0.6.1 | coordinator zome (externs, signals, DHT ops) |
| `holochain_integrity_types` | =0.6.1 | integrity zome (shared wire types) |
| `holochain_serialized_bytes` | =0.0.57 | both zomes (SerializedBytes wire type) |
| `serde` | 1.0 | both zomes (de/serialization) |
| `base64` | 0.22 | coordinator only (decode AgentPubKey from Acl::reader) |

## Pass-7 Wave-4 Dependency Delta (scratch)

Wave-4 adds no Rust crate, npm package, external service, or persistent store.
Its batch limits, `GetOptions`-threaded resolver chain, immutable-entry caches,
roster helpers, and signal hints all rely on the pinned HDK/serde surface above.
The only downstream dependency change is API adoption in humm-tauri after
pass-7 receives a blessing.

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

In-process holochain-0.6.1 Sweettest behavior tests cover the `content`
coordinator and must-get-backed integrity paths. The separate Cargo workspace
keeps conductor dependencies out of lean wasm builds; both workspaces pin
`holochain_serialized_bytes =0.0.57`. Wave-4 adds `batch_reads.rs` for all nine
read externs, ordering, caps, local parity, and complete-roster budgets, plus
`signal_hints.rs` for ciphertext-free remote hints, local full payloads, and
provenance stamping. The full scratch suite is green.

Tests load the pre-built DNA bundle. Run them inside `nix develop` with
`LIBCLANG_PATH` set; transport is **iroh** (`transport-iroh`; tx5/datachannel
dropped in 0.6.1), so the devShell supplies `openssl` and `pkg-config`.
Tryorama cannot boot on hc 0.6.x.

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

Prebuilt release binaries live at `~/hummhive-official-happ-versions/`, with
`MANIFEST.tsv` mapping label → commit → DNA hash → SHA256 → filename. The
shipped/current row remains **pass-6-service-meter v3.3.0** (merge `311e10c`,
tag `v3.3.0`): DNA
`uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz`, integrity wasm
`2656a9100937f7e6d17e2eebd5e744a1ef16e8e36b0efa089dc2f6382a655ae2`,
content wasm
`34676ba03911cda4bbbd3a26852c81003f2aa60d61b3b86b2ed4948fbd98d008`,
and hApp sha256
`b98916f18def33731a93b70c36f60838186a52e4e41efcd58de5071f150430c8`.

The pass-7 Wave-4 value is a **scratch pin only**. M16 moved the branch DNA once
to `uhC0k-HAqM4zW2rCWrKSujEKDZcqybE_ATUjKxkRy2BmRjURYddxP` with integrity wasm
sha256 `ec11ba8f9518cee6aee5d9e1df4fc1f7449f42584213abb4f8636cdceb90fcdd`;
coordinator-only M17–M21 held it. The branch is parked and undistributed: no
Wave-4 row or binary belongs in the official store or humm-tauri bundle before
blessing, and `.baseline-hashes.txt` remains on the shipped pass-6 line.

## Build Artifacts

```
target/wasm32-unknown-unknown/release/
  content_integrity.wasm    → dnas/humm_earth_core/workdir/dna.yaml (integrity)
  content.wasm              → dnas/humm_earth_core/workdir/dna.yaml (coordinator)

workdir/humm-earth-core-happ.happ   → packaged by `hc app pack workdir --recursive`
```
