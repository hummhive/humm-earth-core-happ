# Mutating `acl_spec` on a Committed Entry (Re-Key in Place vs Re-Author)

**Date:** 2026-06-05
**Audience:** humm-tauri client developers (Media Library "change sharing scope")
**DNA version:** pass-4 (`uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`)

---

## TL;DR — verdict: RE-AUTHOR, do not update in place

**`update_encrypted_content` is structurally unsuitable for acl_spec
variant changes.** The integrity validator will accept the update (it
re-runs the full `run_content_validators` on the new content), but the
coordinator does NOT re-create the hive/dynamic/ACL/ContentId link
bundle on update — it only creates `EncryptedContentUpdates` +
`OriginalHashPointer` links. Changing `acl_spec` via update leaves the
entry indexed under the OLD paths, invisible under the NEW paths, and
the old encrypted bytes persist on-DHT in the original action's entry.

**The supported path is: create a new entry under the new ACL, then
delete (or leave) the old one.**

---

## Detailed analysis

### What `update_encrypted_content` does (coordinator, `crud.rs:140-202`)

1. `update_entry(previous_hash, new_content)` — commits a Holochain
   `Update` action pointing at the previous action, with new entry bytes.
2. Creates `EncryptedContentUpdates` link (original → updated).
3. Creates `OriginalHashPointer` link (updated → original).
4. Emits local signal + remote signal fan-out to `public_key_acl.reader`
   of the **new** content.
5. **That's it.** No hive-shape Hive link, no Dynamic links, no ACL
   links, no ContentId link. Those are only created in
   `create_encrypted_content`.

### What the integrity validator does on update (`encrypted_content.rs:807-829`)

1. **M-1 author binding:** `action.author == original_action.author` —
   only the original author can update.
2. **Full re-validation:** calls `run_content_validators(author,
   timestamp, new_content)` — the new `acl_spec` is validated from
   scratch against the new entry. This means:
   - `HiveGroup → Public`: validator accepts (author just needs hive
     Writer+, which they already have from step 2 of HiveGroup).
   - `Public → HiveGroup`: validator accepts if the author has group
     Writer+ and stamps correct witnesses.
   - `HiveGroup → different group`: validator accepts if author has
     Writer+ in the new group and stamps correct witnesses.

**So the validator says "Valid" — but the entry is orphaned from all
query paths.**

### Why update-in-place breaks queries

| Link type | Created on `create` | Created on `update` | Result of acl_spec change |
|---|---|---|---|
| Hive (author-shape) | ✅ `[author, content_type]` | ❌ | OK if content_type unchanged — original link still valid |
| Hive (hive-shape) | ✅ `[hive_genesis_hash, content_type]` | ❌ | **STALE** if hive changed; invisible under new hive's path |
| Dynamic | ✅ `[hive, type, label]` | ❌ | **STALE** — old group's dynamic path; invisible under new group |
| HummContent* ACL | ✅ `[hive, type, group]` | ❌ | **STALE** — old group's ACL links; invisible under new group |
| HummContentId | ✅ `[hive, content_id]` | ❌ | **STALE** if hive changed |
| EncryptedContentUpdates | — | ✅ | Forward link works — `get_encrypted_content` resolves to latest |
| OriginalHashPointer | ✅ (self) | ✅ | Back-pointer works |

**`get_encrypted_content(original_hash)`** would return the latest
content (with new acl_spec) via the update chain. But all `list_by_*`
queries would either return the entry under stale paths or miss it
entirely under the new paths.

### On-DHT persistence of old bytes

Holochain's `Update` action does NOT delete the original entry data from
the DHT. The original `EncryptedContent` (with the old acl_spec and old
encrypted bytes) remains accessible via `get(original_entry_hash)` to
any peer. **Changing from HiveGroup to Public via update does NOT
retroactively make the old encrypted bytes world-readable** (they're
still SaltPack-encrypted under the old key), but the old entry
metadata (acl_spec, PKA) remains visible, which may leak the original
sharing scope.

---

## Recommended pattern: re-author + delete

### Personal → Public (`HiveGroup → Public`)

```
1. Client decrypts the original bytes locally (it holds the key).
2. Client re-encrypts (or leaves plaintext for Public ACL).
3. Client calls create_encrypted_content({
     content_type: "blobMetaData",   // same
     acl_spec: AclSpec::Public { hive_genesis_hash, author_membership_hash },
     bytes: new_bytes,               // plaintext or re-encrypted
     public_key_acl: { owner: "", admin: [], writer: [], reader: [] },
     dynamic_links: [...],           // new group/scope labels
     // ... other fields
   })
4. Client calls delete_encrypted_content(original_action_hash).
   (Or: leave the old entry if you want to preserve history.)
5. The new entry gets fresh links on all correct paths.
```

### Public → Personal (`Public → HiveGroup`)

