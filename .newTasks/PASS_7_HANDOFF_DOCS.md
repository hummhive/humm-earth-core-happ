# PASS_7_HANDOFF_DOCS — complete humm-tauri integration handoff (Phase 1)

**Status: IN PROGRESS** (started 2026-07-23)
**Branch:** `feat-integrity-pass-7` (branch-only deliverable; blessing-gated, NOT distributed)
**Deliverable:** `docs/HUMM_TAURI_PASS_7_INTEGRATION.md` — the complete M0–M22 handoff for
humm-tauri: every client-visible wire change, integrity contract delta (L1–L23), batch/local
read externs, signal hardening, cap-grant table, client-adoption map, BDD acceptance text,
and the "migration moves to the happ" section. Absorbs and replaces
`docs/HUMM_TAURI_WAVE4_INTEGRATION.md` (M19–M21 draft) so exactly one handoff exists.

## Checklist

| # | Step | Status | Date | Notes |
|---|---|---|---|---|
| 1.0 | Create this tracking file | done | 2026-07-23 | — |
| 1.1 | Write `docs/HUMM_TAURI_PASS_7_INTEGRATION.md` (12 sections: TL;DR, breaking, integrity delta, migration-to-happ, Waves 1–3 surface, Wave-4 batch/local, signals, cap table, adoption map, BDD, migration runbook delta, testing/install) | done | 2026-07-23 | 855 lines; wire shapes extracted verbatim from source (librarian-verified, path:line anchored) |
| 1.2 | "Migration moves to the happ" section (§4 + §11): today-vs-pass-7 contrast anchored to humm-tauri `src-tauri/src/migration/` seams | done | 2026-07-23 | §4 contrast table + §11 five-step runbook delta; `mark_migrated_v2` explicitly kept |
| 1.3a | Fold + delete `docs/HUMM_TAURI_WAVE4_INTEGRATION.md`; ledger line in `docs/PASS_7_SCRATCH.md` Decisions | done | 2026-07-23 | §§1–7 absorbed into §§6/7/9/10 + §3.8; ledger line appended |
| 1.3b | `slop-scan` pass over the new doc (ANTI_SLOP.md bar) | done | 2026-07-23 | Word pass clean (zero banned-list hits); structural pass clean (tables/BDD are the format, exempt per skill) |
| V1 | Literal-fidelity grep: every backticked reject/log literal in the doc greps byte-identical in `dnas/ crates/ docs/PASS_7_SCRATCH.md` | done | 2026-07-23 | 157 spans checked (single-process python; shell `while read` loop double-encodes UTF-8 — avoid). Zero literal misses; 30 residuals all prose/pseudo-shape/humm-tauri identifiers, the latter verified against `~/humm-tauri` |
| V2 | Surface completeness: all 12 granted extern names + all new-vs-main externs appear in the doc | done | 2026-07-23 | `hdk_extern` diff main→HEAD = exactly 13 new externs; all present in doc |
| V3 | Close-out ritual: POSTCOMPACTION branch-copy arc entry → commits → `wsl-push.sh` → `wsl-check.sh` agreement | in progress | 2026-07-23 | |

## Implementation notes / deviations

- Ground truth re-verified this session: `AgentToGroupMemberships` is pass-6
  (`main:group/crud.rs:247` writes it on every grant) — the `list_my_groups`
  granted-half reroute has no pre-existing-data coverage gap.
- Doc ships with TBD release-identity hashes (blessing build re-derives DNA/wasm/happ);
  current scratch DNA pin `uhC0k-HAqM4zW2rCWrKSujEKDZcqybE_ATUjKxkRy2BmRjURYddxP`
  labeled "scratch pin, re-derived at blessing build".
- Sources of record: `docs/PASS_7_SCRATCH.md:28-62` (L1–L23 literals), Wave-4 doc §§1–7
  (absorbed verbatim), branch source for all Waves 1–3 shapes.
