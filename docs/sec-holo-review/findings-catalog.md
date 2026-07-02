# Security/Holochain review findings catalog

Scope: catalog of the three independent oracle reports under `docs/sec-holo-review/` for the `dry-refactor` pass-6 candidate. This file is a synthesis only; the source reports remain authoritative.

Source reports:

- `security-review-a.md` — integrity/ACL/security lane.
- `security-review-b.md` — coordinator/capability/migration security lane.
- `holochain-review.md` — Holochain skill / ReviewZome compliance lane.

## Executive catalog

Historical pre-fix verdict across all lanes: **BLOCK before merge/release**.

The local fix wave closed both reported BLOCK items. The source reports remain
historical evidence of what the oracles found before the fix wave; the WARN and
NOTE items below remain follow-up hardening/documentation candidates, not
merge-blocking findings for the two fixed blockers.

## Resolution status

After these reports, the local `dry-refactor` working tree fixed both BLOCK
items and rebuilt pass-6:

- C-BLOCK-1 fixed by validating `OriginalHashPointer` create/delete links in
  integrity and deriving coordinator update roots from native action metadata
  instead of network pointer-link `[0]`.
- C-BLOCK-2 fixed by adding a same-entry-type update gate in validation dispatch,
  including the `StoreRecord::UpdateEntry` EncryptedContent special path.
- Rebuilt candidate DNA:
  `uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz`; hApp SHA:
  `3062de3851eac81fedd425325b30f3cabaaa2000e1e295ba7db5d4d031dda5d3`.
- Re-gated with `cargo fmt --all --check`, `cargo test -p content_integrity --lib`
  (76/76), `cargo test -p content --lib` (25/25), `cargo clippy --workspace
  --all-targets -- -D warnings`, reproducible zome/DNA/hApp build, hash capture,
  and Sweettest (12 active + 1 ignored).

The source reports remain historical evidence of what the oracles found before
the fix wave.

## BLOCK

### C-BLOCK-1 — `OriginalHashPointer` is unvalidated public DHT state trusted by coordinator update/migration plumbing

Reported by:

- Security A: `security-review-a.md:37-63`.
- Security B: `security-review-b.md:44-73`.
- Holochain review: `holochain-review.md:31-55`.

Core evidence from reports:

- Integrity create/delete validation accepts `OriginalHashPointer` unconditionally.
- Coordinator `update_encrypted_content` fetches `OriginalHashPointer` links with `GetStrategy::Network` and trusts `original_hash_link[0]` after committing an update.
- Migration marker writers delegate to `update_encrypted_content`, inheriting the same substrate.

Impact: graph-integrity / availability blocker. Reports did **not** classify this as a third-party content-authority bypass, but did classify it as a trusted-public-DHT-link violation.

Suggested fix direction: validate pointer create/delete in integrity, stop trusting `[0]`, derive/verify update root deterministically, and add hostile pointer tests. Any fix touches integrity and requires recapturing pass-6 DNA/hash artifacts.

### C-BLOCK-2 — Cross-entry-type updates can route through `EncryptedContent` validation and bypass immutable-entry validators

Reported by:

- Security A: `security-review-a.md:64-80`.
- Holochain review: `holochain-review.md:57-76`.

Core evidence from reports:

- Update validation dispatches by the **new** app-entry type.
- `validate_update_encrypted_content` checks same original action author, but does not require the original action to be `EntryTypes::EncryptedContent`.
- Immutable entries such as `HiveGenesis`, `HiveMembership`, `GroupGenesis`, `GroupMembership`, and owner-handoff entries rely on their own update validators being reached.

Impact: integrity-rule blocker. Reports label exploit/consumer impact carefully: current coordinators may not follow these native update chains for authority reads, but the DHT would still accept update edges that violate documented immutability.

Suggested fix direction: enforce original-entry type equality for every update path, at least inside `validate_update_encrypted_content`, preferably centrally in validation dispatch. Add host tests for cross-type update attempts.

## WARN

### C-WARN-1 — Production coordinator read helper has `unwrap()` panic assumptions on cap-granted read paths

Reported by:

- Security A: `security-review-a.md:84-98`.
- Security B: `security-review-b.md:77-93`.
- Holochain review: `holochain-review.md:80-92`.

Impact: availability hardening. Unexpected host/DHT detail shape can trap the WASM guest instead of returning `ExternResult` / `Ok(None)`.

### C-WARN-2 — Non-DM `public_key_acl` buckets are unbounded/undeduped and feed remote-signal fan-out

Reported by:

- Security A: `security-review-a.md:100-117`.
- Holochain review: `holochain-review.md:159-174`.

Impact: resource amplification / routing-hint hardening, not an authority bypass. Public/OpenWrite validation does not constrain `public_key_acl`, and coordinator fan-out iterates reader strings.

