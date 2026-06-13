# Note-to-Self — observability & order-of-operations

**Purpose.** A step-by-step map of every local / conductor / network /
happ touchpoint for each note-to-self flow, so humm-tauri can audit its
existing logging for **gaps** and confirm correct order-of-operations.
humm-tauri already logs robustly; this doc exists to surface the
touchpoints that are *easy to miss* — especially the ones where a step
silently no-ops, fans out to nobody, or returns `[]` during gossip lag.

**Security note.** Touchpoints marked **🔒 SECURITY** are also trust
checkpoints — a missing log there is also a missing defensive check.
See `HUMM_TAURI_SELF_NOTES_INTEGRATION.md` §"Security footguns &
landmines" (L1–L9) for the full threat model; this doc references those
IDs.

**Companion docs:** `HUMM_TAURI_SELF_NOTES_INTEGRATION.md` (architecture
+ wire shapes), `HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md` (validator
guarantees).

---

## 0. Layers & vocabulary

Every step below is tagged with the layer it happens on:

| Tag | Layer | Where it's observable |
|---|---|---|
| `[CLIENT]` | humm-tauri TS (store/api/crypto) | your app logs |
| `[callZome]` | AppWebsocket request/response | client RPC logs; errors surface as `ConductorApiError` |
| `[ZOME]` | coordinator extern body | conductor `info!/debug!` (WASM logs) |
| `[VALIDATE]` | integrity validator at commit | only visible as the **rejection** on `[callZome]` (an `Err`); success is silent |
| `[DHT]` | publish / gossip / get | conductor networking logs; **eventually-consistent** |
| `[post_commit]` | `post_commit` hook (after the call) | conductor `warn!/error!`; emits generic `Signal::*` |
| `[SIGNAL→UI]` | `emit_signal` to the local app | your signal handler |
| `[SIGNAL→NET]` | `send_remote_signal` to peers | `remote_signal_acl_readers` `info!`; best-effort |
| `[recv]` | `recv_remote_signal` on a peer | conductor `info!` on the **receiving** device |

### Universal invariants (true of EVERY write)

1. **One extern call = one atomic source-chain transaction.** Every
   `create_entry` + `create_link` inside a single extern commits
   together or not at all. `create_encrypted_content` either lands the
   entry **and** all its links, or fails wholesale. There is no
   "entry committed but links missing" within one call.
2. **Validation runs in YOUR conductor.** A malformed wire shape is
   rejected by your own node at commit — you do **not** need a remote
   peer to catch it. The rejection is the `ConductorApiError` string
   (e.g. `…Validation failed while committing: <validator msg>`).
   Remote DHT authorities re-run the same deterministic validators
   during gossip; that outcome is only in *their* logs.
3. **Two signal families per write.** (a) the app-level
   `EncryptedContentSignal` emitted *inline* by the extern, and (b) a
   generic `Signal::EntryCreated` / `Signal::LinkCreated` emitted by
   `post_commit` for **every** committed action (the entry + each
   link). If your handler subscribes to only one family it will miss
   events from the other.
4. **A signal is a HINT, never authoritative** (🔒 L7). The
   `recv_remote_signal` threat-model comment is explicit: the payload
   body is attacker-controlled; arrival does not prove the entry exists.
   The legitimate reaction is "wake up and re-query the DHT for this
   hash", then verify authorship.
5. **Remote delivery is best-effort.** `send_remote_signal` only reaches
   **online** peers in `public_key_acl.reader` (minus self). Offline
   devices, and devices linked *after* the write, get nothing by push —
   they must **pull** (`list_by_author` / `list_by_dynamic_link`).
6. **Reads are eventually consistent.** `get_*` / `list_*` hit the
   network and can return stale or `[]` immediately after a write on a
   different device. Distinguish "absent" from "not yet propagated"
   before acting.

### Existing log lines worth grepping

