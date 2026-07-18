# Context injection — keeping standards, change-gravity & the WSL workflow in front of the model

Two layers solve "the model keeps needing reminding of the standards / change
gravity / build / WSL workflow" for humm-earth-core-happ, each at the right altitude:

| Layer | Artifact | What it does | Where |
|---|---|---|---|
| Static, always-on | `.omp/rules/repo-standards.md` (`alwaysApply: true`) | Injects the hard-rules digest + change-gravity + read-order + codemap map into EVERY turn | oh-my-pi (auto, once `.omp` is the repo's config dir) |
| Dynamic, conditional | `hooks/session-context.mjs` | Prints read-order + hard rules + change-gravity always; on a WSL host adds the two-clone workflow + a live hApp-sha-vs-MANIFEST check | run by all three wirings below |
| Wiring ×3 | `.omp/hooks/pre/wsl-session-context.ts` (oh-my-pi) · `.claude/settings.json` SessionStart (Claude Code) · `.codex/hooks.json` SessionStart (Codex) | Run `session-context.mjs` at session start and inject its stdout | session start |

## 1. `repo-standards.md` — the always-on digest (oh-my-pi)

An `alwaysApply` rule's full body is injected into the system prompt every session.
It carries the change-gravity rule (integrity forks the chain — sanctioned pass
only), the Rust hard rules (`?` over `.unwrap()`, no silent-swallow, exhaustive
matching, HDK logging, no NIST curves), the build (`nix develop` → `build:zomes` →
`hc app pack`), and the read-order + `docs/CODEMAPS/*` + GLOSSARY map. It spends
tokens every turn — the deliberate trade for "always present". Disable via
`ttsr.disabledRules: [repo-standards]` if ever too heavy, or trim its body.

The canonical standards themselves live at the repo root: `CODING_STANDARDS.md`
+ `ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` (+ `ANTI_SLOP.md` for prose).
The rule is the digest; the root docs are the contract.

## 2. `session-context.mjs` — the dynamic / host-conditional bit

A zero-dependency, cross-platform Node script that emits session context to stdout.
It always prints the read-order + hard-rules + change-gravity lines; on a WSL host
(detected via `WSL_DISTRO_NAME` / `WSL_INTEROP` / `/proc/version`) it adds the
two-clone workflow and compares the built `workdir/….happ` sha256 against the
LAST row of `~/hummhive-official-happ-versions/MANIFEST.tsv` (the current
generation), printing MATCHES or DIFFERS with the generation label. The manifest
row is read live — no hardcoded pass label to go stale. Keeping this OUT of the
always-on rule is deliberate: it's WSL-specific and genuinely dynamic, so it
belongs at session start.

## 3. Wiring

### oh-my-pi
`.omp/hooks/pre/wsl-session-context.ts` registers a `session_start` handler that
`pi.exec`s the script and `pi.sendMessage`s its output.

**Placement is load-bearing:** omp's native hook discovery scans ONLY
`<cfg>/hooks/pre/` and `<cfg>/hooks/post/` (`discovery/builtin.ts`:
`hookTypes = ["pre","post"]`) — a hook sitting flat in `.omp/hooks/` is never
loaded. The `pre`/`post` folder is just the discovery location; the loader
registers whatever `pi.on(...)` declares. `pi.exec` takes `(command, args[],
options)`.

### Claude Code
`.claude/settings.json` adds a SessionStart hook:

```json
{
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command", "command": "node hooks/session-context.mjs" } ] }
    ]
  }
}
```

The command's stdout is added to the session context.

### Codex
`.codex/hooks.json` wires the same script with a startup/resume matcher:

```json
{
  "SessionStart": [
    { "matcher": "^(startup|resume)$",
      "hooks": [ { "type": "command", "command": "node hooks/session-context.mjs", "timeout": 10 } ] }
  ]
}
```

(`.codex/config.toml` must keep `codex_hooks = true` for this file to be read.)

## Why two layers, not one

- A rule can't run a command (no live hApp-sha, no WSL detection) → the script.
- A hook fires once at session start, not every turn → the `alwaysApply` rule keeps
  the digest resident across compaction.
- The change-gravity + hard-rules line intentionally overlaps a little between the
  rule and the script, so each wiring is useful standalone.
