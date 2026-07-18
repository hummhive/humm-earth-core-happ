# Addendum: Extended CODING_STANDARDS and Guidance

Adapted from `../humm-tauri/ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` for a
Holochain DNA repo. Same headings, earth-core substance.

Magic numbers: avoid them. Well-named constants at the top of the module if
that's the only place they're needed (`MAX_REMEDIATION_BATCH`,
`SUMMARY_MAX_HIVES` — the bound IS the wire contract, name it like one).

DRY: the same batch of lines duplicated 3+ times within a file, or across
files, is the signal to extract into a shared helper (`paging.rs`,
`crates/sweettest/tests/support.rs` are the live examples of this done right).

---

# Logging

**All zome logging goes through the HDK macros** — `debug!`, `warn!`, `trace!`
from `hdk::prelude`. There is NO LoggingService in this repo; that is
humm-tauri's layer, on the other side of the conductor boundary. Never add a
logging framework, never `println!`/`eprintln!` in guest code (there is no
stdout in the WASM guest).

*Logs tell a coherent story.* A good zome log line answers: who is acting (the
calling agent / which extern), what was attempted (the action + its key
inputs, hashes abbreviated), and what the outcome was (committed hash, skip
reason, tolerated failure). Logs for tolerated per-item failures in list reads
(`debug!`) are load-bearing diagnostics — they are the only visibility anyone
gets into a WASM guest at runtime.

*Reject strings are wire contract.* Validator and guard `Err` strings are
asserted verbatim by sweettest and by humm-tauri's error handling. Changing one
is a breaking change — treat reject-string edits with the same gravity as a
field rename.

---

# File/Function Size Rules:

Any file over 600 lines *must* be modularized (over 500 *should* be). This
includes comments — which are themselves discouraged (see CODING_STANDARDS.md
§3).

No functions over single screen height: ~60 lines *must* be split, over 50
*probably should* be. Zome externs split naturally into guard → resolve →
commit → respond helpers; a long extern is almost always hiding a reusable
query or a validation phase that wants its own name.

When modularizing, migrate the relevant tests to unit tests on the extracted
pieces. Remove a test from its original location *only* if it tested exactly
the extracted portion with no side effects — usually you keep both: unit tests
on the extraction plus behavior tests on the calling extern.

---

# Errors:

*Never silently suppressed.* The five-criteria gate in CODING_STANDARDS.md §10
is the whole rule. In Rust that means: no `let _ = fallible()`, no
`if let Err(_)`, no trailing `.ok();`, no masking `unwrap_or_default()`. If a
specific concrete error shape is expected in a context (a tombstoned target in
a list read, "no Record found" for a dead entry), match against that exact
shape, handle it (preferably with a `debug!`), and propagate everything else.
Expected-and-handled errors get tests on the edges of that handling: unexpected
shapes still propagate, similar-but-wrong shapes still propagate, the exact
expected shape takes the documented fallback.

---

# Tests: parametrize same-shaped cases

*When several tests share the same body and differ only by inputs + expected
output, drive them from a flat data table with one shared loop body.*

Case objects are **flat data only** — no closures, no functions, no test bodies
inside a case. TS: `{ it: 'label', ...inputs, expected }` iterated with
`for (const testCase of cases)`. Rust: a `&[Case]` of plain structs iterated in
one `#[test]` (or one per case when failure isolation matters more).

- Run-time values (created hashes, generated keys) are referenced *by name* in
  the case data and resolved inside the loop — never a `() => ...` builder in
  the case.
- Expected strings embedding run-time values hold literal `{token}`
  placeholders the loop fills; the full exact string is still asserted, never a
  partial/regex match.
- A case whose shape genuinely differs (extra setup, different assertion) stays
  its own test — forcing dissimilar cases into one table is the same smell as a
  god-function.

---

# Self-documenting code

*The goal: a reader understands a unit without reading its types, its comments,
or jumping to other files.* Intent lives in names — not in comments, and not
even in type annotations. Types enforce the contract; names REVEAL it.

- *Names carry the intent.* `find_or_create_group_membership`,
  `selectCanonicalByHash`, `wait_for_count_links_by_hive_to` — the name states
  what it does so the body confirms rather than reveals. A name that needs a
  clarifying comment is the wrong name.
- *A comment, when it survives, explains WHY, never WHAT* — the rare
  non-obvious constraint (an HDK cascade quirk, a determinism requirement), two
  lines or fewer, no spec/task/commit references.
- *Prefer boring, linear, early-return control flow* over clever density. The
  next maintainer reads top-to-bottom; fail-fast guards keep the happy path
  unindented.

---

# Test-first — MEASURE, never guess

*Write the test that proves the change BEFORE writing the change.* A test
written after code you only think is correct proves nothing until you have
watched it fail for the right reason.

## Reproducing bugs as RED tests

*Every bug fix starts with a failing test reproducing the EXACT reported
error.* Not a similar error — the exact reject string, the exact
`WasmError` shape, the exact wrong query result.

1. **Collect the evidence first.** The reporter's conductor logs, the exact
   zome-call input, the exact error payload (file/line in `WasmError` pins the
   throwing callsite — trust it over the reporter's attribution).
2. **Write the RED test asserting the DESIRED behavior** — host `cargo test`
   for pure logic, a `crates/sweettest` conductor test for anything touching
   the cascade, links, consistency, or multi-agent flows. Against broken code
   it must fail with the production symptom.
3. **Verify the failure matches production.** If the test fails for a
   different reason (fixture error, wrong agent, missing consistency wait),
   it is not a valid reproduction — fix the test before touching the code.
4. **Only then write the fix.** The fix turns the RED test GREEN; no other
   code changes ride along.
5. **Hypotheses are validated by tests, not reasoning.** If a hypothesis
   predicts a specific failure and the test doesn't show it, the hypothesis is
   wrong — adjust the hypothesis, not the test.

## Red-before-green remediation

When a test was (against this rule) written after the code: revert or comment
out the change, run the test — it MUST fail, and fail the expected way; then
restore — it MUST pass. A test that passes with the change absent does not
test the change.

## When stuck

Repeated speculative edits with no measurement between them are the smell to
stop on. Reproduce in a host test or sweettest before acting on any hypothesis;
consult an oracle/reviewer agent rather than thrashing. Sweettest-specific
gotchas (content-addressing collapse on identical bytes, link-integration
cadence after `await_consistency`, tombstone error shapes) are documented in
`crates/sweettest/tests/` — read the support module before fighting the
conductor.
