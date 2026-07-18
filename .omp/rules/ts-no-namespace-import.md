---
description: No namespace imports (import * as X) — use named imports
condition: "import\\s+\\*\\s+as\\b"
scope: "tool:edit(*.ts), tool:write(*.ts)"
---

Use named imports. Never `import * as X from '...'` in the TypeScript harness (`tests/src/**`, `scripts/*.ts`).

## Why

- `X.foo` call sites obscure where `foo` comes from; a named import makes the dependency explicit at the use site.
- A namespace import pulls a module's entire surface into scope, so a rename or removal upstream fails silently at the wildcard instead of at the one symbol you actually used.

## Avoid

```typescript
import * as common from "./common.js";
const input = await common.sampleCreateEncryptedContentInput();
```

## Use

```typescript
import { sampleCreateEncryptedContentInput } from "./common.js";
const input = await sampleCreateEncryptedContentInput();
```

If a module genuinely exposes no named bindings (rare legacy CJS interop), a default import is the answer — not a namespace import.
