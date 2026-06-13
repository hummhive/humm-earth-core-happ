---
name: standard-workflow
description: The standard coordination + review workflow for anything beyond a trivial, isolated change in humm-earth-core-happ — planning, starting a new task/workload, or executing a multi-phase plan. Covers full-context dispatch, parallel subagents, gating reviewer passes, the build/test gate ladder, and the closing docs pass. Use whenever work spans multiple files/subsystems or a plan has phases.
---

# Standard Coordination and Review Workflow

This is the default operating procedure for non-trivial work. Trivial, isolated
edits (a one-line fix, a typo, a single obvious function) skip it. Everything
else — multi-file changes, a new task spec, executing a plan — runs it.

## Background, don't block

Spawn work in parallel and keep moving. The orchestrator dispatches subagents
for non-importing file edits, multi-subsystem investigation, and any decomposable
work, then continues rather than waiting. Sequence only on a real dependency
(B needs A's finished artifact); otherwise fan out and let peers coordinate over
IRC. Never serialize work that can run concurrently.

## Full context, every dispatch

Hold the full context yourself and hand the relevant slice to every subagent.
Full context for THIS repo is:

- `POSTCOMPACTION.md` — what already shipped this arc; do NOT redo it.
- `CLAUDE.md` — change gravity (integrity forks the chain, coordinator hot-swaps),
  the WSL⇄Windows two-clone workflow, build/test, working agreement.
- `AGENTS.md` — the local agent/skill/command toolkit.
- `.baseline-hashes.txt` — the DNA-hash + wasm reproducibility contract. LOAD-BEARING:
  the integrity wasm sha / DNA hash is frozen per line; a change that perturbs it
  forks the chain.
- `docs/CODEMAPS/` — architecture / backend / data / dependencies.
- `../humm-tauri/GLOSSARY.md` — HummHive ubiquitous language (Hive, Member, Group,
  Sidecar, Node, …). Read-only; never edit humm-tauri.
- the `docs/HUMM_TAURI_*.md` integration/handoff docs for the area you're touching.

The orchestrator MUST have all of the above in context before dispatching. Do NOT
import humm-tauri's `CODING_STANDARDS.md` / `ADDITIONAL_*` / `ARCHITECTURE.md` —
they are not geared for this repo. Naming/immutability/code-smell guidance here is
`skill://coding-standards`; Rust specifics are `skill://rust-patterns` +
`skill://rust-testing`; the dispatch checklist + WSL build rules live in `CLAUDE.md`.

Subagent context also includes: the exact files/symbols, explicit non-goals, the
acceptance criteria, and the standing rules — subagents NEVER run repo-wide gates,
formatters, or any `cargo` command (a stray rebuild clobbers the build the
orchestrator is gating; see CLAUDE.md). On WSL, subagents work in
`~/humm-earth-core-happ/` ONLY (never `/mnt/c/...`); ast/lsp tools take absolute
WSL paths.

Keep your todo list current. Keep `.newTasks/` specs current as you progress; when
a spec is FULLY complete (built + reviewed + gated), `git mv` it to
`.doneTasks/`. If a subagent is stuck, consult an `oracle`. Subagents use
`librarian` for codewalking (external deps + local source), falling back to
`explore`. Coordinate parallel agents over IRC. Give subagents no wiggle room —
spell out the contract.

## Reviewer passes (gating, parallel, read-only)

After feature completion, run the reviewer stack as parallel READ-ONLY lanes.
Each lane writes `local://review-<lane>.md` and is sequential+gating before a
commit. Iterate until clean.

Always-on lanes:
- **security-reviewer** — Holochain/Rust: validator authority bypass, cross-hive
  identity claims, spoofing, capability-grant scope (`set_cap_tokens`), `unsafe`,
  secret handling. Applies `skill://security-review`.
- **silent-failure-hunter** — swallowed errors / over-tolerant decodes
  (`let _ =`, `.ok()` dropping `Err`, `unwrap_or_default` hiding faults,
  `?` that should be `.ok().flatten()` and vice-versa).
- **coding-standards compliance** — `skill://coding-standards`.
- **DRY-refactor** — duplicate logic / brittle per-call-site forks.

Change-specific lanes:
- **rust-reviewer** — ANY zome/Rust change (ownership, lifetimes, error handling,
  `unsafe`, concurrency, idioms). MANDATORY for Rust per `AGENTS.md`.

(No ts-reviewer / mobx lane — this repo has no TS frontend; the only TS is the
`tests/` harness + `scripts/migrate-dna.ts` tooling.)

Lane hygiene: maintain a known-issue register so lanes don't re-flag accepted
nits; if a lane is overloaded, split by subsystem. Then run the fix wave —
behavior fixes are test-first (red before green): host `cargo test` for unit
behavior, or a `crates/sweettest` conductor test for cross-agent behavior. Re-run
any lane whose inputs changed non-trivially.

## Gates and verification (orchestrator-only, once, at the end)

Run the gates yourself over the union of changes — never delegate them:

- `cargo test -p content --lib` + `cargo test -p content_integrity --lib` (host).
- `cargo check` / `cargo clippy` — zero warnings (treat warnings as errors).
- **Conductor behavior** (changes to signals, query tolerance, membership, or any
  cross-agent flow): the `crates/sweettest/` in-process conductor suite. This is
  the conductor-test path — tryorama CANNOT boot on hc 0.6.0 (the `quic`→`webrtc`
  sandbox-CLI rename); do not attempt it.
- **Reproducible build + DNA-hash invariant**:
  `nix develop --command bash -c 'bash scripts/build-zomes.sh && hc dna pack dnas/humm_earth_core/workdir && hc app pack workdir --recursive'`,
  then `hc dna hash dnas/humm_earth_core/workdir/humm_earth_core.dna` MUST equal
  the pinned DNA hash for the line (integrity untouched → no chain fork), and
  `sha256sum workdir/*.happ` recorded against `.baseline-hashes.txt` + the
  official-versions MANIFEST.

Test depth scales with blast radius. Day-to-day: host cargo tests. Before a merge
to main, a release cut, or any toolchain/dep bump: the FULL ladder — both host
suites, `cargo clippy` clean, the Sweettest conductor suite, AND a reproducible
build with the DNA-hash invariant verified + happ sha recorded + distributed
(official-versions + `humm-tauri/.testdata`).

## Closing a plan

Update `POSTCOMPACTION.md` at each phase boundary (and every few commits). Sync
the WSL⇄Windows mirrors via `scripts/wsl-*.sh` (see CLAUDE.md); never push to
origin without explicit instruction. At the very end, run the full docs pass via
`skill://update-docs-workflow` (which refreshes `docs/CODEMAPS/` through
`/update-codemaps` and verifies the hApp-sha / version pins).
