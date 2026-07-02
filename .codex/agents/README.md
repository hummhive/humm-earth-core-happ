# `.codex/agents/` — thin Codex adapters for the shared reviewer specs

One `<name>.toml` per `.claude/agents/<name>.md`. The `.md` is the **single source
of truth** (Claude Code resolves it natively; this TOML makes the same agent
available to a native Codex session). The TOML carries no review checklist — it
points back at the `.md` so the two harnesses never drift.

In the authoring scratchpad this directory holds only this doc (no project agents
live there). In a deployed repo (`humm-tauri/`, `humm-earth-core-happ/`) it holds
one `<name>.toml` adapter per `.claude/agents/<name>.md`.

## Template

```toml
# Codex adapter for the shared reviewer spec in .claude/agents/<name>.md.
# DRY: the .md is the single source of truth — this file only registers the
# agent for Codex + pins the sandbox. Do not copy the checklist here.
name = "code_reviewer"                          # snake_case (agent-path addressing)
description = "<the .md frontmatter `description`, verbatim>"
sandbox_mode = "read-only"                       # read-only lanes only; omit for editing agents
developer_instructions = """
You are the code-reviewer agent for this repo. Before doing anything, READ
`.claude/agents/code-reviewer.md` IN FULL — it is your complete, authoritative
specification (review process, checklist, output format, approval criteria).
Follow it exactly. Honor the standing rules in AGENTS.md + CODING_STANDARDS.md +
ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md. Review and report only — do not
modify files.
"""
```

And register it in `../config.toml`:

```toml
[agents.code_reviewer]
description = "<same description>"
config_file = "agents/code-reviewer.toml"
```

## Conventions

- **`name`**: snake_case of the `.md` name (`silent-failure-hunter` → `silent_failure_hunter`).
- **Filename**: keep the hyphenated `.md` stem (`code-reviewer.toml`).
- **`sandbox_mode`**: `read-only` for review/research/hunter/librarian lanes;
  omit (inherit `workspace-write`) for agents that edit (build-resolver, doc-updater).
- **`model`**: omit — inherit the session model. Never hardcode a slug.
- **`developer_instructions`**: a 2–4 line stub that names the agent's job and
  orders it to read the `.md` first. Never paste the `.md` body.