- `remote_signal_acl_readers: raw_count={} valid_recipients={} action_type={…}` — **`info!`**, the single most useful breadcrumb (see §Gap-1).
- `recv_remote_signal[EncryptedContentSignal]: action_type={…} hash={…} from_agent={…}` — **`info!`** on the receiver.
- `recv_remote_signal: payload did not decode …` — **`Err`**, malformed/misrouted inbound signal.
- `get_messages_since: querying chain seq_range=[…]` — **`debug!`**.
- `signal_entry_created: get_entry_for_action failed; signal skipped` (and `_updated`/`_deleted`/`_link_*` siblings) — **`warn!`** in `post_commit`; the `Ok(None)` skip arms are **silent** (see §Gap-2).
- `Error signaling new action: {…}` — **`error!`** from `post_commit`.

---

## 1. Flow A — First-time bootstrap (`ensureSelfNotesReady`)

Branch: **first-time** (no cache) vs **resume** (cache hit → skip to
Flow B/C). v1 is **same-hive** only; the cross-hive branch is N/A
(deferred — integration doc §3.5).

```
A1 [CLIENT]   read cache → device_set_genesis_hash?      ── hit ⇒ DONE (resume)
A2 [CLIENT]   pick/own a personal hive H
A3 [callZome] list_my_hives()                            ── do I own/belong to H?
A4 [ZOME/DHT] get_links on my pubkey (HiveInvite)        ── eventually consistent
   ── branch: no personal hive ⇒
A3b[callZome] create_hive_genesis({ display_id })
A3c[VALIDATE] permissionless ⇒ Valid (always)
A3d[post_commit] Signal::EntryCreated (HiveGenesis) + LinkCreated (HiveInvite self-link)
A5 [callZome] get_latest_membership({ agent: me, hive_genesis_hash: H })
              ── null ⇒ I am hive author (author_membership_hash = null)
              ── else ⇒ author_membership_hash = membership.hash (MUST grant Writer+)
A6 [callZome] list_my_groups() → filter display_id == "device-set-v1"
A6🔒[CLIENT]  among matches, keep ONLY the one whose genesis author == me
              (🔒 L6 — display_id is forgeable; identity is the action hash)
   ── branch: none mine ⇒
A7 [callZome] create_group_genesis({ hive_genesis_hash: H,
                 display_id: "device-set-v1", hive_wide_role: null,
                 creator_hive_membership_hash: author_membership_hash ?? null })
A7a[VALIDATE] requires hive Admin+ (group.rs:253-269)
A7b[post_commit] Signal::EntryCreated (GroupGenesis)
                 + LinkCreated (HiveToGroups) + LinkCreated (GroupInvite self)
A8 [CLIENT]   cache { device_set_genesis_hash = create hash, author_membership_hash }
```

**OBSERVE**
- Log the **branch taken** at A1 (resume) and A6/A7 (reuse vs create).
  A silent "create" on every launch = a `findMyDeviceSet` bug spawning
  duplicate device-sets.
- A6 returns the GroupGenesis from links — eventually consistent; on a
  just-created hive it can be empty for a beat. Log A4/A6 result sizes.
- 🔒 **Gap candidate:** if A6🔒 (genesis-author == me filter) is not
  logged/enforced, a maliciously-granted "device-set-v1" group can be
  selected → notes written under an attacker-owned group (L6).

---

## 2. Flow B — Single-device note write

