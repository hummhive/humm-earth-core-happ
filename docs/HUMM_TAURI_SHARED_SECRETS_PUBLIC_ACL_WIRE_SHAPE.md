# SharedSecrets (pair-ss) Public-ACL: Wire Shape on Read

**Date:** 2026-06-05
**Audience:** humm-tauri client developers
**DNA version:** pass-4 (`uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`)

---

## TL;DR

For `content_type: "pair-ss"` entries authored with a public-read ACL:

1. **`acl_spec` is `AclSpec::Public`** — `{ hive_genesis_hash, author_membership_hash? }`.
   Neither `list_by_author`, `get_encrypted_content`, `fetch_pair_ss_with_hive_check`,
   nor migration changes this. The DNA returns the entry verbatim — it does not
   rewrite, re-encrypt, or re-wrap `acl_spec` or `bytes` on read.

2. **`bytes` is whatever the authoring client wrote** — the DNA treats
   `EncryptedContent.bytes` as opaque `SerializedBytes`. The DNA never
   inspects, decodes, encrypts, or decrypts this field. If humm-tauri
   authored the entry with a plaintext JSON `SharedSecrets` wrapper
   (X25519 pair-encrypted keys in `secrets[]`), that is exactly what
   comes back on read. If it authored a SaltPack envelope, that is what
   comes back.

3. **The Public-ACL decrypt short-circuit should still fire for SS
   entries on read.** The `acl_spec` variant is `Public`, not
   `HiveGroup` or `DirectMessage`, so any client-side `match acl_spec`
   that branches on `Public` → skip SaltPack decryption will hit
   correctly.

4. **Migration does NOT change `acl_spec` or byte encoding.** The
   `mark_migrated` / `mark_migrated_v2` mechanism only rewrites the
   `content_type` (prepends `_migrated/pair-ss`) and replaces `bytes`
   with a migration-marker payload. It preserves `acl_spec`,
   `public_key_acl`, `id`, `display_hive_id`, and
   `revision_author_signing_public_key` via struct-update spread
   (`..original.header.clone()`). Crucially: a **migrated** entry is no
   longer a SharedSecrets entry — it is a forward-pointer marker. The
   original (unmigrated) entry on the **new** DNA is a fresh
   `create_encrypted_content` call that the importing client authored
   with whatever `acl_spec` + `bytes` it chose at import time.

---

## Wire shape detail

### What the DNA stores (integrity zome)

```rust
pub struct EncryptedContent {
    pub header: EncryptedContentHeader,
    pub bytes: SerializedBytes,           // ← opaque, DNA never decodes
}

pub struct EncryptedContentHeader {
    pub id: String,                       // app-level squuid
    pub display_hive_id: String,          // human alias (NOT security-load-bearing)
    pub content_type: String,             // "pair-ss" for SharedSecrets
    pub acl_spec: AclSpec,                // ← the variant that matters
    pub public_key_acl: Acl,              // routing/signal fan-out hint
    pub revision_author_signing_public_key: String,
}
```

### What the coordinator returns

Every read extern (`get_encrypted_content`, `get_many_encrypted_content`,
`list_by_author`, `list_by_hive_link`, `list_by_dynamic_link`,
`list_by_acl_link`, `get_by_content_id_link`,
`fetch_pair_ss_with_hive_check`) returns the same shape:

```rust
pub struct EncryptedContentResponse {
    pub encrypted_content: EncryptedContent,   // header + bytes, verbatim
    pub hash: String,                          // latest action hash
    pub original_hash: String,                 // original create action hash
}
```

No transformation. No re-encryption. No field rewriting. The entry is
returned exactly as it was committed.

### `AclSpec::Public` variant (the one on public-read SS entries)

```rust
AclSpec::Public {
    hive_genesis_hash: ActionHash,             // cryptographic hive identity
    author_membership_hash: Option<ActionHash>, // author's HiveMembership (None = genesis author)
}
```

