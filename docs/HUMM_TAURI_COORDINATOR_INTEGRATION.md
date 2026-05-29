# humm-tauri Coordinator Integration Guide

**Branch:** `feat-optional-recipient-id`
**hApp version after this pass:** coordinator-only changes on top of
`c326e62`; integrity zome **byte-identical** to that baseline.
**DNA hash:** `uhC0kT0Tkc3b6ccfa75YwWdzpSWvdkXERpdqkxIndRhfK5TJAUusY`
(unchanged from the c326e62 baseline — verified via `hc dna hash`).

This document is the handoff from this hApp pass to the humm-tauri team.
It tells you what the updated coordinator unlocked, exactly where each
change wires in on the humm-tauri side, and the non-obvious caveats —
including one **load-bearing security correction** to the C4 docstring
the planning artifact carried in error. It complements (does not
duplicate) humm-tauri's `docs/dna_spec.md`, which is the wire-level
reference.

---

## TL;DR

- **C0** (`get_messages_since`) already shipped at `2dbeb13`. No
  coordinator change this pass; humm-tauri TS integration was always
  the next-up step.
- **C1** (signal provenance stamping) shipped at `c326e62`, reviewed
  and preserved through the C7b refactor. `recv_remote_signal` now
  always stamps `from_agent = call_info()?.provenance` on every emitted
  signal variant.
- **C2** (`list_by_hive_link` `since_ts` + `limit`) shipped at
  `c326e62`; this pass **fixes a data-loss bug** in the sort order
  (was newest-first + truncate → broke watermark sweeps with `>limit`
  new entries) and standardizes the implementation on the high-level
  `LinkQuery::try_new(...).after(ts)` API.
- **C3** (`count_links_by_hive`) shipped at `c326e62`; this pass adds
  a dedicated `CountByHiveInput` (drops the meaningless `limit` field
  that reusing `ListByHiveInput` carried) and registers the cap grant
  that was missing.
- **C4** (`fetch_pair_ss_with_hive_check`) **new this pass.** Coordinator-
  side intersection of the author and active-hive-dynamic link sets.
  **Read the security caveat in the "C4 — what this DOES and DOES NOT
  defend against" section below before relying on it for H-1
  mitigation.**
- **C5** (cap-grant fixes) **new this pass.** Typo fix
  (`get_many_encrypted_conten` → `get_many_encrypted_content`) and
  grants for the new query externs (C3, C4). `send_dm_*` (C6/C7) are
  **deliberately NOT granted** — see SEC-2 below.
- **C6** (`send_dm_delete_request`) **new this pass.** Ephemeral
  fire-and-forget "please tombstone" signal. Pairs with the existing
  in-payload `kind:'delete_request'` path; both work, you pick.
- **C7** (`send_dm_call_init_request` / `…_init_accept` / `…_sdp_data`)
  **new this pass.** Ephemeral WebRTC signaling. SDP body is opaque to
  the zome.
- **C7b** (`recv_remote_signal` dispatcher) **new this pass.** Single
  conductor callback now ordered-try-decodes against
  `EncryptedContentSignal` first (legacy wire shape, byte-identical to
  c326e62) then `DmRemoteSignal` (the C6/C7 envelope). Disambiguation
  is empirically pinned by six host-side serde unit tests.

### Deploy (no user wipe — coordinator hot-swap)

1. On Linux dev box (`~/humm-earth-core-happ`):
   ```bash
   RUSTFLAGS='--cfg getrandom_backend="custom"' \
     CARGO_TARGET_DIR=target \
     cargo build --release --target wasm32-unknown-unknown
   hc dna pack dnas/humm_earth_core/workdir
   hc dna hash dnas/humm_earth_core/workdir/humm_earth_core.dna
   # MUST print: uhC0kT0Tkc3b6ccfa75YwWdzpSWvdkXERpdqkxIndRhfK5TJAUusY
   hc app pack workdir --recursive
   ```
