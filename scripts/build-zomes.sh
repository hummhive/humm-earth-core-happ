#!/usr/bin/env bash
# Reproducible wasm build for humm-earth-core-happ zomes.
#
# Why this script exists:
#   The bare `cargo build --release --target wasm32-unknown-unknown` invocation
#   produced different `content_integrity.wasm` bytes (and therefore different
#   DNA hashes) depending on the host's filesystem layout. Two contributors
#   building the same git commit could end up with two different DNA hashes,
#   which broke the .baseline-hashes.txt invariant and made downstream
#   hash-pinning (humm-tauri) brittle.
#
#   Two root causes were identified in this project:
#     1. The wasm `name` custom section embeds LLVM codegen-unit hashes
#        derived from the ABSOLUTE PATH of the crate being compiled. This
#        is the dominant drift source.
#     2. The wasm `producers` custom section embeds the rustc version
#        string (incl. a per-toolchain hash + date). Drifts across
#        toolchain updates.
#
#   The path-remap RUSTFLAGS below are defense-in-depth (this project's
#   panic=abort + opt-level=z combination DCEs the `file!()`/registry
#   path strings before they hit the wasm, so remapping is largely a
#   no-op today; the flags stay correct if anyone later flips those
#   settings). The Binaryen post-strip in scripts/strip-wasms.sh is the
#   load-bearing fix — it removes both custom sections.
#
#   See `.baseline-hashes.txt` "Reproducibility contract" section for the
#   full rationale and verification command.
set -euo pipefail

SYSROOT="$(rustc --print sysroot)"

# Holochain wasm zomes need getrandom's custom backend (no OS RNG in the
# host); everything else is the path-remap defense-in-depth.
RUSTFLAGS_ARGS=(
  '--cfg' 'getrandom_backend="custom"'
  '--remap-path-prefix' "$PWD=/build"
  '--remap-path-prefix' "${CARGO_HOME:-$HOME/.cargo}=/cargo"
  '--remap-path-prefix' "$SYSROOT=/rustc"
)

# RUSTFLAGS is whitespace-split by Cargo; paths with spaces are not supported
# by Cargo's RUSTFLAGS parser regardless of how the shell builds the string.
RUSTFLAGS="${RUSTFLAGS_ARGS[*]}" CARGO_TARGET_DIR=target \
  cargo build --release --target wasm32-unknown-unknown

bash "$(dirname "$0")/strip-wasms.sh"
