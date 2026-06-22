---
description: Comments explain WHY, never WHAT — extract a named function instead of a narrating block; keep any surviving comment to a load-bearing line or two
condition:
  - "/\\*(?:(?!\\*/)[^\\n])*\\n"
  - "//[^\\n]*\\n\\s*//"
scope: "tool:edit(*.{rs,ts}), tool:write(*.{rs,ts})"
interruptMode: never
---

**The bar (the `coding-standards` skill: self-documenting code over comments): a
reader understands every chunk, function, and line — with all comments AND every
type annotation stripped.** If the meaning survives only because of a comment or a
type, the names are wrong; rename until the code is clean and self-documenting on
its own. Code says WHAT it does in its own names.

A comment is a maintenance liability the compiler cannot enforce: it wastes tokens,
rots into a stale or misleading reference, and stands as a second (often
soon-to-be-wrong) source of truth for what the code already states exactly once. A
comment earns its keep ONLY for a non-obvious WHY the code cannot carry — a
Holochain/HDK quirk, a validation invariant you don't control, a magic-value
cross-reference, a gotcha that bit once. Never narrate the next N lines.

## A multi-line comment is usually an unnamed function

If you reach for a block comment to explain what the next 5–10 lines do, those
lines are the body of an unnamed function. Name it and call it — the comment
disappears, and the name survives refactors, diffs, and grep.

## Rules for any comment that survives

- WHY, never WHAT.
- Two lines or fewer. A 3+ line block is a smell: narrating → delete; explaining a
  fn/type → rename; explaining a magic value → extract a named `const`.
- NEVER reference commit hashes, spec/task paths, or agent/transcript IDs — they
  rot. Encode load-bearing context in a name, a type, or a test that fails when the
  contract breaks.

## Rust doc comments

`///` and `//!` on public items (zome externs, validation fns) are legitimate and
idiomatic — but lead with one sentence stating what the item *is*, then push the
rest into names. A `# Safety` doc on `unsafe` is required, not optional. This is a
soft reminder (it never blocks); treat it as a cue to compress, not necessarily to
delete.

If your comment is a genuine, non-obvious WHY in one or two lines, keep it and
proceed.
