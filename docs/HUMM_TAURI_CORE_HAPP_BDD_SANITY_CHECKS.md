# Core happ — BDD sanity checks for humm-tauri

**Purpose.** A Given/When/Then catalogue of the **commit-time
guarantees** the `humm_earth_core` integrity zome enforces, so
humm-tauri engineers can robustly sanity-check *anything* that writes to
the core happ — not just note-to-self. Each scenario names the extern
you call, the wire shape, the expected outcome, and (where one exists)
the **core-happ Rust unit test that already pins the validator side**.

**How to read the "Then".**
- *commit succeeds* ⇒ the extern resolves `Ok(...)`.
- *commit rejected with `"<substr>"`* ⇒ the extern rejects and the
  conductor error message contains `<substr>`. At the binding layer
  assert e.g. `expect(String(err)).toContain("<substr>")`.
- *Validator unit test:* `file::test_name` — already proves the
  validator behaves; your test only needs to prove your **wire-shape
  builder** produces the right input. Paths are relative to
  `dnas/humm_earth_core/zomes/integrity/content/src/`.

**Scope of guarantees.** All authority is enforced at **commit time** by
deterministic validators; reads are unguarded (consumers re-derive trust
from entries, never from links — links are a cache). Every entry in the
trust chain is **immutable** (updates/deletes rejected) except
`EncryptedContent` (updatable by original author; deletable per the I-A
rule). Error strings below are copied verbatim from the validators and
are wire-stable; treat them as a contract for failure-path assertions.

**DNA under test:**
`uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV` (pass-4).

---

## Trust chain at a glance

```
HiveGenesis            (permissionless root of trust; action hash = hive id)
   └─ HiveMembership   (Owner→Admin→Writer→Reader grants; chain-walked)
        └─ GroupGenesis      (custom group needs hive Admin+; action hash = group id)
             └─ GroupMembership   (group grants; Path A author / Path B hive / Path C membership)
                  └─ EncryptedContent  (AclSpec: HiveGroup | DirectMessage | Public | OpenWrite)
```

Authority helpers: `check_hive_authority` (Path 1 genesis author / Path 2
membership), `check_group_authority` (Path A group author / Path B hive
Admin+ / Path C group membership). Role order:
`Owner > Admin > Writer > Reader`.

---

## A. HiveGenesis — the root

### A-1 Founding a hive is permissionless (happy)
- **Given** any agent
- **When** `create_hive_genesis({ display_id })` (`hive/crud.rs:43`)
- **Then** commit succeeds; the returned action hash is the hive's
  cryptographic identity
- **Why** `validate_create_hive_genesis` always returns `Valid`
  (`hive.rs:254-259`)

### A-2 HiveGenesis is immutable (expected failure)
- **Given** an existing `HiveGenesis`
- **When** an update is attempted
- **Then** commit rejected with `"immutable"`
- **Validator unit test** `hive.rs::hive_genesis_update_is_invalid`
  (`hive.rs:635`)

### A-3 HiveGenesis is non-deletable (expected failure)
- **Given** an existing `HiveGenesis`
- **When** a delete is attempted
- **Then** commit rejected with `"cannot be deleted"`
- **Validator unit test** `hive.rs::hive_genesis_delete_is_invalid`
  (`hive.rs:648`)

---

## B. HiveMembership — hive role grants

`create_hive_membership` (`hive/crud.rs:94`). Grantor = caller.

### B-1 Genesis author grants a member (happy)
- **Given** Alice authored hive `H` (implicit Owner, Path 1)
- **When** `create_hive_membership({ hive_genesis_hash: H, for_agent:
  Bob, role: "Writer", grantor_membership_hash: null, expiry: null })`
- **Then** commit succeeds (`grantor_membership_hash: null` ⇒ Alice
  proves authority as genesis author, `hive.rs:205-207`)

### B-2 Admin grants a lower role via their membership (happy)
- **Given** Bob holds an `Admin` `HiveMembership` `M_bob` in `H`
- **When** Bob calls `create_hive_membership({ …, for_agent: Carol,
  role: "Writer", grantor_membership_hash: M_bob })`
- **Then** commit succeeds (Path 2, Admin ≥ Admin required)

