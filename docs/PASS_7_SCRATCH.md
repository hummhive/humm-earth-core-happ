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
| M13 (group_acl bucket disjointness + deterministic link-validator rejects) | 5476cb6 | uhC0kbz8DhCkYWaLeihsumn8V726s3ZzWcTsAqLNcFBWIEVaWVPnB | 7a4cd2e03328ed4c23e2329dff5ccf23b42e1a16f0a5ce137f13b7f11434e2ad |
| M14 (create-link publication + membership twin extraction) | 6ae396a | uhC0kbz8DhCkYWaLeihsumn8V726s3ZzWcTsAqLNcFBWIEVaWVPnB (UNCHANGED; coordinator-only refactor) | 7a4cd2e03328ed4c23e2329dff5ccf23b42e1a16f0a5ce137f13b7f11434e2ad |
| M15 (complete link-validator normalization + review doc fixes) | ee47e74 | uhC0kemuLaIzdw19cQwOLB1JB7o7-5ZFXNIwcOYlBpNFfL_8Uicl6 | 39062286742a11836822ab8cf5fcfde2ed6e92321b90106a4ed5de38eb8e92f0 |
| M16 (integrity DRY: shared typed fetch + bucket iterator + expiry containment) | d4459d2 | uhC0k-HAqM4zW2rCWrKSujEKDZcqybE_ATUjKxkRy2BmRjURYddxP | ec11ba8f9518cee6aee5d9e1df4fc1f7449f42584213abb4f8636cdceb90fcdd |
| M17 (coordinator resolve-path perf: record reuse + immutable-entity caches) | 41c34fd | uhC0k-HAqM4zW2rCWrKSujEKDZcqybE_ATUjKxkRy2BmRjURYddxP (UNCHANGED; coordinator-only) | ec11ba8f9518cee6aee5d9e1df4fc1f7449f42584213abb4f8636cdceb90fcdd |
| M18 (coordinator DRY + allocation discipline: shared emit/resolve + borrowed link helpers + O(limit) paging) | 9b7cae6 | uhC0k-HAqM4zW2rCWrKSujEKDZcqybE_ATUjKxkRy2BmRjURYddxP (UNCHANGED; coordinator-only) | ec11ba8f9518cee6aee5d9e1df4fc1f7449f42584213abb4f8636cdceb90fcdd |
| M19 (coordinator content batch read externs: dynamic-links/hive-links/content-id/author + exists, bounded) | abc37e0 | uhC0k-HAqM4zW2rCWrKSujEKDZcqybE_ATUjKxkRy2BmRjURYddxP (UNCHANGED; coordinator-only) | ec11ba8f9518cee6aee5d9e1df4fc1f7449f42584213abb4f8636cdceb90fcdd |
| M20 (coordinator membership/group/local batch read externs: memberships-local/group-members/my-groups-local/hive-link-local-page) | 38ac782 | uhC0k-HAqM4zW2rCWrKSujEKDZcqybE_ATUjKxkRy2BmRjURYddxP (UNCHANGED; coordinator-only) | ec11ba8f9518cee6aee5d9e1df4fc1f7449f42584213abb4f8636cdceb90fcdd |
| M21 (coordinator fetch-hint remote signals + owner-handoff offer hint) | 02ed895 | uhC0k-HAqM4zW2rCWrKSujEKDZcqybE_ATUjKxkRy2BmRjURYddxP (UNCHANGED; coordinator-only) | ec11ba8f9518cee6aee5d9e1df4fc1f7449f42584213abb4f8636cdceb90fcdd |

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
- **M16 (integrity DRY; hash moved to `uhC0k-HA…`):** three pure-refactor helpers,
  every rendered reject literal byte-preserved (no new literal; superset holds).
  (a) `fetch_authored_entry` now takes a caller-supplied not-found message closure and
  is `pub(crate)`, so the two `group/authority.rs` fetchers share its fetch/decode
  skeleton — each group literal (incl. its `group_genesis_hash`/`membership_hash`
  prefix) stays verbatim; the hive/owner callers inline their concrete label
  (interpolation-shape source churn, byte-identical render, L21 precedent).
  (b) `AclByGroupGenesis::groups()` centralizes the owner-first four-bucket walk
  (`first_duplicate_group` + the Step-3 per-group authority loop). (c)
  `validate_expiry_containment` in `globals.rs` hoists the identical grant-window
  containment tail out of the hive and group validators; each caller keeps its own
  guard + permanent-grantor short-circuit. Four pure host tests pin the helper. The
  Err-vs-Invalid normalization of those same fetchers stays the deferred H2 residual.
