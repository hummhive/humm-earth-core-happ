# Humm earth-core DRY refactor

## Source note

Copied from `../ttsrAndHooksScratchpad/20260623_DRY_REFACTOR_NOTES.md` lines 555-567.

> ## 8. humm-earth-core-happ (Rust DNA) — brief
>
> Separate repo, scanned for parity. **6 files > 600 lines, all in the integrity zome:**
> `encrypted_content.rs` (2531), `group.rs` (1001), `hive.rs` (990),
> `coordinator/.../encrypted_content/migration.rs` (881), `integrity/.../lib.rs` (803),
> `coordinator/.../encrypted_content/signals.rs` (669). **Functions > 60:** 4 (zomes) + 5
> (crates/sweettest).
>
> **Caveat — change gravity.** Integrity-zome files (`zomes/integrity/**`) alter the DNA
> hash when edited; splitting them is a real refactor that lands **only under a sanctioned
> pass + migration**, never a drive-by (guarded by `integrity-zome-guard`). The
> coordinator-side `migration.rs` / `signals.rs` are hot-swappable and can be modularized
> freely. core-happ's own standard puts the function line at ≤50 (stricter than 60).

## Reviewed extended notes

`../ttsrAndHooksScratchpad/20260623_DRY_REFACTOR_NOTES_EXTENDED/tests-rust.md` was reviewed and is intentionally not implemented in this repo: the current tree has no `src-tauri/**`, no `cache/tests.rs`, and no `migration/flows/tests.rs`. The matching test work here is the Sweettest support/helper split plus focused behavior coverage for recipient witnesses and hive grant-window containment.

## Execution checklist

- [x] Sync WSL clone from the Windows-side `main` commit.
- [x] Create `dry-refactor` in WSL and Windows clones.
- [x] Extract duplicated Sweettest setup into `crates/sweettest/tests/support/mod.rs`.
- [x] Refactor existing Sweettest files to use shared helpers and hash guards.
- [x] Split coordinator `encrypted_content::migration` into directory modules without changing externs or marker behavior.
- [x] Split coordinator `encrypted_content::signals` into directory modules without changing remote-signal encoding/stamping behavior.
- [x] Verify coordinator-only changes keep pass-5 DNA hash unchanged.
- [x] Split integrity `encrypted_content`, `hive`, `group`, and validation dispatch as sanctioned pass-6 structural refactor.
- [x] Preserve EntryTypes/LinkTypes order, wire fields, serde tags, and validation decisions.
- [x] Fix codewalked stale docs/comments and add Sweettest coverage for recipient witnesses and grant-window containment.
- [x] Run host tests, clippy, zome build/pack, Sweettest, hash capture, and DNA-hash checks.
- [x] Run rust, security, silent-failure, and DRY/code-review lanes; fix all applicable findings.
- [x] Update baseline hashes, POSTCOMPACTION, CLAUDE, codemaps, and codemap diff report.
- [x] Move this file to `.doneTasks/humm-earth-core-dry-refactor.md` when all gates pass.