### C-WARN-3 — `update_encrypted_content` can mutate link-bearing header fields without reindexing discovery links

Reported by:

- Security A: `security-review-a.md:118-133`.
- Holochain review: `holochain-review.md:143-157`.

Impact: stale/missing discovery indexes for ACL/content-type/content-id/hive-context changes. Existing docs already recommend re-authoring for ACL mutation, but code still permits incompatible updates.

### C-WARN-4 — Migration JSON artifacts are sensitive but written/documented with insufficient local-permission hardening

Reported by:

- Security A: `security-review-a.md:135-149`.
- Security B: `security-review-b.md:95-112`.
- Holochain review: `holochain-review.md:176-190`.

Impact: local operational metadata exposure risk. Bundle/remap/hive-bundle files can expose social graph, roles, app IDs, agent keys, action-hash remaps, content metadata, and ciphertext. Reports recommend `0o700` dirs / `0o600` files plus docs/runbook updates.

### C-WARN-5 — Advertised legacy Tryorama/TypeScript test path is stale for pass-6

Reported by:

- Security A: `security-review-a.md:202-217` as NOTE.
- Security B: `security-review-b.md:114-129`.
- Holochain review: `holochain-review.md:94-109`.

Impact: release-process false confidence. Active conductor gate is Sweettest; README/package/legacy TS harness still point at stale Tryorama/payload shapes and `any` usage.

### C-WARN-6 — Documentation drift can mislead downstream integration/release decisions

Reported by:

- Holochain review: `holochain-review.md:111-126`.
- Security B: `security-review-b.md:183-196` for `DNA_MIGRATION_GUIDE.md` update-validation drift.
- Security A: `security-review-a.md:187-200` for same migration-guide drift.

Impact: docs present conflicting current-generation/test/security model facts. Examples include README setup/test, pass roadmap version line, v1 handoff claiming canonical current state, BDD owner-grant wording, and migration-guide text that understates current update validation.

### C-WARN-7 — Query docs/comments understate `list_by_author` pagination support

Reported by:

- Holochain review: `holochain-review.md:128-141`.

Impact: downstream may implement unnecessary workarounds or assume missing behavior.

## NOTE / accepted residuals

### C-NOTE-1 — Owner-transfer re-seizure residual remains accepted and documented

Reported by:

- Security A: `security-review-a.md:153-169`.
- Security B: `security-review-b.md:133-148`.

Status: accepted product/security residual. Blast radius remains governance, not content decryption.

### C-NOTE-2 — Invite `max_uses` is advisory, not hard authority

Reported by:

- Security A: `security-review-a.md:171-185`.

Status: documented soft-cap behavior; real authority is still validated `HiveMembership`.

### C-NOTE-3 — `TimePath` / `TimeItem` are unused but permissively valid link types

Reported by:

- Security B: `security-review-b.md:150-164`.
- Holochain review: `holochain-review.md:194-206`.

Status: not a blocker because no coordinator consumer was found; still public DHT junk surface.

### C-NOTE-4 — Some legacy entries duplicate action-header timestamps

Reported by:

- Holochain review: `holochain-review.md:208-220`.

Status: compatibility note; avoid repeating this pattern in new entry types.

### C-NOTE-5 — Tolerant decode/filter paths appear intentional

Reported by:

- Holochain review: `holochain-review.md:222-232`.
- Security B: `security-review-b.md:207-208` pass category.

Status: documented query-tolerance design, not classified as silent failure.

### C-NOTE-6 — Migration marker fields may be empty by operator choice

Reported by:

- Security B: `security-review-b.md:166-181`.

Status: operator flexibility; release runbooks should treat missing `NEW_DNA_HASH_BASE64` / `NEW_APP_ID` as checklist exceptions.

## Reported pass categories

All lanes recorded passes/no-finding categories for the areas they checked, including:

- Entry/link/wire-shape stability.
- Deterministic validation using `op.flattened` and validation dependency fetches, not coordinator-only reads.
- Hive/group/content ACL authority except the two blockers above.
- Cap-grant mutator exclusion.
- Remote-signal ExternIO pre-encode and provenance stamping.
- Private `DmProbeLog` handling.
- Migration marker author filtering, except shared update-pointer substrate.
- No hardcoded secrets / forbidden NIST curve usage found by targeted scans.
- Pass-6 candidate-only release state and pass-5 current downstream target documented.

## Open follow-up decision points

1. Whether `update_encrypted_content` should reject link-bearing header changes
   or become a full reindex operation.
2. Whether non-DM `public_key_acl` should be globally bounded, deduped, ignored
   for fan-out, or constrained to a sentinel for public/open-write content.
3. Whether migration artifact permissions should be enforced in
   `scripts/migrate-dna.ts` or left as operator runbook policy.
4. Whether legacy Tryorama/TS harness should be retired from advertised gates or
   updated to current pass-6 shapes.
