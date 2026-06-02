# Note-to-Self (single + multi-device) — humm-tauri integration handoff

**Status:** architecture + wire-shape spec, ready to implement.
**Core happ change:** NONE. No DNA bump, no new `.happ`, no validator
edit. This ships entirely as a humm-tauri client-side feature on the
**existing pass-4 DNA**
(`uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`).
**Audience:** humm-tauri engineers wiring "Note to Self".
**Companion docs:**
- `HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md` — Given/When/Then sanity
  checks for every core-happ validator surface this feature composes.
- `HUMM_TAURI_SELF_NOTES_OBSERVABILITY.md` — step-by-step
  order-of-operations + the touchpoints/security checkpoints to log.
- `HUMM_TAURI_ACLSPEC_INTEGRATION.md` — the pass-3/4 `AclSpec` wire
  model this builds on.
- `.extraResearch/SIGNAL_MULTI_DEVICE_RESEARCH.md` — the Signal/Sesame
  evidence base that shaped this design.

---

## 1. TL;DR

Users expect "Note to Self" to (a) work on one device, and (b) sync
across their devices once they have more than one — exactly like
Signal's Note-to-Self. Both work on the current happ with **zero core
changes**:

- **Single-device note** = a `HiveGroup`-scoped `EncryptedContent`
  entry written under a personal **device-set group** the user authored,
  with an **all-empty `public_key_acl`** and **empty
  `recipient_witnesses`**. The author reads it back locally through
  their own SharedSecret.
- **Multi-device note** = the same entry, but the author's *other*
  device pubkeys are listed in `public_key_acl.reader`, each backed by
  a `RecipientWitness` that cites a real `GroupMembership` the author
  granted to that device. The key (`K`) is fanned out to each device
  via SharedSecret entries; every device reads it back.

The feature is **opt-in** and **invisible** to everything else: it
introduces no new entry type, no new extern, and no migration.

> **Why not a DM to yourself?** Because the DM validator correctly
> rejects it — see §2. This is not a bug to fix; it is the same
> invariant Signal enforces. Note-to-self gets its own scope instead.

---

## 2. Why a literal "DM to myself" cannot work (and shouldn't)

The UI error users hit when two devices share one keypair and send a DM
with `recipients = [me, me]`:

```
ConductorApiError: ExternalApiWireError(InternalError(
  "Source chain error: InvalidCommit error: Validation failed while
   committing: DirectMessage recipients contains duplicate pubkey
   uhCAk…"))
```

That rejection comes from `validate_directmessage_acl`
(`dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content.rs:726`).
Three independent rules each forbid a self-DM:

| Rule | Lines | What it rejects |
|---|---|---|
| `recipients.len() >= 2` | `encrypted_content.rs:731-736` | a 1-recipient ("just me") DM |
| no duplicate recipients | `encrypted_content.rs:749-760` | `[me, me]` (the exact error above) |
| author ∈ recipients | `encrypted_content.rs:744-748` | spoofed DMs between others |

**This matches Signal.** Sesame keeps a `UserRecord` for your own
identity but *no `DeviceRecord` for the device you are on*; a Signal
client never encrypts a message to its own current device. "Note to
Self" in Signal is a local conversation thread, and on multi-device it
fans out to your *other* devices' sessions — never to the sending
device itself. See `.extraResearch/SIGNAL_MULTI_DEVICE_RESEARCH.md`
§§3.1, 3.3. Relaxing the DM validator to allow `[me, me]` would weaken
a correct, Signal-aligned invariant. We do not touch it.

---

## 3. Architecture overview

### 3.1 The personal "device-set" group

Once per user (per hive they take notes in — see §3.5), the client
creates a `GroupGenesis` that represents *"the set of devices that are
me"*:

```ts
create_group_genesis({
  hive_genesis_hash,                 // the user's own hive (they are Owner)
  display_id: "device-set-v1",       // routing/display only; NOT security
  hive_wide_role: null,              // custom group (not a system role group)
  creator_hive_membership_hash: null // null = creator IS the hive genesis author
})
// → { genesis, hash }   ⇐  hash === device_set_genesis_hash
```

The returned **action hash is the device-set identity**
(`device_set_genesis_hash`) — the analogue of Signal's per-user ACI,
scoped to this hive. The author of this `GroupGenesis` is its **implicit
Owner** via `check_group_authority` **Path A**
(`group.rs:185-188`): no companion membership entry is needed (and a
self-membership is impossible — see §3.3).

> **Authority constraint.** Creating a custom group requires hive
> **Admin+** (`validate_create_group_genesis`, `group.rs:253-269`,
> demands `Role::Admin`). The user therefore creates the device-set in a
> hive they own (genesis author ⇒ implicit Owner) or hold Admin in. For
> personal notes that is the user's own hive; if they have none, found
> one first (`create_hive_genesis` is permissionless).

### 3.2 Two note shapes, one group

Every self-note is an `AclSpec::HiveGroup` `EncryptedContent` whose
`group_acl.owner = device_set_genesis_hash`. The author always satisfies
group authority through Path A. What changes between single- and
multi-device is **only** the `public_key_acl` + `recipient_witnesses`
fan-out (§5).

### 3.3 Why "self" is never a witness/PKA entry

