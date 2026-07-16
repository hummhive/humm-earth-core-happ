# GitHub Release automation + CI-callable happ verification

- **Status:** DEFERRED (owner, 2026-07-16) — batch with the other release-track work close to the initial RC. Nothing is blocked meanwhile: humm-tauri dev mode treats its repo-local `.testdata/happs/` mirror as the versioned endpoint, and every generation is hand-published there + `~/hummhive-official-happ-versions/` by the blessing runbook.
- **Origin:** 2026-07-16 humm-tauri fleet audit (ReleaseAndVerification lane).
- **Class:** repo/infra only — zero wasm/DNA impact, never part of a coordinator or integrity generation.

## Why it exists

humm-tauri's PRODUCTION build hard-pins its happ source to
`https://github.com/hummhive/humm-earth-core-happ/releases/latest/download/`
(`src-tauri/src/services/app_config/schema.rs:56-61`). That URL 404s today:
this repo has no release workflow (`.github/workflows/test.yaml` is
build+test only) and no GitHub Release has ever been published. Their
`04_PROJECT_ReleasePackagingAndDistribution/03_ReproducibleHappBuildCi.md`
Phases 2–3 and `14_MISC_PlatformInfraAndDataLayer/15_HostedHappRegistry.md`
both dead-end on this gap.

## Scope (when un-deferred)

1. `scripts/verify-happ-dna-hash.sh <commit> <expected-dna-hash>` — CI-callable
   wrapper over the existing reproducible pipeline (`scripts/build-zomes.sh` →
   `hc dna pack` → `hc app pack` → `hc dna hash` compare). Mirrors what the
   blessing runbook does by hand and what humm-tauri's `happ_install.rs`
   `extract_bundled_dna_hash`/`check_dna_parity` do in Rust.
2. Tag-triggered GitHub Actions release job: rebuild at the tag, run the
   verify script, publish `.happ` + the MANIFEST.tsv row + `SHA256SUMS` as
   Release assets. Release identity = git tag + DNA hash + happ sha
   (crate versions stay frozen — see `.baseline-hashes.txt` gotchas).
3. Bump the stale CI Nix pin (`test.yaml` still installs Nix 2.12.0 via
   `cachix/install-nix-action@v18`; matches humm-tauri upstream-checklist
   item 6).
4. Reconcile with `~/hummhive-official-happ-versions/` (local canonical
   store stays; Releases become the public mirror humm-tauri's production
   URL resolves against).

## Acceptance (when un-deferred)

- A fresh clone + `scripts/verify-happ-dna-hash.sh e16b793 uhC0ksXs…` exits 0.
- Pushing a `vX.Y.Z` tag produces a GitHub Release whose assets satisfy
  humm-tauri's `DEFAULT_HAPP_SOURCE` resolution (their `provisionFromManifest`
  contract: MANIFEST row + named `.happ`).
- humm-tauri `04/03`'s planned `verify-happ-source` CI job has something real
  to fetch and diff against.
