# `.codex/` — the Claude ⇄ Codex bridge

This folder mirrors the **Claude-app-native** configuration (`.claude/`) into the
**Codex-native** surface so a Codex CLI session gets the same standing context,
the same reviewer subagents, and the same hard rules a Claude Code session gets.

It was authored here in `ttsrAndHooksScratchpad` (the same way `.omp/` was) and
deployed into `humm-tauri/` and `humm-earth-core-happ/`. Pattern + schema sourced
from [`codex-cli-best-practice`](https://github.com/shanraisshan/codex-cli-best-practice).

## Why this exists (and what it is NOT)

The **TTSR stream rules in `.omp/` are already shared** across Claude and Codex:
the user runs Codex *through oh-my-pi*, and omp enforces `.omp/rules/*` +
injects `repo-standards.md` regardless of backend. **This `.codex/` layer is not
about the TTSR rules.** It mirrors the pieces the *Claude Code app* consumes
directly — and that omp does not supply — so they also exist for a Codex session:

| Claude-native (`.claude/` + root) | Codex-native (this layer) | Notes |
| --- | --- | --- |
| `CLAUDE.md` (auto-loaded instructions) | `AGENTS.md` (auto-loaded, 32 KiB cap) | Codex reads `AGENTS.md`, never `CLAUDE.md`. The HARD RULES digest is mirrored into `AGENTS.md` so a bare Codex session sees them. |
| `.claude/settings.json` → `SessionStart` → `node hooks/session-context.mjs` | `.codex/hooks.json` → `SessionStart` → `node hooks/session-context.mjs` | **Same generator, third front-end.** omp's `.omp/hooks/wsl-session-context.ts` is the second. One source of dynamic context (read-order + WSL workflow + hApp-sha check). |
| `.claude/agents/*.md` (reviewer lanes) | `.codex/agents/*.toml` + `[agents.*]` in `config.toml` | Thin **adapters** — see below. |
| `.claude/settings.json` (model routing, token budget) | `.codex/config.toml` (`[features]`, `[agents]`, sandbox/approval) | Model is left UNSET so agents inherit the session model. |
| `.claude/commands/*.md` (slash commands) | — *(no equivalent)* | **Codex has no custom commands** (`.codex/commands/` does not exist — confirmed in the codex-best-practice README). Those workflows live as skills (`update-codemaps` is already a skill) and are invoked via `$skill-name`. |

So: **`.omp/` = shared rules (both backends, via omp). `.codex/` = the Codex-native
mirror of the Claude-app surface.** Neither duplicates the other.

## Agent role files are thin adapters (DRY)

`.codex/agents/<name>.toml` does **not** re-state the review checklist. The
canonical spec stays in `.claude/agents/<name>.md` (one source, both harnesses —
no drift). Each TOML only (1) registers the agent for Codex, (2) pins the sandbox,
(3) points `developer_instructions` at the `.md`. Schema (per
`best-practice/codex-subagents.md`): required `name` (snake_case — Codex agent-path
addressing allows `[a-z0-9_]`), `description`, `developer_instructions`; optional
`sandbox_mode`, `model`, `model_reasoning_effort`. Read-only lanes (reviewers,
hunters, librarian) pin `sandbox_mode = "read-only"`; agents that edit
(build-resolver, doc-updater, researchers) omit it to inherit. See
[`agents/README.md`](agents/README.md) for the exact template.

## Validate

From the repo root: `node verify-codex.mjs` (lives at the scratchpad root; copied
into each deployed repo). It checks `codex_hooks = true`, that every `[agents.*]`
`config_file` resolves, that `hooks.json` is valid JSON and its command scripts
exist, that every agent TOML has the 3 required fields + a snake_case name + a
registration, and that `AGENTS.md` is ≤ 32 KiB. Parallels `verify-rules.mjs`
(the `.omp/` self-test).

## Deploy (per repo)

1. Copy `.codex/{config.toml, hooks.json, README.md}` to the repo root's `.codex/`.
2. For each `.claude/agents/<name>.md`, write `.codex/agents/<name>.toml` (thin
   adapter) and append an `[agents.<snake_name>]` block to `config.toml`.
3. Prepend the HARD RULES digest to `AGENTS.md` (near the top — Codex truncates
   the tail).
4. Copy `verify-codex.mjs` to the repo root and run it; expect `0 fail`.
5. `.codex/` syncs with the normal `wsl-push.sh` / `wsl-pull.sh` flow once
   committed — no per-clone copy.

> The authoring template lives in `ttsrAndHooksScratchpad/.codex/` (no project
> agents there — `verify-codex.mjs` reports the empty-agents case as a `warn`). This
> deployed copy carries the repo's real reviewer/build/doc lanes under `agents/`.
