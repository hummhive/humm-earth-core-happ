---
description: In tests, read sibling source via ?raw imports — never process.cwd() or fileURLToPath(import.meta.url)
condition:
  - "process\\.cwd\\(\\)"
  - "fileURLToPath\\(\\s*import\\.meta\\.url\\s*\\)"
scope: "tool:edit(*.test.ts), tool:write(*.test.ts)"
---

When a test reads a sibling source file from disk (source scanning, structural pinning, snapshot-of-text invariants), use Vite's `?raw` import suffix. Never compute the path with `path.resolve(process.cwd(), ...)` or `dirname(fileURLToPath(import.meta.url))`. `tests/` runs on Vitest (Vite transform), so this footgun applies directly here.

## Why

Vitest on Windows + the Vite transform expose two path-math footguns that don't appear on POSIX — and this repo builds and tests on WSL against a Windows-mounted clone, so both are live:

- `path.resolve(process.cwd(), 'src/foo.ts')` produces `<cwd>\<cwd>\src\foo.ts` on Windows — vitest's POSIX `path.resolve` leaks in and doesn't treat `C:\` as a drive-letter absolute, so it concatenates instead of replacing.
- `dirname(fileURLToPath(import.meta.url))` resolves to a different depth under vitest's transform, so relative `..` walks march past the repo root.

## Use

```typescript
import commonSource from "./common.ts?raw";

it("exposes cellPubkeyB64", () => {
	expect(commonSource).toMatch(/export\s+function\s+cellPubkeyB64/);
});
```

If the assertion genuinely needs `fs` at runtime, the only acceptable path base is `import.meta.dirname` (Node 20.11+) — never `fileURLToPath(import.meta.url)`. And reconsider whether it is really a unit test rather than a leaked integration concern.
