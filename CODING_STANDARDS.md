# Coding Standards — humm-earth-core-happ

Adapted from `../humm-tauri/CODING_STANDARDS.md` for this repo's surfaces, with
the same section numbering so a dev moving between the two repos keeps their
bearings. These are standards, not laws: when a rule and correctness conflict,
correctness wins — then fix the rule.

Surfaces this file governs:

- **Rust zomes** (`dnas/humm_earth_core/zomes/coordinator/**` — the product;
  `zomes/integrity/**` is DNA-hash-frozen and only changes on a sanctioned pass).
- **Rust test crates** (`crates/sweettest/**`).
- **TypeScript** — ONLY the tryorama/Vitest harness (`tests/src/**`),
  `scripts/*.ts` tooling, and harness config/hook files. There is no frontend.
- **Prose** (docs, commit bodies, mbox messages) — held to `ANTI_SLOP.md`.

The extended addendum `ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md` is part of
this contract and is transcluded at the end.

---

## 1. `async`/`await` over `new Promise(...)` / `.then(...)` chains (TS)

**Rule.** Always express asynchronous TS with `async`/`await`. Never
`new Promise((resolve, reject) => ...)` or `.then().catch()` chains unless ALL
hold: (a) bridging a callback-only API (prefer `util.promisify`), (b) combining
N promises in a way `Promise.all/race/allSettled` cannot express, (c) a
genuinely non-async consumer context.

**Rationale.** Zome-call sequences in tryorama tests are order-sensitive
(commit → `await_consistency` → read); `await` keeps that order readable
top-to-bottom and keeps stack traces pointing at the failing call.

**Action.** Any `new Promise` in a change needs an explicit justification
against the three exceptions.

## 2. Fail-fast / early returns over nested validation

**Rule.** Write the invalid/special case first and `return` / `throw` / `?` /
`continue`. Never wrap the real work in a positive-condition `if`. No `else`
after an early return.

In zome code the guard clause IS the `?` operator and the explicit
`Err(wasm_error!(...))` return: validate input shape, resolve prerequisites,
bail with a precise reject string, and keep the happy path unindented.

**Action.** Every function with more than one condition gets an early-return
audit before commit; real work indented past two levels is a smell.

## 3. Self-documenting names over narrating comments

**Rule.** A reader must understand every chunk, function, and line — with all
comments and every type annotation stripped. Names carry the intent; the body
confirms rather than reveals.

- Comments earn their keep ONLY for a non-obvious WHY the code cannot carry: an
  HDK/holochain quirk (`get_details` cascade behavior, link-integration
  cadence), a validation invariant outside our control, a magic-value
  cross-reference. Never WHAT. A 3+ line comment block is a smell — triage:
  narrating → delete; explaining a fn/type → rename; explaining a magic value →
  extract a named constant; capturing an invariant → compress to ≤2 lines or
  extract a function whose name IS the invariant.
- Comments NEVER reference commit hashes, spec/task file paths
  (`.newTasks/...`), or agent/transcript IDs — all rot. Load-bearing context
  belongs in a name, a type, or a test that fails when the contract breaks.
- `///` docs on public items (externs, validators) are idiomatic Rust: lead
  with one sentence stating what the item is. `# Safety` docs on `unsafe` are
  required (and `unsafe` itself is close to banned in guest code).
- No abbreviations: `hive_genesis_hash` not `hgh`, `link_query` not `lq`.
  Functions are verbs (`resolve_targets`, `page_links`); booleans read as
  questions (`was_created`, `is_founder`).
- Domain terms come from `../humm-tauri/GLOSSARY.md` (the shared ubiquitous
  language): Hive, Member, Group, Content, Sidecar, Node, Cell, Agent,
  ACL/AclSpec. One word, one meaning, across both repos.
