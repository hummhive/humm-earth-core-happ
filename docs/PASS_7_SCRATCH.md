# pass-7 scratch ledger (branch-only; never merges to main)

## DNA hash log
| milestone | commit | dna hash | integrity wasm sha256 |
|---|---|---|---|
| M0 (pre-integrity baseline) | 991b729 | uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz | 2656a9100937f7e6d17e2eebd5e744a1ef16e8e36b0efa089dc2f6382a655ae2 |
| M1 (header bounds + update continuity) | 3a548db | uhC0kC6Rjh9-NE9vHSQ6Zy4EUtjoZvKfwzD8Txo5Hsu6Gw7irpl4C | 86c7950fe65f7e5c24d54f85fbabb7a8fdf3591632fd0d8d7f529b22ca0f8128 |
| M2 (open-write payload caps) | 192047f | uhC0kXQNnSRgwB42kF0RhtyCgm9noYg-VspFoeeetC4LufcMt7geE | f2e4284043b6cd0bb4076342378f1d1c15e2c0c1ac03e2c1782bbed94d610c23 |
| M3 (system-role GroupGenesis uniqueness) | a2350d7 | uhC0k8qyE7-0_OOmMw2beHEmaLTyksE1i6oVqj0EididK2Da2BEJ7 | 4f764c336eb280f8a764475dc1897ded3bd0afb5ec58547a069856492836a85d |
| M4 (cross-generation lineage) | 9ba4244 | uhC0k7pbRFimR34Mc5CzgC_QTbh3Z-9rdIypgTf-2U0tur2ir7vSd | c27ccbe0a97498c0da9be90a6e378039c731ac12c9f11391eb64052399e29fd7 |
| M5 (two-generation conductor proof) | 685b0dd | uhC0k7pbRFimR34Mc5CzgC_QTbh3Z-9rdIypgTf-2U0tur2ir7vSd (unchanged; test-only) | c27ccbe0a97498c0da9be90a6e378039c731ac12c9f11391eb64052399e29fd7 |
| M6 (coordinator riders: reindex + include_liveness) | 63c6ae2 | uhC0k7pbRFimR34Mc5CzgC_QTbh3Z-9rdIypgTf-2U0tur2ir7vSd (UNCHANGED; coordinator-only) | c27ccbe0a97498c0da9be90a6e378039c731ac12c9f11391eb64052399e29fd7 |
| M8 (durable HiveMembershipIndex) | 2b24605 | uhC0kO386QfCNoeQJZ36BbYj8ZFtqvaOjFIbUqZQK8DZo14KsS6o8 | 3edc1dfa021b23c81e1ee94ac1779ac102c386ce9fa9145d2a1e6858ce562ac1 |
| M9 (load-bearing system-role display_id) | 7c3fbd4 | uhC0koUno-fuuCeAdMbEnkHqSWW2k1EHx76Rym8Dt9cyoB4djU_Bv | dff117981cac29f9a20ec14d0309d53d07b9d8dfbe64c1fc07f1cea886ec9891 |
| M10 (idempotent delete + ACL liveness parity + paged inbox) | 97602f5 | uhC0koUno-fuuCeAdMbEnkHqSWW2k1EHx76Rym8Dt9cyoB4djU_Bv (UNCHANGED; coordinator-only) | dff117981cac29f9a20ec14d0309d53d07b9d8dfbe64c1fc07f1cea886ec9891 |
| M11 (role-K downward-closure enumeration) | 34cad93 | uhC0koUno-fuuCeAdMbEnkHqSWW2k1EHx76Rym8Dt9cyoB4djU_Bv (UNCHANGED; coordinator-only) | dff117981cac29f9a20ec14d0309d53d07b9d8dfbe64c1fc07f1cea886ec9891 |
| M12 (review-lane fixes + DRY sweep) | 74d52ea | uhC0koUno-fuuCeAdMbEnkHqSWW2k1EHx76Rym8Dt9cyoB4djU_Bv (UNCHANGED; coordinator docs/nits + test-only) | dff117981cac29f9a20ec14d0309d53d07b9d8dfbe64c1fc07f1cea886ec9891 |
| M13 (group_acl bucket disjointness + deterministic link-validator rejects) | (backfill@M15) | uhC0kbz8DhCkYWaLeihsumn8V726s3ZzWcTsAqLNcFBWIEVaWVPnB | 7a4cd2e03328ed4c23e2329dff5ccf23b42e1a16f0a5ce137f13b7f11434e2ad |
| M14 (create-link publication + membership twin extraction) | (backfill@M15) | uhC0kbz8DhCkYWaLeihsumn8V726s3ZzWcTsAqLNcFBWIEVaWVPnB (UNCHANGED; coordinator-only refactor) | 7a4cd2e03328ed4c23e2329dff5ccf23b42e1a16f0a5ce137f13b7f11434e2ad |
| M15 (complete link-validator normalization + review doc fixes) | (backfill) | uhC0kemuLaIzdw19cQwOLB1JB7o7-5ZFXNIwcOYlBpNFfL_8Uicl6 | 39062286742a11836822ab8cf5fcfde2ed6e92321b90106a4ed5de38eb8e92f0 |

