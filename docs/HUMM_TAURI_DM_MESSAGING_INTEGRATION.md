# Direct Messaging (in-hive + cross-hive) — humm-tauri integration handoff

**Status:** spec + wire-shape + link + BDD + observability + security.
**Core happ change:** NONE. DMs run on the **existing pass-4 DNA**
(`uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`). The only DM
adaptations are **client-side wire-shape conformance** to the pass-3/4
`validate_directmessage_acl` rules (humm-tauri has applied them).
**Audience:** humm-tauri engineers wiring 1:1 / small-group DMs and the
`humm://` DM deep link.
**Companion docs:**
- `HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md` §F — the DM validator
  guarantees (Given/When/Then) this composes.
- `HUMM_TAURI_SELF_NOTES_INTEGRATION.md` / `..._OBSERVABILITY.md` —
  **shared** primitives: SharedSecret per-message key discipline, the
  `Signal::EntryCreated` streaming + `list_by_author` hydration split,
  the "signal is a hint, re-fetch" rule, eventual-consistency handling,
  and the I-A delete rule. This doc references them rather than repeating.
- `HUMM_TAURI_ACLSPEC_INTEGRATION.md` — the canonical `AclSpec` wire model.

---

## 1. TL;DR

- A DM is an `EncryptedContent` entry with `acl_spec:
  AclSpec::DirectMessage { recipients }`. **DMs are hive-less** — the
  variant carries no `hive_genesis_hash` (`encrypted_content.rs:294` ⇒
  `None`), which is *exactly* what makes them cross-hive.
- **In-hive vs cross-hive differ only in one place: where the
  recipient's X25519 encryption key comes from.** In-hive ⇒ resolved
  from the recipient's `Member` entry (DHT, identity-bound). Cross-hive
  ⇒ carried inline in the `humm://` link's **`enc`** param
  (unauthenticated — see §9 DM-L1). Everything else (validator, read
  path, signal path) is identical.
- **Read path is `list_by_author`** (`queries.rs:268`), never
  `list_by_hive_link` (DMs have no hive link). A thread is two
  `list_by_author` queries (me + peer) merged client-side.
- The **`humm://` DM link** (`humm://sidecars/direct-messages?recipient=…&enc=…`)
  is **stable and forward-compatible**; what changed across pass-2→4 is
  the *wire shape the client builds from it* (§8 sanity check).

---

## 2. The `humm://` DM deep link

### 2.1 Canonical format

```
humm://sidecars/direct-messages?recipient=<AGENT_PUBKEY>&enc=<X25519_PUBKEY>[&alias=<NAME>]
```

- `kind` segment = `sidecars`, `target` = `direct-messages` (plural,
  canonical). Legacy singular `humm://sidecar/direct-message?…` is still
  parsed for back-compat; the client **emits** the plural form.
- Builder + parser + validation: humm-tauri
  `src/lib/humm-uri/index.ts` (`buildHummUri` / `parseHummUri`).
- `alias` is an optional display-name hint; **not** load-bearing, never
  trusted for identity.

### 2.2 The two params are TWO DIFFERENT KEYS in two different alphabets

This is the crux and the most common source of confusion:

| Param | Meaning | Bytes | Encoding | Flows into |
|---|---|---|---|---|
| `recipient` | recipient **AgentPubKey** (`uhCAk…` holohash) | 39 | **URL-SAFE** base64 (multibase `u` + `-_`), from `@holochain/client encodeHashToBase64` | `AclSpec::DirectMessage.recipients` + `public_key_acl.reader` (decoded to 39-byte AgentPubKey) |
| `enc` | recipient **X25519 encryption pubkey** | 32 | **STANDARD** base64 (`A-Za-z0-9+/=`, 44 chars incl. one `=`), then percent-encoded (`+`→`%2B`, `/`→`%2F`, `=`→`%3D`) | the pair `SharedSecret` ECDH, **app-layer only** (validator never sees it) |

The parser enforces the alphabet split on purpose: `enc` is matched by
`STANDARD_B64_X25519_RE = /^[A-Za-z0-9+/]{43}=$/` and asserted to decode
to exactly 32 bytes; url-safe input for `enc` is **rejected**
(`HummUriInvalidEncError`). So the two params are visibly distinguishable
(url-safe holohash vs standard-b64 X25519) — a useful sanity check when
eyeballing a link.

### 2.3 Worked example

