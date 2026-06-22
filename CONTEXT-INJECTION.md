# Context injection — keeping standards, change-gravity & the WSL workflow in front of the model

Two layers solve "the model keeps needing reminding of the standards / change
gravity / build / WSL workflow" for humm-earth-core-happ, each at the right altitude:

| Layer | Artifact | What it does | Where |
|---|---|---|---|
| Static, always-on | `.omp/rules/repo-standards.md` (`alwaysApply: true`) | Injects the hard-rules digest + change-gravity + read-order + codemap map into EVERY turn | oh-my-pi (auto, once `.omp` is the repo's config dir) |
| Dynamic, conditional | `hooks/session-context.mjs` | Prints read-order + hard rules + change-gravity always; on a WSL host adds the two-clone workflow + a live hApp-sha check | run by either wiring below |
| Wiring | `.omp/hooks/wsl-session-context.ts` (oh-my-pi) + `.claude/settings.json` SessionStart (Claude Code) | Run `session-context.mjs` at session start and inject its output | session start |

## 1. `repo-standards.md` — the always-on digest (oh-my-pi)

An `alwaysApply` rule's full body is injected into the system prompt every session.
It carries the change-gravity rule (integrity forks the chain — sanctioned pass
only), the Rust hard rules (`?` over `.unwrap()`, no silent-swallow, exhaustive
matching, HDK logging, no NIST curves), the build (`nix develop` → `build:zomes` →
`hc app pack`), and the read-order + `docs/CODEMAPS/*` + GLOSSARY map. It spends
tokens every turn — the deliberate trade for "always present". Disable via
`ttsr.disabledRules: [repo-standards]` if ever too heavy, or trim its body.

## 2. `session-context.mjs` — the dynamic / host-conditional bit

A zero-dependency, cross-platform Node script that emits session context to stdout.
It always prints the read-order + hard-rules + change-gravity line; on a WSL host
(detected via `WSL_DISTRO_NAME` / `WSL_INTEROP` / `/proc/version`) it adds the
two-clone workflow (work in `~/humm-earth-core-happ`, sync scripts, never `/mnt/c`,
the `nix develop` build) and runs a hApp sha256 check on `workdir/…happ`, pointing
at `~/hummhive-official-happ-versions/MANIFEST.tsv` (current line: pass-4, DNA
`uhC0k26b…`). Keeping this OUT of the always-on rule is deliberate: it's WSL-specific
and genuinely dynamic (the sha changes), so it belongs at session start.

## 3. Wiring

### oh-my-pi
`.omp/hooks/wsl-session-context.ts` registers a `session_start` handler that
`pi.exec`s the script and `pi.sendMessage`s its output. **Caveat:** oh-my-pi's hook
subsystem is mid-migration to the extension runner (`docs/hooks.md` — `--hook` may
alias to `--extension`); verify `pi.exec` / `pi.sendMessage` against
`src/extensibility/hooks/types.ts` for your build.

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

The command's stdout is added to the session context. `CLAUDE.md` already points at
the standards; this re-states the hard rules + change-gravity + WSL workflow up
front each session.

## Why two layers, not one

- A rule can't run a command (no live hApp-sha, no WSL detection) → the script.
- A hook fires once at session start, not every turn → the `alwaysApply` rule keeps
  the digest resident across compaction.
- The change-gravity + hard-rules line intentionally overlaps a little between the
  rule and the script, so each wiring is useful standalone.