## New reject literals (accumulates the blessing-time BDD delta)
| # | literal | validator fn | milestone |
|---|---|---|---|
| L1 | `header id must be 1-256 chars` | `validate_header_bounds` | M1 |
| L2 | `header content_type must be 1-128 chars` | `validate_header_bounds` | M1 |
| L3 | `header display_hive_id must be at most 256 chars` | `validate_header_bounds` | M1 |
| L4 | `public_key_acl owner must be at most 64 chars` | `validate_header_bounds` | M1 |
| L5 | `public_key_acl buckets accept at most 256 entries` | `validate_header_bounds` | M1 |
| L6 | `public_key_acl keys must be 1-64 chars` | `validate_header_bounds` | M1 |
| L7 | `public_key_acl buckets must not contain duplicate keys` | `validate_header_bounds` | M1 |
| L8 | `EncryptedContent updates must not change the id` | `validate_update_continuity` | M1 |
| L9 | `EncryptedContent updates must not change the hive context` | `validate_update_continuity` | M1 |
| L10 | `EncryptedContent updates must not change the acl_spec variant` | `validate_update_continuity` | M1 |
| L11 | `EncryptedContent updates may only stamp content_type with the _migrated/ prefix` | `validate_update_continuity` | M1 |
| — | `update original is not an EncryptedContent` (defensive; upstream same-entry-type gate normally fires first) | `validate_update_encrypted_content` | M1 |
| L12 | `Public and OpenWrite payloads accept at most 1000000 bytes` | `validate_open_write_payload_size` | M2 |
| L13 | `a GroupGenesis for this hive and hive-wide role already exists on your chain` | `validate_create_group_genesis` | M3 |
| L14 | `lineage prior dna hash is not a valid DNA hash` | `validate_lineage_shape` | M4 |
| L15 | `lineage prior action hash is not a valid action hash` | `validate_lineage_shape` | M4 |
| L16 | `lineage must cite a prior generation, not this one` | `run_content_validators` | M4 |
| L17 | `lineage is immutable once set` | `validate_update_continuity` | M4 |
| L18 | `lineage prior record did not resolve in the prior-generation cell` | `probe_prior_authorship` | M4 |
| L19 | `lineage prior record was not authored by the caller` | `probe_prior_authorship` | M4 |
| L20 | `lineage prior cell is not reachable on this conductor` | `probe_prior_authorship` | M4 |
| — | `Lineage link base does not match the target's lineage claim` | `validate_create_link_lineage` | M4 |
| — | `Lineage link target has no lineage claim in its header` | `validate_create_link_lineage` | M4 |
| — | `Lineage link delete must be authored by the link creator` | `validate_delete_link_lineage` | M4 |
| — | `HiveMembershipIndex tag must be empty` | `validate_create_link_hive_membership_index` | M8 |
| — | `HiveMembershipIndex target must be a HiveMembership or HiveGenesis` | `validate_create_link_hive_membership_index` | M8 |
| — | `HiveMembershipIndex base must be the membership's for_agent` | `validate_create_link_hive_membership_index` | M8 |
| — | `HiveMembershipIndex base must be the hive genesis author` | `validate_create_link_hive_membership_index` | M8 |
| — | `HiveMembershipIndex link may only be deleted by its author (creator: …, attempted by: …)` | `validate_delete_link_hive_membership_index` | M8 |
| L21 | `system-role GroupGenesis display_id must be 1-256 chars` | `system_role_display_id_verdict` (via `validate_create_group_genesis`) | M9 |
| L22 | `a system-role GroupGenesis with this display_id already exists in this hive on your chain` | `validate_unique_system_role_on_chain` | M9 |
| L23 | `HiveGroup group_acl buckets must be disjoint: {duplicate} appears more than once` | `validate_hivegroup_acl` (via `first_duplicate_group`) | M13 |