### B-3 Self-grant is rejected (expected failure)
- **Given** any agent
- **When** `create_hive_membership({ …, for_agent: <caller>, … })`
- **Then** commit rejected with `"self-grant"`
- **Why** Rule 1, `hive.rs:317-321`
- **Validator unit tests**
  `hive.rs::hive_membership_self_grant_is_invalid` (`hive.rs:709`),
  `hive.rs::hive_membership_self_grant_invalid_regardless_of_role`
  (`hive.rs:726`, all four roles)

### B-4 Granting without authority is rejected (expected failure)
- **Given** Mallory is neither the genesis author nor an Admin+ member
- **When** Mallory tries to grant anyone a role
- **Then** commit rejected with `"is not the genesis author"` /
  `"supplied no authorising HiveMembership"` (`hive.rs:213-216`) — or
  the role/expiry/hive-mismatch messages at `hive.rs:219-243` if a bogus
  witness is supplied

### B-5 Non-Owner cannot grant Owner (expected failure)
- **Given** Bob holds only `Admin` in `H`
- **When** Bob calls `create_hive_membership({ …, role: "Owner", … })`
- **Then** commit rejected with `"only an Owner may grant the Owner
  role"` (`hive.rs:347-350`)

### B-6 Expiring grantor cannot mint a longer window (expected failure)
- **Given** Bob's authorising membership expires at `T`
- **When** Bob grants a membership with `expiry > T` (or `null`)
- **Then** commit rejected by grant-window containment
  (`enforce_hive_grant_window`, Rule 4) — Path-1 grantors are
  unconstrained (`hive.rs::hive_grant_window_unconstrained_without_grantor_membership`,
  `hive.rs:760`)

### B-7 HiveMembership is immutable / non-deletable (expected failure)
- **When** an update or delete is attempted
- **Then** rejected with `"immutable"` / `"cannot be deleted"`
  respectively; revoke by issuing a fresh membership with past `expiry`
- **Validator unit tests** `hive.rs::hive_membership_update_is_invalid`
  (`hive.rs:668`), `hive.rs::hive_membership_delete_is_invalid`
  (`hive.rs:684`)

---

## C. GroupGenesis — group identity

`create_group_genesis` (`group/crud.rs:69`).

### C-1 Hive Admin+ creates a custom group (happy)
- **Given** Alice is Owner (genesis author) of hive `H`
- **When** `create_group_genesis({ hive_genesis_hash: H, display_id,
  hive_wide_role: null, creator_hive_membership_hash: null })`
- **Then** commit succeeds; the action hash is the group identity
  (`validate_create_group_genesis` requires Admin for a custom group,
  `group.rs:253-269`)

### C-2 System-role group requires hive Owner (expected failure)
- **Given** Bob holds only `Admin` in `H`
- **When** `create_group_genesis({ …, hive_wide_role: "Admin", … })`
  (a system role group)
- **Then** commit rejected — system role groups require hive **Owner**
  (`group.rs:257-261`)

### C-3 Creating a group in a hive you lack Admin+ in is rejected (expected failure)
- **Given** Mallory has no Admin+ membership in `H`
- **When** Mallory calls `create_group_genesis({ hive_genesis_hash: H, … })`
- **Then** commit rejected by the underlying `check_hive_authority`
  (`group.rs:262-268`)

### C-4 GroupGenesis is immutable / non-deletable (expected failure)
- **When** an update or delete is attempted
- **Then** rejected with `"immutable"` (`"found a new group instead"`) /
  `"cannot be deleted"` (`"stop granting memberships instead"`)
- **Validator unit tests** `group.rs::group_genesis_update_is_invalid`
  (`group.rs:763`), `group.rs::group_genesis_delete_is_invalid`
  (`group.rs:776`)

---

## D. GroupMembership — group role grants

`create_group_membership` (`group/crud.rs:140`). Three authority paths
for the grantor: **A** group author (implicit Owner), **B** hive Admin+
of the parent hive, **C** the grantor's own group membership.

### D-1 Group author grants a member (happy, Path A)
- **Given** Alice authored group `G` (implicit Owner)
- **When** `create_group_membership({ group_genesis_hash: G, for_agent:
  Bob, role: "Admin", grantor_membership_hash: null,
  grantor_hive_membership_hash: null, expiry: null })`
