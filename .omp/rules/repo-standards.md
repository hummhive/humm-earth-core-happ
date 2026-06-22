---
description: humm-earth-core-happ — always-on hard-rules digest, change-gravity, read-order, and codemap map
alwaysApply: true
---

# humm-earth-core-happ — keep this in context, every session

Holochain DNA for HummHive: one DNA (`humm_earth_core`) = one integrity zome
(`content_integrity`) + one coordinator zome (`content`), packaged into a `.happ`
consumed by `../humm-tauri/`. hdi 0.7 / hdk 0.6 (holonix 0.6).

## Read order (before non-trivial work)

`POSTCOMPACTION.md` (current state) → `README.md` → `CLAUDE.md` → `AGENTS.md`
(toolkit). Skills: `coding-standards`, `rust-patterns`, `rust-testing`. Map:
`docs/CODEMAPS/{architecture,backend,data,dependencies}.md`. Terms:
`../humm-tauri/GLOSSARY.md` (shared ontology — Hive, Member, Group, Content,
Sidecar, Node, Cell, DNA, Agent, ACL/AclSpec).

## Change gravity (the load-bearing rule)

- **Integrity zome (`zomes/integrity/`)** — changing it ALTERS THE DNA HASH and
  FORKS the chain: a new **pass** + migration pipeline + multi-user validation.
  Allowed for a SANCTIONED change (new authority model / role / `AclSpec` — e.g.
  the planned single-owner role), NEVER as a drive-by. Update the pass lineage in
  `CLAUDE.md` + `architecture.md`.
- **Coordinator zome (`zomes/coordinator/`)** — hot-swappable, does NOT change the
  DNA hash. Prefer it; backwards-compatible changes preferred.
- **Wire shapes** — add fields with `#[serde(default)]`; remove only via a
  versioned migration.

## Rust hard rules

- **Errors:** `?` / `ExternResult` over `.unwrap()` (a panic traps the WASM guest).
  Never silently swallow — no `let _ = call()`, `if let Err(_)`, trailing `.ok();`,
  or `unwrap_or_default()` that masks a real failure; propagate or log
  (`warn!`/`debug!`) + handle. `thiserror` (lib) / `anyhow` (app).
- **Matching:** exhaustive — no wildcard `_ =>` for business-logic enums (adding a
  variant must force a compile error).
- **Idiom:** iterator chains over manual loops; borrow don't clone; newtypes for ID
  safety; minimal `pub` (`pub(crate)`); `#[must_use]` on Results; `unsafe` only with
  a `# Safety` doc.
- **Logging:** zome logs go through the HDK macros (`debug!` / `warn!` / `trace!`
  from `hdk::prelude`) — there is NO LoggingService here (that's humm-tauri). Keep
  logs meaningful.
- **Crypto (HARD RULE, project-wide):** never NIST curves (P-256/384/521,
  secp*r1, prime256v1, the `p256`/`p384`/`p521` crates). Agent signing is Ed25519
  via the HDK.

## Quality + tests

- Self-documenting code over comments (WHY, never WHAT; `///` docs lead with one
  sentence). DRY / KISS / YAGNI. Named constants (no magic numbers). Functions
  ≤~50 lines, early returns (no deep nesting). No `any` in the TS tests.
- Tests: tryorama (Vitest) in `tests/src/**`, plus in-process conductor via
  `crates/sweettest` (tryorama can't boot on hc 0.6.0). Given/When/Then or AAA,
  descriptive names, test-first.

## Build & git

- Build (inside `nix develop`): `npm run build:zomes` (cargo wasm + wasm-opt strip)
  → `hc app pack workdir --recursive`. Reproducible (RUSTFLAGS remap + strip →
  deterministic DNA hash). `npm test`.
- Commit-local, **never push**. Conventional commits (`feat(integrity):` /
  `feat(coordinator):` / `chore(build):` / `docs:`). Multi-line via temp file. Tabs
  / LF / single trailing newline.
- WSL: build/test in `~/humm-earth-core-happ` (NEVER `/mnt/c` — corrupts `target/`);
  sync via `scripts/wsl-{pull,push,check}.sh`, never manual cp / cross-clone commit.
  Allowed scopes are the two clones ONLY — never read/write outside them; subagents
  default to `~/humm-earth-core-happ`.

When this digest and the fuller skills/docs disagree, the fuller source wins. When
the docs and the code disagree, the code wins — fix the drift.
