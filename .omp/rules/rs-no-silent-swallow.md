---
description: Don't silently drop a Rust Result — propagate with `?` or log via the HDK macros; no `let _ = call()`, `if let Err(_)`, or trailing `.ok();` without a named reason
condition:
  - "let _ = [a-zA-Z_][\\w:.]*\\("
  - "if let Err\\(_\\)"
  - "(?:^|\\n)[^=\\n]*\\.ok\\(\\);"
scope: "tool:edit(*.rs), tool:write(*.rs)"
interruptMode: never
---

A returned error is propagated (with `?`) or handled — never silently dropped.
Don't discard a `Result` with `let _ = something()`, `if let Err(_) = ...`, or a
trailing `.ok();` unless the swallow meets the "okay to eat" bar AND a one-line
comment names the criterion. (This is exactly what the `silent-failure-hunter`
agent hunts: `.ok()` that drops an `Err`, `unwrap_or_default()` that masks a real
failure, over-tolerant decodes.)

## Avoid

```rust
let _ = repo_delete(id);             // Result silently dropped
if let Err(_) = repo_delete(id) {}   // discriminant inspected, error data thrown away
repo_delete(id).ok();                // .ok() at end of a chain with no context on why
```

## Use

```rust
// Propagate — the zome idiom (ExternResult / WasmError):
repo_delete(id)?;

// Or handle with context + an HDK log, then recover deliberately:
if let Err(e) = repo_delete(id) {
    warn!("repo_delete failed for {id}: {e:?}");
    // documented fallback…
}
```

## When `.ok()` / `let _ =` is allowed

Only with ALL of: a specific named failure mode, a well-defined fallback that IS
the behaviour, isolation to the local handler, and a one-line comment naming the
reason. `unwrap_or_default()` is acceptable ONLY when "absent → empty" is genuinely
correct, never when it masks a decode/validation failure. The default is to
propagate with `?`.