- **Then** commit succeeds (Path A, `group.rs:185-188`)

### D-2 Hive Admin+ grants into any group in their hive (happy, Path B)
- **Given** Carol is hive Admin of `H`; group `G` is in `H`; Carol did
  not author `G`
- **When** `create_group_membership({ group_genesis_hash: G, for_agent:
  Dave, role: "Writer", grantor_membership_hash: null,
  grantor_hive_membership_hash: M_carol_hive })`
- **Then** commit succeeds (Path B; hive Admin+ confers full group
  authority, `group.rs:190-205`)

### D-3 Group member with Admin grants a lower role (happy, Path C)
- **Given** Bob holds an `Admin` group membership `M_bob` in `G`
- **When** `create_group_membership({ …, for_agent: Eve, role: "Reader",
  grantor_membership_hash: M_bob })`
- **Then** commit succeeds (Path C)

### D-4 Self-grant is rejected (expected failure) — *the note-to-self pivot*
- **Given** any agent
- **When** `create_group_membership({ …, for_agent: <caller>, … })`
- **Then** commit rejected with `"self-grant is prohibited; the grantor
  cannot be the grantee"` (`group.rs:318-323`)
- **Validator unit tests**
  `group.rs::group_membership_self_grant_is_invalid` (`group.rs:835`),
  `group.rs::group_membership_self_grant_invalid_regardless_of_role`
  (`group.rs:850`)
- **Consequence** you can never witness *yourself*; note-to-self uses the
  empty-PKA shape instead (see `HUMM_TAURI_SELF_NOTES_INTEGRATION.md`
  §3.3)

### D-5 Granting without group authority is rejected (expected failure)
- **Given** Mallory has no Path A/B/C authority in `G`
- **When** Mallory tries to grant any membership
- **Then** commit rejected with `"is neither the group author … nor a
  hive Admin+ … and supplied no authorising GroupMembership"`
  (`group.rs:209-213`)

### D-6 Non-Owner cannot grant Owner (expected failure)
- **Given** Bob holds only `Admin` in `G`
- **When** `create_group_membership({ …, role: "Owner", … })`
- **Then** commit rejected with `"granting the Owner role requires group
  Owner or hive Admin+ authority"` (`group.rs:349-351`)

### D-7 Expiring grantor cannot extend the window (expected failure)
- **Given** Bob's group membership expires at `T` (Path C is his only
  basis)
- **When** Bob grants a membership with `expiry > T` or `null`
- **Then** commit rejected by grant-window containment
  (`enforce_grant_window`); Path A/B grantors are unconstrained
  (`group.rs::grant_window_unconstrained_without_grantor_membership`,
  `group.rs:871`)

### D-8 GroupMembership is immutable / non-deletable (expected failure)
- **Then** updates/deletes rejected with `"immutable"` / `"cannot be
  deleted"`; revoke via `revoke_group_membership` (`group/crud.rs:233`,
  issues a replacement with past `expiry`)
- **Validator unit tests** `group.rs::group_membership_update_is_invalid`
  (`group.rs:794`), `group.rs::group_membership_delete_is_invalid`
  (`group.rs:810`)

---

## E. EncryptedContent — `AclSpec::HiveGroup` (group-scoped content)

`create_encrypted_content` (`encrypted_content/crud.rs:28`) with
`acl_spec: { HiveGroup: { hive_genesis_hash, author_membership_hash,
group_acl, author_group_membership_hash, recipient_witnesses } }`.
Validator: `validate_hivegroup_acl` (`encrypted_content.rs:422`).
The G-6.2 invariant: **every pubkey in `public_key_acl` must be backed
by exactly one dominating `RecipientWitness`, and every witness pubkey
must appear in its claimed PKA bucket.**