`find_or_create_group_genesis` deliberately catches ONLY
`GROUP_GENESIS_UNIQUENESS_REJECT` (L13) in its find-wins fallback; L22 propagates
as a hard error. Correct: find-wins resolves by `(hive, role)` — a display_id
conflict with a DIFFERENT role's group is a real client-visible conflict, never
silently resolvable (and unreachable with globally-unique squuids). The L22
sweettest therefore drives `create_group_genesis` directly.

## Decisions taken mid-build

- **M1 lineage self-reference guard placement:** the `dna_info()` check
  (L16) runs in `run_content_validators`, not the pure `validate_header_bounds`,
  keeping bounds host-testable. L14/L15 (pure b64 parse) stay in the bounds
  path. All three literals still fire on every create.
- **M6 B10 in-process limitation:** the "dead root still resolves through a
  byte-identical live sibling" path is a multi-node eventual-consistency
  artifact. In one in-process conductor, deleting any create of a shared
  entry integrates immediately and marks the whole entry Dead, so the dead
  root drops rather than resolving with `tombstoned:Some(true)`. The
  `root_tombstoned` probe returns `Some(true)` whenever `get_details` shows
  deletes; the sweettest proves the deterministic contract (live →
  `Some(false)`, flag-off → `None`, ordinarily-deleted → absent). The
  `Some(true)` re-delivery discrimination is the production value humm-tauri
  measured live, not reproducible single-node.
- **M6 reindex path reuse:** `discovery_path_hash` (renamed from
  `acl_path_hash`) builds the shared `[hive, content_type, key]` path for both
  the ACL fan-out and the Dynamic-label reindex; `acl_fanout` centralizes the
  Owner/Admin/Writer/Reader dominance so create and reindex never drift.
- **M8 discovery split (durable index vs inbox):** hive discovery (4 hive
  readers) + `list_my_groups` granted-half rerouted to the durable, author-bound,
  author-only-deletable indexes (`HiveMembershipIndex`, `AgentToGroupMemberships`);
  Inbox `HiveInvite`/`GroupInvite` writes stay as transient notifications.
  Founded-GROUP discovery stays self-Inbox — accepted Wave-2 residual: the shipped
  humm-tauri sweep consumes only `DmCreate`, and a founder re-derives founded
  groups from their own source chain.
- **M8 author-equality literal reuse:** the index create validator's
  link-author-must-equal-target-author gate reuses `require_link_author_is`'s
  existing generic literal, so it is deliberately absent from the new-literal
  table (no new wire literal minted).
- **Review status (M7):** the independent `reviewer` subagent could not run —
  five dispatch attempts hit an account HTTP-429 rate limit with a fixed ~2h
  reset window (retry-after held ~7200s across ~15 min of attempts). Per owner
  direction, a full ADVERSARIAL end-to-end review was completed inline across all
  five lanes (rust, security, silent-failure, standards, DRY): every integration
  point re-read critically + mechanical scans. Confirmed: clippy `-D warnings`
  clean; exhaustive matching (wildcards only on external `Action`/`Details`/
  `ZomeCallResponse`); lineage link author-binding + base-recompute close
  forged-index poisoning; probe raises 3 distinct hard errors with no unprobed
  downgrade; only `resolve_by_prior_generation` cap-granted; GroupGenesis
  absence proof is per-author ToGenesis; zero swallowed errors in added non-test
  code. The five-lane findings above ARE the review record (no external report).
  ONE MINOR (pre-existing): `create_encrypted_content` is 95 lines — extract a
  `create_discovery_links` helper at blessing, not in this scratch branch.
  VERDICT: APPROVE, no blockers. An independent subagent second opinion remains
  nice-to-have when the account limit resets; it is not a gate on the branch.