```
B1 [CLIENT]    K = random symmetric key; nonce_c = random
B2 [CLIENT]    bytes = AEAD_encrypt(noteJSON, K, nonce_c)       (🔒 L5)
B3 [callZome]  create_encrypted_content(<single-device shape, §5.1>)
   ── inside the extern, in THIS order (crud.rs:43-113):
B3a[ZOME]        create_entry(EncryptedContent)
B3b[VALIDATE]    validate_hivegroup_acl → Path A owner, empty PKA, no witnesses ⇒ Valid
B3c[SIGNAL→UI]   emit_signal(EncryptedContentSignal::Create)   ← author's own UI
B3d[SIGNAL→NET]  remote_signal_acl_readers(empty reader)       ← raw_count=0, NO send
B3e[ZOME]        create_link(OriginalHashPointer self)
B3f[ZOME]        create_link(Hive, author-path [me, content_type])
B3g[ZOME]        create_hive_link + create_humm_content_id_link
                 (+ create_dynamic_links if dynamic_links present)   ← hive_context present
B3h[ZOME]        create_acl_links (HiveGroup has group_acl)
B3 [callZome]  → Ok({ hash, … })  (atomic: all of B3a-B3h or nothing)
B4 [post_commit] Signal::EntryCreated(EncryptedContent)
                 + one Signal::LinkCreated PER link in B3e-B3h
B5 [CLIENT]    K_self = ECDH(my_x25519_priv, my_x25519_pub); nonce_s = random  (🔒 L5)
B6 [callZome]  create_encrypted_content(SharedSecret wrapping K → me)  ← SEPARATE call
B6a[VALIDATE]    its own validation + B3-style link bundle + post_commit
B7 [CLIENT]    link the SharedSecret to the note (existing SharedSecretApi linking)
```

**OBSERVE**
- B3d logs `remote_signal_acl_readers: raw_count=0 valid_recipients=0` —
  **expected** for single-device (no other devices). Treat `raw_count>0`
  here as a bug (you should not be listing readers on a single-device
  note).
- B6 is a **separate transaction** from B3. If B3 succeeds and B6 fails,
  you have a **committed-but-unreadable note** (🔒 L8 data-integrity
  trap). Log the pair as one logical unit; alert if the note commit
  lands but the self-wrap does not.
- B4 fires **many** signals (entry + ~4-6 links). If your UI counts
  "notes created" off generic `Signal::EntryCreated`, make sure you
  filter by entry type / content_type — link signals and SharedSecret
  entry signals will otherwise inflate counts.
- 🔒 B1/B5: log *that* a fresh K and fresh nonce were generated (not the
  values). Reused nonce or a static key is L5 — invisible unless you
  assert freshness in tests.

---

## 3. Flow C — Single-device read / resume / re-install

```
C1 [CLIENT]    discover notes:
               list_by_dynamic_link({ …, dynamic_link: "self-notes" })  ← preferred (bounded by path)
               or list_by_author({ author: me, content_type: "humm-self-note-v1" })  ← UNBOUNDED (Gap-3)
C1a[ZOME/DHT]  get_links on the path → resolve each entry  ← eventually consistent; may be partial
C2 [CLIENT]    for each note: fetch its SharedSecret entries
C3 [CLIENT]    K = AEAD_decrypt(sharedSecret, ECDH(my_priv, my_pub))
C4 [CLIENT]    note = AEAD_decrypt(bytes, K)
```

**Branch: re-install (same agent key).** Identical to above — the
self-wrap (`ECDH(my_priv,my_pub)`) is deterministic, so a re-installed
app with the **same lair keypair** re-derives K_self and reads every
note. **If the keypair is lost, the notes are unrecoverable** (🔒 L8 —
data loss, not a breach).

**OBSERVE**
- Log C1 result count and whether it grew vs the cached count. A read
  that returns fewer than expected right after a write on another device
  is gossip lag, not loss (invariant 6) — log a "retry/poll" path,
  don't surface "notes deleted".
- 🔒 **Gap candidate (Gap-3):** `list_by_author` is **unbounded**
  (`queries.rs:261-268`, no `since_ts`/`limit`). For a heavy notes user
  this returns everything every sweep — latency + memory. Prefer the
  dynamic-link path; if you must use `list_by_author`, cap client-side
  and log the returned size.

---

## 4. Flow D — Device-linking ceremony

Branch: **existing device (A)** and **new device (B)** run different
halves. This is the **trust root of the whole feature** (🔒 L4).