```
humm://sidecars/direct-messages?recipient=uhCAk5VvZNZNpF8G8O_LoIekdMdPEHCdr_ey_ZA7JJTCjj7UKvL4f&enc=U1Hw%2B5nxyuMVfEWYSsl3kMmfIDgKoA0BqJAL1BR6VRY%3D
```
- `recipient` → 39-byte AgentPubKey (prefix `hCAk` = AgentPubKey).
- `enc` (%-decoded → `U1Hw+5nxyuMVfEWYSsl3kMmfIDgKoA0BqJAL1BR6VRY=`) →
  exactly 32 bytes = X25519 pubkey.

---

## 3. Why `enc` exists — the cross-hive path

A DM recipient's *signing* identity (AgentPubKey) is not their
*encryption* key. To seal a message you need their **X25519** key.

- **In-hive:** sender and recipient already share a hive, so the
  recipient's X25519 is discoverable on-DHT from their `Member` entry.
  The client **resolves it there and ignores/skips `enc`**. The key is
  identity-bound (the `Member` entry is authored on-chain).
- **Cross-hive:** no shared hive ⇒ no gossiped `Member` entry to look up
  ⇒ the client uses the link's inline **`enc`**:
  `shared_secret = X25519(my_x25519_priv, enc)`. The DM body is sealed to
  that SharedSecret and committed as `AclSpec::DirectMessage` with
  `recipients = [me_agent, recipient_agent]` and
  `public_key_acl.reader = [me_agent, recipient_agent]`.

**Security consequence:** in-hive key binding is DHT/identity-backed;
cross-hive key binding rests entirely on the link's **unauthenticated**
`enc` (§9 DM-L1). Same message body, very different trust posture.

---

## 4. DM wire shape (what commits)

```ts
create_encrypted_content({
  id, display_hive_id,                              // display only; DMs are hive-less
  content_type: "humm-sidecar-direct-message-v1",
  revision_author_signing_public_key: myAgentB64,  // == action.author
  bytes: sealedToPairSharedSecret,                 // app-layer ciphertext
  acl_spec: { DirectMessage: { recipients: [me_agent, recipient_agent] } },
  public_key_acl: {
    owner: "", admin: [], writer: [],              // MUST be empty (pass-4)
    reader: [me_agentB64, recipient_agentB64],     // MUST set-equal recipients
  },
})
```

Validator: `validate_directmessage_acl` (`encrypted_content.rs:726-784`).
The guaranteed-pass invariants (full BDD in sanity-checks §F):
`2 ≤ recipients.len() ≤ 32`; author ∈ recipients; no duplicate
recipients; `public_key_acl.reader` set-equals `recipients`;
`owner/admin/writer` empty. Group DMs: same shape with up to 32
recipients (all in both `recipients` and `reader`).

> **Group DM (N>2) needs a multi-recipient key scheme — see §9 DM-L8.**
> The pairwise SharedSecret is two-party only. A group DM MUST generate a
> random per-message key `K`, encrypt the body once under `K`, and seal
> `K` separately to each recipient's X25519. Sealing only to
> `recipients[0]` commits fine (the validator checks routing, not
> decryptability) but silently locks every other recipient out.

---

## 5. In-hive vs cross-hive — the only differences

| Aspect | In-hive DM | Cross-hive DM |
|---|---|---|
| Validator / wire shape | identical (`AclSpec::DirectMessage`) | identical |
| Recipient X25519 source | recipient's `Member` entry (DHT, authored) | link `enc` param (inline, unauthenticated) |
| Link `enc` needed? | no (resolved on-DHT) | **yes** (only key source) |
| Discovery of peer's outbound | `list_by_author(peer)` (+ optional in-hive `fetch_pair_ss_with_hive_check` intersection, `queries.rs:365`) | `list_by_author(peer)` only |
| Live delivery | `send_remote_signal` to `reader` (best-effort, online-only) | same |
| Trust binding of recipient key | identity-bound | TOFU at best — verify on first shared hive |

Everything else — commit, validation, read, signal, delete authority —
is byte-identical. There is **no separate "cross-hive DM" code path** in
the happ; cross-hive is just "DM where `enc` supplied the key."

---

## 6. Read / write paths

### Write
Pre-build the `AclSpec::DirectMessage` override (do **not** synthesize a
HiveGenesis for the peer — DMs are intentionally hive-less; a synthetic
hive would flip the entry to the wrong validator branch + wrong links +
wrong inbox routing). `create_encrypted_content` (`crud.rs:28`) commits
the entry, fires the local `EncryptedContentSignal`, fans
`send_remote_signal` to `reader` minus self (`signals.rs:180`), and
writes the author-shape `Hive` link (path `[author, content_type]`). No
hive-shape link is written (no hive context).