The author is **never** listed in their own `public_key_acl` and never
has a `RecipientWitness`. Two reasons, both enforced by the validator:

1. The author already has full read authority as Path-A Owner of the
   device-set; they decrypt locally via their own SharedSecret.
2. A self-witness is **impossible**: it would require a `GroupMembership`
   where `grantor == for_agent`, which `validate_create_group_membership`
   Rule 1 rejects unconditionally
   (`group.rs:318-323`, `"self-grant is prohibited; the grantor cannot
   be the grantee"`). So there is no valid witness you could stamp for
   yourself even if you wanted to.

This is the crux: **self = empty network recipient set + local read**;
**other devices = witnessed PKA entries**. Identical to Signal's split.

### 3.4 Multi-device = authorize keys, never copy keys

Each device keeps its **own** agent keypair and its **own** source
chain (copying a private key forks a chain — a Holochain hazard).
Linking a new device means the existing device *grants it a
`GroupMembership` in the device-set group* (§6), authorizing its pubkey.
This mirrors Signal's `DeviceLinkCertificate` model
(`.extraResearch/SIGNAL_MULTI_DEVICE_RESEARCH.md` §4): the linked
device is authorized, not cloned.

### 3.5 Hive scope (v1 = single hive)

The device-set group is hive-local: `validate_hivegroup_acl` step 3
(`encrypted_content.rs:471-478`) rejects any group whose
`hive_genesis_hash` differs from the entry's. v1 ships notes-to-self in
the user's **primary/own hive** only. Cross-hive notes (one device-set
per hive, or a designated notes hive) are deferred — see §13.

---

## 4. Bootstrap sequence (client-side, idempotent)

```
ensureSelfNotesReady(hive_genesis_hash):
  1. membership = get_latest_membership({ agent: me, hive_genesis_hash })
     // null ⇒ I am the hive genesis author (implicit Owner) ⇒ author_membership_hash = null
     // else ⇒ author_membership_hash = membership.hash  (must grant Writer+)
  2. device_set = findMyDeviceSet(hive_genesis_hash)   // list_my_groups, match display_id "device-set-v1"
     if none:
        device_set = create_group_genesis({ hive_genesis_hash,
                       display_id: "device-set-v1", hive_wide_role: null,
                       creator_hive_membership_hash: membership?.hash ?? null })
  3. cache { device_set_genesis_hash, author_membership_hash }
```

`findMyDeviceSet` uses `list_my_groups()` and filters by
`display_id === "device-set-v1"` (display_id is non-authoritative, so
treat the *earliest-authored* match as canonical if duplicates ever
appear; the action hash is the real identity).

---

## 5. Concrete wire shapes (copy-paste)

Both call `create_encrypted_content(CreateEncryptedContentInput { … })`
(`encrypted_content/crud.rs:28`). `revision_author_signing_public_key`
MUST equal the calling agent's pubkey (validator
`check_author_matches_header`).

### 5.1 Single-device note

```ts
create_encrypted_content({
  id,                                          // app content squuid
  display_hive_id,                             // display only
  content_type: "humm-self-note-v1",
  revision_author_signing_public_key: myPubkeyB64,   // == action.author
  bytes: encryptedNoteBytes,                   // ciphertext under K (§7)
  acl_spec: {
    HiveGroup: {
      hive_genesis_hash,
      author_membership_hash: myHiveMembershipHash ?? null,  // null if I'm hive author
      group_acl: {
        owner: device_set_genesis_hash,
        admin: [], writer: [], reader: [],
      },
      author_group_membership_hash: null,      // Path A: I authored the device-set
      recipient_witnesses: [],                  // no other devices
    },
  },
  public_key_acl: { owner: "", admin: [], writer: [], reader: [] },  // ALL empty
  dynamic_links: ["self-notes"],               // optional: a stable thread label
})
```

### 5.2 Multi-device note (author = device_A; other devices B, C)

```ts
create_encrypted_content({
  id, display_hive_id,
  content_type: "humm-self-note-v1",
  revision_author_signing_public_key: deviceA_pubkeyB64,
  bytes: encryptedNoteBytes,
  acl_spec: {
    HiveGroup: {
      hive_genesis_hash,
      author_membership_hash: myHiveMembershipHash ?? null,
      group_acl: {
        owner: device_set_genesis_hash,
        admin: [], writer: [], reader: [],
      },
      author_group_membership_hash: null,      // Path A
      recipient_witnesses: [
        { pubkey: deviceB_pubkey, bucket: "Reader", membership_hash: deviceB_membership_hash },
        { pubkey: deviceC_pubkey, bucket: "Reader", membership_hash: deviceC_membership_hash },
      ],
    },
  },
  public_key_acl: {
    owner: "", admin: [], writer: [],
    reader: [deviceB_pubkeyB64, deviceC_pubkeyB64],   // author NOT listed
  },
  dynamic_links: ["self-notes"],
})
```

`deviceB_membership_hash` / `deviceC_membership_hash` are the
`GroupMembership` action hashes minted during the linking ceremony
(§6). Each device is in `public_key_acl.reader` and backed by exactly
one `Reader`-bucket witness — the validator's bidirectional rule.