### New device B (initiator)
```
D1 [CLIENT,B]  generate own agent keypair (own source chain)
D2 [CLIENT,B]  generate ephemeral X25519 keypair + link_nonce
D3 [CLIENT,B]  display QR { new_device_pubkey: B, ephemeral_x25519_pub, link_nonce }
```

### Existing device A (authorizer)
```
D4 [CLIENT,A]  scan QR
D4🔒[CLIENT,A] show user device-pubkey B + SAS=hash(B || ephemeral_x25519 || link_nonce); require explicit approve
               (🔒 L4 — without SAS / user approval a MITM-substituted QR = account takeover)
D5 [callZome,A] create_group_membership({ group_genesis_hash: device_set,
                  for_agent: B, role: "Admin",            ← (🔒 L4 tradeoff: Admin lets B link more devices)
                  grantor_membership_hash: A_is_author ? null : A_membership_hash,
                  grantor_hive_membership_hash: null, expiry: null })
D5a[VALIDATE]   Rule 1 non-self (A≠B) ✓; Rule 2 A holds Owner/Admin ✓ (group.rs:318-336)
D5b[post_commit] Signal::EntryCreated(GroupMembership)
                 + LinkCreated(AgentToGroupMemberships base=B)
                 + LinkCreated(GroupToGroupMemberships base=device_set, tag=B)
                 + LinkCreated(GroupInvite → B)
D6 [CLIENT,A]  link_bundle = { device_set_genesis_hash, hive_genesis_hash,
                  deviceB_membership_hash, other_device_pubkeys, author_membership_hash }
D7 [CLIENT,A]  send AEAD_encrypt(link_bundle, ephemeral_x25519_pub, fresh nonce) to B  (🔒 L5)
D8 [CLIENT,A]  backfillForNewDevice(B_PERMANENT_x25519_pub)  → Flow F   ← 🔒 permanent key, NOT the ephemeral one (L8 / §8)
```

### New device B (completion)
```
D9 [CLIENT,B]  decrypt link_bundle; cache device_set + memberships
D10[CLIENT,B]  ready to read (via backfilled SharedSecrets) + write
```

**OBSERVE**
- 🔒 **Gap candidate (the big one, L4):** log D4🔒 — the user-approval +
  SAS step. If the implementation grants D5 *before* a verified user
  approval, an attacker who relays/substitutes the QR gets a device-set
  Admin membership and (via D8 backfill) every past note key.
- 🔒 D5 grants **Admin** → B can grant further devices. If you don't
  need that resilience, grant **Reader** and keep linking on the primary
  only (smaller blast radius). Log the role granted.
- D5b emits 3 links; `list_group_members` (the roster) reads
  `GroupToGroupMemberships`. Roster is a **cache** — re-derive from the
  membership entries before trusting (BDD doc J-1/K-1).
- D8 backfill volume scales with note count — log how many SharedSecrets
  were written; an unbounded backfill is a source-chain bloat / DoS
  surface (cap it).

---

## 5. Flow E — Multi-device note write + fan-out

Same as Flow B, but with the multi-device shape (§5.2):

```
E1 [CLIENT]    set = list_group_members(device_set) → other devices' pubkeys  (re-derive, don't trust links)
E2 [CLIENT]    K, bytes as B1-B2
E3 [callZome]  create_encrypted_content(<multi-device shape>:
                 reader = [B, C], recipient_witnesses = [{B,Reader,mB},{C,Reader,mC}])
E3a[VALIDATE]    bidirectional witness check + per-witness fetch ⇒ Valid (BDD E-2)
E3b[SIGNAL→NET]  remote_signal_acl_readers(reader=[B,C])  → raw_count=2 valid_recipients=2
E3c[DHT]         send_remote_signal → B, C (online only; best-effort)
E4 [CLIENT]    for each device in {me, B, C}: write SharedSecret wrapping K → that device  (separate calls)
```