### Read (thread hydration)
```ts
const mine = await list_by_author({ author: me,   content_type: "humm-sidecar-direct-message-v1" });
const peer = await list_by_author({ author: peer, content_type: "humm-sidecar-direct-message-v1" });
const thread = merge(mine, peer)
  .filter(e => recipientsInclude(e, me) && recipientsInclude(e, peer)) // narrow to OUR pair
  .sort(byActionTimestamp);
```
- `list_by_author` (`queries.rs:268`) is the **canonical** path.
  `list_by_hive_link` returns empty for DMs **by construction**, not a
  bug.
- The `recipients` filter is **required**: the peer's chain may hold DMs
  with other counterparties; keep only entries whose recipients include
  both you and the peer.
- **Streaming vs hydration (shared pattern):** consume
  `Signal::EntryCreated` for the DM content_type into a thread cache for
  live updates; the two-query merge is the cold-start/hydration path.
  Don't conflate them. (Same split as self-notes — see
  `..._OBSERVABILITY.md`.)
- `list_by_author` is currently **unbounded** (no `since_ts`/`limit`,
  `queries.rs:261-265`); cap client-side for long threads.

### First contact — the Accept/Block gate (current behavior + recommended hardening)

A DM from an **unknown** sender is gated in the UI behind **[Accept] /
[Block]**. That gate is the natural place to do key exchange *before*
real content is exposed — the strongest mitigation for DM-L1 (don't seal
real content to an unverified link `enc`). It maps Signal's three
first-contact mechanisms onto humm: the **message-request** consent gate,
**X3DH** async key agreement, and **safety-number** verification
(`.extraResearch/SIGNAL_MULTI_DEVICE_RESEARCH.md` — X3DH ref + open
question 5).

**Current behavior (as built).** The client sends the **real first
message together with** the request — i.e. the message shown behind
[Accept]/[Block] *is* the first real DM. Security implication: that real
message is sealed under the link's `enc` (cross-hive: **unverified**) and
committed to the **public DHT before** the recipient accepts or verifies
anything. So under a forged / low-order / stale `enc` (DM-L1, F-1, key
rotation) the **real content of message #1 is already exposed or
mis-sealed** by the time the gate is shown. The gate protects the
recipient from spam, but not the sender's first message from a bad `enc`.

**Recommended hardening — stub first, real content on acceptance.** Split
first contact into a content-free request stub plus a deferred real
message:

```
1. [sender]    DO NOT send real content yet. Commit a REQUEST STUB:
               AclSpec::DirectMessage { recipients:[me, recipient] },
               content_type: "humm-sidecar-dm-request-v1",
               bytes = sender's SIGNED key-binding (AgentPubKey || X25519,
                       signed by sender) + optional NON-secret greeting.
               (authenticated by Holochain authorship: action.author == sender;
                a low-order/forged enc leaks only a content-free handshake)
2. [recipient] UI shows "unknown sender wants to connect" → [Accept]/[Block]
   - Block  → receiver-side filter (+ optional local block record); sender not notified.
   - Accept → publish/confirm the recipient's OWN signed key-binding
              (content_type "humm-dm-keybinding-v1", DM-L1 item 7).
3. [sender]    observe acceptance, fetch + verify the recipient's signed
               key-binding, THEN seal the real first message under the
               VERIFIED X25519 (not the raw link enc) and send it.
```

**Why it's worth doing (security delta).**

| Under a forged/low-order/stale `enc` | Current (msg sent with request) | Stub-first |
|---|---|---|
| What lands on the DHT before verification | the **real** first message, mis-sealed | a **content-free** signed handshake |
| DM-L1 / F-1 blast radius for msg #1 | full message content | ~nothing |
| When real content is sealed | before any Accept / key check | after Accept + **verified** key-binding |
| Key used for real content | unverified link `enc` | recipient's signed, verified X25519 |

**Cost.** Client-side only — **no happ change**. Both the stub and the
real message are ordinary `AclSpec::DirectMessage` commits; the only new
pieces are a distinct `content_type` for the stub and gating the real
send on (acceptance ∧ verified key-binding). The win is concentrated on
the **cross-hive** path (in-hive, the recipient's X25519 already comes
from their identity-bound `Member` entry, so msg #1 is not sealed to an
unverified key in the first place). Recommended: adopt stub-first for
cross-hive (`enc`-supplied) first contacts at minimum.

---

## 7. Validator walk-through (cite, don't duplicate)

Full Given/When/Then with exact error substrings:
`HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md` §F (F-1..F-7) and §I (delete
authority). The load-bearing lines:
`encrypted_content.rs:731-736` (≥2), `:737-743` (≤32), `:744-748`
(author∈recipients), `:749-760` (no duplicate — the exact
`"DirectMessage recipients contains duplicate pubkey {r}"`), `:764-771`
(owner/admin/writer empty), `:772-782` (`reader` == recipients).

