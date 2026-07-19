# pass-7 scratch ledger (branch-only; never merges to main)

## DNA hash log
| milestone | commit | dna hash | integrity wasm sha256 |
|---|---|---|---|
| M0 (pre-integrity baseline) | 991b729 | uhC0ksXsJOTlVvhUn3KWB0nN6j-II_9BxlsRiMqR9ajhFhYS7gSMz | 2656a9100937f7e6d17e2eebd5e744a1ef16e8e36b0efa089dc2f6382a655ae2 |
| M1 (header bounds + update continuity) | <fill> | uhC0kC6Rjh9-NE9vHSQ6Zy4EUtjoZvKfwzD8Txo5Hsu6Gw7irpl4C | 86c7950fe65f7e5c24d54f85fbabb7a8fdf3591632fd0d8d7f529b22ca0f8128 |

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

## Decisions taken mid-build
