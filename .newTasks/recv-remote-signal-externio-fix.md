# Task: recv_remote_signal drops every cross-host push (ExternIO double-encode)

Status: DONE (committed e0e7ed7; pushed to Windows mount; awaiting user `git push` to origin)
Branch: feat-self-notes-architecture
Scope: coordinator-only (content zome). Integrity untouched -> DNA hash HELD uhC0k26b.

## Problem (reported by humm-tauri devs, commit 3a4b71b)
recv_remote_signal(signal: ExternIO) dropped every cross-host push. Senders passed
a typed signal to send_remote_signal; HDK applies one ExternIO::encode, so the
receiver's #[hdk_extern] param-decode into ExternIO saw a msgpack MAP and failed
(WasmError::Deserialize) before the dispatcher ran.

## Root cause (SOURCE-PROVEN)
- hdk-0.6.0 p2p.rs:154 -> ONE ExternIO::encode(input).
- hdi-0.7.0 map_extern.rs:22-41 -> double-decode; param IS ExternIO -> 2nd decode::<ExternIO>().
- holochain_integrity_types-0.6.0 zome_io.rs:19 -> ExternIO = serde_bytes BIN; typed struct = MAP.
- moss group/src/lib.rs:40-41 + presence room/src/remote_signals.rs pre-encode (same recv arch).

## Fix (DRY)
encrypted_content::signals: new send_encoded_remote_signal + remote_signal_payload.
All 5 send sites route through it. recv unchanged.

## MEASURED results
- RED: content_signal_round_trips_through_send_path FAILED pre-fix with
  Deserialize("invalid type: map, expected byte array") — exactly the conductor log.
- GREEN: same test passes via remote_signal_payload. 25 coordinator + 69 integrity tests green.
- Build (nix, reproducible x2): DNA uhC0k26b HELD; integrity wasm 06b01fb3 HELD;
  content.wasm cb51c376 NEW; happ 4aacd52f NEW. Byte-identical across builds.
- Conductor: tryorama 0.19.2 cannot boot on hc 0.6.0 ("unrecognized subcommand quic").
  Measured, not assumed. Authoritative e2e = humm-tauri BDD RS-1 against the new happ.

## Artifacts
- happ: humm-earth-core-happ_pass-4-recv-signal-fix_dna-uhC0k26b_happ-4aacd52f.happ
  -> ~/hummhive-official-happ-versions/ (+ MANIFEST + README row)
  -> ~/humm-tauri/.testdata/happs/ (+ MANIFEST row)
- docs/HUMM_TAURI_RECV_REMOTE_SIGNAL_FIX.md (handoff)
- .baseline-hashes.txt: coordinator follow-up section added

## Progress log
- [x] Investigated + source-proven root cause (librarian + direct hdk read).
- [x] Mapped all 5 send sites (no others repo-wide).
- [x] RED test written + MEASURED failing (Deserialize map-not-BIN).
- [x] DRY helper + routing implemented.
- [x] GREEN MEASURED (25 coordinator + 69 integrity).
- [x] Build + DNA hash HELD + reproducible.
- [x] Conductor MEASURED unavailable (tryorama broken hc 0.6.0).
- [x] Distributed to official versions + humm-tauri/.testdata (label pass-4-recv-signal-fix).
- [x] Committed (e0e7ed7) + wsl-pushed to Windows mount.

## Follow-ups for the user
- `git push` both mirrors to origin when ready (assistant never pushes to remote).
- Run humm-tauri BDD RS-1 against the pass-4-recv-signal-fix happ to flip it GREEN.
- Note: fix lives on feat-self-notes-architecture; commit e0e7ed7 is self-contained
  and cherry-pickable onto a dedicated branch / pass-5 if a cleaner lineage is wanted.