---

## 8. Sanity check — usability of previously-working DM links

**The link format is stable and forward-compatible.** Both the canonical
plural and the legacy singular forms still parse; `recipient`/`enc`/
`alias` are unchanged. A link generated by an old client still opens.

**What can break is the wire shape the client BUILDS from the link**,
because pass-3 and pass-4 tightened `validate_directmessage_acl`. A link
"stops working" only if the consuming client emits a stale shape. The
three regressions to check:

| Stale shape (pre-pass-3/4) | Pass-4 result | BDD |
|---|---|---|
| `recipients = [recipient]` (omits self) | rejected: `recipients.len() = 1 … (must be >= 2)` **and** `author … is not in recipients` | F-3, F-5 |
| `public_key_acl.reader` ≠ recipients | rejected: `public_key_acl.reader … does not match recipients` | F-6 |
| non-empty `owner/admin/writer` on the DM | rejected: `owner/admin/writer must be empty` | F-7 |

humm-tauri has already fixed the client to the conformant shape
(`recipients = [me, recipient]`, `reader = [me, recipient]`,
`owner='' admin=[] writer=[]`) — **no happ change**. The check for you:
ensure every DM-construction site (link-click path **and** reply path
**and** any test relay) builds exactly that shape. Assert it (§10 DM-2).

**Self-DM via link = note-to-self, and is correctly rejected.** A link
whose `recipient` is *your own* pubkey yields `recipients = [me, me]` →
rejected with `… contains duplicate pubkey …` (F-2). That is not a DM
bug; it is the note-to-self case — route it to the self-notes feature
(`HUMM_TAURI_SELF_NOTES_INTEGRATION.md`), do not try to send a self-DM.

**pass-2 `hive_id` is gone.** The pass-2 DM entry carried a display-only
`hive_id` that humm-tauri's old read path keyed on; pass-3's
`AclSpec::DirectMessage` removed it (it was validator-ignored). Any read
path still keying DM threads on a `hive_id` field must switch to the
`list_by_author` pair-merge (§6).

---

## 9. Security footguns & landmines (DM-specific)

DM shares the public-DHT crypto model with self-notes; the items below
are the DM-specific deltas. For the shared crypto discipline (per-message
random key, fresh nonce, never reuse a static key for content) see
`HUMM_TAURI_SELF_NOTES_INTEGRATION.md` §12 L5 — it applies verbatim to
the pair SharedSecret.

### DM-L1 — `enc` is unauthenticated and NOT bound to `recipient` (headline)
- **Risk:** the link carries `recipient` (who the DM is addressed to,
  on-chain) and `enc` (the X25519 key the body is sealed to) as
  **independent** values. Nothing cryptographically binds them. A forged
  link with `recipient = victim` but `enc = attacker_x25519` makes the
  sender commit a DM whose on-chain recipient is the victim
  (routing/delete) **but whose content only the attacker can decrypt**
  (`shared_secret = X25519(sender_priv, attacker_pub)`). The UI shows
  "DM to victim"; the plaintext goes to the attacker. Confused-deputy /
  key-substitution.
- **Why it's possible:** the X25519 key is a *separate* key from the
  AgentPubKey (that's why in-hive it must be looked up in the `Member`
  entry, and why cross-hive it must be carried inline). So `enc` cannot
  be re-derived from `recipient` and checked locally.
