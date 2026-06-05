# POSTCOMPACTION — humm-earth-core-happ

> Current-state-of-the-world for devs and agents picking up this repo.
> Read first after a compaction or at session start. Git log has full history.

---

## Current state

**Branch:** `feat-self-notes-architecture` at `cf7fff9`
**DNA:** pass-4, unchanged. Hash `uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`, hApp `d74e5f2f…`.
**No zome changes on this branch** — docs + dev infrastructure only.

### What's on this branch (10 commits atop pass-4 tip `8503b48`)

1. Self-notes architecture docs (integration handoff, BDD sanity checks, observability)
2. DM messaging handoff (`HUMM_TAURI_DM_MESSAGING_INTEGRATION.md`)
3. Self-notes backfill corrections (X25519 source, K-preserving re-wrap)
4. Codemaps + CLAUDE.md + WSL sync scripts (`24a8efd`)
5. AGENTS.md + local `.claude/` skills/agents/commands (`bd2937b`)
6. rust-reviewer + rust-build-resolver agents (`f98be52`)
7. SharedSecrets public-ACL wire shape doc + BDD scenarios (`cf7fff9`, latest)

### Recent session (2026-06-05)

- Created `docs/CODEMAPS/` — token-lean architecture docs (architecture, backend, data, dependencies)
- Created `CLAUDE.md` — session-start brief with change gravity, WSL workflow, build/test
- Ported WSL sync scripts from humm-tauri (`scripts/wsl-{pull,push,check}.sh` + `bash_helpers.sh`)
- Created `AGENTS.md` + `.claude/` — local agent toolkit (6 skills, 3 agents, 1 command)
- Answered humm-tauri dev question re SharedSecrets public-ACL wire shape → `docs/HUMM_TAURI_SHARED_SECRETS_PUBLIC_ACL_WIRE_SHAPE.md` (6 BDD scenarios, observability checklist)
- Dogfooded WSL sync scripts for every commit (full round-trip works cleanly)

---

## Environment

- **Linux `~/humm-earth-core-happ`** — authoritative. All dev/build/test here.
- **Windows `/mnt/c/proj/github/hummhive/humm-earth-core-happ`** — ff-merge target only (harness cwd).
- **WSL sync:** `scripts/wsl-pull.sh` / `wsl-push.sh` / `wsl-check.sh`. See `CLAUDE.md`.
- **Toolchain:** holochain 0.6.0, hc 0.6.0, Node 24, nix (holonix main-0.6).
- **TRYORAMA IS BROKEN** (0.19.2 vs holochain 0.6.0 transport mismatch). Use `e2e/` harness instead.
- **Build:** `nix develop --command npm run build:happ` (requires nix for `wasm-opt`).

## Other branches (all committed, NOT pushed, NOT merged)

| Branch | Tip | What |
|---|---|---|
| `feat-integrity-pass-4-recipient-witnesses` | `8503b48` | Pass-4 integrity (G-6.2 witnesses, G-4.4 grant-window) — **DNA-changing** |
| `feat-migration-d1-group-track` | `aca142b` | D.1 migration tooling (group track, classification overrides) |
| `test-tryorama-integrity-coverage` | `bf9fad8` | e2e harness (30 scenarios, tryorama-free) |

## Constraints

- NEVER push/merge without explicit user instruction.
- NEVER edit `humm-tauri/**` — read-only reference.
- NEVER run cargo/npm from the Windows mount.
- Append-only for EntryTypes/LinkTypes enums (index stability).
- Commit identity: `Mike <mike@hummhive.com>` (repo-local).

## Gotchas

- AdminWebsocket 400 → pass `wsClientOptions: { origin: "<anything>" }`.
- Two agents, one conductor: same `network_seed` → shared DHT → offline cross-agent validation.
- After `installApp`+`enableApp`, must `authorizeSigningCredentials(cellId)`.
- Reproducibility requires `nix develop` (`wasm-opt` from flake) + `codegen-units = 1`.
- `holochain --piped -c <config>` needs dirs pre-created; prints `###ADMIN_PORT:N###`.

## Key references

- Codemaps: `docs/CODEMAPS/`
- Agent toolkit: `AGENTS.md` + `.claude/`
- Session brief: `CLAUDE.md`
- Pass-4 deploy: `docs/PASS_4_DEPLOY_HANDOFF.md`
- SharedSecrets wire shape: `docs/HUMM_TAURI_SHARED_SECRETS_PUBLIC_ACL_WIRE_SHAPE.md`
- Build: `scripts/build-zomes.sh` + `scripts/strip-wasms.sh`
- Official happ binaries: `~/hummhive-official-happ-versions/` + `MANIFEST.tsv`
- e2e harness: `e2e/README.md`
- Reproducibility: `.baseline-hashes.txt`
