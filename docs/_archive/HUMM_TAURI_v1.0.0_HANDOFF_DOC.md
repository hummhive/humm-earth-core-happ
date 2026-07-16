# humm-earth-core-happ v1.0.0 — humm-tauri Handoff (START HERE)

**Release:** `v1.0.0` (tagged on `main`) = coordinator gen **pass-4-query-tolerance**.
**This is the canonical current-state entry point for humm-tauri integration.** For
the per-subsystem detail, follow the reference index at the bottom.

---

## TL;DR — adopt before returning to your main

**Drop-in coordinator hot-swap. The DNA hash is UNCHANGED** (`uhC0k26b…`) — no
migration, no re-install, existing chains/data untouched. Just swap the `.happ`
binary when convenient. Zero DNA-side risk, so it won't interfere with an in-flight
merge; pick it up before you go back to main.

### Artifact (already in your repo)
- `.testdata/happs/humm-earth-core-happ_pass-4-query-tolerance_dna-uhC0k26b_happ-2205337c.happ`
  (+ its `MANIFEST.tsv` row) — confirm it's committed on your branch.
- sha256: `2205337c085f5d148fdddf63a688e3ad6d4a1b79dc3a70facad180c38b29928b`
- DNA: `uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`
- content_integrity.wasm `06b01fb3…` (byte-identical to every pass-4 gen) · content.wasm `78f0602e…`
- Source: tag `v1.0.0` on earth-core `main`; also in `~/hummhive-official-happ-versions/`.

### Steps (at promotion time, after your merge)
1. Verify: `sha256sum .testdata/happs/...2205337c.happ` → must match above.
2. Bundle: `cp .testdata/happs/humm-earth-core-happ_pass-4-query-tolerance_dna-uhC0k26b_happ-2205337c.happ src-tauri/bin/humm-earth-core-happ.happ`.
3. **DNA-hash pins don't change** (`uhC0k26b` is constant across all pass-4 gens) —
   `coordinator.rs`, `tests/bdd/conductor.ts`, etc. stay as-is. Only update a pin if
   you assert the **happ sha** somewhere: `d74e5f2f`/`4aacd52f` → `2205337c`.
4. Run your BDD suite — especially **RS-1** (`tests/bdd/dm-remote-signal-delivery.test.ts`).

### What you get over the prior pass-4 happ (`4aacd52f` / `d74e5f2f`)
- **Cross-host remote signals now deliver** — DM/content push notifications + WebRTC
  (C7) signalling reach `recv_remote_signal` (the ExternIO double-encode fix).
- **`list_my_hives` / `get_latest_membership` (+ group equivalents) no longer throw**
  on a cross-type Inbox target — the wart you reported (2026-06-12) is fixed.
  `get_latest_membership(genesis)` is now the reliable path to a joiner's HiveMembership hash.
- **`get_many_encrypted_content` (+ `list_by_hive_link`/`_dynamic_link`/`_acl_link`/`_author`)
  tolerate missing/dangling/gossip-lagged targets** — one bad link no longer blanks the batch.

### Build it yourself (reproducible)
```
nix develop --command bash -c 'bash scripts/build-zomes.sh && hc dna pack dnas/humm_earth_core/workdir && hc app pack workdir --recursive'
# hc dna hash … → uhC0k26b… ; sha256sum workdir/*.happ → 2205337c…
```

---

## Mailbox Q&A (answered)

The four questions you sent the earth-core agent are answered in the agent-mailbox
outbox + the topic docs below:
- **DHT entry size cap:** `4,000,000 B` on the whole serialized entry (`ENTRY_SIZE_LIMIT`).
- **DirectMessage validator:** no content_type constraint (recipients 2..32/unique/author-in;
  `reader==recipients`; other PKA buckets empty). Cross-hive `get` is clean (no hive dep).
  → `HUMM_TAURI_CONTENT_TYPE_FILTERING_AND_WITNESS_RULES.md`.
- **Cross-hive >4MB media:** chunk into ≤~3.9MB DirectMessage entries + a manifest entry;
  iroh is transport-side, not the DNA.
- **First entry by a not-yet-member:** use `AclSpec::OpenWrite` (knock); `Public` is rejected
  until the inviter mints the HiveMembership. → `HUMM_TAURI_ACLSPEC_MUTATION_ON_UPDATE.md`
  + `HUMM_TAURI_SELF_NOTES_INTEGRATION.md`.

---

## Current reference docs (read these for detail)

| Doc | Covers |
|---|---|
| `HUMM_TAURI_ACLSPEC_INTEGRATION.md` | Canonical AclSpec wire shape + per-modal wiring |
| `HUMM_TAURI_COORDINATOR_INTEGRATION.md` | Coordinator externs (C0–C7), cap-grant policy |
| `HUMM_TAURI_FEATURE_ENABLEMENT.md` | Per-feature implementation guide (which TS files) |
| `HUMM_TAURI_DM_MESSAGING_INTEGRATION.md` | DM links, key resolution, first-contact handshake |
| `HUMM_TAURI_SELF_NOTES_INTEGRATION.md` + `..._OBSERVABILITY.md` | Note-to-self architecture + logging |
| `HUMM_TAURI_SHARED_SECRETS_PUBLIC_ACL_WIRE_SHAPE.md` | SS public-ACL bytes/acl_spec on read |
| `HUMM_TAURI_CONTENT_TYPE_FILTERING_AND_WITNESS_RULES.md` | content_type filtering + HiveGroup witnesses |
| `HUMM_TAURI_ACLSPEC_MUTATION_ON_UPDATE.md` | Changing acl_spec scope (re-author, not update) |
| `HUMM_TAURI_RECV_REMOTE_SIGNAL_FIX.md` | The cross-host signal fix (root cause + verification) |
| `HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md` | Given/When/Then across the trust chain |
| `DNA_MIGRATION_GUIDE.md` | Migration mechanics for future DNA-bumping passes |
| `PASS_4_DEPLOY_HANDOFF.md` | The pass-3→pass-4 DNA bump + migration (how pass-4 was reached) |
| `CODEMAPS/` | architecture · backend · data · dependencies |

## Historical lineage (archival)

`PASS_1`/`PASS_2`/`PASS_3_DEPLOY_HANDOFF.md` and `HUMM_TAURI_PASS_ROADMAP.md` record
the per-pass history. The authoritative, machine-readable lineage (every gen's DNA
hash + wasm/happ shas + commit) is `.baseline-hashes.txt`; the official binaries are
in `~/hummhive-official-happ-versions/MANIFEST.tsv`. Git tags mark releases.