2. Copy `workdir/humm-earth-core-happ.happ` (~1.08 MB) into
   `humm-tauri/src-tauri/bin/humm-earth-core.happ`.
3. Bump `COORDINATOR_WASM_VERSION` in humm-tauri's install guard so the
   relay sees the version cross and triggers a coordinator hot-swap on
   next start. Conductor re-runs `init` → new cap grants for C3 / C4 /
   `recv_remote_signal` take effect.
4. Restart the humm-tauri relay. No user wipe; no DNA migration; no
   re-bootstrap.

The integrity wasm sha256 is `8c620847f7ae8878769e000452f2f89a4954a747b1c51c129666cdf0978f2c5c`
before AND after this pass. If your build produces a different
integrity sha256, something edited the integrity zome and the change is
no longer coordinator-only — abort the deploy.

---

## Per-change integration

### C1 — sender provenance on every emitted signal

| | |
|---|---|
| **Zome surface** | `recv_remote_signal: ExternIO -> ExternResult<()>` (dispatcher). Emits the decoded payload with `from_agent` set to the conductor-attested caller pubkey. |
| **Wire shape for `EncryptedContentSignal`** | `{ action_type: "Create"\|"Update"\|"Delete", data: EncryptedContentResponse, from_agent?: AgentPubKey }` |
| **Anti-spoof guarantee** | Whatever the wire payload claimed about `from_agent` is **overwritten** by `call_info()?.provenance` on the receiver. A peer cannot impersonate another peer in your locally-emitted signal. |

**humm-tauri call sites to update:**

- **`src/api/core/holochain/zomeSignals.ts`** — `EncryptedContentSignal`
  type gains `from_agent?: AgentPubKey`. The `isEncryptedContentSignal`
  guard stays loose (only validates `action_type`); the new field is
  optional on the wire so older payloads still parse.
- **`src/sidecars/direct-messages/state/DmStore.ts` `ingestIncomingMessage`**
  — when `signal.from_agent` is present, `encodeHashToBase64(from_agent)`
  and compare to the eventually-fetched
  `record.header.revision_author_signing_public_key`. Drop signals where
  the two disagree. Closes the
  "attacker-fabricated-signal-pointing-at-a-real-entry" confusion attack
  the inline doc-comment at `lib.rs:85-138` describes.
- **NB on the wire form:** `from_agent` is `Uint8Array(39)` on the
  msgpack wire. Compare via `encodeHashToBase64` (53-char `'u' +
  URL_SAFE_NO_PAD`) or `Buffer.from(a).equals(Buffer.from(b))`, NOT
  `===`.

Reference: `humm-tauri/.newTasks/T_SECURITY_SENDER_IDENTITY_UNATTESTED.md`.

### C2 — `list_by_hive_link` with `since_ts` + `limit` (sweep-safe)

| | |
|---|---|
| **Zome surface** | `list_by_hive_link(ListByHiveInput) -> Vec<EncryptedContentResponse>` |
| **Input** | `{ hive_id: string, content_type: string, since_ts?: Timestamp, limit?: number }` |
| **Return order** | **OLDEST-FIRST** by `link.timestamp`. Truncation to `limit` keeps the OLDEST `limit` entries. |
| **`since_ts` semantics** | Microseconds (Holochain `Timestamp`). Boundary inclusivity follows the conductor's `LinkQuery::after`; treat as approximately exclusive and dedupe by action hash on the host. |

**Why oldest-first matters (the fix).** The c326e62 implementation
sorted newest-first then truncated. For a watermark sweep with `>limit`
new entries, that dropped the older entries past `limit`, the host
advanced its watermark past them, and they were never re-fetched →
silent data loss. The fix sorts ascending so a `(since_ts, limit)` sweep
is gap-free: the host sets `next_since_ts = max(returned.timestamp)` and
re-sweeps; entries that didn't fit in this batch survive into the next
one.

**humm-tauri call sites to update:**

- **`src/api/core/hummContent/index.ts` `listAllByHive`** (~lines
  452-454): pass the new optional fields when sweeping. Multiply your
  JS ms timestamps by 1000 to get microseconds.
