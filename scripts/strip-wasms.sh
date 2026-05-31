#!/usr/bin/env bash
# Strip non-semantic custom sections from the release wasm zomes so the
# wasm bytes — and therefore the DNA hash that `hc dna pack` derives
# from them — are reproducible across build hosts.
#
# Two sections matter:
#   `name`       — carries LLVM codegen-unit suffixes (`.llvm.NNNN…`) that
#                  hash the crate's ABSOLUTE PATH. wasm-opt's
#                  --strip-debug removes it (binaryen treats `name` as
#                  part of debug info).
#   `producers`  — embeds the rustc version string (compiler hash + date).
#                  --strip-producers removes it.
#
# Both sections are no-op for execution: the wasm spec marks custom
# sections as having no semantic effect, and wasmer (Holochain's runtime)
# ignores them. See `.baseline-hashes.txt` "Reproducibility contract"
# section for the full rationale.
#
# Requires Binaryen's `wasm-opt` on PATH. `flake.nix` adds `pkgs.binaryen`
# to the devShell so it is available inside `nix develop`.
set -euo pipefail

if ! command -v wasm-opt >/dev/null 2>&1; then
  echo "strip-wasms.sh: wasm-opt not on PATH; run inside 'nix develop'" >&2
  exit 1
fi

TARGET="${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release"
WASMS=(
  "$TARGET/content_integrity.wasm"
  "$TARGET/content.wasm"
)

# Two-pass: validate every wasm exists BEFORE stripping any (so a missing
# second wasm does not leave the first one half-mutated).
for w in "${WASMS[@]}"; do
  if [ ! -f "$w" ]; then
    echo "strip-wasms.sh: missing wasm: $w" >&2
    exit 1
  fi
done

for w in "${WASMS[@]}"; do
  wasm-opt --strip-debug --strip-producers -o "$w" "$w"
done
