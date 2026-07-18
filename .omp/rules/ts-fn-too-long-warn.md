---
description: A TS function over ~50 lines SHOULD be modularized — early warning before the 60-line hard cap
condition: "(?:^|\\n)(?<indent>[\\t ]*)(?:(?:export\\s+)?(?:default\\s+)?(?:async\\s+)?function\\b[^\\n]*|(?!(?:if|for|while|switch|catch|do|else|return|with)\\b)(?:(?:public|private|protected|static|readonly|async|get|set|override)\\s+)*\\w+\\s*(?:<[^>\\n]*>)?\\s*\\([^\\n]*\\)\\s*(?::\\s*[^\\n{]+)?|[^\\n]*=>\\s*)\\{\\n(?:\\k<indent>[\\t ][^\\n]*\\n|[ \\t]*\\n){50,}?\\k<indent>\\}"
scope: "tool:edit(*.ts), tool:write(*.ts)"
interruptMode: never
---

A TypeScript function over ~50 lines *should* be modularized (`ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` §File/Function Size) — this is the early warning before the 60-line hard cap (`ts-fn-too-long`). Find the phase boundaries now, while the extraction is small: the steps you'd otherwise introduce with a `// comment` are the named helpers waiting to exist. In a tryorama scenario (`tests/src/**`), the setup / act / assert phases are the natural seams — a shared builder in `common.ts` often absorbs the setup.

This is a soft reminder — it never blocks. It matches `function`, arrow, and method bodies whose closing brace sits ~50+ lines below the opener (control-flow keywords excluded); only a one-shot too-long write trips it, not a small edit inside an existing long function.
