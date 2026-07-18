---
description: Prefer async/await over new Promise(...) and .then(...) chains
condition:
  - "new Promise\\("
  - "\\.then\\("
  - "\\.catch\\("
scope: "tool:edit(*.ts), tool:write(*.ts)"
---

Express asynchronous code with `async` / `await`. Do not introduce `new Promise((resolve, reject) => ...)` or `.then(...).catch(...)` chains in the TypeScript harness (`tests/src/**`, `scripts/*.ts`).

## Why

- `await` preserves the call stack across async boundaries; `.then(...)` chains lose it (you get the resolver location, not the caller) — which matters when a zome call rejects and you need the failing test line.
- Reading order matches execution order: `const r = await cell.callZome(...); use(r);` reads top-to-bottom; `.then(use).catch(handle)` reads inside-out.
- A single `try/catch` around an `await` block catches sync throws AND rejections; a `.then(...).catch(...)` only catches rejections in the same chain — a sync throw inside the `.then` callback bypasses it.
- `if (await x)` branches cleanly; the `.then` form needs an inner closure.

## Avoid

```typescript
function loadRecord(cell: CallableCell, hash: ActionHash): Promise<Record> {
	return cell.callZome({ zome_name: "content", fn_name: "get", payload: hash }).then(decodeRecord);
}
```

## Use

```typescript
async function loadRecord(cell: CallableCell, hash: ActionHash): Promise<Record> {
	const record = await cell.callZome({ zome_name: "content", fn_name: "get", payload: hash });
	return decodeRecord(record);
}
```

## The only legitimate `new Promise`

These are infrastructure, not test/setup logic:

1. Bridging a callback-only API into a promise (prefer `util.promisify` first).
2. An externally-resolvable promise (event-driven: awaiting a Holochain signal via `conductor.appWs().on(...)`, resolved from the handler).
3. A `Promise.race`-style timeout whose timer needs a manual `reject`.

If one applies, keep `new Promise` and add a one-line comment naming which. Combining N promises is `Promise.all` / `Promise.race` / `Promise.allSettled`, never a hand-rolled `new Promise`.

## `.then(...)` / `.catch(...)` tails

A `.then(cb)` or `.catch(err => ...)` chain is the same anti-pattern — convert it to `await` inside a `try/catch`. The tryorama / Vitest harness has no logger layer to route errors through, so let a genuine failure propagate (the test fails, which is the point); catch only to assert an *expected* rejection or to run a documented fallback (see `ts-no-bare-catch`). A signal-awaiting `new Promise` whose handler calls `resolve` is the one place a promise is constructed by hand — everything else is `await`.