- **M10 idempotent delete (clean cutover):** `delete_encrypted_content` returns
  `DeleteContentResponse { was_deleted, delete_action_hash }`; an already-absent
  target is a no-op success, gated on the two wire-stable absent literals
  (`no Record found at given hash` / `Could not find the EncryptedContent`) via
  `is_absent_content_error` — any other error still propagates. Every caller
  migrated (remediation now reports "original already tombstoned" instead of a
  spurious failure detail on re-runs; three sweettest callsites assert
  `was_deleted`). No alias extern: nothing consuming pass-7 is distributed.
- **M10 paging engine reuse:** `probe_inbox_page` rides the content page engine —
  `page_links` / `resolve_page_limit` promoted `pub(crate)` and the cursor
  pairing check extracted into shared `decode_paired_cursor`, so the inbox page
  and the three content `*_page` externs emit byte-identical cursor/limit
  literals from one source. Legacy `probe_inbox` wire-unchanged.
- **M10 ACL liveness scope (pre-registered for M12 lanes):** the
  `list_by_acl_link` rider test proves the deterministic single-node contract —
  flag off → `tombstoned` absent, flag on → live root `Some(false)`, deleted →
  absent. The dead-duplicate-root `Some(true)` + live-sibling discrimination
  remains production-only observable (the M6 in-process limitation above: a
  single conductor marks the shared entry Dead, so the dead root DROPS rather
  than resolving). Fixture gotcha worth keeping: `HummContent*` ACL links exist
  ONLY for `AclSpec::HiveGroup` content (`create_acl_links` hard-errors on other
  specs), keyed by GroupGenesis action-hash strings — an OpenWrite fixture can
  never appear in `list_by_acl_link`.
- **M11 duplicate-pick coverage moved host-side (pre-registered for M12
  lanes):** same-hive duplicate system-role groups can only arise from a
  FORKED founder chain (M3's walk blocks same-chain duplicates and only the
  founder may mint system-role groups), and sweettest cannot stage a chain
  fork. The lowest-b64-STRING canonical pick is therefore proven by host unit
  tests on `canonical_role_group` (+ `dominated_roles` exhaustive-order
  tests); the conductor tests prove closure wiring, ordering, and `None` for
  missing role groups. `role_key_closure` returns identities only — no key
  material, no cross-role derivation (anti-HKDF per the role-K ruling) — and
  is cap-granted beside the other public DHT-link readers.
