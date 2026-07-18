# Agent Toolkit — humm-earth-core-happ

Quick reference for the local `.claude/` skills, agents, and commands
available in this repo. Copied/adapted from `~/.claude/` (global ECC install)
and the humm-tauri toolkit, curated and pruned for Holochain DNA + Rust development.

## HARD RULES (Codex / agents — read first)

Codex loads this file natively (its `CLAUDE.md` counterpart). Under oh-my-pi these
are also enforced by the `.omp/` TTSR rules + the `repo-standards` sticky context;
running bare `codex`, this digest is the enforcement. Full rationale: `CLAUDE.md` +
the root standards canon (`CODING_STANDARDS.md` +
`ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md`; prose bar `ANTI_SLOP.md`) + the
`rust-patterns` / `rust-testing` skills.

- **Read order:** `POSTCOMPACTION.md` → `README.md` → `CLAUDE.md` → `AGENTS.md`. Map: `docs/CODEMAPS/*`. Terms: `../humm-tauri/GLOSSARY.md`.
- **Change gravity (LOAD-BEARING):** editing the **integrity** zome (`zomes/integrity/`) changes the DNA hash and **forks the chain** — only for a sanctioned new pass + migration, never a drive-by. The **coordinator** (`zomes/coordinator/`) is hot-swappable. Wire shapes: add fields with `#[serde(default)]`, remove only via migration.
- **No panics in guest code:** `?` / `ExternResult` over `.unwrap()` / `.expect()` (a panic traps the WASM guest).
- **No silent-swallow:** never `let _ = call()`, `if let Err(_)`, `.ok();`, or a masking `unwrap_or_default` that drops an `Err`.
- **Exhaustive matching:** no catch-all `_ =>` arm for business enums.
- **Iterators over loops.** HDK `debug!` / `warn!` for logs (there is no LoggingService here).
- **Crypto HARD RULE:** never NIST curves (P-256 / secp256r1 / …). Approved: Curve25519 / Ed25519 / X25519 / XChaCha20-Poly1305 / Argon2id / HKDF-SHA512 / OsRng.
- **TS tests:** no `any`. **Size:** functions ≤ ~50 lines.
- **Git:** commit-local only — **never push** without an explicit instruction. Tabs, LF, single trailing newline.
- **Filesystem:** never read/write outside the two clones; on WSL work in `~/humm-earth-core-happ/...`, never `/mnt/c/Users/...`.

## Commands

| Command | File | Purpose |
|---|---|---|
| `/update-codemaps` | `.claude/commands/update-codemaps.md` | Scan codebase, generate/update `docs/CODEMAPS/`, diff detection, freshness headers |
| `/skill-health` | `.claude/commands/skill-health.md` | Skill-portfolio health dashboard — success rates, failure clustering, staleness |

## Agents

| Agent | Model | Purpose |
|---|---|---|
| `doc-updater` | haiku | Codemap + documentation specialist. Runs `/update-codemaps`, generates `docs/CODEMAPS/*`, updates READMEs. |
| `rust-reviewer` | sonnet | Rust code reviewer: ownership, lifetimes, error handling, unsafe, concurrency, idiomatic patterns. MUST use for Rust changes. |
| `rust-build-resolver` | sonnet | Fixes cargo build errors, borrow checker issues, Cargo.toml problems. Surgical minimal changes only. |
| `code-reviewer` | sonnet | General correctness/quality/maintainability review over a diff; ties the specialist lanes together (defers Rust idioms to rust-reviewer). |
| `security-reviewer` | sonnet | Holochain/Rust security: validator authority, spoofing, cap-grant scope, unsafe, secrets. Applies `skill://security-review`. |
| `silent-failure-hunter` | sonnet | Hunts swallowed errors / over-tolerant decodes / masking fallbacks (`.ok()` dropping `Err`, `unwrap_or_default`, etc.). |
| `typescript-reviewer` | sonnet | Read-only reviewer for the ONLY TS surfaces (tryorama harness `tests/src/**`, `scripts/*.ts`, hook/config TS): type safety, async correctness, error handling, test quality. |
| `librarian` | pi/smol | Source-reading research on external libraries/APIs (hdk, holochain, tryorama internals); returns source-verified answers. omp provides it built-in; this file wires native Claude Code / Codex. |
| `technical-researcher` | inherit | Ecosystem research: GitHub repos, crates.io/docs.rs, API docs, version histories, implementation comparisons. |

## Skills

| Skill | When to use |
|---|---|
| `rust-patterns` | Writing/reviewing Rust: ownership, error handling, traits, concurrency, module layout |
| `rust-testing` | Unit tests, integration tests, TDD cycle, parameterized tests, coverage |
| `security-review` | Auth, user input, secrets, API endpoints, sensitive data — security checklist |
| `coding-standards` | Baseline index over the root canon (`CODING_STANDARDS.md` + addendum): naming, readability, immutability, DRY/KISS/YAGNI, code-smell review |
| `verification-loop` | Post-change verification: build → types → lint → tests → security → diff review |
| `strategic-compact` | When to `/compact` — phase boundaries, not mid-implementation |
| `standard-workflow` | Default coordination + review workflow for non-trivial / multi-phase work: full-context dispatch, parallel subagents, gating reviewer lanes, the cargo/Sweettest/nix gate ladder, closing docs pass |
| `update-docs-workflow` | Full docs-freshness pass (run before a release/merge): stale-mention audit, `.newTasks`/`.doneTasks` reconcile, codemap refresh, DNA-hash/happ-sha/version pin verification |
| `slop-scan` | Prose-quality pass against `ANTI_SLOP.md` before shipping docs, commit bodies, `.newTasks` specs, or mbox replies |
| `search-first` | Research-before-coding: check hdk/holochain/existing helpers (via `librarian`/`technical-researcher`) before writing custom code |

## What's NOT here (and why)

- **nondominium-holochain-dna-dev** — ValueFlows/REA economic resource modeling; different domain from our encrypted-content/hive/group patterns.
- **Frontend skills** (react-expert, frontend-patterns, web-state-mobx, etc.) — no UI in this repo; humm-tauri owns the frontend. `typescript-reviewer` here is deliberately scoped to the test/tooling TS only.
- **Backend-web skills** (api-design, nestjs-patterns, etc.) — this is Holochain WASM, not HTTP services.
