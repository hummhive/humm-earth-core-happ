---
description: Never bare `catch {` or an empty catch — bind (err), check the expected shape, and rethrow the unexpected
condition:
  - "catch\\s*\\{"
  - "\\.catch\\(\\s*\\(\\s*\\)\\s*=>"
scope: "tool:edit(*.ts), tool:write(*.ts)"
---

Every caught error binds `(err)`. There is no project logger in the TypeScript harness, so a caught error is either **asserted** (you expected this rejection — prove its shape) or **rethrown** (you didn't — let the test fail loudly). Never `catch {` (no binding), never an empty `catch (err) {}`, never a swallowing `.catch(() => ...)` tail.

## Why

Silent catch arms turn a red test green: a zome call that starts rejecting with a *different* error — a validation-string change, a wire-shape drift, a missing grant — sails through a swallowing arm and the regression ships. A test that eats the error it was supposed to check is worse than no test.

## Avoid

```typescript
try { await createEncryptedContent(alice); } catch { /* ignore */ }   // no binding, no signal
try { await createEncryptedContent(alice); } catch (e) {}             // bound, ignored
await createEncryptedContent(alice).catch(() => null);                // swallowing tail
```

## Use

Assert an *expected* rejection with Vitest's `rejects` matcher — that is the whole point of the negative-path test:

```typescript
await expect(createEncryptedContent(bob, badInput)).rejects.toThrow(/InvalidCommit/);
```

When you must `try/catch` (e.g. to inspect the error before asserting), check the exact expected shape and rethrow anything else:

```typescript
try {
	await createEncryptedContent(bob, badInput);
	throw new Error("expected the zome call to reject");
} catch (err) {
	const message = err instanceof Error ? err.message : String(err);
	if (!message.includes("author does not match")) throw err;  // unexpected → fail loudly
}
```

## When eating is allowed

Only when ALL hold: the failure mode is specific and named, it is expected often enough that asserting each occurrence would be noise, the fallback IS the documented behaviour, and the site is isolated to its handler — plus a one-line `// expected: <reason>` comment. The default answer to "can I swallow this?" is no — assert it or rethrow it.
