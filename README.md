# humm-earth-core-happ

The Holochain DNA behind HummHive. This repo builds ONE artifact: a `.happ`
bundle containing the `humm_earth_core` DNA — an integrity zome (the frozen
validation rules; changing it forks the network) plus a coordinator zome (the
callable API; hot-swappable). The desktop app in the sibling
[humm-tauri](https://github.com/hummhive/humm-tauri) repo embeds a Holochain
conductor and loads this `.happ` — there is no UI and no web app in this repo.

New here? Read in this order: `POSTCOMPACTION.md` (current state) →
this file → `CLAUDE.md` (change gravity + workflow) → `AGENTS.md` (toolkit).
Standards: `CODING_STANDARDS.md` + `ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md`.
Architecture maps: `docs/CODEMAPS/`. Domain vocabulary:
`../humm-tauri/GLOSSARY.md`.

## Environment

Everything builds inside the pinned nix shell (holonix — Holochain's nix
toolchain distribution):

```bash
nix develop
npm install
```

On WSL: work in the native-filesystem clone (`~/humm-earth-core-happ`), never
the `/mnt/c` mount — see `CLAUDE.md` for the two-clone workflow.

## Build

```bash
npm run build:zomes   # cargo → wasm32 + wasm-opt strip (deterministic hashes)
npm run build:happ    # build:zomes + `hc app pack workdir --recursive`
```

Output: `workdir/humm-earth-core-happ.happ`. Official prebuilt generations
live in `~/hummhive-official-happ-versions/` (`MANIFEST.tsv`: label → commit →
DNA hash → hApp sha256; LAST row = current). `.baseline-hashes.txt` is the
reproducibility contract — the build must reproduce those hashes byte-for-byte.

## Test

Three layers, from fastest to fullest:

```bash
# 1. Host unit tests (pure Rust, no conductor)
cargo test -p content --lib
cargo test -p content_integrity --lib

# 2. Conductor behavior tests (in-process Holochain via sweettest)
cd crates/sweettest && cargo test -- --test-threads=1 --nocapture

# 3. Tryorama harness (tests/) — currently DORMANT
npm test
```

Layer 2 is the real conductor gate. Layer 3 (`tests/`, tryorama + Vitest)
cannot boot a conductor on holochain 0.6.x — kept for the eventual hc-0.7
revival; do not treat a tryorama failure to boot as a regression.

## Releases

A "pass" = one integrity-zome generation (one DNA hash). Coordinator-only
releases reuse the held DNA hash and hot-swap the API. The full lineage and
per-pass handoff docs: `CLAUDE.md` (lineage), `docs/PASS_6_DEPLOY_HANDOFF.md` +
`docs/HUMM_TAURI_*_INTEGRATION.md` (wire contracts per generation),
`docs/_archive/` (superseded generations).