- **M17 (coordinator resolve-path perf; hash HELD):** wire-identical internal
  refactors, no new reject literal. P1: `get_latest_typed_from_eh` reuses the
  `get_details` entry for the no-update case (rebuilds the record locally, 3->2
  reads/item); `get_many_encrypted_content` memoizes resolution by input hash (rows
  never deduplicated). P2: `resolve_update_base` fetches the predecessor once via a
  shared `get_encrypted_content_chain_action` (was fetched twice). D1:
  `get_typed_entry_with_timestamp` in `lib.rs`, adopted in the hive + group membership
  walks. P4: `my_hive_ids_network` for the pair-SS None arm mirrors `list_my_hives`'
  inclusion set (genesis-resolvability gate preserved) minus the discarded display
  strings; `list_my_hives`/`list_my_groups` gain per-call genesis caches (multiplicity
  preserved). P8: owner-offer fetch skipped once resolved. P9: hoisted the
  loop-invariant `hive_b64` in `content_summary`. New sweettest
  `batch_reads::list_my_groups_returns_one_row_per_grant` guards the P5 cache.
- **M18 (coordinator DRY + allocation discipline; hash HELD):** wire-identical, no
  new reject literal. D2: one `emit_content_change(action_type, response)` replaces the
  three create/update/delete emit sites (delete-path ACL clone dropped). D4:
  `resolve_content_link_targets` (targets -> memoized `get_many` -> `apply_liveness`)
  shared by the four legacy list externs + `content_id_records_by_author`, replacing
  `resolve_targets`. P6: `page_links` collects `limit+1` (O(limit) temporaries) and
  `link_page` builds source positions + targets in a single projection pass. P3: the
  four `create_*` link helpers borrow `&EncryptedContentHeader`/`&ActionHash`/`&[String]`
  and return `()`. P7: `acl_fanout` streams borrowed `ActionHash` iterators per link
  class with no per-class `Vec<String>`, and the update reindex reuses one path-hash
  cache across delete+add. The link SET is byte-identical (buckets are disjoint under
  L23, so concat == union).
- **M19 (coordinator content batch read externs; hash HELD):** five additive
  read-only externs, each cap-granted beside its singleton twin and BOUNDED three
  ways — item count, per-item first page, and a shared aggregate resolve budget.
  `list_encrypted_content_by_dynamic_links` (<=64 labels, per-label bounded first
  page via `link_page`), `list_by_hive_links_many` (<=32 requests),
  `list_by_author_many` (<=64 lookups), `get_many_by_content_id_link` (<=64 lookups,
  first-target select mirrored, unresolvable -> `record:None` with a `warn!`, rows
  never dropped), and the scalar `content_id_exists` (link-set non-emptiness,
  resolves zero records). The three page-based externs share
  `enforce_batch_resolve_budget`: the sum of normalized per-item limits must be
  <= `BATCH_RESOLVE_BUDGET`=4096 (matching the existing `MY_CONTENT_HARD_LIMIT`
  single-call resolve ceiling), bounding per-call target RESOLUTION (`get_details`);
  the underlying `get_links` enumeration stays unbounded metadata (same profile as the
  existing page externs, pre-registered for security review). Five NEW coordinator
  reject literals: `dynamic_links batch accepts at
  most 64 labels`, `hive-link batch accepts at most 32 requests`, `content-id batch
  accepts at most 64 lookups`, `author batch accepts at most 64 lookups`, `batch
  total requested records exceed the 4096 budget`. Measure-first: red sweettests
  proved BOTH the initial unbounded-per-label B2 AND the missing aggregate budget
  before each fix. 13 batch_reads tests green.
