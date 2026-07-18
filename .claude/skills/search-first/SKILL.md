---
name: search-first
description: Research-before-coding workflow. Search the HDK/HDI API, crates.io, and in-repo patterns for an existing solution before writing custom code. Invokes the librarian or technical-researcher agent.
origin: ECC
---

# /search-first — Research Before You Code

Systematizes the "search for existing solutions before implementing" workflow.

## Trigger

Use this skill when:
- Starting a coordinator-zome feature that likely has an existing HDK primitive or in-repo helper
- Adding a Rust dependency to a `Cargo.toml`
- The user asks "add X" and you are about to write a helper by hand
- Before creating a new utility, query wrapper, or abstraction

## Workflow

```
┌─────────────────────────────────────────────┐
│  1. NEED ANALYSIS                           │
│     Define what functionality is needed      │
│     Identify HDK/HDI + stack constraints     │
├─────────────────────────────────────────────┤
│  2. PARALLEL SEARCH                         │
│     ┌──────────┐ ┌──────────┐ ┌──────────┐  │
│     │ librarian│ │  in-repo │ │ tech-    │  │
│     │ HDK/crate│ │  grep    │ │ researcher│ │
│     │ source   │ │ zomes/,  │ │ crates.io │ │
│     │          │ │ tests/   │ │ /GitHub   │ │
│     └──────────┘ └──────────┘ └──────────┘  │
├─────────────────────────────────────────────┤
│  3. EVALUATE                                │
│     Score candidates (fit, maintenance,     │
│     API stability, deps, license, WASM cost)│
├─────────────────────────────────────────────┤
│  4. DECIDE                                  │
│     ┌─────────┐  ┌──────────┐  ┌─────────┐  │
│     │  Adopt  │  │  Extend  │  │  Build   │  │
│     │ as-is   │  │  /Wrap   │  │  Custom  │  │
│     └─────────┘  └──────────┘  └─────────┘  │
├─────────────────────────────────────────────┤
│  5. IMPLEMENT                               │
│     Use the HDK primitive / reuse the in-   │
│     repo helper / write minimal custom code │
└─────────────────────────────────────────────┘
```

## Decision Matrix

| Signal | Action |
|--------|--------|
| HDK/HDI already exposes it, or an in-repo helper matches | **Adopt** — call it directly, no new code |
| Partial match (HDK primitive or existing helper covers most of it) | **Extend** — build a thin wrapper on top |
| A well-maintained crate fits and clears the crypto HARD RULE | **Adopt** — add the dep, use it directly |
| Nothing suitable found | **Build** — write custom, but informed by research |

## How to Use

### Quick Mode (inline)

Before writing a helper or adding a dependency, mentally run through:

0. Does this already exist in the repo? → `grep` through `zomes/coordinator/`, `zomes/integrity/`, and `tests/src/` first
1. Does the HDK/HDI already do this? → check the `hdk`/`hdi` prelude before hand-rolling. Need a paging cursor? Check `GetLinksInputBuilder` / `LinkQuery` first. Need a hash? `hash_entry`. Need time? `sys_time`. Need entry history? `get_details` before walking actions by hand.
2. Is it a solved Rust problem? → search crates.io / docs.rs — but respect the crypto HARD RULE (never NIST curves; agent signing is Ed25519 via the HDK).
3. Is there a skill for this? → check `.claude/skills/`.
4. Is there a reference implementation? → GitHub code search across `holochain/holochain` and the holochain-open-dev orgs before writing net-new zome logic.

### Full Mode (agent)

For a source-level answer about the HDK/HDI or a specific crate, launch the
**librarian** agent — it reads the actual source and returns verbatim-cited API
signatures:

```
Task(subagent_type="librarian", prompt="
  What does hdk's GetLinksInputBuilder support for cursor-based paging?
  Version: match the pinned hdk in the workspace Cargo.toml.
  Return: the builder API + which fields bound the query, with source excerpts.
")
```

For ecosystem / version / comparative research across GitHub, crates.io, and
docs sites, launch the **technical-researcher** agent:

```
Task(subagent_type="technical-researcher", prompt="
  Research existing crates for: [DESCRIPTION]
  Constraints: no_std-friendly / WASM-safe / no NIST curves
  Search: crates.io, docs.rs, GitHub
  Return: a structured comparison with a recommendation
")
```

## Search Shortcuts by Category

### Holochain primitives (check these before writing a helper)
- Links & paths → `hdk` link helpers (`create_link`, `get_links`, `GetLinksInputBuilder`, `Path`)
- Entries & CRUD → `create_entry`, `update_entry`, `delete_entry`, `get`, `get_details`
- Hashing → `hash_entry`, `hash_action`
- Time → `sys_time`
- Validation → `hdi` op-based validation (`OpType` / `FlatOp`) — do not re-derive what the host already checks

### Rust tooling
- Typed errors → `thiserror` (library) / `anyhow` (app) — already the project convention
- Serialization → `serde` + `holochain_serialized_bytes` (`SerializedBytes`); on the TS harness side, `@msgpack/msgpack`
- Iterators → the std iterator adapters (prefer them over manual loops); `itertools` only if it earns its place

### Testing
- In-process conductor → the `crates/sweettest` harness (tryorama cannot boot on hc 0.6.x)
- Wire-level / cross-agent → tryorama + Vitest in `tests/src/**`

## Integration Points

### With the standard-workflow skill
Run search-first before the implementation phases of a plan, not after:
- Research surfaces the HDK primitive or in-repo helper that already solves the slice
- The plan then reuses it instead of budgeting time to reinvent it
- Findings that touch the DNA (any integrity-zome candidate) feed the change-gravity call

### With the rust-build-resolver agent
When a new dependency is on the table, the researcher confirms it is WASM-safe and
crypto-compliant before it lands in `Cargo.toml` — the build-resolver then owns
fitting it into the workspace.

## Examples

### Example 1: "Add a paging cursor to a list_by_* extern"
```
Need: Cursor-based paging over Hive-path links
Search: hdk prelude — GetLinksInputBuilder, LinkQuery
Found: GetLinksInputBuilder supports before/after + limit bounds
Action: ADOPT — build the cursor on the HDK builder, no custom link walk
Result: Zero hand-rolled pagination; matches the existing list_by_hive_link pattern
```

### Example 2: "Add typed errors to a new module"
```
Need: A typed error enum for a coordinator helper
Search: Cargo.toml — thiserror already a dependency; project convention
Found: thiserror (library-side), anyhow (app-side)
Action: ADOPT — derive thiserror; no bespoke error type
Result: Consistent with the rest of the zome, no new dependency
```

### Example 3: "De-duplicate content on create"
```
Need: Canonical selection when two byte-identical entries could exist
Search: grep zomes/coordinator — find_or_create_* + selectCanonicalByHash already exist
Found: the lowest-b64-STRING canonical rule (JS-parity) is already implemented
Action: EXTEND — reuse the existing find-wins helper; do not re-derive the canonical rule
Result: One shared invariant, no divergent second implementation
```

## Anti-Patterns

- **Jumping to code**: hand-rolling a helper without checking the HDK prelude first
- **Ignoring the host**: re-implementing validation the integrity zome / host already performs
- **Over-wrapping**: wrapping an HDK primitive so heavily it loses its guarantees
- **Dependency bloat**: pulling a large crate for one small feature — and every dep is a WASM-size and audit cost
