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

## Replaced legacy tests

The pre-pass-3 tryorama suite under `tests/src/humm_earth_core/content/`
was removed. Every file used `@holochain/tryorama`'s `runScenario`,
which cannot launch a holochain 0.6.0 conductor (the version gap above),
and its content/link shapes predated the pass-3 `acl_spec` reshape. The
security-critical, fetch-dependent validator branches they would have
covered now live in this harness; the remainder were read-path or
signal-delivery tests, out of scope for a commit-time validator suite.

| Removed file | Class | Where it's covered now |
|---|---|---|
| `encrypted-content.test.ts` | content CRUD | `scenarios/authority.ts` + `scenarios/variants.ts` (create / read / update across variants) |
| `validate-author.test.ts` | author binding | `scenarios/variants.ts` — M-1 self-update accept + cross-author reject |
| `fetch-pair-hive-check.test.ts` | DM pair-fetch | `scenarios/variants.ts` — DirectMessage accept + reject cases |
| `linking/acl_links.test.ts` | ACL link read (happy path) | ACL authority links (`HummContent{Owner,Admin,Writer,Reader}`) are created + validated at commit on every HiveGroup write — `scenarios/witnesses.ts`. The link *read* is a read path. |
| `linking/hive_link.test.ts` | hive link read | read path; the `Hive` link is created on hive-bound content in `authority.ts` / `variants.ts` |
| `linking/content_id_link.test.ts` | content-id link read | read path; `HummContentId` link created on every content create |
| `linking/dynamic_links.test.ts` | dynamic link read | read path; the `Dynamic` link is opt-in (`dynamic_links` field) |
| `count-by-hive.test.ts` | query | read path, not a validator; exercised by humm-tauri integration in production |
| `list-by-hive-pagination.test.ts` | query / pagination | read path; same as above |
| `remote-signal.test.ts` | signal delivery | out of scope (see "Not covered") |
| `recv-signal-dispatch.test.ts` | signal delivery | out of scope |
| `recv-signal-provenance.test.ts` | signal delivery | out of scope |
| `webrtc-signals.test.ts` | signal delivery | out of scope |
| `dm-delete-signal.test.ts` | signal delivery | out of scope |
| `common.ts` | shared helper | removed (orphaned once the above were gone) |

The `EncryptedContentUpdates` link validator (L-1: link author == base
author == target author, fetch-dependent) is exercised at commit by the
`scenarios/variants.ts` update cases — `update_encrypted_content` creates
that link as part of the update, so the self-update accepts it and the
cross-author update rejects the whole commit.

`tests/src/humm_earth_core/content/tryorama-pending.test.ts` is kept as
the port-back marker for when a tryorama release supports holochain
0.6.0's network grammar.

## Not covered (future work)

- **Signal delivery** — remote signals, recv-signal dispatch /
  provenance, WebRTC signals, DM-delete signals. These need a
  multi-conductor harness with real signal emission + receipt across
  separate conductors; the single-conductor, commit-time validator focus
  here does not exercise them.
- **Read-path queries** — count / list-by-hive, pagination, and the
  link-based read queries. Not security-critical; exercised by
  humm-tauri integration in production.
