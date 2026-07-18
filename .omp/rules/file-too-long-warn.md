---
description: A source file over 500 lines SHOULD be modularized — early warning before the 600-line hard cap
condition: "(?:[^\\n]*\\n){501}"
scope: "tool:edit(*.{ts,js,rs}), tool:write(*.{ts,js,rs})"
interruptMode: never
---

A file over 500 lines *should* be modularized (`ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` §File/Function Size) — this is the early warning before the 600-line hard cap (`file-too-long`). Find the seam now, while the split is small: the export groups, the `impl` blocks / submodules, the helper cluster that only talks to itself.

## Why

- 500 lines is where a file starts hiding a second responsibility. Naming the module now is cheaper than at 600+, and far cheaper than at 1,200.
- Line count *includes comments*, which are themselves a smell of code that isn't self-documenting (`CODING_STANDARDS.md` §Comments) — narration spends your line budget on a maintenance liability.

This is a soft reminder — it never blocks an edit. It fires when a single write/edit lands 500+ lines; a file nudged past 500 by small later edits won't trip it (only the changed region reaches the matcher). Standard: files ≤500 *should*, ≤600 *must*.