- **Wire names are API.** Extern names, input/output struct fields, link tags,
  and reject strings are consumed by humm-tauri and its tests — they are
  load-bearing the moment they ship. Name them like you can never rename them
  (mostly, you can't: additive evolution only).

## 4. Vertical whitespace

**Rule.** Exactly one blank line between top-level items, functions, and large
blocks — never two. Inside a function, blank lines mark phases (validate →
resolve → commit → assemble), not every statement boundary.

**Action.** Sweep any touched file: `\n{3,}` → `\n\n`.

## 5. Invariants live at construction

**Rule.** The constructor/decode boundary is the validation boundary — bad data
must not be constructible past it.

- Rust: newtypes for ID safety; parse-don't-validate constructors that return
  `Result`; `#[must_use]` where dropping a value is a bug. TS types and serde
  shapes are erased at the wire — msgpack decode is the runtime boundary, so
  every invariant the integrity zome relies on must be CHECKED there, not
  assumed from the struct definition.
- TS (tests): helper builders produce valid-by-construction fixtures; a fixture
  that can silently express an invalid state hides the exact bug the test
  exists to catch.

## 6. Iteration

**Rule.**
- Rust: iterator chains over manual index loops; `filter_map`/`collect` over
  push-accumulators. A `for` loop is fine when it reads better than a fold —
  boring beats clever.
- TS: `for...of` over `.forEach()` — banned because `await` inside `forEach`
  does not pause iteration (the single most common async bug class), and
  `break`/`continue`/narrowing don't work across the callback boundary.
  `.map/.filter/.reduce/.some/.every/.find` remain fine.

## 7. Imports

**Rule (TS).** No namespace imports (`import * as X`), no wildcard re-exports
(`export * from`) — they obscure call-site origin and silently mask name
collisions.

**Rule (Rust).** `use hdk::prelude::*` / `use hdi::prelude::*` are the one
sanctioned glob — they are the SDK's designed surface. Everything else imports
by name; no other glob `use`.

## 8. Code lands with its consumable surface

**Rule.** A zome change that nothing can consume is dead weight. Every shipped
extern lands WITH: its cap-grant decision (granted in `set_cap_tokens()` or
deliberately local-only — decided, not defaulted), its test (host unit or
sweettest conductor, per blast radius), and its wire contract documented in the
generation's `docs/HUMM_TAURI_*_INTEGRATION.md` handoff.

**Test:** "can humm-tauri (or a sweettest agent) exercise this end-to-end
today?" If no, the change isn't done — finish the surface or don't ship the
plumbing.

## 9. (Reserved — MobX × React reactive reads)

Does not apply: this repo has no frontend. Kept as a numbered stub so section
numbers stay aligned with `../humm-tauri/CODING_STANDARDS.md` §9.

## 10. Errors are loud; silent swallow is the exception

**Rule.** Zome code propagates with `?` / `ExternResult` — a panic traps the
WASM guest, so `.unwrap()`/`.expect()` are banned in guest code. When an error
is handled instead of propagated, it goes through the HDK log macros
(`warn!`/`debug!`) before any fallback runs. There is NO LoggingService here —
that is humm-tauri's layer; zome logs are HDK macros, kept meaningful.

Silent `let _ = fallible()`, `if let Err(_)`, trailing `.ok();`, and masking
`unwrap_or_default()` are prohibited UNLESS all five hold:

1. the failure mode is specific and named;
2. the failure is expected often enough that logging would be noise;
3. the fallback IS the documented, well-defined behavior (list tolerance —
   dropping an unresolvable link target from a list read — is the canonical
   sanctioned case);
4. the swallow is isolated to its handler and does not propagate a corrupted
   state;
5. a one-line comment names the criterion.

Default answer to "can I swallow this?" is NO. Prefer loud-and-spammy over
quiet-and-broken — downgrade to `debug!` or dedupe rather than delete the call.

TS test bodies are stricter: no general-purpose `catch` inside `it(...)` —
assert with `expect(...).rejects` / `toThrow` against the exact expected
shape; unknown shapes must rethrow.

## 11. Test path math: Vite-native resolution

**Rule.** A Vitest test reading a sibling file uses the `?raw` import suffix.
Never `path.resolve(process.cwd(), ...)` and never
`dirname(fileURLToPath(import.meta.url))` + relative segments — both produce
wrong paths on Windows under Vitest's transform. Only fallback when `?raw`
can't express it: `import.meta.dirname`.

**Reviewer cue.** Any `node:path` / `node:url` import in a new test file is
suspect; "fails on Windows, passes on Linux" is the prime path-math symptom.

## 12. Suppression policy (`#[allow]` / lint disables)

**Rule.** Every suppression — Rust `#[allow(...)]`, `#[expect(...)]`, or any
lint disable in TS config — names the exact lint and carries a one-sentence
structural justification a reviewer can accept WITHOUT other context. If the
code could be restructured to satisfy the lint, restructure it. Repo-wide
allows in `Cargo.toml`/config are a design decision and get reviewed as one.

`cargo clippy` runs clean — warnings are errors at gate time. A suppression
that exists to mute a real defect is a defect.

## 13. Test structure: Given / When / Then

**Rule.** `describe` = Given (the starting context, never just the type name);
`it`/`#[test]` name = When + Then in one label. Test output alone — no source
open — must convey the behavioral contract.

- Multi-event sequences assert the FULL ordered slice (`toEqual` on the ordered
  array; Rust `assert_eq!` on the whole `Vec`), never a stack of `toContain`.
- Assert the exact full expected object/error (minus nondeterministic fields),
  not one spot-checked field. Reject-string assertions in sweettest pin the
  validator's wire-stable contract — assert them exactly.
- Same-shaped cases are driven from flat data tables — see the addendum.

---

@ADDITIONAL_CODING_STANDARDS_AND_GUIDANCE.md