- **`src/sidecars/direct-messages/state/DmStore.ts` `_sweepInbox`**
  (~lines 239, 700-705, 1167-1171): use `since_ts = lastSweepAt_us`
  and `limit = 500`. Advance `lastSweepAt_us = max(returned[*].timestamp)`
  AFTER processing the batch; loop until `returned.length < limit`.
  Dedupe by `hash` to absorb the µs-collision boundary case.
- **Demo mode mocks** (`inMemoryStore.ts`, `zomeHandlers.ts`): mirror
  the oldest-first + truncate semantics so demo and live behaviour
  match.

Reference: `humm-tauri/.newTasks/20260525_ListByHiveLinkPagination.md`.

### C3 — `count_links_by_hive`

| | |
|---|---|
| **Zome surface** | `count_links_by_hive(CountByHiveInput) -> usize` |
| **Input** | `{ hive_id: string, content_type: string, since_ts?: Timestamp }` |
| **Empty path** | Returns 0 (not an error). |

Efficient path (when `since_ts` is `None`) uses `count_links(LinkQuery)`
— no link fan-out. With `since_ts` set, falls back to
`get_links(...).len()` because the host's `count_links` has no time
filter.

**humm-tauri call sites:** unread badges, hive item counts,
`SyncIndicator`. Anywhere the UI needs "how many" without paying for the
full link fan-out.

NB: this is the C3 input shape change from this pass — c326e62 reused
`ListByHiveInput` with a meaningless `limit` field. The new
`CountByHiveInput` drops `limit`. Old TS callers that sent `{ hive_id,
content_type }` keep working; callers that sent `{ ..., limit: 0 }` get
that field ignored by serde (msgpack tolerates unknown fields).

### C4 — `fetch_pair_ss_with_hive_check`

| | |
|---|---|
| **Zome surface** | `fetch_pair_ss_with_hive_check(FetchPairWithHiveCheckInput) -> Vec<EncryptedContentResponse>` |
| **Input** | `{ author: string (`'u'+URL_SAFE_NO_PAD(39)` pubkey), active_hive_id: string, content_type: string, group_id: string }` |
| **Behavior** | Intersects two `ActionHash` sets: links under the author path `[author, content_type]→Hive` and links under the active-hive dynamic path `[active_hive_id, content_type, group_id]→Dynamic`. Returns entries reachable from both. Best-effort fetch: hashes that fail to resolve are dropped from the result (single bad/unresolvable AH does NOT fail the whole call). |

#### What this DOES defend against (the realistic threat)

Against an **unmodified-client** attacker who can only invoke the stock
`create_encrypted_content` extern with arbitrary inputs: the
intersection narrows results to entries that are both
authored-by-target AND placed under the caller's chosen
`(active_hive_id, content_type, group_id)` dynamic path. Such an
attacker, lacking access to put their poison under the victim's active
hive's dynamic path via the normal create flow, will at most place their
entry under their OWN `hive_id` — which fails the intersection.

#### What this DOES NOT defend against (load-bearing — read this)

**This function does NOT close H-1** against any attacker willing to
run modified coordinator WASM (the standard Holochain adversary —
coordinator code is not a security boundary). Today the integrity zome
validators for `LinkTypes::Hive` and `LinkTypes::Dynamic` are no-op
`Ok(Valid)` stubs
(`dnas/humm_earth_core/zomes/integrity/content/src/lib.rs:119,127` and
`:302,310`); a modified-coordinator Mallory can directly publish
arbitrary `Hive` and `Dynamic` links pointing at her poison entry,
landing it in BOTH sets the intersection consults — including the
victim's active hive's dynamic path. The intersection therefore returns
the poisoned entry.

Closing H-1 properly requires integrity-level validators that prove
(a) the `Hive` author-path link's base equals the link author, and
(b) the `Dynamic` link's author has writer rights to the hive named by
the base path. Both are DNA-hash-bumping changes deferred to the
second-pass scope (see "What was NOT done" → second-pass items at the
end of this doc).

