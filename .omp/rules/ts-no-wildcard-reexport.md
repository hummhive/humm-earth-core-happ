---
description: No wildcard re-exports (export * from) — list exports explicitly
condition: "export\\s+\\*\\s+from\\b"
scope: "tool:edit(*.ts), tool:write(*.ts)"
---

Re-export by name. Never `export * from '...'` in the TypeScript harness (`tests/src/**`, `scripts/*.ts`).

## Why

- Wildcard re-exports hide a module's public surface and create implicit coupling.
- They silently mask name collisions: two source files exporting the same name — the second wins with no diagnostic.
- Explicit re-exports are a self-documenting contract.

## Avoid

```typescript
export * from "./common.js";
```

## Use

```typescript
export { createEncryptedContent, sampleCreateEncryptedContentInput, cellPubkeyB64 } from "./common.js";
```
