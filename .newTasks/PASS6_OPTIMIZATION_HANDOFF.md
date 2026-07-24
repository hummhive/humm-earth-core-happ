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
| T1a | `probe_inbox_page` + shared paired-cursor decode (`decode_paired_cursor`; `page_links`/`resolve_page_limit` shared) | done | 2026-07-23 | `53473f8`; wire-identical to reference |
| T1b | `role_key_closure` (+ `dominated_roles`/`canonical_role_group` host tests) | done | 2026-07-23 | `53473f8` |
| T1c | `get_many_by_content_id_link` + `content_id_exists` | done | 2026-07-23 | `53473f8` |
| T1d | `list_group_members_many` (complete rosters, 4096 roster-link budget, reject-not-truncate) | done | 2026-07-23 | `53473f8` |
| T1e | `list_my_groups_local` + shared `list_my_groups_via` core (granted-half moves to the durable `AgentToGroupMemberships` index) | done | 2026-07-23 | `53473f8`; index write-on-grant confirmed in `group/crud.rs` |
| T1f | `OwnerHandoffOfferHint` + best-effort send in `initiate_owner_handoff` + recv arm + 4-family fall-through literal | done | 2026-07-23 | `53473f8` |
| T2a | B10 liveness rider: `include_liveness` on the 7 read externs, `tombstoned` on `EncryptedContentResponse`, `root_tombstoned` probe | done | 2026-07-23 | `1b289f2` |
| T2b | Resolve-path slices: `get_latest_typed_from_eh` record reuse + memoized `resolve_many_encrypted_content` | done | 2026-07-23 | `1b289f2`; graceful (no unwrap/unreachable) |
| T2c | Page core: early-stop `page_links` (limit+1) + shared `resolve_content_link_targets` | done | 2026-07-23 | `1b289f2` |
| T3a | `list_encrypted_content_by_dynamic_links` / `list_by_hive_links_many` / `list_by_author_many` + `enforce_batch_resolve_budget` (4096) | done | 2026-07-23 | `89dde30` |
| T3b | `list_by_hive_link_local_page` + `GetOptions` threading (all existing callers network — zero behavior change) | done | 2026-07-23 | `89dde30` |
| G1 | Cap grants (10) beside singleton twins | done | 2026-07-23 | `53473f8` (6) + `89dde30` (4); `get_latest_memberships_local_many` correctly NOT granted |
| G2 | Sweettest ports + new B10/budget/local-twin tests | done | 2026-07-23 | `7c14e51`; 26 conductor tests across 5 files + support mirrors |
| G3 | Gates: fmt → host tests → clippy → nix build + DNA hash HOLD → full sweettest → reject-literal superset diff vs v3.3.0 | done | 2026-07-23 | fmt clean; host 53/53; clippy clean; **DNA hash held `uhC0ksXs…`**; sweettest all green (see notes); literal superset clean (adds = 8 batch + log/test strings; only loss = pre-registered 3→4 fall-through + a removed `unreachable!` panic msg + a reworded doc comment) |
| G4 | Review lanes: rust, security, silent-failure | done | 2026-07-23 | all 3 lanes "correct"/"clean", zero blockers/majors; 3 non-blocking nits ACCEPTED (byte-identical to reference — perf/visibility polish belongs upstream to keep the generation in lockstep with the pass-7 hot-swap) |
| G5 | Handoff doc `docs/HUMM_TAURI_BATCH_READS_INTEGRATION.md` | done | 2026-07-23 | v3.4.0, no pass-7/fork/lineage refs; happ sha256 `601fc44…` recorded |
| G6 | Close-out: merge `--no-ff` + tag `v3.4.0` (owner pushes), POSTCOMPACTION entry, `wsl-push.sh` + `wsl-check.sh` | pending | | gated on final sweettest confirmation + owner |

## Implementation notes / deviations

- Intentional literal change (pre-registered for the superset diff): the
  `recv_remote_signal` fall-through error gains the fourth family —
  `recv_remote_signal: payload did not decode as EncryptedContentSignal, DmRemoteSignal, BlobPinSignal, or OwnerHandoffOfferHint`.
- `list_my_groups` (network) granted-half durability: granted-group discovery now
  survives an Inbox sweep (durable index); founded-group discovery stays self-Inbox.
- Merge/tag/main-sync gated on the owner's pending branch push + dual clone cutover
  to main; all work until then stays on this branch locally.

## Review dispositions (G4 — all lanes "correct"/"clean", zero blockers/majors)

- **security:** correct. One NIT (root_tombstoned probe-failure not `debug!`-logged) — ACCEPTED: byte-identical to the reviewed reference; the silent-failure specialist lane ruled it a documented tri-state contract, not a defect. All 10 cap grants confirmed read-only/public-DHT or self-scoped-local.
- **silent-failure:** clean (0.93). Full tolerant-drop inventory logged or documented-contract; net tolerance improvement (removed 2 silent `.ok()` chains + 3 panic paths vs main); zero `let _=`/`if let Err(_)`/`.ok();`/masking `unwrap_or_default` in the coordinator.
- **rust:** correct, no blockers. Three non-blocking nits ACCEPTED as reference-parity: two hot-path clones (`resolve_many_encrypted_content` memo, `get_latest_typed_from_eh` entry) and one over-broad `pub(crate)` (`group_members_of`). All byte-identical to the reference generation; optimizing only the backport would be reverted by the pass-7 coordinator hot-swap (regression), so any polish lands upstream in the reference to keep both in lockstep.

## Gate evidence (G3)

- fmt: `cargo fmt --all --check` clean.
- host tests: `content` lib 53/53, integrity untouched.
- clippy: zome workspace `-D warnings` clean; sweettest `--no-run` warning-free.
- reproducible build: `hc dna hash` == `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz` (HELD — integrity untouched); happ sha256 `601fc4499e5d4a5a5553077fe960227318d6036d0aeef9c570e52ce2f81975bc`.
- sweettest: new/changed binaries — batch_reads 17, inbox_and_delete 3, role_key_closure 2, signal_hints 2, liveness_and_reindex 2, pinned_hosts 9 (all pass). Existing regression — coordinator_cleanup 2, coordinator_query_tolerance 2, idempotent_writes 7, migration_rescue 3 (+1 ignored), owner_and_acl 4 pass; recipient_witnesses + service_records revalidating.
- literal superset vs v3.3.0: adds = 8 batch reject literals + new extern/cap/log/test strings; losses = the pre-registered 3→4-family fall-through, a removed `unreachable!` panic message (graceful hardening), and one reworded doc comment. Zero wire-visible reject literals lost.
