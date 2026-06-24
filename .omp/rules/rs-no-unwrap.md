---
description: In zome code, propagate with `?` (ExternResult) — never `.unwrap()` (a panic traps the WASM guest)
condition: "\\.unwrap\\(\\)"
scope: "tool:edit(**/zomes/**/*.rs), tool:write(**/zomes/**/*.rs)"
interruptMode: never
---

In zome (integrity / coordinator) code, propagate errors with `?` over
`ExternResult<T>` / `Result<T, WasmError>`. Never `.unwrap()`: in a WASM guest a
panic traps the whole zome call, turning a recoverable error into a hard
`RibosomeError` with no context for the caller. (rust-patterns: "`?` over
`unwrap()` — never panic in production code.")

## Avoid

```rust
let record = get(hash, GetOptions::default())?.unwrap();   // panics if absent
let entry = record.entry().to_app_option::<Foo>().unwrap();
```

## Use

```rust
let record = get(hash, GetOptions::default())?
    .ok_or(wasm_error!(WasmErrorInner::Guest("record not found".into())))?;
let entry: Foo = record
    .entry()
    .to_app_option()
    .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?
    .ok_or(wasm_error!(WasmErrorInner::Guest("entry missing".into())))?;
```

## Narrow exceptions

- `.expect("…proven invariant…")` with a real message is acceptable for a genuine,
  locally-proven invariant — but prefer `?` and a typed error.
- **Test code** (`#[cfg(test)]` modules, `crates/sweettest`) may `.unwrap()` freely
  — a panic there is a test failure, which is the point. This reminder targets
  zome production paths.
