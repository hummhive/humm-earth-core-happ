---
name: slop-scan
description: Checks prose for AI-generated writing patterns and fixes them before they ship. Scans commit bodies, docs, task notes, and agent-mailbox replies against the repo-root ANTI_SLOP.md word and structure lists.
origin: ECC
---

# Slop Scan

Checks prose for AI-generated writing patterns and fixes them before they ship.

## When to Use

After writing any block of prose longer than a few sentences: commit message
bodies, `docs/` sections (CODEMAPS, guides, `PASS_N_DEPLOY_HANDOFF.md`),
`.newTasks/` and `.doneTasks/` task descriptions, README paragraphs,
`POSTCOMPACTION.md` state notes, and `~/agent-mailbox` outbox replies to the
humm-tauri devs. Not for code, not for pure task lists where a list is the
correct format, not for short one-liners.

Do not run this on every change. Run it when the output will be read by a person
and the writing quality matters.

## How It Works

Read `ANTI_SLOP.md` at the repo root for the full banned-word list and forbidden
structural patterns. The scan has two passes.

**Pass 1 — word-level.** Flag any instance of the banned verbs, adjectives, nouns,
and stock expressions. Common offenders: `foundational`, `robust`, `seamless`,
`comprehensive`, `leverage`, `harness` (only when used as a verb — "test harness"
and "the oh-my-pi harness" are domain terms and are fine), `Furthermore`,
`Moreover`, `However` at the start of a sentence. Replace each with a direct
alternative or cut it.

**Pass 2 — structural.** Check for:
- Repeated sentence pattern across three or more consecutive items (same opener, same shape)
- "A is not B, but C" redefinition structure — state the point directly without the negation
- Every subsection using the same intro → list → close mold
- Closing paragraph that recaps the body rather than carrying forward

## Exceptions

- Big lists are fine when a list is the point of the document (a task list, a changelog, a dependency table, the MANIFEST rows).
- Domain terms with specific meanings are fine even if the word appears in the banned list: "test harness" and "the oh-my-pi harness" are not the verb "harness"; "leverage" in a financial context means the ratio, not "use effectively".
- Commit subject lines are too short for structural patterns to apply — scan the body only.

## Output

For each violation: quote the offending phrase, name the rule it breaks, and give a concrete replacement. Do not rewrite the whole document — patch the specific lines.

After patching, re-read the output aloud mentally. If any sentence still sounds like a model summarising its own work, rewrite that sentence.

## Reference

`ANTI_SLOP.md` at the repo root — full banned-word list, structural pattern catalogue, and rationale.