> **Transient `Err` vs deterministic `Invalid` (retry vs reject).**
> The witness checks below call `must_get_valid_record` (so do
> `create_group_membership`, `create_group_genesis`, and any authority
> walk). On a *validating peer* that has not yet received the cited
> record, that host call returns an `Err` and validation is **deferred
> and retried** — it is NOT a permanent rejection. Only a response
> carrying the literal `"Validation failed while committing: <substr>"`
> (the substrings in these scenarios) is a deterministic `Invalid` that
> must not be retried with the same payload. Treat a commit **timeout /
> network error** as "retry shortly", never as "wire shape wrong".

### E-1 Author writes under a group they own, no extra recipients (happy)
- **Given** Alice authored group `G` (Path A) in hive `H` she owns
- **When** she writes content with `group_acl.owner = G`,
  `public_key_acl` all-empty, `recipient_witnesses: []`
- **Then** commit succeeds (this is the single-device self-note shape;
  the bidirectional check returns `None` because both passes iterate
  nothing — `encrypted_content.rs:599`, `:632`)

### E-2 Reader recipient backed by a matching witness (happy)
- **Given** Bob holds a `Reader`+ membership `M_bob` in a group `G` that
  is in `group_acl`
- **When** Alice writes content with `public_key_acl.reader = [Bob]` and
  `recipient_witnesses = [{ pubkey: Bob, bucket: "Reader", membership_hash:
  M_bob }]`
- **Then** commit succeeds (forward + reverse bidirectional pass;
  per-witness fetch confirms `M_bob`)

### E-3 PKA entry with no witness is rejected (expected failure)
- **Given** `public_key_acl.reader = [Bob]` but `recipient_witnesses: []`
- **When** the write is attempted
- **Then** commit rejected with `"not backed by any dominating
  recipient_witness"` (`encrypted_content.rs:608-613`)
- **Validator unit tests**
  `encrypted_content.rs::witnesses_empty_with_nonempty_pka_rejected`
  (`:2227`),
  `encrypted_content.rs::witnesses_missing_pka_entry_rejected` (`:2248`)

### E-4 Witness over-claiming a bucket is rejected (expected failure)
- **Given** a witness `{ pubkey: Mallory, bucket: "Reader", … }` but
  Mallory is **not** in `public_key_acl.reader`
- **When** the write is attempted
- **Then** commit rejected with `"claims bucket Reader"` … `"not in
  public_key_acl.Reader"` (`encrypted_content.rs:625-629`)
- **Validator unit test**
  `encrypted_content.rs::witnesses_over_claim_without_pka_entry_rejected`
  (`:2276`)

### E-5 Under-powered witness cannot back a higher bucket (expected failure)
- **Given** `public_key_acl.admin = [Bob]` with a witness
  `{ pubkey: Bob, bucket: "Reader", … }`
- **When** the write is attempted
- **Then** commit rejected with `"public_key_acl.Admin"` … `"not backed
  by any dominating recipient_witness"` — a Reader witness does not
  dominate an Admin PKA entry (`encrypted_content.rs:603-613`)
- **Validator unit tests**
  `encrypted_content.rs::witnesses_reader_cannot_back_admin_pka_step5`
  (`:2332`),
  `encrypted_content.rs::witnesses_reader_cannot_back_owner_pka_entry`
  (`:2425`); dominance matrix
  `encrypted_content.rs::acl_bucket_dominance_matrix` (`:2455`)

### E-6 Duplicate witness pubkey is rejected (expected failure)
- **Given** two witnesses with the same pubkey (different buckets)
- **When** the write is attempted
- **Then** commit rejected with `"duplicate"` … `"(one canonical witness
  per pubkey)"` (`encrypted_content.rs:587-590`)
- **Validator unit test**
  `encrypted_content.rs::witnesses_duplicate_pubkey_rejected` (`:2402`)

### E-7 Too many witnesses is rejected (expected failure)
- **Given** `recipient_witnesses.len() > 256`
- **When** the write is attempted
- **Then** commit rejected with `"HIVEGROUP_MAX_WITNESSES"`
  (`encrypted_content.rs:527-533`)
- **Validator unit test**
  `encrypted_content.rs::witnesses_exceed_max_count_rejected` (`:2360`)

### E-8 Forged witness for a non-member is rejected (expected failure)
- **Given** `public_key_acl.reader = [Mallory]` with a witness whose
  `membership_hash` does not resolve to a real `GroupMembership` for
  Mallory in a `group_acl` group
