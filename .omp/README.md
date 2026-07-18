# humm-earth-core-happ TTSR rules (`.omp/`)

TTSR ("Time-Travelling Stream Rules") rules that enforce this repo's standards at
**code-generation time**: when an agent's output stream matches a rule's
`condition`, the stream is either interrupted (comply before the edit lands) or
allowed through with a reminder appended. This is the
[oh-my-pi](https://omp.sh/docs/ttsr) mechanism.

Adapted from the humm-tauri TTSR set for this **Rust / Holochain DNA** repo. The
frontend-only rules (React / MobX / MUI) and the `LoggingService`-specific rules
were dropped as inapplicable — zomes log via the HDK `debug!`/`warn!`/`trace!`
macros, there is no `LoggingService` here — and core-happ-specific rules were added
(`integrity-zome-guard`, `rs-no-unwrap`). The TypeScript rules that *do* apply were
kept and re-pointed at this repo's only TS surface: the tryorama / Vitest harness
(`tests/src/**`) plus `scripts/*.ts` (there is no JSX, so `*.tsx`/`*.jsx` are out of
scope). The `.claude/` agents + skills (`rust-reviewer`, `rust-patterns`,
`silent-failure-hunter`, …) already cover review; this layer is the write-time
enforcement + always-on context they didn't have.

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

`always` = hard interrupt before the edit lands; `never` = reminder appended after.
Every TS rule scopes to `*.ts` only (the tryorama / Vitest harness + `scripts/*.ts`);
every Rust rule to `*.rs`.

| Rule | Source | Mode | Triggers on |
| --- | --- | --- | --- |
| `integrity-zome-guard` | change-gravity (NEW) | always | ANY edit under `zomes/integrity/**` |
| `rs-no-unwrap` | rust-patterns (NEW) | never | `.unwrap()` in `zomes/**` |
| `rs-no-silent-swallow` | silent-failure-hunter | never | `let _ = call()`, `if let Err(_)`, `.ok();` |
| `rs-no-nist-curve` | crypto HARD RULE | always | `secp256r1`/`prime256v1`/`P-256`/`p256`… |
| `rs-fn-too-long-warn` | ADDITIONAL §File/Function Size | never | Rust fn body 51–60 lines (one write) |
| `rs-fn-too-long` | ADDITIONAL §File/Function Size | always | Rust fn body >60 lines (one write) |
| `ts-fn-too-long-warn` | ADDITIONAL §File/Function Size | never | TS fn 51–60 lines (one write) |
| `ts-fn-too-long` | ADDITIONAL §File/Function Size | always | TS fn >60 lines (one write) |
| `file-too-long-warn` | ADDITIONAL §File/Function Size | never | file 501–600 lines (one write) |
| `file-too-long` | ADDITIONAL §File/Function Size | always | file >600 lines (one write) |
| `comment-no-narration` | coding-standards | never | 2+ line block / consecutive `//` comments |
| `dry-duplicate-block` | coding-standards DRY | never | a 3-line block repeated within one edit |
| `ts-no-any` | coding-standards | always | `: any` / `as any` (tryorama TS) |
| `ts-prefer-async-await` | coding-standards | always | `new Promise(`, `.then(`, `.catch(` |
| `ts-no-bare-catch` | coding-standards | always | `catch {`, `.catch(() =>` |
| `ts-no-namespace-import` | coding-standards | always | `import * as` |
| `ts-no-wildcard-reexport` | coding-standards | always | `export * from` |
| `ts-no-foreach` | coding-standards | always | `.forEach(` |
| `ts-test-no-cwd-path` | coding-standards (tests) | always | `process.cwd()`, `fileURLToPath(import.meta.url)` in `*.test.ts` |

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

## File & function size rules (two-tier: warn / hard-stop)

`file-too-long[-warn]` + `rs/ts-fn-too-long[-warn]` enforce
`ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` §File/Function Size as heuristic stream
rules — the warn tier (`never`, files >500 / fns >50) reminds; the hard tier
(`always`, files >600 / fns >60) interrupts before the edit lands. This repo's own
standard puts the *should* line at ≤~50 for functions. The file rules scope to
`*.{ts,js,rs}` (the frontend-only `*.tsx`/`*.jsx`/`*.css` extensions are dropped); the
fn rules to `*.rs` and `*.ts` respectively.

The matcher sees only what one tool call carries: a `write`'s full content, but an
`edit`'s **changed region only** — so a small edit to an existing oversized file/fn
does NOT trip them (and a hard-stop can't block an incremental, edit-by-edit
modularization). The `fn` rules key on rustfmt/prettier brace alignment and exclude
control-flow keywords; they're soft heuristics, so a wrapped signature can slip by.
With `after-gap` repeat a hard rule interrupts at most once per window, so a full-file
reductive rewrite lands on retry. There is **no self-certified size exception**: a file
you believe genuinely can't be split (a generated data table) is added to an explicit
allow-list by name, in review — never waved through at the keyboard (the rule body says
the same).

## Deliberately not adopted (and why)

The humm-tauri set carries several rules that encode conventions this repo does not
have. They were left out on purpose:

- **`rs-no-direct-tracing`** — contradicts this repo's logging convention. Zomes log
  through the HDK macros (`debug!` / `warn!` / `trace!` from `hdk::prelude`); there is
  no `LoggingService` and no `tracing::` layer to route through (that's humm-tauri).
- **`ts-no-console`** — `LoggingService`-specific. There is no such logger in the TS
  harness, and this repo's `coding-standards` allows `console.error` in tests/scripts.
- **`ts-no-mui-barrel`** — no frontend, no MUI.
- **`mobx-reactive-reads`** (and the other MobX / web-state-adjacent rules) — no
  React, no MobX, no reactive client state in this repo.
- **`ts-no-eslint-disable`** — there is no ESLint config in this repo (no
  `eslint.config.*` / `.eslintrc*`, no eslint dependency), so the rule has nothing to
  guard. Adopt it if/when ESLint is introduced to the TS harness.

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
Rust builtins like `rs-parking-lot` back in, and note omp's bundled `ts-bare-catch` /
`ts-promise-with-resolvers` defaults *contradict* the rules here, which is why the
builtins are off). All knobs are the `ttsr.*` settings group; set them here or
globally via `omp config set ttsr.<key> <value>`. To silence a rule without deleting
it: `ttsr.disabledRules: [<name>]`.

## Verifying the rules

The self-test + reality-scan scripts (`verify-rules.mjs`, `find-duplication.mjs`)
live in the humm-tauri scratchpad where this rule set was authored; they are not part
of this `.omp/` and were not ported (no scanner harness ships in this repo). The
practical check here is the design itself: every code rule scopes to
`tool:edit`/`tool:write`, so a rule fires only on a *real* edit — the model can
discuss `forEach` or `.unwrap()` in prose without tripping anything, and a matcher
false-positive surfaces immediately as an interrupt on clean code. Start a session and
make a violating edit to confirm a rule is live.

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
