---
description: A Rust function over ~50 lines SHOULD be modularized — early warning before the 60-line hard cap
condition: "(?:^|\\n)(?<indent>[\\t ]*)(?:pub(?:\\s*\\([^)]*\\))?\\s+)?(?:default\\s+)?(?:const\\s+)?(?:async\\s+)?(?:unsafe\\s+)?(?:extern\\s+\"[^\"]+\"\\s+)?fn\\s+\\w+[^\\n]*\\{\\n(?:\\k<indent>[\\t ][^\\n]*\\n|[ \\t]*\\n){50,}?\\k<indent>\\}"
scope: "tool:edit(*.rs), tool:write(*.rs)"
interruptMode: never
---

A Rust function over ~50 lines *should* be modularized (`ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` §File/Function Size) — this is the early warning before the 60-line hard cap (`rs-fn-too-long`). This repo's standard puts the *must* line here, at ≤~50: look for the phase boundaries now, while the extraction is small: the steps you'd otherwise introduce with a `// comment` are the helper `fn`s waiting to be named. A zome extern that validates, then reads, then writes is three helpers wearing one name.

This is a soft reminder — it never blocks. It fires heuristically when a function's closing brace sits ~50+ lines below its signature (rustfmt brace alignment); only a one-shot too-long write trips it, not a small edit inside an existing long `fn`.
