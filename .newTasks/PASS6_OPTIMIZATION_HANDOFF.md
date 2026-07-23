# PASS6_OPTIMIZATION_HANDOFF — batch-reads coordinator generation (v3.4.0)

**Status: IN PROGRESS** (started 2026-07-23)
**Branch:** `feat-coordinator-pass6-batch-reads` (off main `63cba86`)
**Deliverable:** coordinator-only generation `pass-6-batch-reads`, tag `v3.4.0`, on the
HELD pass-6 DNA `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz` — batch read
externs, local read twins, the B10 liveness rider, a paged inbox probe, role-key
closure enumeration, and the owner-handoff offer hint, so humm-tauri collapses its
N+1 zome-call loops and adopts the perf wire once. Integrity crate UNTOUCHED — the
DNA hash is verified at every gate; any drift aborts the tranche.

## Checklist

| # | Item | Status | Date | Notes |
|---|---|---|---|---|
| 0 | Branch + this tracking file | done | 2026-07-23 | — |
| T1a | `probe_inbox_page` + shared paired-cursor decode (`decode_paired_cursor`; `page_links`/`resolve_page_limit` shared) | pending | | |
| T1b | `role_key_closure` (+ `dominated_roles`/`canonical_role_group` host tests) | pending | | |
| T1c | `get_many_by_content_id_link` + `content_id_exists` | pending | | |
| T1d | `list_group_members_many` (complete rosters, 4096 roster-link budget, reject-not-truncate) | pending | | |
| T1e | `list_my_groups_local` + shared `list_my_groups_via` core (granted-half moves to the durable `AgentToGroupMemberships` index) | pending | | |
| T1f | `OwnerHandoffOfferHint` + best-effort send in `initiate_owner_handoff` + recv arm + 4-family fall-through literal | pending | | |
| T2a | B10 liveness rider: `include_liveness` on the 7 read externs, `tombstoned` on `EncryptedContentResponse`, `root_tombstoned` probe | pending | | |
| T2b | Resolve-path slices: `get_latest_typed_from_eh` record reuse + memoized `resolve_many_encrypted_content` | pending | | |
| T2c | Page core: early-stop `page_links` (limit+1) + shared `resolve_content_link_targets` | pending | | |
| T3a | `list_encrypted_content_by_dynamic_links` / `list_by_hive_links_many` / `list_by_author_many` + `enforce_batch_resolve_budget` (4096) | pending | | |
| T3b | `list_by_hive_link_local_page` + `GetOptions` threading (all existing callers network — zero behavior change) | pending | | |
| G1 | Cap grants (10) beside singleton twins | pending | | |
| G2 | Sweettest ports + new B10/budget/local-twin tests | pending | | |
| G3 | Gates: fmt → host tests (integrity must stay untouched-green) → clippy → nix build + DNA hash HOLD → full sweettest → reject-literal superset diff vs v3.3.0 | pending | | |
| G4 | Review lanes: rust, security (batch bounds/budgets/self-scoping), silent-failure (tolerant-drop paths), standards, DRY | pending | | |
| G5 | Handoff doc `docs/HUMM_TAURI_BATCH_READS_INTEGRATION.md` | pending | | |
| G6 | Close-out: merge `--no-ff` + tag `v3.4.0` (owner pushes), POSTCOMPACTION entry, `wsl-push.sh` + `wsl-check.sh` | pending | | |

## Implementation notes / deviations

- Intentional literal change (pre-registered for the superset diff): the
  `recv_remote_signal` fall-through error gains the fourth family —
  `recv_remote_signal: payload did not decode as EncryptedContentSignal, DmRemoteSignal, BlobPinSignal, or OwnerHandoffOfferHint`.
- `list_my_groups` (network) granted-half durability: granted-group discovery now
  survives an Inbox sweep (durable index); founded-group discovery stays self-Inbox.
- Merge/tag/main-sync gated on the owner's pending branch push + dual clone cutover
  to main; all work until then stays on this branch locally.
