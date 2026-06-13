# recv_remote_signal ExternIO Pre-Encode — Fix Handoff

**Date:** 2026-06-05
**Audience:** humm-tauri developers (reported in `RECV_REMOTE_SIGNAL_EXTERNIO_REPRODUCTION.md`, commit 3a4b71b)
**DNA version:** pass-4 (`uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`) — UNCHANGED
**Change class:** coordinator-only hot-swap (no chain fork)

---

## TL;DR

Fixed. Every `send_remote_signal` call site in the `content` coordinator now
pre-encodes its typed signal with `ExternIO::encode` before handing it to HDK,
so the recipient's `recv_remote_signal(signal: ExternIO)` parameter decodes
correctly instead of erroring with `WasmError::Deserialize` at the param
boundary. Cross-host pushes (DM/content notifications, C6 delete-request, C7
WebRTC signalling) now reach `emit_signal` on the recipient.

**Use this happ:** `pass-4-recv-signal-fix` — DNA `uhC0k26b…` (same as pass-4),
hApp sha256 `4aacd52fb15d9f1f27e9a09f78ee285adfa28b0088e56e666359065bf7641865`.
Available in `~/hummhive-official-happ-versions/` and `.testdata/happs/`.

Because the DNA hash is **unchanged**, this is a drop-in coordinator hot-swap:
no migration, no re-install, existing chains keep working. humm-tauri only needs
to bundle the new `.happ` to pick up working cross-host signals.

---

## Root cause (source-proven)

- HDK `send_remote_signal<I>(input, agents)` applies **exactly one**
  `ExternIO::encode(input)` (`hdk-0.6.0` `src/p2p.rs:154`).
- The `#[hdk_extern]` macro decodes a function's parameter via
  `map_extern_preamble!` (`hdi-0.7.0` `src/map_extern.rs:22-41`): `host_args`
  strips one msgpack-BIN layer, then — because the parameter type **is**
  `ExternIO` — `extern_io.decode::<ExternIO>()` strips a **second** BIN layer.
- `ExternIO` is `#[serde(with = "serde_bytes")]` (`holochain_integrity_types-0.6.0`
  `src/zome_io.rs:19`) → a msgpack **BIN**. A typed struct encodes to a msgpack
  **MAP**, which satisfies the first BIN decode but **fails the second**
  (`invalid type: map, expected byte array`).

So a typed signal sent directly arrived as a single-encoded MAP and was dropped
at the recipient's param boundary before any handler ran — exactly the captured
conductor log (`[130, 171, "action_type"]` = `0x82` fixmap + `"action_type"`).

This matches the ecosystem convention: moss (`group/src/lib.rs`) and presence
(`room/src/remote_signals.rs`) both pre-encode with `ExternIO::encode(payload)?`
at every send site, with the same `recv_remote_signal(signal: ExternIO)` +
`signal.decode::<T>()` architecture.

---

## The fix (DRY)

A single source of truth in `encrypted_content::signals`:

```rust
fn send_encoded_remote_signal<I>(signal: I, recipients: Vec<AgentPubKey>) -> ExternResult<()>
where I: Serialize + std::fmt::Debug {
    send_remote_signal(remote_signal_payload(&signal)?, recipients)
}

fn remote_signal_payload<I>(signal: &I) -> ExternResult<ExternIO>
where I: Serialize + std::fmt::Debug {
    ExternIO::encode(signal).map_err(|e| wasm_error!(e))
}
```

All five send sites route through `send_encoded_remote_signal` (no more inline
`send_remote_signal` with a typed payload, so the encode contract cannot drift
per call site):

1. `remote_signal_acl_readers` — content create/update/delete fan-out.
2. `send_dm_delete_request` (C6).
3. `send_dm_call_init_request` (C7).
4. `send_dm_call_init_accept` (C7).
5. `send_dm_call_sdp_data` (C7).

`recv_remote_signal` is **unchanged** — its ordered try-decode dispatcher
(`EncryptedContentSignal` then `DmRemoteSignal`) already `.decode::<T>()`s the
`ExternIO` param; with the pre-encode the param now decodes to the inner
`ExternIO` the dispatcher expects.

---

## Verification

- **Host wire-contract tests** (`encrypted_content::signals::tests`, deterministic,
  `cargo test -p content --lib`):
  - `content_signal_round_trips_through_send_path` — red→green driver: modelled
    the pre-fix single-encode wire shape and measured it fail with
    `invalid type: map, expected byte array`; now routes through
    `remote_signal_payload` and passes.
  - `dm_remote_signal_round_trips_through_send_path` — same proof for the C6/C7
    envelope.
  - `single_encode_payload_is_rejected_by_receiver_externio_param` —
    characterization guard locking the bug shut (single-encode MAP must NOT
    decode into the `ExternIO` param).
  - 25 coordinator + 69 integrity host tests green.
- **Reproducible build**: DNA hash held at `uhC0k26b…`; `content_integrity.wasm`
  byte-identical (`06b01fb3…`); `content.wasm` → `cb51c376…`; hApp → `4aacd52f…`.
  Verified byte-identical across two builds.
- **Conductor-level**: the in-repo tryorama suite cannot boot on hc 0.6.0
  (`unrecognized subcommand 'quic'` — the `quic`→`webrtc` sandbox-CLI rename),
  which is why the regression shipped. The authoritative end-to-end proof is
  humm-tauri's tryorama-free BDD `RS-1` in
  `tests/bdd/dm-remote-signal-delivery.test.ts`, which flips GREEN against the
  `pass-4-recv-signal-fix` happ.

---

## What humm-tauri should do

1. Bundle the `pass-4-recv-signal-fix` happ (`.testdata/happs/` has it; copy to
   `src-tauri/bin/humm-earth-core-happ.happ` when promoting to production).
2. Run BDD `RS-1` against it to confirm GREEN locally.
3. No code changes required on the humm-tauri side — the recipient-PUSH path
   starts working the moment the new coordinator wasm is loaded. DHT-polling
   fallback paths are unaffected.

## hApp artifact

| Field | Value |
|---|---|
| Label | `pass-4-recv-signal-fix` |
| DNA hash | `uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV` (unchanged) |
| content_integrity.wasm | `06b01fb3527e266a5cb1b5ffbf01b83541d7a572c4b4a252521154c3e0c2cd83` (unchanged) |
| content.wasm | `cb51c376a3e443ea4f580d0af6de603f04a08d80415d542b00ab9e5145acb3cf` (new) |
| hApp bundle | `4aacd52fb15d9f1f27e9a09f78ee285adfa28b0088e56e666359065bf7641865` (new) |
| Filename | `humm-earth-core-happ_pass-4-recv-signal-fix_dna-uhC0k26b_happ-4aacd52f.happ` |