**OBSERVE**
- 🔒 **Gap candidate (Gap-1, the canary):** watch
  `remote_signal_acl_readers: raw_count=N valid_recipients=M`. If
  **raw>0 but valid=0**, every recipient pubkey failed to decode — the
  exact historical silent-drop bug (STANDARD vs URL_SAFE base64). If
  **raw>valid by 1**, you (correctly) listed/own self and it was
  filtered — but if you did **not** intend to list self, that gap means
  a wire-shape bug (🔒 L3: you may be publishing your own pubkey
  needlessly). Surface this gap as an alert, not just a log line.
- E4 writes N SharedSecrets in N separate transactions. Partial failure
  ⇒ some devices can't read this note. Log the set as one unit.
- 🔒 Every device pubkey in `reader` + every witness pubkey + the
  device-set membership links are **plaintext on the DHT** (L3). There
  is no log to add here — it's a design property to surface in your
  privacy docs, not a bug.

---

## 6. Flow F — New-device first-time sync (backfill + discovery)

```
F1 [CLIENT,B]  (from D9) have device_set + own membership
F2 [callZome,B] list_by_dynamic_link / list_by_author → discover notes  ← PULL (push won't reach a new device)
F3 [CLIENT,B]  for each note: find SharedSecret wrapped to B (written by A at D8)
F4 [CLIENT,B]  K = AEAD_decrypt(thatSharedSecret, B_x25519_priv); note = decrypt(bytes, K)
```

**OBSERVE**
- 🔒 **Gap candidate (Gap-4):** a freshly-linked device that only listens
  for signals and never **pulls** (F2) will never see notes written
  before it was linked — `send_remote_signal` fired before B existed in
  any reader set. The pull sweep is mandatory on first run after
  linking. Log the first-run pull explicitly.
- F3 depends on A's backfill (D8) having propagated — eventually
  consistent. A note with no decryptable SharedSecret for B yet is "not
  propagated", not "corrupt"; retry.

---

## 7. Flow G — Live receive on a linked device

```
G1 [recv,B]    recv_remote_signal[EncryptedContentSignal]: action_type=Create hash=… from_agent=A
G2 [CLIENT,B]  🔒 verify from_agent ∈ my device-set roster (validated GroupMembership)  (🔒 L7)
G3 [callZome,B] get_encrypted_content(hash)   ← re-fetch; NEVER render the signal body
G4 [CLIENT,B]  decrypt via SharedSecret as Flow F
```

**OBSERVE**
- 🔒 **Gap candidate (Gap-5, L7):** the receiver logs the signal (G1
  already exists at `info!`), but if it renders the signal's
  `encrypted_content` body **without** G2 (author ∈ device-set) and G3
  (re-fetch), any peer can spoof a "self-note" into the user's notes
  view. The re-fetch + authorship check is the defense; log both.
- G1's `from_agent` is conductor-attested (anti-spoof) — trust it as the
  *sender identity*, but still verify that identity is one of your
  devices (G2). A real but **foreign** agent can still send you a
  signal; only your own device pubkeys should populate the notes thread.
- 🔒 **Gap candidate (Gap-6, L7 rate-limit):** the open cap grant lets a
  peer flood `[recv]`; re-fetching (G3) on every signal turns that into a
  `get_*` flood. Budget the follow-up fetch per `from_agent` (e.g. ≤5/s),
  debounce duplicate hashes, and drop the signal on a `None` re-fetch.
- 🔒 **Gap candidate (Gap-7, L9 delete authority):** if this note used
  the PKA-listed shape, device B (and every PKA-listed device) can
  **delete** it (`encrypted_content.rs:850-858`). Log delete-authority
  origin; prefer the empty-PKA + SharedSecret-only variant so only the
  author can hard-delete.

---

## 8. Branch matrix (quick reference)

