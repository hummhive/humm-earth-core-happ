# humm-earth-core-happ TTSR rules (`.omp/`)

TTSR ("Time-Travelling Stream Rules") rules that enforce this repo's standards at
**code-generation time**: when an agent's output stream matches a rule's
`condition`, the stream is either interrupted (comply before the edit lands) or
allowed through with a reminder appended. This is the
[oh-my-pi](https://omp.sh/docs/ttsr) mechanism.

Adapted from the humm-tauri TTSR set for this **Rust / Holochain DNA** repo: the
TS/React/MobX/Tauri rules were dropped as inapplicable, the direct-tracing rule was
dropped (zomes log via the HDK `debug!`/`warn!` macros — there is no
`LoggingService` here), and two core-happ-specific rules were added. The `.claude/`
agents + skills (`rust-reviewer`, `rust-patterns`, `silent-failure-hunter`, …)
already cover review; this layer is the write-time enforcement + always-on context
they didn't have.

## How a rule works

Each `rules/*.md` is frontmatter + a markdown body:

```markdown
---
description: short summary
condition: "regex"            # or a YAML list (OR'd); matched against the output stream
scope: "tool:edit(*.rs), tool:write(*.rs)"   # which streams may trigger
interruptMode: never          # optional; default is the global mode (always)
---
The body is the instruction injected when the rule fires.
```

- **`condition`** — JS regex(es) tested against the accumulating output buffer.
- **`scope`** — stream tokens: `text`, `thinking`, `tool`/`toolcall`, or
  `tool:edit(<glob>)` / `tool:write(<glob>)`. Every code rule here scopes to
  `tool:edit`/`tool:write`, so the model can *discuss* a pattern in prose — only a
  real edit triggers.
- **`interruptMode`** — `always` (default) aborts the stream and forces compliance
  *before* the edit lands; `never` lets it land and appends a `<system-reminder>`.
- **Repeat** — `config.yml` sets `repeatMode: after-gap` / `repeatGap: 8`, so a rule
  re-reminds every ~8 turns a violation recurs (not just once per session).

## Active rules (`rules/`)

| Rule | Source | Mode | Triggers on |
| --- | --- | --- | --- |
| `integrity-zome-guard` | change-gravity (NEW) | always | ANY edit under `zomes/integrity/**` |
| `rs-no-unwrap` | rust-patterns (NEW) | never | `.unwrap()` in `zomes/**` |
| `rs-no-silent-swallow` | silent-failure-hunter | never | `let _ = call()`, `if let Err(_)`, `.ok();` |
| `rs-no-nist-curve` | crypto HARD RULE | always | `secp256r1`/`prime256v1`/`P-256`/`p256`… |
| `comment-no-narration` | coding-standards | never | 2+ line block / consecutive `//` comments |
| `dry-duplicate-block` | coding-standards DRY | never | a 3-line block repeated within one edit |
| `ts-no-any` | coding-standards | always | `: any` / `as any` (tryorama TS) |

Plus `rules/repo-standards.md` (`alwaysApply: true`) — not a matcher; the always-on
digest (see below).

### The two core-happ-specific rules

- **`integrity-zome-guard`** hard-interrupts on ANY edit under `zomes/integrity/`,
  because an integrity change alters the DNA hash and forks the chain. It is NOT a
  "never": sanctioned passes (e.g. the planned single-owner role with handoff
  handshake) are expected — it guards against an *accidental / drive-by* fork. The
  body explains the pass + migration + multi-user-validation requirements.
- **`rs-no-unwrap`** nudges `?` / `ExternResult` over `.unwrap()` in zome code (a
  panic traps the WASM guest). Soft (`never`); `#[cfg(test)]` + `crates/sweettest`
  exempt.

### Skipped from the humm-tauri set (and why)

`rs-no-direct-tracing` (zomes use HDK macros; no `LoggingService`) ·
`ts-no-console` (this repo's `coding-standards` skill *allows* `console.error`) ·
`ts-no-foreach`, `ts-no-mui-barrel`, `mobx-reactive-reads` (no React / MUI / MobX) ·
`ts-no-namespace-import`, `ts-no-wildcard-reexport`, `ts-no-bare-catch`,
`ts-no-eslint-disable`, `ts-test-no-cwd-path`, `ts-prefer-async-await`
(humm-tauri-specific TS conventions, not this repo's standards; the TS surface is a
small tryorama harness).

## Always-on context (sticky rule + hooks)

A context-injection layer so the model always has the standards, change-gravity,
build, and (on WSL) the workflow in front of it — full writeup in
`../CONTEXT-INJECTION.md`:

- **`rules/repo-standards.md`** (`alwaysApply`) — the hard-rules digest +
  change-gravity + read-order + codemap map, injected into every turn. Silence via
  `ttsr.disabledRules: [repo-standards]` if ever too heavy.
- **`hooks/wsl-session-context.ts`** (oh-my-pi `session_start`) + the shared
  **`hooks/session-context.mjs`** (repo root) — inject read-order + hard rules +
  change-gravity always, and the WSL two-clone workflow + a live hApp-sha check only
  on a WSL host. `.claude/settings.json` SessionStart wires the same script for
  Claude Code.

## Configuration (`config.yml`)

`config.yml` sets `repeatMode: after-gap` + `repeatGap: 8` (re-reminder cadence) and
`builtinRules: false` (curated set only — see the file's comment for opting omp's
Rust builtins like `rs-parking-lot` back in). All knobs are the `ttsr.*` settings
group; set them here or globally via `omp config set ttsr.<key> <value>`. To silence
a rule without deleting it: `ttsr.disabledRules: [<name>]`.

## Lifecycle

omp reads `.omp/` (rules + config) **once at session creation** — no file watcher,
no in-session reload. Start or resume a session to pick up rule/config edits.

## Deployment

This `.omp/` already lives in the repo root, so omp auto-loads it when run with the
repo as cwd. The context hook is wired in `.claude/settings.json` (Claude Code) and
`.omp/hooks/wsl-session-context.ts` (oh-my-pi), both running
`hooks/session-context.mjs`. On the WSL two-clone setup it syncs with the normal
`scripts/wsl-{pull,push}.sh` flow (these are plain text config files — safe to sync,
unlike `target/`).