The integrity validator enforces:
- `action.author` holds Writer+ in `hive_genesis_hash` (via
  `check_hive_authority`).
- `check_author_matches_header` (pass-1 guard).
- No group/recipient constraints — this is world-readable by design.

### `bytes` content for public-read SS entries

The DNA is agnostic. The `bytes` field is `SerializedBytes` (msgpack
envelope wrapping whatever the client put in). For public-read
SharedSecrets, humm-tauri currently authors:

```
bytes = msgpack(SharedSecretsWrapper {
    secrets: [
        { recipient_pubkey, encrypted_x25519_key },  // X25519-pair-encrypted per-recipient
        ...
    ],
    // ... other fields per humm-tauri's SharedSecrets schema
})
```

This is NOT SaltPack-encrypted (the whole point of Public ACL is that
the content is readable without a SaltPack decrypt step). The client's
read path should detect `AclSpec::Public` and skip SaltPack decryption,
decoding `bytes` directly as the `SharedSecrets` JSON/msgpack wrapper.

---

## Migration path — what happens to `acl_spec` and `bytes`

### Unmigrated entries (the normal case)

Returned as-is. `acl_spec` = whatever the author committed.
`bytes` = whatever the author committed.

### Migrated entries (cross-DNA forward pointer)

`mark_migrated` / `mark_migrated_v2` issues an `update_entry` that:

| Field | Old value | New value |
|---|---|---|
| `content_type` | `"pair-ss"` | `"_migrated/pair-ss"` |
| `bytes` | original SharedSecrets payload | `MigrationMarkerV1` or `V2` msgpack |
| `acl_spec` | preserved verbatim | preserved verbatim |
| `public_key_acl` | preserved verbatim | preserved verbatim |
| `id`, `display_hive_id`, `revision_author_signing_public_key` | preserved | preserved |

The migrated entry is a forward pointer, not a SharedSecrets entry.
Clients detecting `content_type.starts_with("_migrated/")` should follow
the pointer, not attempt SS decode.

### Imported entries on the new DNA

These are fresh `create_encrypted_content` calls. The importing client
decides the `acl_spec` and `bytes`. For public-read SS, it should
re-author with `AclSpec::Public { hive_genesis_hash: <new DNA's hive> }`
and the same plaintext SharedSecrets wrapper bytes (or re-encrypt if
the key material changed).

---

## BDD test scenarios for humm-tauri

### SS-PUB-1: Public SharedSecrets round-trip preserves acl_spec

```
Given  a hive with Member alice (Writer+)
And    alice authors content_type "pair-ss" with AclSpec::Public
       and bytes = plaintext SharedSecretsWrapper { secrets: [...] }
When   any agent calls get_encrypted_content(action_hash)
Then   response.encrypted_content.header.acl_spec is AclSpec::Public
And    response.encrypted_content.header.acl_spec.hive_genesis_hash == alice's hive
And    response.encrypted_content.bytes decodes as SharedSecretsWrapper (NOT SaltPack)
```

### SS-PUB-2: list_by_author returns Public SS with acl_spec intact

```
Given  alice has authored 3 entries: 1× "pair-ss" Public, 1× "post" HiveGroup, 1× "dm" DirectMessage
When   any agent calls list_by_author({ author: alice, content_type: "pair-ss" })
Then   result contains exactly 1 entry
And    that entry's acl_spec is AclSpec::Public (not HiveGroup, not DirectMessage)
And    bytes decode as the original SharedSecretsWrapper
```

### SS-PUB-3: fetch_pair_ss_with_hive_check returns Public SS with acl_spec intact

```
Given  alice authored "pair-ss" Public under hive H with dynamic_links: ["bob"]
When   bob calls fetch_pair_ss_with_hive_check({
         active_hive_genesis_hash: H,
         content_type: "pair-ss",
         author: alice,
         dynamic_link_label: "bob"
       })
Then   result contains alice's SS entry
And    acl_spec is AclSpec::Public { hive_genesis_hash: H }
And    bytes is the original plaintext SharedSecretsWrapper
```

