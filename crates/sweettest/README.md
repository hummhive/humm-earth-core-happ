# humm-sweettest — in-tree conductor tests

In-process holochain 0.6.0 (Sweettest) behavior tests for the `content`
coordinator zome. This is a **separate Cargo workspace** on purpose: the
`holochain` conductor crate requires `holochain_serialized_bytes =0.0.57`
while the zome workspace pins `=0.0.56` — sharing a workspace conflicts.

Why this exists: in-repo **tryorama cannot boot** on the flake's `hc 0.6.0`
(the `quic`→`webrtc` sandbox-CLI rename — see `.baseline-hashes.txt`).
Sweettest spawns the conductor in-process (no `hc sandbox` CLI), matches
the flake conductor exactly (no version drift), and is the official test
path (Holochain Dev Pulse 154). It loads the pre-built DNA bundle, so the
coordinator under test is whatever `npm run build:zomes` last produced.

## Prerequisites

- Run inside the repo's `nix develop` shell (provides rust, cmake, C++ for
  `libdatachannel`, and `clang`).
- Build the DNA first so the bundle exists:
  `npm run build:zomes && hc app pack workdir --recursive`
- Export `LIBCLANG_PATH` to the nix `clang` lib dir — `datachannel-sys`'s
  bindgen otherwise falls back to a broken system clang. Find it with:
  `find /nix/store -maxdepth 3 -name 'libclang.so' | head -1` → use its dir.

## Run

```bash
cd crates/sweettest
nix develop ../.. --command bash -c '
  export LIBCLANG_PATH=<nix-clang-lib-dir>
  cargo test -- --test-threads=1 --nocapture
'
```

First compile is long (~15-40 min cold: full conductor + wasmer +
libdatachannel). Subsequent runs are fast. Tests use
`#[tokio::test(flavor = "multi_thread")]` (mandatory — the conductor
deadlocks single-threaded) and an in-memory keystore (no external lair).

## Tests (`tests/coordinator_query_tolerance.rs`)

- `get_many_encrypted_content_tolerates_a_missing_target` — pins the
  all-or-nothing fix: `get_many_encrypted_content([unresolvable_hash])`
  returns `Ok([])` (the resolvable subset) instead of the pre-fix
  `"no Record found at given hash"` batch throw that poisoned
  `list_by_hive_link` / `_dynamic_link` / `_acl_link` / `_author`.
- `joiner_lists_hive_without_cross_type_decode_failure` — 2-conductor
  rendezvous: Alice founds a hive + grants Bob a membership; Bob's
  `list_my_hives` returns the joined hive (and Alice still lists hers,
  role `None`). Pins the cross-type Inbox decode fix (`.ok().flatten()`
  instead of `?`-propagating the wrong-type deserialize).

Both verified green (2/2) against the pass-4-query-tolerance coordinator.