**Until those integrity validators ship, the TS-side trust checks
remain the load-bearing control:**

- `from_agent` from C1 (cryptographically attested by the conductor).
- Decrypt-and-verify the SS body using the expected sender's pubkey;
  reject SS that fails MAC/signature verification.
- Treat C4 as a defense-in-depth narrowing, not a cryptographic
  guarantee.

**humm-tauri call site:** `src/api/content/sharedSecret/index.ts`
`fetchPairFromAuthor` (~line 600). The TS-side filter against
`listAllByAuthor` can be replaced by a single C4 call once you have a
known `active_hive_id`. Resolve `active_hive_id` from the
`ActiveHiveStore` (open question from the original plan doc — humm-tauri
already tracks active hive state).

**Empty-result semantics:** `[]` means "not visible yet on this arc";
re-poll, do NOT treat as "definitely does not exist". The eventual-
consistency case where one side of the intersection has not yet
gossiped to the caller looks identical to the empty case.

Reference: `humm-tauri/.newTasks/T_SECURITY_FETCH_PAIR_FROM_AUTHOR_POISONING.md`.

### C5 — cap-grant audit

| Fix | Effect |
|---|---|
| Typo `get_many_encrypted_conten` → `get_many_encrypted_content` | Cross-agent remote callers using the correct name no longer silently fail the cap check. |
| Grants added for `count_links_by_hive` (C3), `fetch_pair_ss_with_hive_check` (C4) | New query externs are remotely callable, matching the pattern of every other `list_by_*` / `get_*`. |
| `recv_remote_signal` grant **preserved** | Conductor still invokes it on every recipient of `send_remote_signal`. |
| **NOT granted** (deliberate — SEC-2): `send_dm_delete_request`, `send_dm_call_init_request`, `send_dm_call_init_accept`, `send_dm_call_sdp_data` | These are sender-side; granting them `Unrestricted` would let any peer use my agent as a reflector to a third party (amplification DoS + spoof-by-proxy). They remain callable from humm-tauri's local UI via the conductor's AppWebsocket auth (same precedent as `create_encrypted_content` / `update_encrypted_content` / `delete_encrypted_content`, which are intentionally not in the cap grant either). |

### C6 — `send_dm_delete_request` (ephemeral delete)

| | |
|---|---|
| **Zome surface** | `send_dm_delete_request(SendDmDeleteRequestInput) -> ()` |
| **Input** | `{ thread_id: string, target_action_hash: ActionHash, recipient: AgentPubKey }` |
| **Receiver sees** | `DmRemoteSignal::DmDeleteRequest { thread_id, target_action_hash, from_agent: <caller> }` |
| **Persistence** | NONE. Fire-and-forget. Offline recipients miss the signal. |

Use when you want **lower-latency, no-metadata-leak** delete signaling
to an online peer. The receiver UI decides whether to honor (validate
that `from_agent` is a thread participant; the zome does NOT enforce
authorization).

If you need **guaranteed delivery** (offline recipient gets it on next
sweep), use the in-payload `kind:'delete_request'` path
(`T_DM_DELETE_IMPL.md` Tier A). Both work; pick per use case. Many
deployments will want both — fire C6 for the immediate UX, fall through
to the persisted DM for offline coverage.

**humm-tauri call site:** add a `DmStore.sendDeleteRequest` method
calling this extern.

### C7 — `send_dm_call_*` + `DmCallSignal`

Three externs, one signal envelope, one design rationale: port the
ephemeral signaling pattern from `presence/.../remote_signals.rs`.

| Extern | Input | Inner signal variant |
|---|---|---|
| `send_dm_call_init_request` | `{ call_id, recipient }` | `DmCallSignal::InitRequest { call_id, from_agent }` |
| `send_dm_call_init_accept` | `{ call_id, recipient }` | `DmCallSignal::InitAccept { call_id, from_agent }` |
| `send_dm_call_sdp_data` | `{ call_id, data, recipient }` | `DmCallSignal::SdpData { call_id, data, from_agent }` |

