# CLAUDE.md — Session-start brief for AI agents working on humm-earth-core-happ

You are an AI agent working on the HummHive core hApp (Holochain DNA).
Read this file at the start of every session before touching code.

---

## What this project is

**humm-earth-core-happ** — the Holochain DNA that backs HummHive. This
repo produces a `.happ` binary containing the `humm_earth_core` DNA
(one integrity zome + one coordinator zome). The hApp is consumed by
[humm-tauri](https://github.com/hummhive/humm-tauri), which embeds a
Holochain conductor and loads this `.happ` at runtime.

- Domain vocabulary (Hive, Sidecar, Node, Cell, Vault, …): `../humm-tauri/GLOSSARY.md`
- Zome workspace members: `dnas/*/zomes/coordinator/*`, `dnas/*/zomes/integrity/*`
- Holochain SDK: hdi 0.7.0 / hdk 0.6.0 (holonix main-0.6)

---

## The hApp binary

Build output: `workdir/humm-earth-core-happ.happ` (gitignored). Official
prebuilt binaries for every generation live at
`~/hummhive-official-happ-versions/` with `MANIFEST.tsv` mapping
label → commit → DNA hash → hApp SHA256.

**Current generation: pass-4** (current code matches).

The built `.happ` goes into `../humm-tauri/src-tauri/bin/humm-earth-core-happ.happ`
for integration with the Tauri app.

**Pass lineage:**

```
main-hc060 → pass-1 → pass-2 → pass-2.5 → pass-3 → pass-4 (current)
```

- `pass-2`, `pass-2.5`, `pass-2.5-cleanup` share the same DNA hash
  (coordinator-only changes between them).
- `pass-4-prerepro` and `pass-4-repro` share the same DNA + hApp hash
  (reproducible-build fix only).
- Current: DNA `uhC0k26b…`, hApp `d74e5f2f…`

---

## Change gravity

### Integrity zome (`content_integrity`)

Changes the DNA hash → **forks/splits the chain**. Existing agents on
the old DNA cannot gossip with agents on the new DNA. MUST NOT modify
without significant good cause and multi-user validation/verification.
Every integrity change is a new "pass" (pass-1, pass-2, …) and requires
a migration pipeline run for all existing users.

### Coordinator zome (`content`)

Does NOT change the DNA hash. Hot-swappable — downstream humm-tauri
just needs the updated `.happ` to use new features. Backwards-compatible
changes always preferred; breaking coordinator changes require a
corresponding humm-tauri update.

### Wire shapes

- **Add fields** with `#[serde(default)]` so old agents can decode new
  records (the new field deserializes to its `Default`).
- **Remove fields** only via a versioned migration — never drop a field
  from a live wire shape without a pass bump.

---

## WSL ⇄ Windows two-clone workflow

The native Linux FS (`~/humm-earth-core-happ`) is ~30× faster than
`/mnt/c/proj/…` for cargo builds. **On WSL, do all dev/build/test in
`~/humm-earth-core-happ`, not the Windows mount.**

**One-time setup:**

```bash
git clone /mnt/c/proj/github/hummhive/humm-earth-core-happ ~/humm-earth-core-happ
```

**Syncing between clones — MUST use the sync scripts:**

```bash
scripts/wsl-pull.sh     # start of session: pull Windows-side commits into Linux clone
scripts/wsl-push.sh     # end of session: push Linux commits to Windows mount
scripts/wsl-check.sh    # read-only divergence check
```

**HARD RULES:**

- **NEVER** run `cargo build` or `npm install` from the Windows mount
  while the Linux clone exists — `target/` and `node_modules/` hold
  platform-specific binaries and will silently corrupt each other.
- **NEVER** manually `cp` files between clones. The sync scripts exist
  to prevent commit graph divergence and duplicate commits.
- **NEVER** `git commit` directly on the Windows mount with content from
  the WSL clone, and never `git format-patch | git am` across clones.

All three scripts are auto-detecting and abort cleanly on conflict.
If a script fails, fix the underlying issue (conflict, dirty tree) and
re-run — do not fall back to manual steps.

**Building** requires nix:

```bash
nix develop --command bash -c 'npm run build:zomes && hc app pack workdir --recursive'
```

---

## Filesystem boundaries (HARD RULES)

- **Allowed scopes:** `~/humm-earth-core-happ/` (WSL native clone) and
  `/mnt/c/proj/github/hummhive/humm-earth-core-happ/` (Windows mount).
  NEVER read, write, or list outside these two paths.
- **Subagents default to `~/humm-earth-core-happ/` on WSL.** Never
  dispatch a subagent against `/mnt/c/…` paths unless the task is
  explicitly the Windows-side commit step.
- **Carve-out:** `~/.claude/` on the WSL side IS allowed.
- This rule applies to `find`, `read`, bash one-liners, internal URIs,
  and every other path-taking interface.

---

## Build & test

**Build** (inside `nix develop`):

```bash
npm run build:zomes      # cargo build --release --target wasm32-unknown-unknown + wasm-opt strip
hc app pack workdir --recursive
```

`build:zomes` runs `scripts/build-zomes.sh` (cargo + RUSTFLAGS
path-remap) then `scripts/strip-wasms.sh` (wasm-opt `--strip-debug
--strip-producers`). Both steps are required for deterministic DNA
hashes.

**Test:**

```bash
npm test                 # builds zomes, packs happ, runs vitest/tryorama
```

**Reproducibility:** build scripts use `RUSTFLAGS` path-remap +
wasm-opt strip for deterministic DNA hashes across build hosts.
See `.baseline-hashes.txt` "Reproducibility contract" for the full
rationale and verification commands.

---

## Working agreement

- **No pushing.** Commit locally only; never `git push` without explicit
  instruction.
- **Multi-line commit messages** via temp file: write the message to
  `/tmp/commit-msg-*.txt`, then `git commit -F`. No inline `-m` for
  anything beyond a one-liner.
- **Conventional commits:** `feat(integrity):`, `feat(coordinator):`,
  `chore(build):`, `docs(handoff):`, etc.
- **Match existing formatting.** Tabs, LF, single trailing newline —
  follow the file you are editing.

---

## Agent toolkit

Local `.claude/` carries curated skills, agents, and commands for this
repo. See **`AGENTS.md`** for the quick-reference table.