| Dimension | Branch | Key difference in touchpoints |
|---|---|---|
| Setup | first-time | Flow A creates hive (maybe) + device-set; log create-vs-reuse |
| Setup | resume | cache hit at A1 → straight to B/C |
| Devices | single | reader empty; `raw_count=0`; **no** `[SIGNAL→NET]`; **no** `[recv]` anywhere; self-wrap only |
| Devices | multi | reader populated ⇒ those devices also get **delete authority** (L9); fan-out; per-device SharedSecret; `[recv]` on other devices. Safer default: empty-PKA + SharedSecret-only fan-out |
| Device age | pre-existing | gets live signal (Flow G) |
| Device age | newly linked | gets nothing by push; **must** pull (Flow F) + consume backfill |
| Hive | same-hive (v1) | the only supported case; device-set + note share `hive_genesis_hash` |
| Hive | cross-hive | **N/A in v1** (validator rejects mismatch, BDD E-9); deferred |

---

## 9. Commonly-missed touchpoints — audit checklist

Tick each against your current logging/handlers:

- [ ] **Gap-1 (🔒 L3):** alert on `remote_signal_acl_readers raw_count >
      valid_recipients` (pubkey decode failure or unintended self-list).
- [ ] **Gap-2:** `post_commit` `Ok(None)` arms drop generic signals
      **silently** during gossip lag (`signal_entry_created` &
      siblings). Your UI must not depend on the generic `Signal::*`
      family for correctness — it's best-effort. The inline
      `EncryptedContentSignal` + the pull sweep are the reliable paths.
- [ ] **Gap-3:** `list_by_author` is unbounded — cap client-side, prefer
      the dynamic-link path, log returned size.
- [ ] **Gap-4 (🔒 L4-adjacent):** a newly-linked device MUST pull on
      first run; signals won't backfill it.
- [ ] **Gap-5 (🔒 L7):** on receiving any `humm-self-note-v1` signal,
      verify `from_agent ∈ device-set` AND re-fetch the entry before
      rendering.
- [ ] **Self-wrap pairing:** the note write (B3) and the self-wrap
      SharedSecret write (B6) are separate transactions — track them as
      one unit; a committed note with no self-wrap is unreadable (🔒 L8).
- [ ] **Two signal families:** confirm your handler distinguishes the
      inline `EncryptedContentSignal` from `post_commit`'s generic
      `Signal::EntryCreated/LinkCreated`, and filters by content_type.
- [ ] **🔒 L6 device-set selection:** log + enforce "genesis author ==
      me" when picking the `device-set-v1` group.
- [ ] **🔒 L4 link approval:** log the explicit user-approval + SAS step
      before granting a device membership — and confirm the SAS is over
      the **whole** QR payload `hash(B || ephemeral || nonce)`, not the
      ephemeral key alone (substitution attack otherwise).
- [ ] **🔒 L9 multi-device delete:** if any note lists devices in
      `public_key_acl.reader`, every such device can hard-delete it;
      prefer empty-PKA + SharedSecret-only fan-out, or app-level
      soft-delete.
- [ ] **🔒 F-D backfill key:** confirm backfill wraps `K` to each new
      device's **permanent keystore encryption key** — an independent
      SLIP-0010 key obtained from the link bundle / the device's
      `humm-dm-keybinding-v1`, **NOT** derived from its AgentPubKey —
      never the one-time ephemeral handshake key (silent unreadable-note
      failure otherwise).
- [ ] **🔒 F-B revocation cascade:** after revoking a device, enumerate
      the roster and individually expire any Admin grants that device
      issued — revocation does not cascade.
- [ ] **Validation rejections:** the `ConductorApiError` string carries
      the exact validator message (BDD doc) — log it verbatim; it is the
      single best signal that a wire shape is wrong.
- [ ] **Eventual consistency:** every `list_*`/`get_*` empty result
      right after a remote write should route to "poll/retry", logged
      distinctly from "confirmed absent".