`data` is an **opaque** string (SDP / ICE JSON / whatever your
application layer puts on the wire). The zome never parses it. Sized
for typical SDP exchanges (a few KB). Use a dedicated transport for
media itself.

**Receiver dispatch:** wrapped as
`DmRemoteSignal::DmCall(DmCallSignal::…)` — see C7b below.

**humm-tauri new code:** add a `src/sidecars/dm-webrtc/dm-webrtc-store.ts`
modeled on `presence/ui/src/streams-store.ts`. The thread-participant
check is **host-side** (the zome does not know about humm-tauri threads
or call sessions).

Reference: `humm-tauri` / ecosystem `PRESENCE_WEBRTC_REFERENCE.md`.

### C7b — `recv_remote_signal` multi-signal dispatcher

| | |
|---|---|
| **Signature change** | `recv_remote_signal: ExternIO -> ExternResult<()>` (was previously typed against `EncryptedContentSignal`). The WASM symbol name is unchanged so it remains the single conductor callback. |
| **Dispatch** | Ordered try-decode: (1) `ExternIO::decode::<EncryptedContentSignal>()` (shipped/legacy wire shape, byte-identical to c326e62 — old senders unaffected); (2) `ExternIO::decode::<DmRemoteSignal>()` (new C6/C7 envelope); (3) explicit `wasm_error!` on no-match (auditable, not silently dropped). |
| **`from_agent` stamping** | Both decode arms overwrite `from_agent` with `call_info()?.provenance` BEFORE `emit_signal`. Defense-in-depth: `DmRemoteSignal::stamp_from_agent` is an exhaustive match with no wildcard arm, so adding a new variant fails to compile until stamp coverage is added. |
| **Safety proof** | Six host-side serde round-trip tests (`encrypted_content/signals.rs::tests`, runs under plain `cargo test`, no wasm) empirically pin that `EncryptedContentSignal` and `DmRemoteSignal` cannot cross-decode under msgpack — the load-bearing property the dispatcher relies on. |

**humm-tauri TS shape** (`src/api/core/holochain/zomeSignals.ts`):

```ts
export type ZomeSignal =
  // Existing wire shape (no `kind` field).
  | {
      action_type: "Create" | "Update" | "Delete";
      data: EncryptedContentResponse;
      from_agent?: AgentPubKey;
    }
  // New ephemeral envelope (`kind`-tagged).
  | {
      kind: "DmDeleteRequest";
      thread_id: string;
      target_action_hash: ActionHash;
      from_agent?: AgentPubKey;
    }
  | {
      kind: "DmCall";
      type: "InitRequest" | "InitAccept";
      call_id: string;
      from_agent?: AgentPubKey;
    }
  | {
      kind: "DmCall";
      type: "SdpData";
      call_id: string;
      data: string;
      from_agent?: AgentPubKey;
    };
```

Discriminate on `'kind' in signal` first, then fall back to
`'action_type' in signal` for the legacy shape. `from_agent` is stamped
on every variant.

---

## Caveats and non-obvious behavior

Things you'd only learn by reading the Rust — collected here so future
humm-tauri devs don't re-discover them by accident:

1. **`from_agent` is `Some` only on remote arrivals.** Local
   `emit_signal` paths in `create_encrypted_content` /
   `update_encrypted_content` / `delete_encrypted_content` all emit with
   `from_agent: None` (the author IS the receiver — there's no remote
   caller to attest). Treat `None` as "this signal originated on my own
   conductor."
2. **Signal arrival is a HINT, not proof.** Per the long doc-comment at
   `lib.rs:85-138`: the cap grant on `recv_remote_signal` is
   `Unrestricted`, so any peer can send any decodable payload. The
   `from_agent` field is trustworthy (conductor-attested); every OTHER
   field in the payload is attacker-controlled. Sidecars MUST re-fetch
   the DHT entry before treating the payload data as authoritative.