- **Do:**
  - **(Strongest — defer real content.)** For *first contact* use the §6
    Accept/Block contact-request handshake: send a content-free stub
    first and seal the real message only after acceptance + a *verified*
    key-binding. The numbered items below harden the per-message path the
    handshake builds on.
  1. **Prefer the on-DHT key.** Whenever sender and recipient share a
     hive, resolve X25519 from the recipient's `Member` entry and
     **ignore the link's `enc`** (or compare and warn on mismatch).
  2. **TOFU + pin.** On first cross-hive contact, pin the `(recipient,
     enc)` pair; warn loudly if a later link for the same `recipient`
     presents a different `enc`.
  3. **Verify before reuse.** Once a shared hive exists, reconcile the
     pinned `enc` against the `Member` entry's X25519; surface a
     "key changed / verify safety number" prompt on mismatch (Signal
     safety-number analogue).
  4. **Never auto-send.** The link should open a *compose* view showing
     the resolved/aliased recipient for explicit user confirmation, not
     fire a DM on open.
  5. **Reject unsafe `enc` (see DM-L6/F-1).** Before sealing, verify the
     ECDH output is not the all-zero value — a low-order `enc` makes the
     "shared secret" a public constant readable by *anyone*, not just a
     forger. Strictly worse than the substitution above.
  6. **Key rotation vs attack.** A recipient who rotates their X25519 must
     redistribute links; a stale link silently yields ciphertext they can
     no longer decrypt. Distinguish *legitimate rotation* from *attack* in
     the mismatch UI: if the new `enc` reconciles against the recipient's
     current `Member` entry on a shared hive, show "key updated —
     verified"; else "key mismatch — verify safety number".
  7. **(Protocol upgrade — closes the TOFU window.)** Recipients MAY
     publish a self-signed key-binding: `AclSpec::OpenWrite`, content_type
     `humm-dm-keybinding-v1`, `bytes = sign(recipient_signing_key,
     AgentPubKey || X25519_pubkey)`. A cross-hive sender fetches it via
     `list_by_author(recipient, "humm-dm-keybinding-v1")` and verifies the
     signature with the `recipient` AgentPubKey from the link — binding
     `enc` to `recipient` with no TOFU assumption or shared-hive
     prerequisite (the Signal signed-prekey analogue).

### DM-L2 — the DM link is unauthenticated routing+key data (unlike invite links)
- **Risk:** unlike `humm://` **invite** links (which carry an
  HMAC token over an on-chain invite action — see the URL-scheme
  hardening notes), the DM link has **no HMAC, no signature, no on-chain
  backing**. It is pure routing + key material. Opening it must trigger
  **no** side effects beyond populating a compose view.
- **Do:** apply the same URL-scheme handler hardening as invites:
  OS-level dispatch has no origin, so defend in the UI — required user
  gesture, no persistent state until the user acts, render the consent
  surface as an OS-level window (not a webview overlay) to resist
  clickjacking. A cross-origin page that fires `humm://…direct-messages`
  must at most open a compose view.
- **Do (alias is attacker-controlled):** `alias` MUST NOT be the primary
  identity signal. Show the resolved identity as primary — the recipient's
  `Member` display-name when in-hive (identity-bound), else the
  abbreviated AgentPubKey checksum. If `alias` is shown at all, mark it
  visually secondary and explicitly unverified ("name provided by link").
  A compose view that headlines `alias` (e.g. "DM to Alice Security Team")
  is a phishing vector — a forged link names itself anything while
  pointing at the attacker's own `recipient`+`enc` (no key mismatch to
  catch it).

### DM-L3 — DM ciphertext is world-readable on the DHT
- Same as self-notes L1: the entry `bytes` are public; only pair
  SharedSecret holders decrypt. The `recipients`/`reader` buckets gate
  commit + routing + **delete**, not reading. Never reason about secrecy
  from the ACL. **Body length is plaintext:** `len(bytes)` reveals
  `len(plaintext)` within the AEAD tag overhead. Pad DM bodies to a fixed
  block (e.g. next multiple of 256 bytes) **by default**, not opt-in — the
  overhead is bounded and modest; the alternative is per-message length
  fingerprinting for any DHT observer.

### DM-L4 — symmetric delete authority (by design, but note it)
- The I-A delete rule (`encrypted_content.rs:850-858`, BDD I-2) lets the
  original author **and** any pubkey in `public_key_acl` delete the
  entry. For a DM `reader = [me, peer]`, so **either party can delete
  any message in the thread** (their own or the counterparty's copy of
  the shared entry). This is the intended DM contract (both authored the
  conversation), but surface it: a deleted DM is a real DHT tombstone,
  not a local hide. Use app-level soft-delete if "delete for me only" is
  the desired UX.
- **Escalation for group DMs (N>2):** delete authority is N-lateral, not
  bilateral. With `reader = [A,B,…,N]`, *any* participant can permanently
  tombstone *any* message in the thread, including everyone else's. A
  32-member group DM grants 32 agents independent unilateral
  destroy-the-whole-history power; one compromised member's key inherits
  it. For conversations needing delete governance or audit integrity, use
  `AclSpec::HiveGroup` (explicit roles) instead of `DirectMessage`.