> **🔒 Before you ship this shape:** listing devices in
> `public_key_acl.reader` gives each of them **delete authority** over
> the note (§12 **L9**) and publishes the device pubkey set as plaintext
> metadata (§12 **L3**). For note-to-self the **safer default** is the
> §5.1 empty-PKA shape plus a SharedSecret-only fan-out to your other
> devices (§7 step 4) — readers sync by pull, cannot delete, and stay
> out of the entry metadata. Use this PKA-listed shape only when you
> need cross-device **push** notification.

---

## 6. Validator walk-through (line-cited proof both shapes commit)

All references are to the integrity zome under
`dnas/humm_earth_core/zomes/integrity/content/src/`.

> **Validation `Err` vs `Invalid` (retry vs reject).** A *deterministic*
> rejection carries a `"Validation failed while committing: <validator
> msg>"` string (the BDD-doc substrings) — never retry the same payload.
> A *transient* failure (a `must_get_valid_record` for a witness
> membership that has not yet propagated to the validating peer) surfaces
> as a timeout / network `Err`, not an `Invalid`, and IS safe to retry
> after a short delay. Do not treat a validation timeout as "wire shape
> wrong".

### 6.1 Single-device shape

`run_content_validators` → `validate_hivegroup_acl`
(`encrypted_content.rs:422`):

1. **group_acl cardinality** (`:435-444`): `total_groups = 1` (owner
   only) ≤ `GROUP_ACL_MAX_GROUPS (64)`. ✓
2. **hive authority** (`:445-455`): `check_hive_authority(me, hive, …,
   Writer)`. If I authored the hive → Path 1 implicit Owner
   (`hive.rs:205-207`). Else my `author_membership_hash` must grant
   Writer+. ✓
3. **per-group authority** (`:463-490`): the loop visits `group_acl.owner
   = device_set`. Cross-hive check passes (device-set is in this hive).
   `check_group_authority(me, device_set, …, Writer)` → **Path A** Owner
   (`group.rs:185-188`). ✓
4. **witnesses** → `validate_recipient_witnesses([], emptyPKA, …)`
   (`:520`): cardinality 0 ✓; `check_witness_pka_bidirectional`
   (`:560`) — `witness_strings` empty, the owner PKA entry is filtered
   out because it is `""` (`:599`), all other buckets empty ⇒ the
   forward loop body (`:603-614`) never executes and the reverse loop
   (`:617-631`) never executes ⇒ returns `None` (valid) (`:632`); the
   per-witness fetch loop (`:540-545`) iterates nothing. ✓

**Result: Valid.** No DHT fetch beyond hive + device-set genesis.

### 6.2 Multi-device shape

Steps 1-3 are identical (group authority via Path A). Step 4 now has
work:

- **cardinality** (`:526-534`): 2 witnesses ≤ `HIVEGROUP_MAX_WITNESSES
  (256)`. ✓
- **bidirectional** (`check_witness_pka_bidirectional`, `:560-633`):
  - no duplicate witness pubkeys (`:579-593`). ✓
  - **forward** (`:603-614`): each `reader` PKA entry (deviceB, deviceC)
    is `backed` by a witness whose bucket `dominates(Reader)` — each has
    a `Reader` witness, and `Reader.dominates(Reader)` is true. ✓
  - **reverse** (`:617-631`): each witness's pubkey appears in
    `public_key_acl.reader`. ✓
- **per-witness verify** (`verify_recipient_witness`, `:639-717`) for
  each device:
  - `membership.for_agent == witness.pubkey` (`:645-651`). ✓ (the grant
    was *for* that device)
  - group containment (`:684-688`, Reader bucket accepts
    owner∪admin∪writer∪reader): the membership's
    `group_genesis_hash == device_set` which is `group_acl.owner`. ✓
  - role satisfies bucket (`:698-705`): `bucket_required_role(Reader) =
    Reader`; the device holds `Admin` (granted in §6 of the ceremony),
    and `role_satisfies(Admin, Reader)` is true. ✓
  - unexpired (`:707-715`): device-set memberships are permanent
    (`expiry: null`). ✓

**Result: Valid.**

### 6.3 The self-witness that can't exist (and why it's fine)

If you tried to "include yourself" by listing your own pubkey in
`public_key_acl.reader`, the forward check (`:603-614`) would demand a
witness for you, and minting that witness requires
`create_group_membership({ for_agent: me })`, which Rule 1
(`group.rs:318-323`) rejects. The empty-PKA path (§5.1, §6.1) is the
*intended* self-read mechanism, so this is a non-issue — but the
attempt is line-cited here so nobody re-discovers it the hard way.

---

## 7. SharedSecret encryption flow

Reuse humm-tauri's existing `hummhive-core-shared-secrets-v1`
content-type and `SharedSecretApi`. Per self-note **write**:

1. Generate a per-note symmetric key `K`.
2. `bytes = encrypt(noteJson, K)`.
3. **Self-encryption (always):** publish a SharedSecret entry that wraps
   `K` to the author's own X25519 public key via deterministic
   `ECDH(my_x25519_priv, my_x25519_pub)`. This is what lets the author
   re-read after re-install on the same agent. *(This is the one
   genuinely new app-layer capability: the existing fan-out helper skips
   self; self-notes need the self-wrap branch.)*