- **When** the write is attempted
- **Then** commit rejected at `verify_recipient_witness` — identity
  mismatch (`encrypted_content.rs:645-651`), group-not-in-bucket
  (`:690-696`), role-insufficient (`:699-705`), or expired
  (`:707-715`). *(This path fetches from the DHT — tryorama only; the
  step-5→step-6 boundary is pinned by
  `encrypted_content.rs::witnesses_step5_passes_when_round_trip_consistent_step6_triggers_fetch`,
  `:2307`.)*

### E-9 Cross-hive group reference is rejected (expected failure)
- **Given** a `group_acl` group whose `hive_genesis_hash` ≠ the entry's
  `hive_genesis_hash`
- **When** the write is attempted
- **Then** commit rejected with `"references group … in hive … but entry
  claims hive …"` (`encrypted_content.rs:472-478`)

### E-10 Too many group references is rejected (expected failure)
- **Given** `group_acl` totals > 64 groups
- **When** the write is attempted
- **Then** commit rejected with `"GROUP_ACL_MAX_GROUPS"`
  (`encrypted_content.rs:439-443`)

### E-11 Author lacking hive Writer+ is rejected (expected failure)
- **Given** the author holds < Writer in the entry's hive and is not its
  genesis author
- **When** the write is attempted
- **Then** commit rejected by the hive-authority step
  (`encrypted_content.rs:445-455`)

---

## F. EncryptedContent — `AclSpec::DirectMessage`

`acl_spec: { DirectMessage: { recipients } }`; validator
`validate_directmessage_acl` (`encrypted_content.rs:726`).
**Invariant:** `2 <= recipients.len() <= 32`, author ∈ recipients, no
duplicates, `public_key_acl.reader == sorted(recipients)`, other PKA
buckets empty.

### F-1 Two-party DM commits in any reader order (happy)
- **Given** Alice and Bob; Alice is the author
- **When** `create_encrypted_content({ acl_spec: { DirectMessage: {
  recipients: [Alice, Bob] } }, public_key_acl: { reader: [Bob, Alice],
  owner: "", admin: [], writer: [] }, … })`
- **Then** commit succeeds regardless of reader-bucket order
- **Validator unit test**
  `encrypted_content.rs::directmessage_accepts_2_recipients_in_any_order`
  (`:2120`)

### F-2 Self-DM with duplicate pubkey is rejected (expected failure) — *the reported bug*
- **Given** two devices sharing one keypair
- **When** a DM with `recipients: [me, me]` is written
- **Then** commit rejected with `"DirectMessage recipients contains
  duplicate pubkey <key>"` (`encrypted_content.rs:749-760`)
- **Use** note-to-self instead (`HUMM_TAURI_SELF_NOTES_INTEGRATION.md`).
  This is correct, Signal-aligned behavior — do **not** try to suppress
  it.

### F-3 Zero / one recipient is rejected (expected failure)
- **When** `recipients` has 0 or 1 entries
- **Then** commit rejected with `"recipients.len() = 0"` /
  `"recipients.len() = 1"` … `"(must be >= 2)"`
  (`encrypted_content.rs:731-736`)
- **Validator unit tests**
  `encrypted_content.rs::directmessage_rejects_zero_recipients`
  (`:2010`),
  `encrypted_content.rs::directmessage_rejects_one_recipient` (`:2024`)

### F-4 Over-cap recipients rejected (expected failure)
- **When** `recipients.len() > 32`
- **Then** commit rejected with `"exceeds DM_MAX_RECIPIENTS"`
  (`encrypted_content.rs:737-743`)
- **Validator unit test**
  `encrypted_content.rs::directmessage_rejects_over_max_recipients`
  (`:2038`)

### F-5 Author not in recipients rejected (expected failure)
- **When** the author is absent from `recipients` (spoofing a DM between
  two others)
- **Then** commit rejected with `"not in recipients"`
  (`encrypted_content.rs:744-748`)
- **Validator unit test**
  `encrypted_content.rs::directmessage_rejects_author_not_in_recipients`
  (`:2055`)