3. **C2 `since_ts` is microseconds, not milliseconds.** Multiply
   `Date.now() * 1000` before passing. Boundary inclusivity is
   approximately exclusive; dedupe by action hash to absorb the
   µs-collision case.
4. **C3 `count_links_by_hive` empty-path returns 0, not Err.** Safe
   for UI badges that don't want to handle errors for never-used hives.
5. **C4 `[]` ≠ "definitely empty".** Could be eventual-consistency lag
   on either side of the intersection. Re-poll. The polling sweep
   (existing `list_by_hive_link` consumer) is the authoritative
   backstop.
6. **C6/C7 are ephemeral — no DHT entry, no offline delivery.**
   Pair with the in-payload `kind:'delete_request'` path for offline
   coverage of deletes; pair with whatever your call setup model is for
   missed-call-while-offline UX.
7. **Cap grants for new externs are picked up after the hot-swap re-runs
   `init`.** The conductor calls `init` on a clean coordinator load;
   the hot-swap path triggers this. If you skip the
   `COORDINATOR_WASM_VERSION` bump and the hot-swap doesn't fire, the
   new C3/C4 grants won't exist and cross-agent calls to them will fail
   the cap check.
8. **The `.happ` ABI must byte-match conductor 0.6.0.** Holochain crate
   family ships lockstep with exact `=` pins. The 0.6.0→0.6.1 bump
   changes the DNA hash and requires the conductor side
   (`tauri-plugin-holochain` `main-0.6` → would need `main-0.7`) to
   move first. See "Dependency refresh" in the planning doc.
9. **`send_dm_*` are local-only by design.** See C5 / SEC-2. Calling
   them via `call_remote` from another agent's cell will fail the cap
   check — that's intentional. If a future feature genuinely needs a
   peer to invoke these remotely, gate them with `CapAccess::Assigned`
   to specific trusted agent keys, NEVER `Unrestricted`.
10. **Test harness gap.** This repo's `tests/` workspace targets
    `@holochain/tryorama` (currently bumped to `^0.19.2`). None of the
    published tryorama versions on npm pair cleanly with the `hc 0.6.0`
    CLI binary installed in the dev environment (tryorama 0.17 needs
    a missing `hc-run-local-services` binary; 0.18 uses removed
    `hc sandbox run -e` flag; 0.19 targets the iroh transport in
    holochain 0.6.1+). The seven TR-C1..C7b tryorama tests are written
    and checked in (see `tests/src/humm_earth_core/content/*.test.ts`),
    but `npm test` cannot drive them end-to-end in this environment.
    The load-bearing C7b safety proof is the host-side `cargo test
    -p content --lib` (6/6 green, no harness needed). The tryorama
    tests pass as soon as the harness is paired — this is a separate
    follow-up.

---

## What was NOT done + why (deferred items)

### Coordinator-layer items deferred to a future pass

- **Full cursor pagination (Phase 2 of C2)** — `LinkQuery` in HDK 0.6.0
  has no native limit/tiebreaker, so a true cursor would have to be
  emulated on top of `since_ts` with action-hash tiebreakers. The
  watermark-sweep + dedupe pattern covers the actual humm-tauri
  consumer (DmStore inbox sweep) without it. Re-evaluate when humm-tauri
  has a paginated list view that requires deterministic cursor
  semantics.
- **Dependency refresh (holochain 0.6.1+)** — gated on humm-tauri's
  `tauri-plugin-holochain` advancing past `main-0.6`. Currently blocked
  by darksoil-studio's private p2p-shipyard branch and iroh transport
  conflicts in humm-tauri's blob-store (see planning doc "Dependency
  refresh" section).

### Integrity-layer items — **second pass, separate branch**

These all change the DNA hash → forks the network → require a planned
migration / user wipe. They are the natural second-pass scope. From the
ecosystem research at
`/mnt/c/proj/github/hummhive/holochain-ecosystem/HAPP_COORDINATOR_CHANGES.md`
(deferred I-class) and from this pass's reviewer findings:

| ID | Name | Driving doc | Why required |
|---|---|---|---|
| **I-A** | Receiver-initiated native HC tombstone for DMs | `humm-tauri/.newTasks/T_DM_DELETE_IMPL.md` §"DNA changes (Tier B)" | Restrict deletes in `validate_delete_encrypted_content` to author OR `original_entry.public_key_acl.reader`. Today it returns `Valid` unconditionally. |
| **I-B** | Dual sender-key fields in `EncryptedContentHeader` | `humm-tauri/.newTasks/T_SECURITY_SENDER_IDENTITY_UNATTESTED.md` §"Scope of fix" §1 | New `sender_signing_pubkey: String` carrying the Tauri-keyring Ed25519 key separate from `revision_author_signing_public_key`. New validator enforces `action.author == header.revision_author_signing_public_key`. |
| **I-C** | DHT Inbox link type + `DmProbeLog` private entry | (ecosystem research) | New link type + new private entry type for offline-deliverable DM signaling. |
| **I-D (NEW — from this pass's reviewers)** | **Hive/Dynamic link integrity validators (true H-1 fix)** | This pass's security review SEC-1; planning doc's C4 caveat | Today `LinkTypes::Hive` and `LinkTypes::Dynamic` validators are no-op `Ok(Valid)` stubs (integrity `lib.rs:119,127` and `:302,310`). Add: (a) Hive author-path link's base MUST equal link author; (b) Dynamic link's author MUST have writer rights to the hive named by the base path. Without these, C4's intersection is a defense-in-depth narrowing but NOT a cryptographic H-1 guarantee. |

The second-pass branch should pick these up as a coherent unit — they
all share the migration story (existing users wipe + re-bootstrap), so
it makes sense to ship them together rather than bumping the DNA hash
multiple times in succession. Suggested branch ordering inside that
pass:

1. **I-D first** (closes the real C4 H-1 gap — directly enables the
   security claim humm-tauri's TS layer currently has to enforce).
2. **I-B second** (the dual-keypair issue — humm-tauri has two
   signing keys, the current header only attests one).
3. **I-A third** (delete authorization at the integrity layer —
   completes the DM lifecycle).
4. **I-C last** (new entry/link types for offline DM delivery).

Each one needs its own validator tests in the integrity zome (`cargo
test -p content_integrity`); the DNA-hash invariant check from this pass
becomes the inverse check in the second pass (hash MUST change in a
predictable, reproducible way).

---

## References

- **Planning doc:** `/mnt/c/proj/github/hummhive/holochain-ecosystem/HAPP_COORDINATOR_CHANGES.md`
  (1525 lines — the authoritative spec this pass implements)
- **C1 driver:** `humm-tauri/.newTasks/T_SECURITY_SENDER_IDENTITY_UNATTESTED.md`
- **C2 driver:** `humm-tauri/.newTasks/20260525_ListByHiveLinkPagination.md`
  (a.k.a. `T_HAPP_COORDINATOR_C2_LIST_PAGINATED.md` in some refs)
- **C4 driver:** `humm-tauri/.newTasks/T_SECURITY_FETCH_PAIR_FROM_AUTHOR_POISONING.md`
- **C6 driver:** `humm-tauri/.newTasks/T_DM_DELETE_IMPL.md`
- **C7 driver / ecosystem ref:** `holochain-ecosystem/PRESENCE_WEBRTC_REFERENCE.md`
- **Signal-dispatch pattern:** `holochain-ecosystem/MOSS_REFERENCE.md` §1
- **HDK reference:** `holochain-ecosystem/HOLOCHAIN_HC_REFERENCE.md`
- **Tryorama reference:** `holochain-ecosystem/TRYORAMA_REFERENCE.md`
- **In-repo plan artifact:** `local://happ-coordinator-enablement.md`
  (this pass's execution plan)
- **In-repo baseline hashes:** `.baseline-hashes.txt`
  (DNA + wasm sha256 before/after invariant check)
