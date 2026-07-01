---
description: "Never `: any` or `as any` in the TS test harness — use unknown, a generic, or the real type"
condition:
  - ":\\s*any\\b"
  - "as\\s+any\\b"
scope: "tool:edit(*.ts), tool:write(*.ts)"
---

Never `: any` or `as any` in the tryorama / Vitest TypeScript (`tests/src/**`,
`scripts/*.ts`). They disable type checking exactly where a boundary needs
precision. (`coding-standards` skill: "FAIL: BAD: Using 'any'".)

## Use instead

- `unknown` for unvalidated input, then narrow with a type guard.
- A domain type when the shape is known (the zome wire types, `@holochain/client`
  types).
- A generic when the caller supplies the shape.

## Avoid

```typescript
function readId(value: any): any { return value.id; }
const record = (await callZome("get_thing", hash)) as any;
```

## Use

```typescript
function readId(value: unknown): string | undefined {
	if (value && typeof value === "object" && "id" in value) {
		const candidate = (value as { id: unknown }).id;
		return typeof candidate === "string" ? candidate : undefined;
	}
}
const record = (await callZome("get_thing", hash)) as Record | null;
```

If a library boundary truly forces an unchecked cast, use `as unknown as T` with a
short reason. Never leave a bare `any`.
