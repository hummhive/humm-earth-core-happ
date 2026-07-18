---
description: A source file over 600 lines MUST be modularized into focused modules/helpers — hard stop
condition: "(?:[^\\n]*\\n){601}"
scope: "tool:edit(*.{ts,js,rs}), tool:write(*.{ts,js,rs})"
interruptMode: always
---

**A file over 600 lines MUST be modularized** (`ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` §File/Function Size). Don't land a 600+ line file — split it along the seams already in it (the export groups, the `impl` blocks / submodules, the helper cluster that only talks to itself), and re-export the public surface from the original path if callers depend on it.

## Why

- A 600+ line file is never one cohesive unit; it's several that drifted together. Those internal boundaries are the modules waiting to be named.
- Line count *includes comments* — and comments are themselves a smell of code that isn't self-documenting (`CODING_STANDARDS.md` §Comments). A file padded with narration is doubly over budget.
- Big files hide duplication and dead code, balloon merge conflicts, and tax every reader who scrolls past the part they don't care about. Splitting at 1,200 lines later is the chore nobody volunteers for.

## What to do

- Group by what actually collaborates; give each group its own module. Rust: break a long `mod` into submodules (`foo/mod.rs` + `foo/bar.rs`) — the coordinator zome already does this (`encrypted_content/{crud,queries,paging,remediation}.rs`, `signals/{dm,outbound,blob_pin}.rs`). TS: lift the cohesive cluster out of a test file into a shared `common.ts` helper and import it back.
- Migrate the tests with the code: sweettest a coordinator submodule directly (`crates/sweettest`), and keep the caller's tryorama tests too (`tests/src/**`) — remove a test from its old home only if it tested *exactly* the extracted unit with no side effects (`ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` §File/Function Size).
- Don't game the count by deleting blank lines — the total is a proxy for "too many responsibilities," not a formatting target.

This gate sees only what a single tool call lands: a fresh `write` of 600+ lines, or an edit that *adds* 600+ lines at once. A normal edit to an existing large file shows the matcher only its changed region, so an incremental, edit-by-edit modularization of an already-oversized file proceeds untouched (a reviewer owns that residual debt). With `after-gap` repeat it interrupts at most once per window, so a full-file reductive rewrite lands on retry. There is no self-certified exception: a file you believe genuinely cannot be split (a generated data table) is raised in review and added to an explicit allow-list by name — never waved through at the keyboard. Default: split it.