### DM-L5 — recipient metadata is plaintext
- `recipients` + `reader` (both parties' AgentPubKeys) and the
  `content_type` are plaintext in the entry header → the social graph
  "who DMs whom" is observable on the DHT (same class as self-notes L3).
  Document the limitation; there is no sealed-sender here.
- **`action.timestamp` is also plaintext.** Every Holochain signed action
  header carries a microsecond-precision send time, DHT-published and
  unforgeable. Combined with the plaintext `recipients`, any observer
  reconstructs a *timed* directed-interaction graph (who DMs whom, when,
  how often) independent of content. For higher-privacy needs apply
  app-layer send-time jitter (random delay before commit); it cannot be
  enforced at the integrity layer.

### DM-L6 — `enc` corruption must fail closed (and does)
- `%2B`→space corruption (a form-decoder mangling the link, or a
  copy/paste through something that turns `+` into space) makes `enc`
  un-decodable → `parseHummUri` throws `HummUriInvalidEncError` and the
  send is **refused**. This is the correct fail-closed behavior: better a
  blocked send than one sealed to a corrupted/wrong key. Keep the parser
  on manual `decodeURIComponent` (never a form/query decoder). Log the
  parse failure distinctly from "recipient unreachable."
- **Do (F-1 — the 32-byte check is necessary but NOT sufficient):** a
  well-formed 32-byte `enc` can be a Curve25519 **low-order point**. After
  X25519 scalar clamping the cofactor is cleared, so
  `X25519(any_priv, low_order_enc)` is a fixed value (often `[0u8;32]`) —
  the "shared secret" becomes a public constant and the ciphertext is
  recoverable by **anyone**, not just a forger (strictly worse than
  DM-L1). After ECDH and **before** the KDF/seal, abort if the output is
  all-zero (RFC 7748 §6.1). This belongs in the seal step, not the parser.

### DM-L7 — No forward secrecy (static pair SharedSecret)
- **Risk:** `SharedSecret = X25519(my_priv, peer_x25519)` is **static**
  for the life of both keypairs. Per-message `K`/nonce isolation
  (self-notes L5) stops one message leaking another's content key, but
  does **not** give forward secrecy: compromise of *either* party's X25519
  private key (device theft, lair breach, insecure backup, coercion) lets
  the attacker recompute the root SharedSecret and decrypt the **entire**
  past and future conversation straight off the public DHT (nonces +
  ciphertext are all there). There is no double-ratchet / ephemeral DH.
- **Do:** keep X25519 private keys in lair with **no plaintext export
  path**; on any suspected compromise treat all past DM content as exposed
  and rotate (DM-L1 item 6); tell high-threat users DMs give per-message
  isolation, not session forward secrecy.

### DM-L8 — Group DM (N>2) multi-recipient key scheme (or silent lockout)
- **Risk:** X25519 is two-party. For N>2 there is no single pairwise
  SharedSecret all members can derive. Sealing a group DM to
  `recipients[0]` only **commits successfully** (the validator checks
  routing, not decryptability) while silently excluding the other N−2
  members — a functional breach with no error.
- **Do:** generate a random per-message key `K`; encrypt the body once
  under `K` (AEAD, fresh nonce); seal `K` separately to each recipient via
  `X25519(my_priv, recipient_x25519_i)`; frame the body as the N key-wraps
  prepended to the ciphertext in a defined layout. Document the framing.
  Test that every one of N recipients independently decrypts (BDD DM-11).

---

## 10. BDD scenarios — DM end-to-end

Composes the validator guarantees (sanity-checks §F) at the binding /
tryorama layer; what you verify here is your **link parser** + **wire-
shape builder** + **thread hydration**. "commit succeeds" = extern
resolves `Ok`; "rejected with `<substr>`" = error contains `<substr>`.

### DM-1 — cross-hive send via link (happy)
- **Given** a valid link `recipient=peer&enc=<peer X25519>` and no shared
  hive
- **When** the user opens it, confirms compose, and sends
- **Then** `parseHummUri` yields `{ recipient, enc(32B) }`; the client
  seals the body to `X25519(my_priv, enc)` and commits
  `AclSpec::DirectMessage { recipients:[me,peer] }`,
  `reader:[me,peer]`, empty owner/admin/writer
- **And** commit succeeds; the peer can decrypt with their X25519 priv

### DM-2 — wire shape is pass-4-conformant (regression pin)
- **Given** any DM-construction site (link path, reply path, test relay)
- **When** it builds the DM input
- **Then** assert `recipients` includes self, `reader` set-equals
  `recipients`, and `owner/admin/writer` are empty (guards §8 regressions)

### DM-3 — in-hive send resolves X25519 from Member, ignores `enc` (happy)
- **Given** sender + recipient share hive `H`
- **When** the client builds the DM
- **Then** it resolves the recipient's X25519 from their `Member` entry
  (not the link), and (DM-L1) warns if a supplied `enc` disagrees

### DM-4 — link round-trips through build/parse (happy)
- **Given** `buildHummUri({ recipient, enc, alias })`
- **When** `parseHummUri` consumes the output
- **Then** `recipient` is url-safe holohash, `enc` is standard-b64 32B,
  and both decode to the original bytes; the legacy singular form also
  parses

### DM-5 — corrupted `enc` fails closed (expected failure)
- **Given** a link whose `%2B` was turned into a space (form-decoded)
- **When** `parseHummUri` runs
- **Then** it throws `HummUriInvalidEncError`; no DM is sent

### DM-6 — self-DM via link is rejected → route to note-to-self (expected failure)
- **Given** a link whose `recipient` is the user's own AgentPubKey
- **When** the client builds `recipients:[me,me]` and commits
- **Then** rejected with `DirectMessage recipients contains duplicate
  pubkey …` — detect this case at parse time and route to note-to-self
  instead (cross-ref self-notes)

### DM-7 — `enc`/`recipient` mismatch is detectable, not silent (security)
- **Given** a forged link `recipient=victim` + `enc=attacker_key`
- **When** the user later shares a hive with `victim`
- **Then** the client reconciles the pinned `enc` against `victim`'s
  `Member` X25519 and surfaces a key-mismatch warning (DM-L1)

### DM-8 — thread hydration merges both chains, filtered to the pair (happy)
- **Given** DMs authored by both me and peer (some to other counterparties)
- **When** hydrating via two `list_by_author` queries + merge
- **Then** only entries whose `recipients` include **both** me and peer
  appear, sorted by action timestamp; entries to other counterparties are
  excluded

### DM-9 — group DM up to 32 (happy) / over 32 (expected failure)
- **Given** N recipients all in `recipients` and `reader`
- **When** N ≤ 32 → commit succeeds; N > 32 → rejected with `exceeds
  DM_MAX_RECIPIENTS` (F-4)

### DM-10 — low-order `enc` is refused before commit (expected failure)
- **Given** a link whose `enc` is a Curve25519 low-order point (e.g. 32
  zero bytes)
- **When** the sender derives the SharedSecret and it equals `[0u8;32]`
- **Then** the seal step aborts with a distinct error **before** any
  network commit (DM-L6/F-1) — the DM is never written

### DM-11 — group DM: every recipient can decrypt (happy)
- **Given** a group DM with N=3 recipients sealed via the DM-L8
  multi-recipient key-wrap
- **When** each of the three independently opens the entry
- **Then** all three derive `K` from their own wrap and decrypt the body
- **And** sealing to only `recipients[0]` would commit identically but
  leave the other two unable to decrypt (the silent-lockout regression)

### DM-12 — first-contact stub handshake defers real content (recommended)
- **Given** an unknown cross-hive sender clicking a DM link
- **When** the client follows the §6 stub-first flow
- **Then** the first committed entry is a `humm-sidecar-dm-request-v1`
  stub carrying only the sender's signed key-binding (no secret content);
  the recipient sees [Accept]/[Block]; the real first message is committed
  only after Accept **and** the sender verifies the recipient's published
  key-binding — sealed under the verified X25519, never the raw link `enc`
- **And (contrast)** the current "send the real message with the request"
  path commits msg #1 under the unverified `enc` before any Accept (the
  exposure window DM-12 removes)

