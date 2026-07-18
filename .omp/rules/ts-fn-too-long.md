---
description: A TS function over ~60 lines MUST be modularized into named functions — hard stop
condition: "(?:^|\\n)(?<indent>[\\t ]*)(?:(?:export\\s+)?(?:default\\s+)?(?:async\\s+)?function\\b[^\\n]*|(?!(?:if|for|while|switch|catch|do|else|return|with)\\b)(?:(?:public|private|protected|static|readonly|async|get|set|override)\\s+)*\\w+\\s*(?:<[^>\\n]*>)?\\s*\\([^\\n]*\\)\\s*(?::\\s*[^\\n{]+)?|[^\\n]*=>\\s*)\\{\\n(?:\\k<indent>[\\t ][^\\n]*\\n|[ \\t]*\\n){60,}?\\k<indent>\\}"
scope: "tool:edit(*.ts), tool:write(*.ts)"
interruptMode: always
---

**A function whose body runs past ~60 lines — a single screen height — MUST be modularized** (`ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` §File/Function Size). It's several operations wearing one name; pull the cohesive steps into named functions until the top-level function reads like a short summary of what it does.

## Why

- A function taller than the screen forces the reader to scroll and hold its whole state in their head at once; named sub-functions let them stop at the level of detail they need.
- The blocks you'd introduce with a comment (`// set up players`, `// then assert`) are exactly the helper boundaries — name them and the comment disappears (comments are a smell, `CODING_STANDARDS.md` §Comments).
- Smaller functions are independently testable and reusable; a 100-line test body is neither. A long tryorama scenario usually wants its fixtures in a `common.ts` builder and its assertions in named checks.

## What to do

- Extract each logical phase into a named function; pass it the data it needs, return what the caller needs. In `tests/src/**`, lift shared setup into `common.ts` and keep each `test(...)` body focused on one act-and-assert.
- Prefer `.map`/`.filter`/`.reduce` pipelines over long imperative loops and early returns over deep nesting — both shed line count without hiding logic.
- A fat `switch` is the signal to split into per-case functions.

This is the hard cap; the 50-line *should* tier is the soft companion `ts-fn-too-long-warn`. It matches `function`, arrow (`=> {`), and method bodies whose closing brace sits ~60+ lines below the opener (control-flow keywords like `if`/`for`/`switch` are excluded); a wrapped signature or an unusual layout may slip by, and only a one-shot too-long write trips it (a small edit inside a long function shows the matcher only its changed region, and `after-gap` repeat limits it to one interrupt per window). A reviewer still owns the rest.
