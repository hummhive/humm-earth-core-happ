---
description: Editing the integrity zome changes the DNA hash and FORKS the chain — proceed only for a sanctioned new pass (with migration), never as a drive-by
condition: ".*"
scope: "tool:edit(**/zomes/integrity/**/*.rs), tool:write(**/zomes/integrity/**/*.rs)"
---

You are editing the **integrity zome** (`content_integrity`). This is the
highest-gravity change in the repo: an integrity change **alters the DNA hash**,
which **forks/splits the chain** — every existing Agent on the old DNA can no
longer gossip with Agents on the new one.

## What a sanctioned integrity change requires

- A new **pass** (pass-1 → pass-2 → … → the next one), recorded in `CLAUDE.md`
  "Pass lineage" + `docs/CODEMAPS/architecture.md` "hApp Version Lineage" (new DNA
  hash prefix, "Integrity Change? YES", the key change).
- A **migration pipeline** run for every existing user (export → migrate-hive →
  import → mark-migrated; `scripts/migrate-dna.ts`).
- **Multi-user validation/verification** — not a solo change.
- A corresponding **humm-tauri** update (it bundles the new `.happ`).

## Wire shapes

Add fields with `#[serde(default)]` so old Agents can still decode new records
(the new field deserializes to its `Default`). Remove or rename a field ONLY via
a versioned migration — never drop a field from a live wire shape without a pass.

## This guard is "not by accident", not "never"

Integrity changes ARE expected and allowed **when sanctioned** — a new authority
model, a role change, an `AclSpec` variant. The planned **single-owner role**
(exactly one owner at a time, superseding other roles so others cannot
de-authorize the owner, with a handshake/handoff required on owner change) is
precisely such a sanctioned integrity change: cutting it as a new pass is correct.
What this guard prevents is an **accidental or drive-by** integrity edit — a
"quick fix" in an integrity file that silently forks the chain.

If this IS a deliberate, sanctioned pass: proceed — bump the pass, plan the
migration, validate multi-user, update the lineage docs. If you only meant a
behaviour tweak, check whether it belongs in the **coordinator** zome instead
(`zomes/coordinator/`) — coordinator changes are hot-swappable and do NOT change
the DNA hash.
