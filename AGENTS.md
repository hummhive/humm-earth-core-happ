# Agent Toolkit — humm-earth-core-happ

Quick reference for the local `.claude/` skills, agents, and commands
available in this repo. All copied from `~/.claude/` (global ECC install)
and curated for Holochain DNA + Rust development.

## Commands

| Command | File | Purpose |
|---|---|---|
| `/update-codemaps` | `.claude/commands/update-codemaps.md` | Scan codebase, generate/update `docs/CODEMAPS/`, diff detection, freshness headers |

## Agents

| Agent | Model | Purpose |
|---|---|---|
| `doc-updater` | haiku | Codemap + documentation specialist. Runs `/update-codemaps`, generates `docs/CODEMAPS/*`, updates READMEs. |
| `rust-reviewer` | sonnet | Rust code reviewer: ownership, lifetimes, error handling, unsafe, concurrency, idiomatic patterns. MUST use for Rust changes. |
| `rust-build-resolver` | sonnet | Fixes cargo build errors, borrow checker issues, Cargo.toml problems. Surgical minimal changes only. |

## Skills

| Skill | When to use |
|---|---|
| `rust-patterns` | Writing/reviewing Rust: ownership, error handling, traits, concurrency, module layout |
| `rust-testing` | Unit tests, integration tests, TDD cycle, parameterized tests, coverage |
| `security-review` | Auth, user input, secrets, API endpoints, sensitive data — security checklist |
| `coding-standards` | Naming, readability, immutability, DRY/KISS/YAGNI, code-smell review |
| `verification-loop` | Post-change verification: build → types → lint → tests → security → diff review |
| `strategic-compact` | When to `/compact` — phase boundaries, not mid-implementation |

## What's NOT here (and why)

- **nondominium-holochain-dna-dev** — ValueFlows/REA economic resource modeling; different domain from our encrypted-content/hive/group patterns.
- **Frontend skills** (react-expert, frontend-patterns, etc.) — no UI in this repo; humm-tauri owns the frontend.
- **Backend-web skills** (api-design, nestjs-patterns, etc.) — this is Holochain WASM, not HTTP services.
