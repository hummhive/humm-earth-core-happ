# Content-Type Filtering & HiveGroup Witness Rules

**Date:** 2026-06-05
**Audience:** humm-tauri client developers
**DNA version:** pass-4 (`uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`)

---

## Q1: content_type filtering — caller-supplied, NOT hardcoded

### Answer

**Every content_type-filtered read extern uses the caller-supplied string
verbatim.** There is no hardcoded `"pair-ss"` anywhere in the DNA. The
`"pair-ss"` string in earlier docs was a shorthand example from the
header comment (`content_type: "dm", "post", "pair-ss", ...`) — it is
NOT what humm-tauri actually writes.

If humm-tauri authors SharedSecrets with
`content_type: "hummhive-core-shared-secrets-v1"`, then queries MUST
pass that exact string. Passing `"pair-ss"` will land on a different
Path and return empty results.

### Code trace

All read externs build Paths from the caller-supplied `content_type`:

| Extern | Path construction | Source |
|---|---|---|
| `list_by_author` | `Path([input.author, input.content_type])` → `Hive` links | `queries.rs:269-271` |
| `list_by_hive_link` | `Path([input.hive_genesis_hash, input.content_type])` → `Hive` links | `queries.rs:112-137` |
| `list_by_dynamic_link` | `Path([input.hive_genesis_hash, input.content_type, input.dynamic_link_label])` → `Dynamic` links | `queries.rs:58-76` |
| `list_by_acl_link` | `Path([input.hive_genesis_hash, input.content_type, input.entity_id])` → `HummContent*` links | `queries.rs:222-259` |
| `count_links_by_hive` | `Path([input.hive_genesis_hash, input.content_type])` → count | `queries.rs:160-176` |
| `fetch_pair_ss_with_hive_check` | Both legs: `Path([input.author, input.content_type])` AND `Path([input.active_hive_genesis_hash, input.content_type, input.group_id])` | `queries.rs:365-433` |
| `get_by_content_id_link` | `Path([input.hive_genesis_hash, input.content_id])` — no content_type filter | `queries.rs:185-209` |
| `get_encrypted_content` | ActionHash direct lookup — no content_type filter | `crud.rs:119-131` |

The write side (`create_encrypted_content`) builds the same Paths from
`input.content_type` when creating links. The content_type must match
end-to-end: author writes links under `"hummhive-core-shared-secrets-v1"`,
reader queries with `"hummhive-core-shared-secrets-v1"`. Any mismatch
means the query lands on a different Path and returns `[]`.

### Impact on W4 cross-hive DM "awaiting accept forever"

If any humm-tauri callsite was passing `"pair-ss"` instead of
`"hummhive-core-shared-secrets-v1"` to `fetch_pair_ss_with_hive_check`
or `list_by_author`, the query would silently return `[]` — the
SharedSecrets entries exist on the DHT but are invisible because the
Path key doesn't match. This would produce the "awaiting accept forever"
symptom independently of any `enc=` bug, because the DM accept flow
depends on finding the counterparty's SharedSecrets entry.

**Action item for humm-tauri devs:** grep all `call_zome` invocations
for `fetch_pair_ss_with_hive_check`, `list_by_author`,
`list_by_dynamic_link`, and `list_by_hive_link` where the target is
SharedSecrets content, and confirm every one passes
`content_type: "hummhive-core-shared-secrets-v1"` (or whatever the
canonical string is in `SharedSecretsContentType` / equivalent constant).

### Note on the `"pair-ss"` doc error

The previous doc (`HUMM_TAURI_SHARED_SECRETS_PUBLIC_ACL_WIRE_SHAPE.md`)
used `"pair-ss"` as the content_type in BDD scenarios. Those scenarios
are structurally correct (they test wire shape, not content_type
matching), but the example string should be read as
`"hummhive-core-shared-secrets-v1"` for humm-tauri integration.

---

## Q2: HiveGroup with `recipient_witnesses: []` — expected for empty PKA, NOT a validation gap