- **M20 (coordinator membership/group/local batch read externs; hash HELD):** four
  additive externs + the deferred M18 "options-aware" resolution threading.
  `get_latest_memberships_local_many` (B5, self-scoped to `agent_info()`, one Local
  HiveMembershipIndex walk, newest-unexpired per requested hive via the shared
  `latest_membership_via` selection; <=64 hives). `list_group_members_many` (B7,
  COMPLETE rosters — ACL derivation needs every member — via `resolve_roster`; <=64
  groups AND a pre-resolve aggregate `GROUP_MEMBERS_LINK_BUDGET`=4096 that REJECTS an
  over-budget batch rather than truncating, so the caller falls back to per-group
  calls). `list_my_groups_local` (L4/L5) + `list_by_hive_link_local_page` (L6): the
  network bodies re-expressed via a shared `list_my_groups_via(strategy, options)` and
  an options-threaded `link_page`. To resolve EncryptedContent locally, `GetOptions`
  is threaded through `get_eh`/`get_latest_typed_from_eh`/`resolve_encrypted_content`/
  `resolve_many_encrypted_content`/`resolve_action_targets`/`link_page` — EVERY existing
  caller passes `GetOptions::network()` (zero behavior change);
  `resolve_content_link_targets` keeps its signature. B5 + `list_my_groups_local` are
  self-scoped (own data); B7 reads public rosters, hence its budget. THREE NEW reject
  literals: `membership batch accepts at most 64 hives`, `group-members batch accepts
  at most 64 groups`, `group-members batch roster links exceed the 4096 budget`. Also
  hardened while touching the resolution chain: `resolve_many`'s tolerant drop is now
  an explicit `match` + `debug!` (no `.ok()` swallow), and `get_latest_typed_from_eh`
  replaced its `.unwrap()`/`unreachable!()` with graceful `match`/`Ok(None)`. 19
  batch_reads (6 new, close-but-wrong nuances: superseded-grant newest-wins, expired
  filtering, duplicate request rows, cross-scope decoys, singleton parity) + a 24-test
  regression on the refactored functions.
- **M21 (coordinator fetch-hint remote signals + owner-handoff offer hint; hash HELD):**
  the S1 leakage fix. The cross-host content channel no longer carries ciphertext:
  `emit_content_change` keeps the FULL `EncryptedContentSignal` for the author's LOCAL
  `emit_signal`, but `remote_signal_acl_readers` now fans out a new `EncryptedContentHint`
  (`{action_type, hash, original_hash, from_agent}`) — identifiers only; the reader
  re-queries + `get`-verifies. `recv_remote_signal` gains a hint arm that stamps
  `from_agent = call_info().provenance` (overwriting any sender-supplied value) and
  re-emits locally; the legacy full-signal arm stays for decode robustness. L13:
  `initiate_owner_handoff` best-effort sends an `OwnerHandoffOfferHint`
  (`{offer_hash, hive_genesis_hash, from_agent}`) to the recipient (warn-never-block; not
  a new cap-granted extern). The two hints are structurally DISJOINT from every other
  dispatcher family (hint lacks `data`; full signal lacks `hash`/`original_hash`; offer
  hint lacks `action_type`) — pinned by four host round-trip/cross-decode tests.
  Sanctioned clean cutover (no additive rider): pass-7's network is disjoint from shipped
  pass-6, so no live receiver decodes the old remote shape; humm-tauri adopts hint ingest
  at blessing (§I). The recv fall-through error now names all five families — ONE
  intentional coordinator literal change (old 3-family text lost, new 5-family text added;
  documented for the superset check). Sweettest `signal_hints`: reader gets a hint whose
  wire does NOT decode as the full payload (`deny_unknown_fields` airtight), local author
  keeps the full ciphertext, a forged `from_agent` is overwritten by real provenance, the
  handoff hint delivers stamped. 56 host + signal_hints 3 + pinned_hosts 9.