### SS-PUB-4: Client decrypt short-circuit fires for Public acl_spec

```
Given  the client receives an EncryptedContentResponse with:
       - content_type: "pair-ss"
       - acl_spec: AclSpec::Public { ... }
When   the client's content-decode pipeline runs
Then   the pipeline DOES NOT attempt SaltPack decryption
And    the pipeline decodes bytes directly as SharedSecretsWrapper JSON/msgpack
And    secrets[].encrypted_x25519_key values are accessible
```

### SS-PUB-5: Migrated SS entry has _migrated/ prefix but preserves acl_spec

```
Given  alice authored "pair-ss" Public on old-DNA
And    alice ran mark_migrated(action_hash, marker)
When   any agent calls get_encrypted_content(action_hash) on the old DNA
Then   response.encrypted_content.header.content_type == "_migrated/pair-ss"
And    response.encrypted_content.header.acl_spec is STILL AclSpec::Public
And    response.encrypted_content.bytes decodes as MigrationMarkerV1 (NOT SharedSecretsWrapper)
```

### SS-PUB-6: Imported SS entry on new DNA is a fresh create with correct acl_spec

```
Given  the migration CLI imported alice's "pair-ss" Public entry onto new-DNA hive H'
When   any agent calls get_encrypted_content(new_action_hash) on the new DNA
Then   response.encrypted_content.header.content_type == "pair-ss" (no _migrated/ prefix)
And    response.encrypted_content.header.acl_spec is AclSpec::Public { hive_genesis_hash: H' }
And    response.encrypted_content.bytes decodes as SharedSecretsWrapper
```

---

## Observability / logging sanity checks

### On authoring (create_encrypted_content for pair-ss Public)

- [ ] **Log the acl_spec variant** at create time — confirm `"Public"`, not
      `"HiveGroup"` or `"OpenWrite"`. A SharedSecrets entry accidentally
      authored as HiveGroup would require group witnesses and fail
      validation if none are stamped.
- [ ] **Log bytes length** — public SS entries should be relatively small
      (N recipients × ~80 bytes per X25519 encrypted key). Unusually
      large bytes (>10KB) suggest accidental SaltPack wrapping or
      duplicate secrets.
- [ ] **Log content_type** — must be exactly `"pair-ss"`. Variants like
      `"pair-ss-v2"` or `"shared-secrets"` would silently miss
      `fetch_pair_ss_with_hive_check` (which filters by content_type on
      the path).

### On reading (get/list returning pair-ss entries)

- [ ] **Log acl_spec variant before decrypt branch** — the variant
      discriminator MUST be checked before attempting SaltPack decode.
      If `Public` → skip decrypt. If this log shows `HiveGroup` for a
      pair-ss entry, the authoring path has a bug.
- [ ] **Log bytes decode success/failure** — if the Public short-circuit
      fires but bytes fail to decode as SharedSecretsWrapper, either:
      (a) the entry was authored with SaltPack despite Public ACL (author bug), or
      (b) the SharedSecretsWrapper schema changed between write and read.
- [ ] **Log content_type for _migrated/ entries** — if the content_type
      starts with `_migrated/`, the bytes are a MigrationMarker, not SS
      data. Attempting SS decode on a migrated entry is a client bug.

### On migration (mark_migrated for pair-ss entries)

- [ ] **Log original acl_spec vs marker acl_spec** — they MUST be identical
      (struct-update spread preserves all header fields except content_type).
      Any mismatch is a bug in `build_marker_payload`.
- [ ] **Log the _migrated/ content_type** — must be `"_migrated/pair-ss"`,
      not `"_migrated/_migrated/pair-ss"` (idempotent prefix guard
      prevents double-marking, but log to confirm).
