# Project-scoped Claude agents

Subagents Claude Code discovers at this path when you work inside
`humm-earth-core-happ`. Cloned from a maintainer's `~/.claude/agents/` and
adapted for this repo so every contributor gets the same review surface — no
"my Claude found the bug, yours didn't" delta.

## What's here

This is a Rust / Holochain DNA repo: one integrity zome + one coordinator zome,
packaged into a `.happ` and consumed by `../humm-tauri/`. The only TypeScript is
the tryorama/Vitest test harness (`tests/src/**`) plus `scripts/*.ts`. The roster
reflects that — Rust/HDK review is the load-bearing lane, and there are no
browser/React reviewers here.

| Agent | When Claude reaches for it | Model |
|---|---|---|
| `code-reviewer.md` | General first-pass review over a git diff — correctness, quality, maintainability, AI-generated-code smells. Ties the specialist lanes together and flags what falls between them. | sonnet |
| `rust-reviewer.md` | Rust/zome review — ownership, lifetimes, error handling, `unsafe`, and idiomatic HDK/HDI patterns. The primary lane; MUST be used for Rust changes. | sonnet |
| `security-reviewer.md` | Security review for the DNA — validator authority, identity/spoofing, capability-grant scope, `unsafe`, and secret handling. Gating lane on any zome change. Applies `skill://security-review`. | sonnet |
| `silent-failure-hunter.md` | Targeted hunt for silently-swallowed failures — dropped errors, masking fallbacks, results discarded without propagation. Reach for it after a `code-reviewer` pass when you suspect error-handling slop. | sonnet |
| `typescript-reviewer.md` | Review for the repo's only TypeScript — the tryorama/Vitest harness (`tests/src/**`), `scripts/*.ts`, hook/config TS. Type safety, async correctness, error handling, test quality. No frontend scope. | sonnet |
| `rust-build-resolver.md` | Rust build, compilation, and dependency error resolution — fixes `cargo build`/borrow-checker/`Cargo.toml` problems with minimal, surgical changes. Use when a build fails. Edits code. | sonnet |
| `doc-updater.md` | Documentation and codemap specialist — runs `/update-codemaps` / `/update-docs`, regenerates `docs/CODEMAPS/*`, refreshes READMEs and guides from the source of truth. Edits docs. | haiku |
| `librarian.md` | External library / API research by reading source code — returns definitive, source-verified answers with verbatim excerpts. Cloned verbatim from `can1357/oh-my-pi`; native to the omp harness (see the header note for other harnesses). | inherit (pi/smol native) |
| `technical-researcher.md` | Repo + library-version + API-surface research over GitHub, crates.io, and docs sites. Uses `gh` CLI and `git clone` for source-level inspection; tracks version histories and comparative implementations. | inherit |

## Model tiers

- **sonnet** — the review lanes (`code-reviewer`, `rust-reviewer`,
  `security-reviewer`, `silent-failure-hunter`, `typescript-reviewer`) and the
  `rust-build-resolver`. These carry the judgment calls.
- **haiku** — `doc-updater`. Mechanical regeneration from the source of truth.
- **inherit** — the researchers (`librarian`, `technical-researcher`) run on
  whatever model the session selects. `librarian` is native to the omp harness
  and resolves on `pi/smol` there.

## Read-only vs editing

Reviewers, the hunter, and the `librarian` have **read-only** tools (Read, Grep,
Glob, Bash). They report findings; the main session decides what to apply. That
separation is why a finding from them is signal — the reviewer has no stake in
whether the diff lands.

`rust-build-resolver`, `doc-updater`, and `technical-researcher` edit files (fix
a build, regenerate a codemap, write research notes), so they carry write tools.

## How to use

In your message to Claude, ask for a review and name the agent if you want a
specific one:

> review the staged diff with the rust-reviewer agent

Or let Claude pick — the `description` field in each agent's YAML front-matter
tells the orchestrator when to invoke it automatically.

For high-stakes work — anything that changes the DNA hash (integrity-zome
changes), capability-grant scope, or the wire format — run **two in parallel**:
`rust-reviewer` plus `security-reviewer`. Integrity-zome changes fork the chain
(see the change-gravity rule in `AGENTS.md` / `CLAUDE.md`), so they get the
widest review. The `standard-workflow` skill drives this parallel reviewer-lane
fan-out and the fix waves that follow.

## How they work

Each `.md` file is a self-contained agent definition. YAML front-matter names the
agent, declares the model it runs on, and gives the orchestrator a `description`
it matches against the user's request. The body is the system prompt the agent
runs with.

## See also

`.claude/skills/` — project-scoped skills cloned for the same reason these agents
are. The ones that earn their place here:

- `standard-workflow/SKILL.md` — the coordination loop that wraps these agents
  into review lanes + fix waves. Read first before starting or committing
  non-trivial work.
- `search-first/SKILL.md` — research-before-coding workflow that the `librarian`
  and `technical-researcher` agents power.
- `slop-scan/SKILL.md` — prose-quality pass against the repo-root `ANTI_SLOP.md`
  before docs / commit bodies / mailbox replies ship.
- `verification-loop/SKILL.md` — the build → clippy → test gate ladder this repo
  runs before a change lands.

## Provenance and updating

Most of these definitions originate from two upstream sources:

- **[affaan-m/everything-claude-code](https://github.com/affaan-m/everything-claude-code)**
  — the community bundle of Claude Code agents and skills. The reviewer agents
  (`code-reviewer`, `rust-reviewer`, `typescript-reviewer`, `security-reviewer`,
  `silent-failure-hunter`) and `rust-build-resolver` / `doc-updater` are adapted
  from there, retargeted to this repo's Rust/Holochain surface.
- **[can1357/oh-my-pi](https://github.com/can1357/oh-my-pi)** — the omp harness.
  `librarian.md` is cloned verbatim from its
  `packages/coding-agent/src/prompts/agents/` directory; the `tools` and `model`
  fields may need translating for non-omp harnesses (see the header block in the
  file).

The project-local copies are authoritative for this repo. To pull upstream
improvements: re-copy from the source, trim `tools` to read-only
(`["Read", "Grep", "Glob", "Bash"]`) for reviewer agents, retarget any
web/frontend scope to the Rust/Holochain surface, and commit.

If you find a bug class an existing agent missed, either append a "Known patterns
this repo has hit" section to that agent's body, or file a new agent under this
directory with a tight scope. Either way, commit it — the agents are part of the
project's shared review surface. The mirror for a native Codex session lives in
`.codex/agents/` (thin adapters that point back at these `.md` files).