- **Wave-4 privacy/security acceptances (S5, S6):** blessing-time surface record.
  **S6 (accepted for pass-7):** the pass-7 integrity fork WIDENED public relationship
  metadata — `HiveMembershipIndex` (agent pubkey -> membership/genesis targets =
  affiliation enumeration) and `Lineage` (plaintext prior-action tag + header lineage =
  cross-generation correlation). Both are inherent to the M8/M4 index designs; a narrower
  shape is a FUTURE integrity redesign, NOT a coordinator drive-by. Accepted as the price
  of retraction-safe durable discovery + portable identity. **S5 (doc note):** path bases
  are HASHED but discovery still leaks via public headers + plaintext link TAGS (Dynamic
  label, ACL group-hash) + guessable low-entropy bases; new sensitive dynamic labels
  should be OPAQUE client-side ids — a client convention, no zome change.
- **M22 review lanes + superset (VERDICT: APPROVE, no blockers):** four independent
  read-only lanes over `git diff 0232c56..HEAD`. **Security: CLEAN** — all nine new externs
  correctly cap-classed; self-scoped ones `agent_info()`-derived; every batch path bounded
  fail-closed; the M21 remote channel type-enforced ciphertext-free; `from_agent`
  unconditionally provenance-stamped (forged value cannot survive). **Silent-failure:
  CLEAN** — new resolution-failure drops log explicitly; the pre-existing polymorphic
  `.ok().flatten()` decode idiom + documented probes are accepted, not masked failures.
  **Rust + Standards/DRY: no blocker/major**, NIT/MINOR only, all FIXED here:
  `my_hive_ids_network` reuses `membership_index_links`; `list_my_hives` display cache →
  `cached_hive_display`; `create_acl_link_at` shared by create + reindex (validator-pinned
  tag bytes byte-identical); reindex delete path drops the discarded entity_id String alloc
  (`cached_acl_path_hash` returns EntryHash only); `resolve_many` drop-log neutrally named;
  the `EncryptedContentSignal`/migration-writer/budget docs corrected to the hint-only
  remote channel + `BATCH_RESOLVE_BUDGET`; two non-falsifiable asserts removed from
  signal_hints. ACCEPTED/deferred: (a) M17 P1 record-reuse returns a duplicate-create entry
  still Live via a sibling even when its FIRST create was deleted (old code returned None) —
  more correct (entry IS Live), matches the B10 liveness posture, accepted as a refinement
  over strict byte-identity; (b) `my_hive_ids_network` mirrors `list_my_hives`' inclusion
  policy by copy (documented tether; a shared `classify_membership_index_target` is a future
  extraction); (c) batch wire-struct names mix `Bucket`/`Result` — recorded as-shipped,
  future batch externs pick one; (d) at the default per-item limit (100) the 4096 budget
  binds before the 64-item caps (dynamic-links, author: 64x100=6400, so ~40 items fit)
  but NOT the 32-request hive-links cap (32x100=3200 < 4096, reachable) — documented
  interplay, all bounds coherent.
  **Reject-literal superset vs `0232c56`: CLEAN** — zero validator/reject/link-tag literal
  lost; the only LOST source strings are two reworded COMMENTS (`I founded this group`,
  `wrong entry type`), the M16 interpolation-shape churn (renders byte-identical), the
  removed `unreachable!` panic message, and the M21 fall-through (intentionally replaced by
  the 5-family version). ADDED = the eight new batch reject literals + M16 inlined fetcher
  literals + M21 signal/log strings + new extern/cap-token + test-assertion strings.

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
