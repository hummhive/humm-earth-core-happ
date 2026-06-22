---
description: NIST elliptic curves (P-256/384/521, secp*r1, prime256v1) are banned project-wide — use Curve25519 / Ed25519 / X25519
condition: "secp256r1|secp384r1|secp521r1|prime256v1|nistp(256|384|521)|P-(256|384|521)|\\bp(256|384|521)\\b"
scope: "tool:edit(*.rs), tool:write(*.rs), tool:edit(Cargo.toml), tool:write(Cargo.toml)"
---

You MUST NEVER use NIST elliptic curves (P-256, P-384, P-521, secp256r1,
secp384r1, secp521r1, prime256v1) anywhere in this project — not in code, and not
as the choice forced by a new dependency. This is a hard rule, project-wide
(shared with humm-tauri).

## Why

NIST curves are NSA-designed; their primes resist constant-time implementation,
and the short-Weierstrass addition rule has exceptional cases that invite
side-channel attacks. The decision rule: if you must choose between something the
NSA designed and something the academic community designed, pick the academic one.

## Approved primitives

- **Asymmetric: Curve25519 / Ed25519 / X25519.** Holochain Agent signing is
  Ed25519 via the HDK — that is the only signing you should need in a zome.
- AEAD: ChaCha20-Poly1305 / XChaCha20-Poly1305. Hash: SHA-512 / BLAKE2b / BLAKE3
  (never MD5 / SHA-1). Password KDF: Argon2id; key derivation: HKDF-SHA512. RNG for
  secret material: OS CSPRNG only.

If a new dependency forces a NIST curve even as a fallback, reject the dependency —
there is no scenario where the fallback is preferable. When adding any crypto
dependency, state (1) the primitive chosen, (2) the rationale citing this rule,
(3) any deviation (there should be none without explicit approval).

(`secp256k1` — the Koblitz curve — is a different curve and is not what this rule
bans; the ban is the NIST `r1` / `prime256v1` / `P-NNN` family.)