### F-6 reader bucket ≠ recipients rejected (expected failure)
- **When** `public_key_acl.reader` does not set-equal `recipients`
- **Then** commit rejected with `"public_key_acl.reader"` … `"does not
  match recipients"` (`encrypted_content.rs:772-782`)
- **Validator unit test**
  `encrypted_content.rs::directmessage_rejects_reader_bucket_mismatch`
  (`:2076`)

### F-7 Non-empty owner/admin/writer bucket rejected (expected failure)
- **When** a DM carries a non-empty `owner`, `admin`, or `writer` PKA
  bucket
- **Then** commit rejected with `"owner/admin/writer must be empty"`
  (`encrypted_content.rs:764-771`)
- **Validator unit test**
  `encrypted_content.rs::directmessage_rejects_nonempty_non_reader_buckets`
  (`:2100`)

---

## G. EncryptedContent — `AclSpec::Public` and `AclSpec::OpenWrite`

### G-1 Public content by a hive Writer+ (happy)
- **Given** Alice holds Writer+ in hive `H` (or authored it)
- **When** `create_encrypted_content({ acl_spec: { Public: {
  hive_genesis_hash: H, author_membership_hash } }, … })`
- **Then** commit succeeds (recipient set unconstrained; `reader: ["*"]`
  or empty are both fine as routing hints)

### G-2 OpenWrite with no target commits without a fetch (happy)
- **Given** any agent (no prior hive membership)
- **When** `create_encrypted_content({ acl_spec: { OpenWrite: {
  target_hive_genesis_hash: null } }, … })` (e.g. cross-network
  discovery)
- **Then** commit succeeds after only the author-vs-header check
- **Validator unit test**
  `encrypted_content.rs::openwrite_with_no_target_accepts_without_fetch`
  (`:2154`)

### G-3 OpenWrite member-request to a real hive (happy)
- **Given** an outsider knocking on hive `H`
- **When** `OpenWrite { target_hive_genesis_hash: H }` with `H` a real
  `HiveGenesis`
- **Then** commit succeeds (target existence is verified; a fake target
  is rejected — tryorama only)

---

## H. Author-identity guard (applies to every content write)

### H-1 Header pubkey must equal the committing agent (expected failure)
- **Given** any `EncryptedContent` write
- **When** `revision_author_signing_public_key` ≠ the committing agent's
  pubkey
- **Then** commit rejected with `"revision_author_signing_public_key"`
  (`check_author_matches_header`)
- **Validator unit tests**
  `encrypted_content.rs::check_rejects_when_header_pubkey_does_not_match_action_author`
  (`:1433`),
  `encrypted_content.rs::check_rejects_empty_header_pubkey` (`:1460`),
  `encrypted_content.rs::check_accepts_when_header_pubkey_matches_action_author`
  (`:1449`)
- **Builder rule** always set
  `revision_author_signing_public_key = <calling agent pubkey b64>`

---

## I. Content delete authority (I-A)

`delete_encrypted_content` (`encrypted_content/crud.rs:205`); validator
`validate_delete_encrypted_content`. **Permitted deleters:** the original
author, or any agent whose pubkey string appears in *any*
`public_key_acl` bucket (owner/admin/writer/reader).

### I-1 Original author deletes own content (happy)
- **Validator unit test**
  `encrypted_content.rs::delete_accepts_original_author` (`:1529`)

### I-2 DM recipient (in reader bucket) can delete (happy)
- **Given** a DM where Bob is in `public_key_acl.reader`
- **When** Bob deletes it
- **Then** commit succeeds (symmetric delete authority for DMs)
- **Validator unit tests**
  `encrypted_content.rs::delete_accepts_recipient_in_public_key_acl_reader`
  (`:1562`), plus admin/writer/owner-bucket variants
  (`:1581`/`:1598`/`:1615`), and the accept/reject pin
  `delete_reader_acl_accept_reject_pair` (`:1716`)

### I-3 Stranger cannot delete (expected failure)
- **Given** Mallory is the author of nothing and is not in any PKA bucket
- **When** Mallory deletes the entry
- **Then** commit rejected
- **Validator unit test**
  `encrypted_content.rs::delete_rejects_stranger_with_empty_public_key_acl`
  (`:1545`)

