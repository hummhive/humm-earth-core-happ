---
name: coding-standards
description: Baseline coding conventions for this repo — naming, readability, immutability, error-handling, and code-quality review. The canonical rules live in root CODING_STANDARDS.md + ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md; use rust-patterns / rust-testing for Rust specifics.
---

# Coding Standards — earth-core baseline

The canonical standards are the repo-root docs — read them, they are the
contract this skill only indexes:

- **`CODING_STANDARDS.md`** — §1 async/await (TS) · §2 fail-fast early returns
  · §3 self-documenting names + comment discipline · §4 vertical whitespace ·
  §5 invariants at construction · §6 iteration · §7 imports · §8 code lands
  with its consumable surface · §10 loud errors + the five-criteria swallow
  gate · §11 test path math · §12 suppression policy · §13 Given/When/Then.
- **`ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md`** — HDK-macro logging,
  600/500-line file + 60/50-line function caps, never-silent errors, flat-data
  parametrized tests, self-documenting code, test-first RED-reproduction
  methodology.
- **`ANTI_SLOP.md`** — the prose bar for docs, commit bodies, and mbox
  messages (see the `slop-scan` skill).

Framework specifics live in their own skills: `rust-patterns` (ownership,
errors, traits), `rust-testing` (TDD cycle, parameterized tests),
`security-review` (validator authority, cap grants, secrets).

## When to Activate

- Reviewing any change for quality and maintainability
- Refactoring or modularizing existing code
- Naming anything that ships on the wire (externs, fields, reject strings)
- Onboarding a contributor or subagent to this repo's conventions

## The floor (memorize this much)

- **Readability first.** Code is read more than written; intent must be
  obvious from names alone — without comments, without type annotations.
- **KISS / DRY / YAGNI.** Simplest thing that works; extract at the third
  duplication; build nothing speculative.
- **Immutability defaults.** Rust: borrow, don't clone; TS tests: spread, never
  mutate fixtures.
- **Errors are loud.** `?`/`ExternResult` in guest code; the five-criteria gate
  before any swallow; HDK `warn!`/`debug!` — no LoggingService here.
- **Everything carries intent.** Wire names are API; magic numbers become named
  bounds; a function over ~50 lines is hiding a name it owes you.