4. **Per-other-device (multi-device only):** for each *other* device in
   your device-set, publish a SharedSecret entry wrapping `K` to that
   device's X25519 public key. This fan-out (who can **read**) is
   **independent** of `public_key_acl.reader` (who gets a **push signal**
   + **delete authority**). For note-to-self prefer fanning SharedSecrets
   to other devices while keeping PKA **empty** (the §5.1 shape) — other
   devices then sync by *pull* and **cannot delete** the note; see §12
   L9. Use the §5.2 PKA-listed shape only when you specifically want
   cross-device *push* notification.

Per self-note **read** (on any of the user's devices):

1. Resolve the note's SharedSecret entries (existing linking).
2. Try to unwrap each with the local device's X25519 private key; first
   success yields `K`.
3. `note = decrypt(bytes, K)`.

> Routing note: because `public_key_acl` is empty in the single-device
> case, `create_encrypted_content`'s `send_remote_signal` fan-out has no
> targets — single-device self-notes produce **no network signal**
> (correct: there is no other device to notify). Multi-device self-notes
> fan a signal out to the *other* devices (self is excluded by the
> existing minus-self rule). Cross-device discovery on a freshly linked
> device is **query-based** via `list_by_author({ author: me,
> content_type: "humm-self-note-v1" })` (`queries.rs:268`), not
> push-based. (If you want incremental sync on the notes thread, prefer
> a `dynamic_link` label + `list_by_dynamic_link`, since `list_by_author`
> is currently unbounded — see roadmap note on `since_ts`/`limit`.)

---

## 8. Link-time backfill (so a new device sees old notes)

A device linked *after* notes were written cannot decrypt them (its
pubkey wasn't a SharedSecret recipient yet). At link time, the existing
device backfills:

```
backfillForNewDevice(new_device_x25519_pub, sinceLimit?):
  notes = list_by_author({ author: me, content_type: "humm-self-note-v1" })
          // or list_by_dynamic_link({ …, dynamic_link: "self-notes" })
  for each note (bounded by sinceLimit, client's choice):
     K = unwrap(note.sharedSecret, my_x25519_priv)
     publish SharedSecret wrapping K → new_device_x25519_pub
```

Linear in (#notes); no new entry type, no validator involvement — these
are ordinary SharedSecret writes. The new device then discovers and
decrypts via the §7 read flow. This is the analogue of Signal's
"Link-and-Sync" transcript transfer
(`.extraResearch/SIGNAL_MULTI_DEVICE_RESEARCH.md` §5), done with the
primitives we already have.

Forward-fan for **future** notes: once a device is in the set, include
its pubkey in `public_key_acl.reader` + a witness on every new note
(§5.2). Maintain the "current device set" client-side from
`list_group_members(device_set_genesis_hash)` (`group/queries.rs:132` —
the authoritative roster).

> **🔒 Critical — which X25519 key?** `new_device_x25519_pub` is the new
> device's **permanent keystore encryption key** — an **independent**
> SLIP-0010-derived key that is **NOT** derivable from its `AgentPubKey`.
> humm-tauri's keystore derives signing (`m/44'/1517'/0'/0'/0'`) and
> encryption (`m/44'/1517'/0'/1'/0'`) on separate paths and never
> converts one to the other (signing/encryption key reuse is unsafe).
> A therefore obtains B's X25519 **only from B's self-authored**
> `humm-dm-keybinding-v1` (`action.author == B`, B in the device-set
> roster) — used for BOTH link-time backfill (A `waitFor`s it to gossip,
> then wraps each past note's K) and ongoing fan-out + rotation. The
> A→B link bundle CANNOT carry B's key (A doesn't have it; the bundle is
> encrypted to B's ephemeral and holds device-set info only). Putting B's
> permanent X25519 in the QR instead would force the SAS (§12 L4) to cover
> it (4 fields) or a MITM swaps it and A backfills every past note's K to
> the attacker — not worth it; the keybinding is self-authenticated (B's
> signature) and needs no SAS field. **Never** use the QR's one-time
> `ephemeral_x25519_pub` for backfill: device B discards the ephemeral
> private key after the link bundle, so SharedSecrets wrapped to it are
> permanently undecryptable — and the failure is **silent** (entry
> commits; the later read just finds "no decryptable SharedSecret"). See
> §12 L8.

---

## 9. Device-linking ceremony (full spec)

Signal-inspired authorize-don't-copy flow.

**New device (B):**
1. Generates its own agent keypair (own source chain) + an ephemeral
   X25519 keypair for the link handshake.
2. Displays a QR / pairing code:
   `{ new_device_pubkey: B, ephemeral_x25519_pub, link_nonce }`.

   > **🔒 Before A grants anything (§12 L4):** A MUST display
   > `new_device_pubkey` for explicit user approval, AND both devices MUST
   > compare a short-authentication-string over the **whole** QR payload —
   > `SAS = hash(new_device_pubkey || ephemeral_x25519_pub || link_nonce)`,
   > not the ephemeral key alone. A SAS over only the ephemeral key lets a
   > MITM swap `new_device_pubkey` (SAS still matches) and redirect the
   > grant to an attacker.

**Existing device (A, already in the set):**
3. Scans, then grants B a device-set membership:
   ```ts
   create_group_membership({
     group_genesis_hash: device_set_genesis_hash,
     for_agent: deviceB_pubkey,
     role: "Admin",                          // Admin ⇒ B can link further devices
     grantor_membership_hash:
       A_is_device_set_author ? null          // Path A (A authored the device-set)
                              : A_device_set_membership_hash,  // Path C (A is a linked device)
     grantor_hive_membership_hash: null,
     expiry: null,                            // permanent (avoids grant-window containment)
   })
   // → deviceB_membership_hash
   ```
   - Non-self-grant (A ≠ B) ⇒ passes Rule 1.
   - A holds Owner (Path A) or Admin (Path C) ⇒ passes Rule 2
     (`group.rs:325-336`).
   - Granting `Admin` (not `Owner`) ⇒ Rule 3 escalation check does not
     fire.
   - Permanent grant ⇒ Rule 4 grant-window containment is vacuous.
4. Encrypts a **link bundle** to `ephemeral_x25519_pub`:
   ```
   { device_set_genesis_hash,
     hive_genesis_hash,
     deviceB_membership_hash,
     other_device_pubkeys: [A, …],            // current set minus B
     author_membership_hash }                 // so B can write notes too
   ```
5. Runs `backfillForNewDevice(deviceB_x25519_pub)` (§8).
   (`deviceB_x25519_pub` = B's **permanent** X25519 key, NOT the
   ephemeral one used in step 4 — see §8.)

**New device (B):** decrypts the bundle, caches the set, and is ready to
read (via backfilled SharedSecrets) and write (using its own membership
as a witness source for future notes).

**Resilience:** because linked devices get `Admin`, any device can link
the next one — losing the *primary* device does not lock the account.
Losing *all* devices is unrecoverable (the device-set `GroupGenesis` is
immutable and non-deletable, `group.rs:285-293`); a recovery key is
future work (§13).

---

## 10. Migration story

**None required.** Self-notes are a new, opt-in feature. No prior
`[me, me]` self-DMs were ever committed (the validator rejected them),
so there is no on-DHT data to migrate. Existing DMs, groups, and content
are untouched. The DNA hash is unchanged, so installed conductors need
no re-install.

---

## 11. BDD test scenarios — note-to-self end-to-end

Given/When/Then scenarios humm-tauri should cover at the binding /
tryorama layer. These compose the primitive validator checks (each of
which is *already* unit-tested in the core happ — see
`HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md`); what humm-tauri verifies
here is that its **wire-shape builder** produces inputs that commit and
round-trip. "Then commit succeeds" means
`create_encrypted_content` resolves `Ok`; "Then commit is rejected with
`<substr>`" means it rejects and the error message contains `<substr>`.

### SN-1 — Bootstrap creates the device-set once (happy)
- **Given** a fresh user who is the genesis author (Owner) of their own
  hive `H` and has no `device-set-v1` group
- **When** `ensureSelfNotesReady(H)` runs, then runs a second time
- **Then** the first run commits a `GroupGenesis` with
  `display_id = "device-set-v1"` and the second run finds and reuses it
  (no duplicate created)
- **And** the user is the device-set's Path-A Owner (no `GroupMembership`
  is or can be written for themselves)

### SN-2 — Single-device note commits and round-trips (happy)
- **Given** a ready device-set and `public_key_acl` all-empty,
  `recipient_witnesses: []`
- **When** the author writes a `humm-self-note-v1` per §5.1, then reads
  it back via the SharedSecret self-wrap
- **Then** commit succeeds and the decrypted note equals the original
- **And** `create_encrypted_content` emits **no** remote signal (empty
  reader bucket ⇒ no fan-out targets)

### SN-3 — Single-device note survives re-install on the same agent (happy)
- **Given** a single-device note written in SN-2
- **When** the app is reinstalled with the *same* agent key and reads via
  the self-wrapped SharedSecret (`ECDH(my_priv, my_pub)`)
- **Then** the note decrypts successfully

### SN-4 — Self in PKA without a witness is rejected (expected failure)
- **Given** a self-note whose `public_key_acl.reader = [myPubkeyB64]` but
  `recipient_witnesses: []`
- **When** the author writes it
- **Then** commit is rejected with
  `"not backed by any dominating recipient_witness"`
  (`encrypted_content.rs:608-613`)
- **And** this is *expected* — the empty-PKA shape (§5.1) is the correct
  single-device form; never list yourself

### SN-5 — Attempting to witness yourself is impossible (expected failure)
- **Given** a user trying to mint a device-set membership for their own
  pubkey to "witness themselves"
- **When** `create_group_membership({ group_genesis_hash:
  device_set, for_agent: me, … })`
- **Then** commit is rejected with
  `"self-grant is prohibited; the grantor cannot be the grantee"`
  (`group.rs:318-323`)

### SN-6 — Device linking grants a non-self membership (happy)
- **Given** device A is the device-set Owner (Path A) and device B has
  its own keypair
- **When** A runs the §9 ceremony:
  `create_group_membership({ group_genesis_hash: device_set, for_agent:
  B, role: "Admin", grantor_membership_hash: null, expiry: null })`
- **Then** commit succeeds and returns `deviceB_membership_hash`
- **And** A could equally be a previously-linked device using its own
  membership as `grantor_membership_hash` (Path C)

### SN-7 — Multi-device note fans out and every device reads it (happy)
- **Given** device-set {A, B} with B linked + backfilled
- **When** A writes a note per §5.2 (`reader: [B]`, one Reader witness
  citing `deviceB_membership_hash`) and B reads via its SharedSecret
- **Then** commit succeeds, B decrypts the note, and a remote signal
  reaches B (not A)

### SN-8 — Witness for a non-member device is rejected (expected failure)
- **Given** device C that was never granted a device-set membership
- **When** A writes a note with `reader: [C]` and a witness citing a
  `membership_hash` that is not a real membership for C
- **Then** commit is rejected at per-witness verification
  (`verify_recipient_witness`, `encrypted_content.rs:645-651` /
  `:690-696`) — a forged recipient cannot be injected

### SN-9 — Wrong-bucket witness is rejected (expected failure)
- **Given** a note listing B in `public_key_acl.reader`
- **When** the witness for B claims `bucket: "Owner"` but B's pubkey is
  not in `public_key_acl.owner`
- **Then** commit is rejected with
  `"claims bucket Owner but pubkey is not in public_key_acl.Owner"`
  (`encrypted_content.rs:625-629`)

### SN-10 — Link-time backfill makes old notes readable on B (happy)
- **Given** notes N1, N2 written by A *before* B was linked
- **When** A runs `backfillForNewDevice(B_x25519_pub)` over
  `list_by_author({ author: A, content_type: "humm-self-note-v1" })`
- **Then** B can subsequently decrypt N1 and N2 via the new SharedSecret
  entries
- **And** no `EncryptedContent` entry is re-written (backfill only adds
  SharedSecret wraps)

### SN-11 — Device-set genesis cannot be deleted (expected failure)
- **Given** an existing device-set `GroupGenesis`
- **When** any agent attempts to delete it
- **Then** commit is rejected with
  `"GroupGenesis entries cannot be deleted; stop granting memberships
  instead"` (`group.rs:285-293`) — documents the all-devices-lost
  irrecoverability

### SN-12 — Self-note in a hive where the user lacks Writer+ is rejected (expected failure)
- **Given** a hive `H2` where the user holds only `Reader` (or no
  membership) and is not the genesis author
- **When** the user tries to write a `humm-self-note-v1` under a
  device-set in `H2`
- **Then** commit is rejected by the hive-authority step
  (`validate_hivegroup_acl` step 2, `encrypted_content.rs:445-455`) —
  reinforcing that the device-set lives in a hive the user owns/admins
  (§3.1, §3.5)

---

## 12. Security footguns & landmines (READ BEFORE IMPLEMENTING)

This feature is encryption + key-management + access-delegation on a
**public, immutable, append-only DHT**. The validators (BDD doc) protect
*authorship and routing integrity*; they do **not** protect
confidentiality — that is entirely the client's encryption. The items
below (referenced as **L1–L9** from the observability doc) are the ways
a well-meaning implementation can leak or lose user data. Treat each as
a required review item, not advice.

### L1 — Witnesses + `public_key_acl` are NOT read access control
- **Risk:** Believing "I put the note in a private group / empty PKA, so
  only I (or my devices) can read it." **False.** Every
  `EncryptedContent.bytes` blob is world-readable on the DHT. Anyone can
  `get` it.
- **Why:** `group_acl`, `public_key_acl`, and `recipient_witnesses` gate
  **commit authority, signal fan-out, and delete authority** — not
  decryption. There is no read-side validator (reads are unguarded by
  design; see BDD §K).
- **Do:** Confidentiality rests **only** on the per-note key `K` and who
  holds a SharedSecret wrapping it. Never put anything in `bytes` you
  would not encrypt. Never reason about secrecy from the ACL.

### L2 — Device removal is NOT retroactive (the biggest landmine)
- **Risk:** "I revoked the lost device's membership, so it can no longer
  read my notes." **False for every note it already had.**
- **Why:** The DHT is **immutable and append-only**. Each note's `K` was
  published wrapped to that device's pubkey and **stays there forever**;
  `revoke_group_membership` (expiry) only stops *future* witness-backed
  fan-out + signal routing. The device (or whoever holds its key) can
  re-fetch the old wrapped `K` and the old ciphertext at any time.
  `GroupGenesis`/`GroupMembership` are non-deletable (`group.rs:285-293`).
- **Do:** Treat device linking as **irreversible for past content**. To
  protect content after a device compromise you must **re-key**:
  re-encrypt notes under a fresh `K` and fan out to the *remaining*
  devices only — and accept that the **old** ciphertext + old wrapped
  keys remain on the DHT permanently. Surface this to users honestly
  ("removing a device stops new notes from reaching it; it cannot
  un-share notes it already had"). A true revocation/forward-secrecy
  story is out of scope (§13).
- **Do (re-key detail):** re-keying MUST **write new `EncryptedContent`
  entries**, never `update_encrypted_content` on the old ones. An update
  leaves the original action hash addressable with its original `bytes`
  on the DHT, so a device that cached the original hash still reads the
  original ciphertext + old wrapped `K`. Only brand-new entries fanned
  solely to the remaining devices exclude the removed one going forward.

### L3 — The device-membership graph is PUBLIC metadata
- **Risk:** The multi-device shape lists every other-device pubkey in
  `public_key_acl.reader` **and** in `recipient_witnesses`, all
  plaintext in the entry header; the device-set roster
  (`GroupToGroupMemberships` links + `GroupMembership` entries) is also
  public. An observer can enumerate **all of a user's device pubkeys**
  and link them to one logical identity. `content_type:
  "humm-self-note-v1"` is plaintext, so self-notes are enumerable by
  type, and ciphertext length leaks plaintext length.
- **Why:** Only `bytes` is encrypted; the header is not. Holochain has no
  sealed-sender equivalent here. (This is strictly worse than Signal,
  which hides device lists.)
- **Do:** Document the privacy limitation for users. Consider padding
  note ciphertext to fixed buckets to blunt length analysis. Do not put
  identifying data in `id` / `display_hive_id` / `dynamic_links` (all
  plaintext, all on the DHT).

### L4 — Device linking is the entire trust root
- **Risk:** The pairing handshake (§9) is the one moment an external
  pubkey becomes "one of my devices." An attacker who **relays or
  substitutes the QR** (MITM) gets the existing device to grant *them* a
  device-set membership → they receive the backfill of every past note
  key (§8) and, if granted `Admin`, can **link further devices
  themselves**. That is full, self-propagating account takeover.
- **Do (all required):**
  1. Require an explicit **user approval** step that displays the new
     device's pubkey before the grant (§9 D4).
  2. Add a **short-authentication-string (SAS)** comparison (Signal
     "safety number" style) over the **entire QR payload** —
     `SAS = hash(new_device_pubkey || ephemeral_x25519_pub || link_nonce)`,
     with a fixed canonical encoding (e.g. SHA-256 over length-prefixed
     fields). A SAS over the ephemeral key **alone** is insufficient: a
     MITM keeps the real ephemeral key (SAS matches) but substitutes
     `new_device_pubkey`, so the membership grant — and every future-note
     SharedSecret — goes to the attacker. Both devices must display the
     same SAS before A calls `create_group_membership`.
  3. Prefer granting the new device **`Reader`** unless
     link-more-devices resilience is genuinely needed; `Admin` widens
     the blast radius of a single bad link. Document the tradeoff.
  4. Never auto-grant on scan; never accept a pairing payload from any
     channel but the live in-person QR.
  5. **Revocation does NOT cascade.** `revoke_group_membership` ages out
     one device's membership, but any memberships that device **already
     granted** while valid stay valid (each `GroupMembership` is
     independently immutable). A compromised `Admin` device can pre-link
     attacker devices that **survive** its revocation and keep linking
     more. After revoking a device, enumerate the full roster
     (`list_group_members`) and individually expire every Admin grant it
     issued. For high-security profiles, allow links **only** from the
     Path-A device-set author (a strict linear chain), never from linked
     Admins.

### L5 — Self-wrap / per-note key crypto discipline
- **Risk:** The self-wrap uses a **deterministic** key
  `ECDH(my_x25519_priv, my_x25519_pub)` — the same value every time. Two
  classic mistakes turn it catastrophic: (a) encrypting note **content**
  directly under this static key (every note shares a key); (b) reusing
  an AEAD **nonce** under it (nonce reuse ⇒ key/plaintext recovery for
  GCM/ChaCha-Poly).
- **Do:** Always generate a **fresh random per-note `K`**, encrypt
  content under `K`, and wrap `K` to recipients (including self) with a
  **fresh random nonce per wrap**. The static ECDH value encrypts only
  the small `K`, never content, and never with a repeated nonce. Confirm
  the X25519 library accepts equal sender/recipient keys (mathematically
  fine; verify the implementation does not special-case or reject it).

### L6 — Always use the device-set you AUTHORED
- **Risk:** `display_id: "device-set-v1"` is non-authoritative and
  **forgeable** — anyone can create a group with that label, and a
  malicious membership grant could surface a foreign "device-set-v1" in
  your `list_my_groups()`. Writing notes under an attacker-owned group
  makes the attacker (its Owner) a legitimate party.
- **Do:** Select the device-set strictly by **genesis author == my
  pubkey** (the action hash is the real identity, not `display_id`). Log
  + assert this (observability Gap-L6).

### L7 — Inbound signals are spoofable; verify + re-fetch
- **Risk:** `recv_remote_signal` has an open cap grant; any peer can send
  a fabricated `EncryptedContentSignal` claiming a self-note. Rendering
  the signal body, or trusting it without checking the author, lets a
  stranger inject content into the user's notes view.
- **Do:** On any `humm-self-note-v1` signal, (1) confirm the
  conductor-attested `from_agent` is a member of **your** device-set
  (validated `GroupMembership` roster), and (2) re-fetch the entry with
  `get_encrypted_content(hash)` before display. Never render the signal
  payload directly (the zome's own threat-model comment says so —
  `lib.rs:159-182`).
- **Do (roster caveat):** the "is `from_agent` in my device-set" check
  reads `list_group_members`, which walks **deletable** discovery links
  (BDD §J-1). A hostile grantor can delete a legit device's roster link,
  making it appear absent. Re-derive trust from the `GroupMembership`
  entries themselves rather than link presence, and fail **safe** (treat
  "unknown" as "not yet trusted", not "permanently rejected").
- **Do (rate-limit):** the open cap grant lets a peer flood signals;
  re-fetching on every signal turns that into a `get_*` flood. Budget the
  follow-up fetch per `from_agent` (e.g. ≤5/s), debounce duplicate
  hashes, and on a `None` re-fetch drop the signal — do not retry the
  same hash from the same burst.

### L8 — Availability / data-loss traps (not breaches, but user-fatal)
- **Single-device key loss:** the self-wrap is the only copy of `K` for a
  one-device user; losing the agent keypair makes every note
  permanently unreadable. Encourage linking a second device (or an
  explicit backup) before notes accumulate.
- **All-devices-lost:** the device-set `GroupGenesis` is immutable +
  non-deletable; losing every device is unrecoverable. A recovery-key
  escrow is future work (§13).
- **Committed-but-unreadable:** the note entry (B3) and the self-wrap
  SharedSecret (B6) are **separate** transactions; if the note lands and
  the self-wrap fails, the note is unreadable. Orchestrate them as one
  unit and verify the self-wrap before declaring success.
- **Backfill amplification:** linking a device writes one SharedSecret
  per existing note; an unbounded backfill bloats the source chain and
  is a DoS surface. Bound it (last N) and log the count.

### L9 — Multi-device delete authority is SYMMETRIC
- **Risk:** Single-device shape (§5.1, empty PKA) ⇒ only the author can
  delete. **Multi-device** shape (§5.2) ⇒ `public_key_acl.reader =
  [deviceB, …]`, and the I-A delete rule
  (`validate_delete_encrypted_content`, `encrypted_content.rs:850-858`)
  grants delete authority to the author **∪ any pubkey in any PKA
  bucket**. So **every PKA-listed device can permanently tombstone any
  such note** — one compromised/hostile device can destroy the whole
  history. (Correct-by-design for DMs; dangerous for a note store.)
  Granting `Reader` instead of `Admin` (L4) does **not** help — reader
  pubkeys have the same delete authority.
- **Do:** For note-to-self prefer the **empty-PKA + SharedSecret-only
  fan-out** variant (§7 step 4): keep `public_key_acl` empty (§5.1) yet
  still wrap `K` to each other device. Other devices then **read** (by
  pull; no push signal) but have **no delete authority** and do not
  appear in the entry's PKA/witness metadata. Use the §5.2 PKA-listed
  shape only when cross-device **push** is required, and then prefer
  app-level **soft-delete** (a tombstone content-update the author owns)
  over the `delete_encrypted_content` extern. Tell users plainly: a
  PKA-listed device can delete shared notes, and removing it later does
  not undo deletes it already made.

---

## 13. Out of scope for v1 (documented future work)

- **Cross-hive notes-to-self.** v1 is single-hive. Options: one
  device-set per hive (link ceremony adds B to all sets) or a designated
  notes hive. (§3.5)
- **Forward secrecy / ratcheting.** Per-note `K` gives confidentiality
  but not FS; a Signal-style double-ratchet would be an app-layer
  project of its own.
- **Correspondent-visible device sets.** When Bob DMs you, his app sends
  to the one pubkey he knows; it does not yet fan out to your device
  set. A discoverable per-user device-set record + sender-side fan-out is
  a separate design pass.
- **Recovery from total device loss.** The device-set genesis is
  immutable/non-deletable; a recovery-key escrow is future work.
- **Coordinator convenience externs** (e.g. `list_device_set`,
  `write_self_note`). Not needed — the existing externs suffice; add
  sugar later if the call sites get noisy.

---

## 14. References

- Validator source (pass-4), all under
  `dnas/humm_earth_core/zomes/integrity/content/src/`:
  - `group.rs:185-188` Path A implicit Owner;
    `group.rs:318-323` no-self-grant;
    `group.rs:285-293` GroupGenesis immutable/non-deletable;
    `group.rs:253-269` group-create requires hive Admin+.
  - `encrypted_content.rs:422-499` `validate_hivegroup_acl`;
    `:560-633` `check_witness_pka_bidirectional`;
    `:639-717` `verify_recipient_witness`;
    `:726-784` `validate_directmessage_acl`.
  - `hive.rs:205-207` hive Path 1 implicit Owner.
- Coordinator externs:
  `encrypted_content/crud.rs:28` `create_encrypted_content`;
  `group/crud.rs:69` `create_group_genesis`;
  `group/crud.rs:140` `create_group_membership`;
  `group/queries.rs:132` `list_group_members`;
  `encrypted_content/queries.rs:268` `list_by_author`.
- `HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md` — primitive-level
  Given/When/Then for every validator surface above.
- `.extraResearch/SIGNAL_MULTI_DEVICE_RESEARCH.md` — Signal/Sesame
  evidence (single-device local thread; multi-device sealed-sender
  fan-out; device linking; Link-and-Sync).
- Pass-4 invariant: DNA
  `uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`; `.happ`
  `d74e5f2f272ab6da7e0e429da2f5419cd7d74f364055c238378decf02a681861`
  (unchanged by this feature).