### I-4 Substring confusion does not grant delete (expected failure)
- **Given** an ACL value that is a strict super/substring of the
  deleter's pubkey
- **When** that agent deletes
- **Then** commit rejected — exact-string match required in all four
  buckets
- **Validator unit tests**
  `encrypted_content.rs::delete_rejects_when_author_string_is_substring_of_acl_value`
  (`:1632`),
  `encrypted_content.rs::delete_rejects_when_acl_value_is_substring_of_author_string`
  (`:1691`)

---

## J. Discovery links are a cache, never authority

### J-1 Links may be deleted by their author; entries are not (invariant)
- **Given** a `GroupMembership` and its discovery links
  (`AgentToGroupMemberships`, `GroupToGroupMemberships`)
- **When** the grantor deletes a discovery link
- **Then** the link row disappears but the `GroupMembership` entry
  remains valid
- **Builder rule** **never** gate an access decision on link presence;
  always re-derive authority from the entry via
  `get_latest_group_membership` / `list_group_members`
  (`group/queries.rs:54`/`:132`). A missing link does **not** prove a
  missing membership.

### J-2 Only the entry author may publish/delete its discovery links (expected failure)
- **When** a non-author tries to publish or delete a discovery link for
  someone else's entry
- **Then** commit rejected
- **Validator unit tests** `group.rs::link_author_mismatch_is_invalid`
  (`group.rs:898`), `group.rs::group_link_delete_requires_link_author`
  (`group.rs:912`), and the content-link author-equality tests
  `encrypted_content.rs::delete_link_hive_rejects_third_party` (`:1827`),
  `delete_link_dynamic_enforces_author_equality` (`:1843`),
  `delete_link_humm_content_id_enforces_author_equality` (`:1868`),
  `delete_link_humm_content_acl_enforces_author_equality_per_class`
  (`:1893`)

### J-3 The update-chain index cannot be deleted (expected failure)
- **When** anyone deletes an `EncryptedContentUpdates` link
- **Then** commit rejected with `"cannot be deleted"` — the update chain
  index is immutable
- **Validator unit test**
  `encrypted_content.rs::delete_link_encrypted_content_updates_is_invalid`
  (`:1941`)

---

## K. Reading is unguarded — re-derive trust on the client

### K-1 Roster reads are advisory; verify before trusting
- **Given** `list_group_members(G)` (`group/queries.rs:132`) or
  `list_my_groups()` (`group/queries.rs:228`)
- **When** the client uses the result for access decisions
- **Then** it must pair each entry with `get_latest_group_membership`
  and check role + expiry itself — these list externs dedupe by latest
  but carry no commit-time authority guarantee on the *link* layer

### K-2 `list_by_author` is currently unbounded (operational caveat)
- **Given** `list_by_author({ author, content_type })`
  (`encrypted_content/queries.rs:268`)
- **When** an author has many entries of a type (e.g. many self-notes)
- **Then** the call returns **all** of them with no `since_ts`/`limit`
  (confirmed `queries.rs:261-268`). For paginated sweeps prefer a
  `dynamic_link` label + `list_by_dynamic_link`, or `list_by_hive_link`
  (which *does* take `since_ts`/`limit`, `queries.rs:112`). A
  `since_ts`/`limit` extension to `list_by_author` is tracked separately.

---

## Quick assertion crib (binding layer)

```ts
// Happy path
const res = await client.callZome({ fn_name: "create_encrypted_content", payload });
expect(res.hash).toBeTruthy();

// Expected-failure path — assert on the wire-stable substring
await expect(client.callZome({ fn_name: "create_encrypted_content", payload }))
  .rejects.toThrow(/DirectMessage recipients contains duplicate pubkey/);

await expect(client.callZome({ fn_name: "create_group_membership", payload }))
  .rejects.toThrow(/self-grant is prohibited/);

await expect(client.callZome({ fn_name: "create_encrypted_content", payload }))
  .rejects.toThrow(/not backed by any dominating recipient_witness/);
```

The substrings above are copied verbatim from the pass-4 validators and
are part of the contract — assert on them directly. If a substring stops
matching after a happ upgrade, that is a deliberate validator change to
review, not a flaky test.
