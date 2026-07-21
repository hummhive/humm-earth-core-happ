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