---

## 11. Observability — order-of-operations

Layer tags + invariants are defined in `..._OBSERVABILITY.md` §0
(`[CLIENT]/[callZome]/[ZOME]/[VALIDATE]/[DHT]/[post_commit]/[SIGNAL→UI]/
[SIGNAL→NET]/[recv]`). DM-specific flows:

### Send (from link click)
```
S1 [CLIENT]   parseHummUri(link) → { recipient, enc?, alias? }   (🔒 fail-closed on bad enc — DM-L6)
S2 [CLIENT]   resolve recipient X25519: in-hive → Member entry; cross-hive → enc  (🔒 DM-L1)
S3 [CLIENT]   open compose; REQUIRE explicit user send (🔒 DM-L2, no auto-send)
S4 [CLIENT]   K/SharedSecret = X25519(my_priv, recipient_x25519); seal body (fresh nonce — self-notes L5)
S5 [callZome] create_encrypted_content(DirectMessage [me,peer], reader [me,peer], empty o/a/w)
S5a[VALIDATE]   validate_directmessage_acl ⇒ Valid
S5b[SIGNAL→UI]  EncryptedContentSignal::Create (own UI)
S5c[SIGNAL→NET] remote_signal_acl_readers(reader) → raw_count=2 valid_recipients=1 (self filtered)
S5d[ZOME]       author-shape Hive link only (NO hive-shape link — DMs hive-less)
S6 [post_commit] generic Signal::EntryCreated + LinkCreated per action
```

