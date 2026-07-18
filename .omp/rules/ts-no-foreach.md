---
description: Use for...of, never .forEach(callback)
condition: "\\.forEach\\("
scope: "tool:edit(*.ts), tool:write(*.ts)"
---

Iterate with `for (const x of xs)`. Never `xs.forEach(callback)` — the ban also covers `Map` / `Set` `.forEach`.

## Why

- `await` works in a `for...of` body — iteration pauses. Inside `forEach` the callback is fire-and-forget: the outer function returns before any iteration finishes, rejections escape into nowhere, and sequential semantics are silently lost. In a tryorama test this is the classic false green — the assertions run before the awaited zome calls resolve. This is the single most common production bug `.forEach` causes.
- `return` / `break` / `continue` work normally. `return` inside `forEach` returns from the callback only (the early-exit guard becomes a no-op); `break` / `continue` are syntax errors there.
- A throw inside `for...of` lands on the actual loop line; `forEach` synthesises a callback frame that hides which iteration failed.
- Shared lexical scope: variables above the loop and TypeScript narrowing flow into the body.

## Avoid

```typescript
records.forEach(async (record) => {
	await createEncryptedContent(alice, record); // scenario resolves before any call finishes
});
```

## Use

```typescript
for (const record of records) {
	if (record.skip) continue;
	created.push(await createEncryptedContent(alice, record));
}
```

`.map` / `.filter` / `.reduce` / `.flatMap` / `.some` / `.every` / `.find` are fine — they return values and carry functional semantics. The ban is `forEach` specifically. A genuinely justified synchronous exception takes a one-line comment naming why sequential-await semantics don't apply.