- **M12 security-lane dispositions (verdict APPROVE; every finding closed):**
  S-1 `was_deleted:false` conflates "already tombstoned" with "target
  unresolvable from this node" — FIXED by doc-contract on the extern (callers
  deleting cross-author content re-probe rather than treat it as terminal); a
  behavioral fix is impossible single-node (network `get` cannot disambiguate)
  and remediation only ever deletes self-authored originals, which resolve
  locally. S-2 unsolicited `create_hive_membership` grants permanently
  populate the grantee's `HiveMembershipIndex` base with rows only the
  grantor can retract — ACCEPTED residual inherent to the author-only-delete
  design (UI griefing only; zero privilege escalation); mitigation is
  client-side (hide-list keyed on genesis hash) or a future
  grantee-acknowledgement pass; humm-tauri owns the suppression UX at
  blessing handoff. S-3 M8 create validator hard-errors (validation-retry
  limbo) instead of `Invalid` on malformed link targets — DEFERRED: identical
  to the pre-existing `group/links.rs` pattern; normalize codebase-wide at a
  future sanctioned pass (this branch's hash is frozen post-M9). Also
  deferred to blessing: the integrity-side `membership.for_agent.clone()`
  allocation nit in `hive/links.rs` (rust lane) — dropping it would move the
  frozen DNA hash.
- **M13 batch (Wave-3; hash intentionally moved off the M9 freeze to
  `uhC0kbz8Dh…`):** four edits in one integrity milestone. (a) `group_acl`
  bucket-disjointness — the H2 zome-only ACL gap — via pure `first_duplicate_group`
  + a pre-fetch Step 1.5 reject in `validate_hivegroup_acl` (L23); a group listed in
  two buckets is redundant under the witness dominance chain, not ambiguous, but is
  now rejected before any authority walk. The existing `recipient_witnesses`
  accept-fixture reused one group in `owner`+`reader`; retargeted to `owner`-only
  (Reader witness still validates via owner→Reader dominance). (b) Err→Invalid
  normalization of the LOCAL link-validator rejects — `target_action_hash` →
  `require_action_target` returning `Result<ActionHash, ValidateCallbackResult>`
  (5 callsites: 4 in `group/links.rs`, 1 in `hive/links.rs`), plus the 6 local
  `to_app_option` type-mismatch rejects in `group/links.rs` (4) and
  `encrypted_content/links/updates.rs` (2). Every message string is byte-identical;
  only the reject class moved (Guest→Invalid), so the superset check is unaffected.
  This RESOLVES the M12 S-3 deferral for the local cases and the rust-lane
  `membership.for_agent.clone()` nit (dropped in `hive/links.rs`). (c) L21 now
  interpolates `GROUP_DISPLAY_ID_MAX_CHARS` — renders byte-identical to the shipped
  literal (matches the `HEADER_ID_MAX_CHARS` precedent). (d) the clone drop above.
  **NAMED RESIDUAL (still `Err`, next sanctioned pass):** the two entry authority
  fetchers `group/authority.rs` + `hive/authority.rs` — a different pre-pass-2
  class whose shared helper hands a decoded entry tuple to many callers; deferred
  to co-design with the per-entry-type ACL validators (H2).
- **M15 review-wave completion (hash moved again to `uhC0kemu…`, the FINAL scratch
  hash):** all four lanes (rust/security/silent/standards+DRY) returned APPROVE;
  three independently flagged that the local Err→Invalid normalization was still
  incomplete. M15 finishes it: (a) the two `updates.rs` base/target
  `into_action_hash` rejects → `Invalid`; (b) `common.rs`
  `fetch_target_encrypted_content` → nested verdict
  `ExternResult<Result<T, ValidateCallbackResult>>` (BOTH the non-action-target and
  the type-mismatch branch), 5 callers (acl/content_id/dynamic/hive/lineage)
  match-return the `Invalid`; (c) `original_pointer.rs`
  `require_encrypted_content_record` + `encrypted_content_root_hash` → nested verdict
  (incl. the chain-action `_` reject), callers updated. Every message string stays
  byte-identical (superset holds). The security lane traced the vendored
  holochain-0.6.1 sources to confirm these Err→Invalid moves cannot turn a
  should-retry data-availability case into a wrongful permanent reject
  (`GetRecordDetailsQuery` returns only StoreRecord-backed rows; `(Public, NotStored)`
  is malformed at sys-validation; real not-found short-circuits host-side as
  `UnresolvedDependencies`; the AgentPubKey corner is a permanent mismatch where
  `Invalid` is correct). Two pre-fetch host tests pin the `Invalid` class
  (`create_link_hive_rejects_non_action_target`,
  `create_link_encrypted_content_updates_rejects_non_action_base`). Plus three
  hash-safe doc fixes: dropped the "ambiguous" wording on `first_duplicate_group`
  (dominance resolves a duplicate deterministically — redundant, not ambiguous),
  folded the disjointness bullet into ladder step 1 (fixes a rustdoc `1.5.`
  list-marker quirk), restored the per-variant `public_key_acl.reader` doc on
  `emit_create_signals`.

## DEFERRED — H2 sketch (per-entry-type ACL validators; blessing-time co-design)

H2's targets (invite `max_uses`/HMAC/expiry, DM `pair_hash`, pair-SS reader
binding) live inside encrypted `EncryptedContent.bytes` the guest never
decodes. Zome-side validation therefore requires EITHER cleartext header
fields — which leaks DM-edge confirmation to the DHT (an attacker probing a
`pair_hash` learns "these two agents have a DM edge") and changes the
humm-tauri wire shape — OR new typed entries humm-tauri must populate. Both
need their co-design; neither is buildable zome-only. Also structurally
unavailable: `max_uses` cannot be made authoritative by counting redemption
links inside a pure entry validator (links are not part of the entry's
validation package). The one genuine zome-only ACL gap found while scoping —
`group_acl` bucket-disjointness enforcement — LANDED at M13 on this fork (L23,
`first_duplicate_group` in `validate_hivegroup_acl`), together with the deferred
hygiene items (Err-vs-Invalid normalization of the local link-validator rejects,
L21 const interpolation, the `membership.for_agent` clone drop). The DM-existence validator that ships today
(`validate_directmessage_acl`) plus Public/OpenWrite dispatch remain the
enforced surface.