### First contact (stub-first hardening — see §6)
```
F1 [CLIENT]    click link → commit REQUEST STUB (content_type humm-sidecar-dm-request-v1,
               bytes = signed key-binding only; NO real content)        (🔒 DM-L1)
F2 [recv,recipient] recv_remote_signal → UI shows [Accept]/[Block]
F3a[recipient] Block  → local filter; no signal back (sender not notified)
F3b[recipient] Accept → commit recipient key-binding (humm-dm-keybinding-v1)
F4 [CLIENT,sender]  observe acceptance; list_by_author(recipient,"humm-dm-keybinding-v1");
               VERIFY signature against recipient AgentPubKey           (🔒 DM-L1 binding)
F5 [CLIENT,sender]  seal real msg#1 under VERIFIED X25519 (not raw enc) → Send flow above
```
- 🔒 **Gap-DM6:** log whether msg #1 was sealed under a *verified*
  key-binding or the raw link `enc`. A real first message sealed under an
  unverified `enc` (the current path) is the DM-L1/F-1 exposure window.

### Receive
```
R1 [recv,peer]  recv_remote_signal[EncryptedContentSignal]: action_type=Create hash=… from_agent=me
R2 [CLIENT]     get_encrypted_content(hash) — re-fetch; never render signal body (self-notes L7)
R3 [CLIENT]     decrypt with pair SharedSecret (X25519(peer_priv, my_x25519)); render
```

> The L7 re-fetch guarantee holds identically for **cross-hive
> strangers**: Holochain's DHT is per-DNA and network-wide, so
> `get_encrypted_content(hash)` (`GetStrategy::Network`) resolves any
> committed entry regardless of shared-hive membership, and integrity
> validation runs at every validating peer. No cross-hive trust gap on the
> receive path.

### Hydration (cold start)
```
H1 [callZome]  list_by_author(me)  +  list_by_author(peer)   ← canonical; NOT list_by_hive_link
H2 [CLIENT]    merge + filter recipients ⊇ {me,peer} + sort by ts
```

**Gap candidates (audit your logging):**
- 🔒 **Gap-DM1 (DM-L1):** log the X25519 key *source* per send (Member
  entry vs link `enc`) and any `enc`-vs-Member mismatch. Cross-hive sends
  on an unverified `enc` are the key-substitution surface.
- **Gap-DM2 (the canary):** `remote_signal_acl_readers: raw_count=2
  valid_recipients=1` is **expected** for a 1:1 DM (self filtered). `raw=2
  valid=0` ⇒ the recipient pubkey failed to decode (historical
  STANDARD-vs-URLSAFE base64 silent-drop) ⇒ the peer gets **no push** and
  delivery silently degrades to gossip/poll. Alert on `valid=0 raw>0`.
- **Gap-DM3:** a peer offline at send time gets no `[recv]`; their thread
  only fills on their next `list_by_author` hydration. Don't log this as
  "delivery failed" — it's the pull fallback.
- **Gap-DM4 (DM-L6):** log `HummUriInvalidEncError` distinctly from
  "recipient unreachable" — they have different causes and fixes.
- **Gap-DM5:** `list_by_author` empty right after the peer's write is
  gossip lag (DMs ride normal DHT replication cross-hive), not message
  loss — route to poll/retry.

---

## 12. References

- humm-tauri: `src/lib/humm-uri/index.ts` (`buildHummUri`/`parseHummUri`,
  `STANDARD_B64_X25519_RE`, `HummUriInvalidEncError`).
- earth-core integrity (`dnas/humm_earth_core/zomes/integrity/content/src/`):
  `encrypted_content.rs:726-784` `validate_directmessage_acl`;
  `:294` `AclSpec::DirectMessage` ⇒ no hive context;
  `:850-858` delete authority (I-A).
- earth-core coordinator
  (`dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/`):
  `crud.rs:28` `create_encrypted_content`;
  `queries.rs:268` `list_by_author` (canonical DM read);
  `queries.rs:365` `fetch_pair_ss_with_hive_check` (in-hive intersection);
  `signals.rs:180` `remote_signal_acl_readers` (fan-out + the raw/valid
  canary).
- `HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md` §F (DM validator) + §I
  (delete); `HUMM_TAURI_SELF_NOTES_INTEGRATION.md` §12 (shared crypto
  discipline L5, public-DHT L1/L3, signal-spoof L7) +
  `..._OBSERVABILITY.md` (layers, signal families, eventual consistency).
- Pass-4 invariant unchanged: DNA
  `uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`.
```
