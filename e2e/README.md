# e2e — manual single-conductor integration suite

Live, tryorama-free end-to-end tests for the pass-4 `humm_earth_core`
DNA. Boots a **real holochain 0.6.0 conductor** with a fresh in-process
lair keystore + a throwaway `--data-dir`, installs the packed `.happ`
for three agents on **one shared DHT**, and drives the `content` zome
over the normal AppWebsocket — exactly the surface humm-tauri uses.

## Why not tryorama?

The published `@holochain/tryorama` (0.19.2, the latest on npm) creates
conductor configs via `hc sandbox create network quic …`. Holochain
0.6.0 — the installed toolchain — **removed the `quic` transport
subcommand** (`hc sandbox create network` now offers `mem` / `webrtc`).
Tryorama 0.19.2 cannot launch a conductor here, and no newer tryorama
is published. Rather than pin an older holochain (which would invalidate
the pass-4 DNA hash baseline), this harness spawns the conductor
directly and talks to it with `@holochain/client` (the same library the
migration script uses). When tryorama catches up, the
`tests/.../tryorama-pending.test.ts` stub documents the port-back path.

## How it works

- `conductor.ts` — `E2EConductor`: writes a conductor config with a free
  admin port (the format mirrors `hc sandbox generate`), spawns
  `holochain --piped -c <config>`, waits for `Conductor ready`, then
  installs + enables the happ for N agents (`generateAgentPubKey` →
  `installApp` with a shared `network_seed` → `enableApp` →
  `authorizeSigningCredentials`). All agents share one DHT, so
  cross-agent `must_get_valid_record` validation works offline with no
  bootstrap/signal networking. Teardown kills the process group + deletes
  the temp dir.
- `acl.ts` — pure pass-4 wire-shape builders (`aclSpec*`, `witness`,
  `groupAcl`, …) + a tiny `step` / `expectReject` / `report` framework.
- `ops.ts` — typed `content`-zome call wrappers (`createHiveGenesis`,
  `createGroupMembership`, `createContent`, …).
- `scenarios/*.ts` — the state-gated scenario groups.
- `run.ts` — boots one conductor + alice/bob/carol, runs every scenario
  group in order, prints a tally, exits non-zero on any failure.

## Run

From the repo root (after `npm run build:happ` so the `.happ` exists):

```bash
npx tsx e2e/run.ts
```

or from this dir: `npm run e2e`. A full run is ~12s. Override the happ
path or binary with `HUMM_HAPP_PATH` / `HOLOCHAIN_BIN` env vars.

## Coverage (30 scenarios)

These are the fetch-dependent branches host-side `cargo test` cannot
reach (each needs a live `must_get_valid_record`):

| Group | Scenarios |
|---|---|
| hive authority | Path 1 (genesis author), Path 2 (granted Writer), non-member reject, wrong-hive-witness reject |
| group authority | Path A (group author), Path B (hive sovereign), Path C (group member), hive-Writer-non-group-member reject |
| AclSpec variants | DM accept + author-not-in-recipients/reader-mismatch/cardinality rejects; Public post; OpenWrite target=None/real-target accept + fake-target reject |
| update binding | M-1 self-update accept + cross-author reject |
| G-6.2 witnesses | fully-witnessed accept; missing-witness reject (attack #5); over-claim reject; bucket-dominance accept; expired-witness reject |
| G-4.4 windows | group + hive layers: expiring grantor cannot mint permanent / extend window; in-window grant accepted |
| invite links | E.4.l publish → outsider read → redeem → mint → joined |

## Caveats

- Requires `holochain` + `lair-keystore` 0.6.x on `PATH` and the packed
  `workdir/humm-earth-core-happ.happ`.
- One conductor, three agents, one shared DHT — covers single- and
  cross-agent validation. It does NOT exercise real peer-to-peer
  networking / gossip across separate conductors (not needed for
  commit-time validation, which is what every scenario asserts).
- Scenarios use distinct hives/groups so they stay independent within
  the single shared conductor.
