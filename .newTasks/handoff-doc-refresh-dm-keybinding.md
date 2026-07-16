# Handoff-doc refresh — DM keybinding trust model (stale in-bytes signature spec)

- **Status:** OPEN — small; fold into the next coordinator-generation docs pass.
- **Origin:** 2026-07-16 humm-tauri fleet audit (IdentityPlatformData lane).
- **Class:** docs only — zero code, zero wasm/DNA impact.

## The drift

`docs/HUMM_TAURI_DM_MESSAGING_INTEGRATION.md` still specifies that
`humm-dm-keybinding-v1` entries carry an in-bytes Ed25519 signature
(`bytes = sign(recipient_signing_key, AgentPubKey || X25519_pubkey)`).

humm-tauri's LIVE code dropped that: `src/sidecars/direct-messages/wire/handshake.ts:1-8`
states the binding now relies SOLELY on the DNA's
`revision_author_signing_public_key == action.author` author-binding
validator ("no in-bytes signature on the DHT path") — i.e. it leans entirely
on the shipped pass-6 C-1 check plus the pass-2 I-H link validators.

## Work

1. Update the keybinding section of `HUMM_TAURI_DM_MESSAGING_INTEGRATION.md`
   to describe the author-binding-only trust model; delete the in-bytes
   signature recipe (or mark it historical) so nobody re-implements a
   redundant check.
2. While in there: sweep the doc for other pre-pass-6 trust caveats
   ("advisory", "until the validator lands") and reconcile against the
   shipped validator set (`entry_validation.rs` C-1, `links/{hive,dynamic}.rs` I-H).
