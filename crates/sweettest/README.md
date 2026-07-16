# humm-sweettest — in-tree conductor tests

In-process Holochain 0.6.1 (Sweettest) behavior tests for the `content`
coordinator zome and must-get-backed integrity paths. This remains a separate
Cargo workspace because it depends on the full `holochain` conductor crate; the
zome workspace shares the current `holochain_serialized_bytes =0.0.57` line but
should not pull conductor-only dependencies into WASM builds.

Why this exists: in-repo tryorama cannot boot on the flake's hc 0.6.x line, while
Sweettest spawns the conductor in-process and matches the flake conductor without
sandbox CLI drift. It loads the pre-built DNA bundle, so the code under test is
whatever `npm run build:zomes` last produced.

## Prerequisites

- Run inside the repo's `nix develop` shell (provides Rust, cmake, C++ toolchain,
  OpenSSL/pkg-config, and clang for iroh-era conductor dependencies).
- Build the DNA first so the bundle exists:
  `npm run build:zomes && hc app pack workdir --recursive`
- Export `LIBCLANG_PATH` to the nix `clang` lib dir. Bindgen otherwise may pick a
  broken system clang. Example known-good path in this WSL environment:
  `/nix/store/v09fr9r4ma9qxiwv2mfbanha28aiwwc5-clang-18.1.8-lib/lib`.

## Run

```bash
cd crates/sweettest
nix develop ../.. --command bash -c '
  export LIBCLANG_PATH=<nix-clang-lib-dir>
  cargo test -- --test-threads=1 --nocapture
'
```

First compile is long because it builds the conductor stack and Wasmer.
Subsequent runs are fast. Tests use `#[tokio::test(flavor = "multi_thread")]`
and an in-memory keystore.

## Active tests

21 active tests + 1 ignored dormancy differential:

- `coordinator_cleanup.rs`: delete-link sweep and `get_messages_since(0)` replay.
- `coordinator_query_tolerance.rs`: missing-target tolerance and mixed Inbox decode.
- `migration_rescue.rs`: local hive enumeration, local joiner enumeration,
  `mark_migrated_v2` fail-soft, plus one ignored live-network dormancy tripwire.
- `owner_and_acl.rs`: owner handoff, owner resolution, owner revoke protection,
  and hive grant-window containment.
- `recipient_witnesses.rs`: HiveGroup recipient witness accepts a real group
  membership through a live conductor.
- `pinned_hosts.rs`: bounded source-cursor paging (hive/dynamic/author walks
  without dupes or skips), exact-own content-id lookup (foreign-collision and
  hive scoping), `latest_action_micros` recency (None on create, advances on
  update), BlobPinSignal dispatch + provenance stamping (direct extern call,
  junk-payload rejection, cross-agent delivery), and the legacy
  `list_by_hive_link` `since_ts`/`limit` watermark sweep proof.

Shared wire mirrors and conductor helpers live in `tests/support/mod.rs`.
