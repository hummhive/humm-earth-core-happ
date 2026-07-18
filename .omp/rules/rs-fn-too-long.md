---
description: A Rust function over ~60 lines MUST be modularized into named helpers — hard stop
condition: "(?:^|\\n)(?<indent>[\\t ]*)(?:pub(?:\\s*\\([^)]*\\))?\\s+)?(?:default\\s+)?(?:const\\s+)?(?:async\\s+)?(?:unsafe\\s+)?(?:extern\\s+\"[^\"]+\"\\s+)?fn\\s+\\w+[^\\n]*\\{\\n(?:\\k<indent>[\\t ][^\\n]*\\n|[ \\t]*\\n){60,}?\\k<indent>\\}"
scope: "tool:edit(*.rs), tool:write(*.rs)"
interruptMode: always
---

**A function whose body runs past ~60 lines — a single screen height — MUST be modularized** (`ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` §File/Function Size). It's several operations wearing one name; pull the cohesive steps into named helper `fn`s until the top-level function reads like a short summary of what it does.

## Why

- A function taller than the screen forces the reader to scroll and hold its whole state in their head at once; named sub-functions let them stop at the level of detail they need.
- The blocks you'd introduce with a comment (`// validate author`, `// then persist`) are exactly the helper boundaries — name them and the comment disappears (comments are a smell, `CODING_STANDARDS.md` §Comments).
- Smaller functions are independently testable and reusable; a 100-line extern is neither, and a sweettest can't pin one phase of it.

## What to do

- Extract each logical phase into a `fn` named for what it returns or does; pass it the data it needs, return what the caller needs. Migrate the matching tests onto the extracted `fn` (`ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` §File/Function Size).
- Prefer iterator chains over long manual loops and early returns (`?` / `ok_or`) over deep nesting — both shed line count without hiding logic.
- A long `match` with fat arms is the signal to give each arm its own function.

This is the hard cap; the 50-line *should* tier is the soft companion `rs-fn-too-long-warn`. It fires heuristically when a function's closing brace sits ~60+ lines below its signature, keyed on rustfmt's brace alignment — a non-`rustfmt` body or a signature wrapped across lines may slip by, and only a one-shot too-long write trips it (a small edit inside a long `fn` shows the matcher only its changed region, and `after-gap` repeat limits it to one interrupt per window). A reviewer still owns the rest. This repo's standard: functions ≤~50.