### Answer

**This is expected behaviour, not a validation gap.** The pass-4
validator enforces a **bidirectional set-equality** between
`public_key_acl` pubkeys and `recipient_witnesses` pubkeys. If the PKA
is effectively empty (owner = `""`, admin/writer/reader = `[]`), then
zero witnesses is the only valid count — stamping any witness would fail
the reverse-direction check.

The witness requirement is: **every pubkey in `public_key_acl` must be
backed by exactly one dominating witness, AND every witness must point at
a pubkey that exists in the PKA bucket it claims.** When there are no
pubkeys in the PKA, there's nothing to witness, so `[]` is correct.

### Why the 4 entries passed validation

For a HiveGroup entry authored by the group owner with
`author_membership_hash: None`:

1. **Step 2 (hive authority):** `author_membership_hash: None` means the
   author IS the `HiveGenesis` creator → implicit Owner. Valid.
2. **Step 3 (group authority):** `author_group_membership_hash: None` →
   the author is checked via `check_group_authority` Path A (group
   creator) or Path B (hive sovereign). Owner satisfies Writer+. Valid.
3. **Step 4 (cardinality):** `witnesses.len() == 0 <= 256`. Valid.
4. **Step 5 (bidirectional cross-check):** The forward pass iterates
   every PKA pubkey and requires a dominating witness. If PKA is empty
   (or contains only empty strings, which the `!s.is_empty()` filter on
   owner skips), the forward pass has **zero iterations** → no missing
   witnesses. The reverse pass iterates every witness and checks it
   appears in the claimed PKA bucket — zero witnesses → zero
   iterations. Both pass. Valid.
5. **Step 6 (per-witness fetch):** Zero witnesses → zero iterations.
   Valid.

### When witnesses ARE required

Witnesses are required iff `public_key_acl` contains non-empty pubkey
strings. The moment humm-tauri populates `reader: ["uhCAk..."]` (to
enable signal fan-out to specific recipients), those pubkeys must each
be backed by a witness with a valid `GroupMembership`.

### SharedSecrets-specific PKA patterns

For SharedSecrets entries, the typical PKA patterns and their witness
requirements:

| PKA shape | Witnesses needed | Scenario |
|---|---|---|
| `owner: "", admin: [], writer: [], reader: []` | `[]` (zero) | SS entry with no signal fan-out target — self-only or public discovery |
| `owner: "author", admin: [], writer: [], reader: ["counterparty"]` | 2 witnesses (1 Owner for author, 1 Reader for counterparty) | Pair-SS where both parties need signal notifications |
| `owner: "", admin: [], writer: [], reader: ["*"]` | depends on literal `"*"` — treated as a pubkey string, not a wildcard by the validator | Public-read hint (but `"*"` is not a real AgentPubKey, so any witness claiming it would fail step 6's `fetch_group_membership`) |

**Edge case:** if humm-tauri writes PKA `reader: ["*"]` on a HiveGroup
entry (rather than using `AclSpec::Public`), the bidirectional check
would demand a witness for `"*"` — which cannot resolve to a real
`GroupMembership`. This combination would fail validation. HiveGroup +
`reader: ["*"]` is structurally incoherent; use `AclSpec::Public` for
world-readable content.

---

## BDD test scenarios

### CT-1: content_type is caller-supplied, not hardcoded

```
Given  alice authors content_type "hummhive-core-shared-secrets-v1"
       with AclSpec::Public under hive H
When   bob calls list_by_author({ author: alice, content_type: "pair-ss" })
Then   result is [] (empty — wrong content_type)

When   bob calls list_by_author({ author: alice, content_type: "hummhive-core-shared-secrets-v1" })
Then   result contains alice's SharedSecrets entry
```

### CT-2: fetch_pair_ss_with_hive_check uses caller content_type on both path legs

```
Given  alice authored "hummhive-core-shared-secrets-v1" HiveGroup
       under hive H with dynamic_links: ["bob"]
When   bob calls fetch_pair_ss_with_hive_check({
         author: alice,
         active_hive_genesis_hash: H,
         content_type: "pair-ss",
         group_id: "bob"
       })
Then   result is [] (author path matches nothing — content_type mismatch)

When   bob calls fetch_pair_ss_with_hive_check({
         author: alice,
         active_hive_genesis_hash: H,
         content_type: "hummhive-core-shared-secrets-v1",
         group_id: "bob"
       })
Then   result contains alice's SS entry
```

### CT-3: content_type mismatch on write vs read returns empty, not error

```
Given  alice authored content_type "hummhive-core-shared-secrets-v1"
When   any extern is called with content_type "shared-secrets"
Then   result is [] or 0 (not an error — just a different Path with no links)
```

### WIT-1: HiveGroup with empty PKA accepts zero witnesses

```
Given  alice is hive Owner (author_membership_hash: None)
And    alice is group Owner via Path A (author_group_membership_hash: None)
When   alice creates HiveGroup content with:
       - public_key_acl: { owner: "", admin: [], writer: [], reader: [] }
       - recipient_witnesses: []
Then   validation accepts (zero PKA pubkeys → zero witnesses required)
```

### WIT-2: HiveGroup with populated PKA rejects zero witnesses

```
Given  alice is hive Owner, bob is group Reader
When   alice creates HiveGroup content with:
       - public_key_acl: { owner: "alice", admin: [], writer: [], reader: ["bob"] }
       - recipient_witnesses: []
Then   validation rejects with "public_key_acl.Owner entry alice is not
       backed by any dominating recipient_witness"
```

### WIT-3: HiveGroup with populated PKA and correct witnesses passes

```
Given  alice is hive Owner with GroupMembership M_alice (Owner bucket)
And    bob has GroupMembership M_bob (Reader bucket) in the same group
When   alice creates HiveGroup content with:
       - public_key_acl: { owner: "alice", admin: [], writer: [], reader: ["bob"] }
       - recipient_witnesses: [
           { pubkey: alice, bucket: Owner, membership_hash: M_alice },
           { pubkey: bob,   bucket: Reader, membership_hash: M_bob }
         ]
Then   validation accepts
```

### WIT-4: HiveGroup with PKA reader: ["*"] rejects (no valid witness possible)

```
Given  alice is hive Owner
When   alice creates HiveGroup content with:
       - public_key_acl: { owner: "", admin: [], writer: [], reader: ["*"] }
       - recipient_witnesses: [{ pubkey: "*", bucket: Reader, membership_hash: ??? }]
Then   validation rejects at step 6 (membership_hash cannot resolve to a
       GroupMembership for pubkey "*")

When   alice creates the same entry with recipient_witnesses: []
Then   validation rejects at step 5 forward pass ("reader entry * is not
       backed by any dominating recipient_witness")
```

---

## Observability / logging sanity checks

### Content-type matching

- [ ] **Log the exact content_type string** on every `list_by_*` /
      `fetch_pair_ss_with_hive_check` call. Compare against the constant
      used at authoring time. Any mismatch is a silent-miss bug.
- [ ] **Log empty results distinctly from errors** — `[]` from a
      content_type mismatch looks identical to `[]` from DHT propagation
      delay. If the callsite expects non-empty results, log the
      content_type + author + hive_genesis_hash tuple for debugging.

### Witness validation

- [ ] **Log PKA population at authoring time** — if PKA is empty and
      content is HiveGroup, witnesses should be `[]`. If PKA has entries,
      log the count and confirm matching witness count before `call_zome`.
- [ ] **Log validation rejections** — if a `create_encrypted_content`
      call fails with a witness-related message, the rejection string
      contains the specific step and pubkey. Parse and surface it.
- [ ] **Log author_membership_hash** — `None` (hive genesis author) vs
      `Some(hash)` determines the authority path. Misuse (passing `None`
      when NOT the genesis author) will fail at step 2, not at witnesses.