```
1. Client has the plaintext bytes (Public = no SaltPack).
2. Client SaltPack-encrypts under the target group's SharedSecrets.
3. Client calls create_encrypted_content({
     acl_spec: AclSpec::HiveGroup {
       hive_genesis_hash,
       author_membership_hash,
       group_acl,
       author_group_membership_hash,
       recipient_witnesses,       // ← MUST stamp via list_group_members
     },
     bytes: encrypted_bytes,
     public_key_acl: derived_pka,  // from group roster
     ...
   })
4. Client calls delete_encrypted_content(original_action_hash).
```

### HiveGroup → different group

Same as above but with the new group's `group_acl`,
`author_group_membership_hash`, and `recipient_witnesses`. The old
entry's witnesses are irrelevant — new witnesses are stamped from the
new group's roster.

---

## Does G-6.2 witness re-stamping happen "automatically" on update?

**No.** The coordinator's `update_encrypted_content` takes a raw
`UpdateEncryptedContentInput { previous_hash, updated_encrypted_content }`
— the caller provides the full `EncryptedContent` struct including
`header.acl_spec`. There is no `groupContext` parameter on the update
path, and no coordinator-side witness-stamping logic. The caller must
build the complete `AclSpec::HiveGroup` (including `recipient_witnesses`)
before calling update.

But as established above, update is the wrong tool for acl_spec changes
anyway. On the create path, the caller also builds the full AclSpec
client-side (the DNA is acl_spec-pass-through) — so the witness-stamping
contract is the same: caller stamps witnesses from `list_group_members`,
passes the complete AclSpec, DNA validates it.

---

## Existing doc references

| Doc | Relevant section |
|---|---|
| `_archive/PASS_4_DEPLOY_HANDOFF.md` | "REQUIRED humm-tauri callsite update" — `stampWitnessesFromGroupAcl` recipe |
| `HUMM_TAURI_ACLSPEC_INTEGRATION.md` | § 2 classification table (which content_type → which AclSpec variant) |
| `HUMM_TAURI_FEATURE_ENABLEMENT.md` | Per-feature wiring including E.4.e (Media Library selective sharing) |
| `HUMM_TAURI_SHARED_SECRETS_PUBLIC_ACL_WIRE_SHAPE.md` | Public-ACL bytes encoding (plaintext vs SaltPack) |
| This doc | The update-vs-re-author verdict and the link-orphan analysis |

---

## BDD test scenarios

### MUT-1: Re-author from HiveGroup to Public creates correct links

```
Given  alice authored "blobMetaData" HiveGroup under hive H, group G
When   alice creates a new "blobMetaData" Public entry with the same content
And    alice deletes the original HiveGroup entry
Then   list_by_hive_link({ hive: H, content_type: "blobMetaData" }) returns the Public entry
And    get_encrypted_content(new_action_hash).acl_spec is AclSpec::Public
And    get_encrypted_content(original_action_hash) fails (deleted)
```

### MUT-2: Re-author from Public to HiveGroup with witnesses

```
Given  alice authored "blobMetaData" Public under hive H
When   alice creates a new "blobMetaData" HiveGroup entry with group G,
       recipient_witnesses stamped from list_group_members(G)
Then   list_by_acl_link returns the new entry under group G
And    the new entry's acl_spec.recipient_witnesses is non-empty
```

### MUT-3: Update-in-place with acl_spec change orphans from query paths

```
Given  alice authored "blobMetaData" HiveGroup under hive H, group G1
When   alice calls update_encrypted_content with acl_spec: HiveGroup { group G2 }
Then   get_encrypted_content(original_hash) returns the updated entry (via update chain)
But    list_by_dynamic_link({ hive: H, type: "blobMetaData", label: "G2" }) returns []
       (no Dynamic links created for G2 — entry is orphaned from the new path)
And    list_by_dynamic_link({ hive: H, type: "blobMetaData", label: "G1" }) still returns the entry
       (stale link from original create still exists)
```

### MUT-4: Delete of original entry after re-author succeeds

```
Given  alice authored "blobMetaData" and then re-authored under new acl_spec
When   alice calls delete_encrypted_content(original_action_hash)
Then   delete succeeds (alice is the original author — I-A permits)
And    the original entry's links remain on-DHT (Holochain does not
       cascade-delete links on entry delete) but resolve to a deleted entry
```

---

## Observability / logging

- [ ] **Log re-author flow** — on scope change, log both the original
      action hash (being deleted/abandoned) and the new action hash.
      This creates the audit trail for "why did the sharing scope change."
- [ ] **Log stale link resolution** — if any `list_by_*` returns an entry
      whose `get` resolves to a deleted record, log it as a stale-link
      hit (expected after re-author + delete; the link persists but the
      entry is gone).
- [ ] **Do NOT log update-based acl_spec changes** — they should not
      happen. If detected (entry's acl_spec differs from what the
      original create's links imply), log as a warning: "acl_spec changed
      via update — links may be stale."
