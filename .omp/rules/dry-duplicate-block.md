---
description: Don't repeat a 3+ line block within a file — extract a shared helper (DRY). Heuristic reminder.
condition: "(?:^|\\n)(?<dup>[^\\n]{20,}\\n[^\\n]{16,}\\n[^\\n]{16,})[\\s\\S]{1,4000}?\\n\\k<dup>"
scope: "tool:edit(*.{rs,ts}), tool:write(*.{rs,ts})"
interruptMode: never
---

Don't copy a batch of three or more substantial lines within a file. When the same
lines appear more than once, extract them into a shared helper / `fn` / module.
(`coding-standards` skill: DRY — "Extract common logic into functions; avoid
copy-paste programming.")

## Why

- One source of truth: a bug fixed in one copy silently survives in the others.
- A named helper documents the intent that inlined duplication does not.
- Repeated boilerplate is the single most common thing a reviewer flags by hand.

This is a heuristic reminder, not a hard gate: it fires when a substantial
three-line block repeats within a single edit. Genuinely distinct code that happens
to share a shape is fine — extract only when the duplication is real and the helper
earns a name. In Rust, reach for a private `fn`, an iterator chain to collapse the
repetition, or a macro only as a last resort.
