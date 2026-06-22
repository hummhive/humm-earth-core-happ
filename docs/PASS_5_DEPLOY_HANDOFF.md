# Pass-5 deploy handoff — humm-tauri integration

Short-form handoff for the humm-tauri team to integrate the pass-5
integrity-zome fork shipped on `feat-integrity-pass-5-owner-role`.

Pass-5 **intentionally bumps the DNA hash** (first integrity change since
pass-4) and is the next intentional DNA bump after pass-4. Existing pass-4 data
MUST be migrated forward via the `scripts/migrate-dna.ts` pipeline (see the
Migration section of
[`HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md`](./HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md)).

For the full wire-shape + frontend-cutover reference, see
[`HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md`](./HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md);
for the commit-time BDD guarantees, see
[`HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md`](./HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md).

## TL;DR

- **DNA hash CHANGED** from
  `uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV` (pass-4) to
  `uhC0k2dXMIa1yI-V4ibCWMiTY5G6-p0laq6IOAVQ2F8XXReDHSxyS` (pass-5 — the **reproducible** hash; see
  `.baseline-hashes.txt` "Reproducibility contract" for build + verify
  commands). Coordinator hot-swap does NOT work for this pass; users see a new
  cell on install and require the migration flow to keep their data.
- **Toolchain bumped to holochain 0.6.1** (hdk 0.6.1 / hdi 0.7.1 / HSB 0.0.57),
  matching the runtime humm-tauri already runs. The only zome-facing API change
  was `GetOptions { strategy: … }` → `GetOptions::network()`. The sweettest
  harness moved off the removed `datachannel-vendored` transport to
  `transport-iroh` (tx5 dropped everywhere).
- **Hive Owner role SHIPPED** — single owner per hive, transferable only by a
  two-party offer/accept handshake, undemotable by admins. `Role::Owner` is no
  longer grantable via `HiveMembership`. Current owner resolved by
  `get_hive_owner` / `get_member_hive_role`.
- **Reader read-only bugfix** — the content-delete validator no longer
  authorizes the `reader` bucket on non-DM content.
- **Role-grant hardening** — only the current owner may grant Admin (integrity
  ever-owner floor + coordinator current-owner precheck); the founder cannot be
  re-cast into a membership role.
- **Adjacent fixes** — `delete_group_genesis` (author-gated, refuses live
  groups), `revoke_hive_membership` (expiry-based, owner-protected), invite
  `max_uses` advisory soft-cap (`redeem_invite_grant`), `list_by_author`
  `since_ts`/`limit` bounds.
- **humm-tauri read helpers** — `get_hive_owner`, `content_summary`,
  `my_pair_shared_secret_exists`, `changes_since` (see the integration doc §6).

## Distribution

- Staged in `~/hummhive-official-happ-versions/` and
  `../humm-tauri/.testdata/happs/` with a new `MANIFEST.tsv` row + the pass-5
  DNA hash; `.baseline-hashes.txt` gains a Pass-5 section.
- `src-tauri/bin/humm-earth-core-happ.happ` is NOT overwritten by default
  (avoids forcing a live conductor migration mid-RC) — opt in via
  `../humm-tauri/scripts/provision-happ.mjs` after updating `.testdata`.

## Verification

Multi-user behavior proven in-process via `crates/sweettest`
(`tests/owner_and_acl.rs`: owner handshake + admin-grant authority + owner
reject; two-transfer cross-node determinism; revoke owner-protect) — tryorama
cannot boot on hc 0.6.x. Host validator coverage: `cargo test -p
content_integrity --lib` + `cargo test -p content --lib`.
