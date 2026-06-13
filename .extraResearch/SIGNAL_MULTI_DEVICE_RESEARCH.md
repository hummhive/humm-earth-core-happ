# Signal multi-device + Note-to-Self — research for humm note-to-self / multi-device design

**TL;DR.** Signal gives every *account* a single long-term identity key pair (per the ACI, plus a second one for the PNI) that is **shared across all of a user's devices**, while every individual device keeps its **own** prekeys and therefore its **own** pairwise Double-Ratchet sessions; multi-device fan-out, "Note to Self," read-state sync, etc. are all coordinated by the **Sesame** session-management layer riding on a **central server** that holds a per-device mailbox. Crucially, **Signal never sends a message from a device to *itself***: a device keeps a record for its own *user* but not for its own *device*, so "send to self" always means "send a copy to my *other* device sessions" plus a **"Sent" sync transcript**, and "Note to Self" is just an ordinary conversation whose recipient is your own account ID (a real thread, not a protocol primitive, not a degenerate self-session). For humm this is mostly *inspiration, not a blueprint*: Signal's two load-bearing assumptions — (a) one identity private key copied onto many devices and (b) a server that fans messages out to per-device mailboxes — both collide head-on with Holochain's "one Ed25519 keypair = one append-only source chain, no server, public deterministic validators." The transferable ideas are Sesame's *explicitly-documented* **per-device-identity-key** alternative, the **QR provisioning handshake** (authorize a new key, don't copy the old one), the **Sent-transcript** pattern, and treating note-to-self as its **own conversation type** rather than a self-addressed DM.

## Date + scope

- **Research date:** 2026-06-01 (UTC). This is a **point-in-time snapshot** of Signal's *public* design. Signal ships continuously; protobuf fields, file paths, and product behavior (especially "Link-and-Sync," which was still rolling out from beta in 2025) change. Re-verify against `main` before depending on any exact field number or path.
- **What was examined:** the formal Signal protocol specs (Sesame, X3DH, PQXDH, Double Ratchet), Signal's engineering blog, Signal's support center, and current source in `signalapp/libsignal` and `signalapp/Signal-Android` (default branch `main`), plus one peer-reviewed secondary analysis.
- **Evidence tags.** The brief asked for `spec | source | secondary-analysis | inference`. To label provenance *accurately* I split "primary" into three honest sub-kinds and keep the rest:
  - `[evidence: spec]` — a formal signal.org protocol specification.
  - `[evidence: source]` — observed directly in Signal's published source code / protobuf.
  - `[evidence: official-doc]` — stated by Signal in its engineering blog or support center (authoritative, but prose, not spec/code).
  - `[evidence: secondary-analysis]` — third-party / academic.
  - `[evidence: inference]` — my reasoning from the above; **not** asserted as fact.
- **Not a directive to copy Signal.** humm's constraints differ (agent-centric, no server, public DHT). The "Sanity check / Inspiration / Does-not-translate" sections are where Signal's design meets ours.

---

## Q1 — Identity model: per-user vs per-device keys, ACI/PNI, device IDs, provisioning handshake

**One identity key per *account*, shared across devices; one session set per *device*.** Signal's own engineering blog states it plainly: *"All devices attached to an account share some keys for establishing and verifying the owner's identity, but every device has its own unique set of keys for encrypting and decrypting messages,"* and the footnote is unambiguous: *"linked devices for an account share an identity key pair, but independent prekeys, so they end up with independent session keys"* ([A Synchronized Start for Linked Devices](https://signal.org/blog/a-synchronized-start-for-linked-devices/)). `[evidence: official-doc]` The Sesame spec frames this as a *choice*: *"Sesame supports two different models for key pairs: With **per-user identity keys**, all devices under a user share the same key pair. With **per-device identity keys**, each device may have a different key pair"* ([Sesame §3.1](https://signal.org/docs/specifications/sesame/#device-state)). `[evidence: spec]` Signal picked **per-user identity keys**. (Remember this — humm is essentially forced into Sesame's *other* documented option; see Sanity check.)

**The smoking gun in source: linking copies the identity *private* key.** The provisioning message a primary device sends to a new device carries the identity key **pairs themselves**, private halves included, for both the ACI and PNI:

```proto
message ProvisionMessage {
  optional bytes aciIdentityKeyPublic  = 1;
  optional bytes aciIdentityKeyPrivate = 2;
  optional bytes pniIdentityKeyPublic  = 11;
  optional bytes pniIdentityKeyPrivate = 12;
  optional string aci = 8;   optional string pni = 10;
  optional string number = 3;             // E.164 phone number
  optional string provisioningCode = 4;   // one-time linking token
  optional bytes profileKey = 6;
  optional string accountEntropyPool = 15; // backup key material
  ...
}
```
([Provisioning.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/Provisioning.proto)) `[evidence: source]` So a "secondary" device does not *derive* its own identity key — it **receives** the account's identity private key(s) over the linking channel. `[evidence: source]`

**ACI vs PNI.** A Signal account is addressed by *service IDs*: the **ACI** (Account Identifier, a UUID) and the **PNI** (Phone-Number Identifier, a UUID tied to the phone number). Each is a distinct identity with **its own identity key pair** — visible both in `ProvisionMessage` above and in the wire `Envelope`, whose binary service-ID comment reads *"16 byte UUID for ACI, 1 byte prefix + 16 byte UUID for PNI"* ([SignalService.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/SignalService.proto)). `[evidence: source]` The PNI exists so a phone number can be de-emphasized as the primary identifier — the basis for usernames + "hide my phone number," where *"you will still need a phone number to register for Signal"* but it is no longer the visible handle ([Keep your phone number private with Signal usernames](https://signal.org/blog/phone-number-privacy-usernames/)). `[evidence: official-doc]`

**Device IDs.** Within an account each device has a `DeviceID` *"unique for the UserID"* ([Sesame §2.2](https://signal.org/docs/specifications/sesame/#preliminaries)) `[evidence: spec]`; the wire envelope carries `sourceDeviceId` ([SignalService.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/SignalService.proto)) `[evidence: source]`. The **primary** device is an Android/iOS phone (with the phone number); **linked** devices are Desktop/iPad ([A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/)). `[evidence: official-doc]` A Signal *session* (X3DH/PQXDH → Double Ratchet) is therefore keyed by the pair **(serviceId, deviceId)**, not by the account alone. `[evidence: spec]` ([X3DH](https://signal.org/docs/specifications/x3dh/), [PQXDH](https://signal.org/docs/specifications/pqxdh/), [Double Ratchet](https://signal.org/docs/specifications/doubleratchet/))

**The provisioning / linking handshake (cryptographic detail).** The new device drives it:
1. The new (linked) device generates a **temporary Curve25519 key pair** and registers a **provisioning address** (a mailbox) with the server, then encodes *both* the address and its **public key** into a **QR code** ([A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/)). `[evidence: official-doc]`
2. The primary device scans the QR, then encrypts the `ProvisionMessage` **to that public key**. The construction (observed in source) is a one-shot ECIES-style box, *not* an X3DH session:
   ```
   ephem = Curve25519.generate()
   shared = ECDH(ephem.priv, theirPublic)          // theirPublic = QR key
   keys   = HKDF(shared, info="TextSecure Provisioning Message", 64)  // → 32B AES + 32B HMAC
   ct     = AES-256-CBC(keys[0], ProvisionMessage)
   mac    = HMAC-SHA256(keys[1], 0x01 || ct)
   body   = 0x01 || ct || mac
   send ProvisionEnvelope{ publicKey: ephem.pub, body }
   ```
   ([PrimaryProvisioningCipher.java](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/java/org/whispersystems/signalservice/internal/crypto/PrimaryProvisioningCipher.java)) `[evidence: source]`
3. The new device (holding the QR private key) reverses the ECDH, verifies the MAC, decrypts, and thereby **adopts the account's identity private key(s)**, ACI/PNI, phone number, profile key, and a **one-time-use `provisioningCode` linking token** that *"cryptographically proves that the new device has permission to add itself to the Signal account"* ([A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/); [Provisioning.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/Provisioning.proto)). `[evidence: official-doc + source]`

So: secondary devices receive identity material by **transfer**, then mint their **own** prekeys and registration ID and stand up **independent** sessions. `[evidence: official-doc + source]`

---

## Q2 — Note to Self: real conversation, special thread, or sync-only? What's the envelope when sender == recipient?

**It is a real conversation with yourself — not a protocol primitive.** Signal's support center: *"This contact entry is a chat to send messages to yourself. Use this feature to jot down a note for yourself to review later or to share messages and files with your linked devices. All messages in Note to Self are end-to-end encrypted Signal messages"* ([Note to Self](https://support.signal.org/hc/en-us/articles/360043272451-Note-to-Self)). `[evidence: official-doc]` In the protocol there is **no "note to self" message type**: `Content` and `SyncMessage` contain `dataMessage`, `syncMessage`, `sent`, etc., but nothing self-specific ([SignalService.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/SignalService.proto)). `[evidence: source]` A note to self is an ordinary `DataMessage` whose **destination is your own ACI**, recorded locally as a thread keyed to yourself, and (if you have other devices) propagated via the **`Sent` sync transcript** (Q3). `[evidence: source + inference]`

**The cryptographic envelope when sender == recipient: there is no self-session; the Double Ratchet is *not* run against your own device.** This is the key insight, and it is spec-level:

- *"Each device stores a UserRecord for its own UserID, but **does not store a DeviceRecord for its own DeviceID**. This UserRecord enables a device to send a copy of each outbound message to the user's other devices."* ([Sesame §3.1](https://signal.org/docs/specifications/sesame/#device-state)) `[evidence: spec]`
- When sending to your own UserID, the device encrypts *"for each non-stale DeviceRecord in the UserRecord that contains an active session"* — i.e. to your **other** devices only ([Sesame §3.3](https://signal.org/docs/specifications/sesame/#sending-messages)). `[evidence: spec]`

Consequences:
- **Single device:** the "own UserID" record has **no other DeviceRecords**, so the per-device send list is **empty** — nothing crosses the wire; the note is simply stored locally. `[evidence: spec (§3.3) + inference]`
- **Multiple devices:** the note is encrypted to each of your *other* devices using the **normal pairwise Double-Ratchet session** between those two distinct devices (device A ↔ device B), exactly like messaging a contact. The two devices share the account identity key but have **independent sessions** ([A Synchronized Start… fn.1](https://signal.org/blog/a-synchronized-start-for-linked-devices/)). `[evidence: official-doc + spec]`

So the Double-Ratchet/Sesame machinery is **used**, but **never as a degenerate self-session** — the same (key, device) never ratchets with itself. "Send to self" is reframed at the protocol layer as "send to my *other* device IDs," which is why it never reduces to the degenerate case. `[evidence: spec; the "never degenerate" framing is inference]`

---

## Q3 — Multi-device message sync: Sesame across N devices, "Sent" transcripts, fan-out, avoiding the degenerate self-send

**Sesame = "establish Signal sessions between all devices" and converge on one active session per peer device.** *"encrypting a message from Alice to Bob might require creating sessions from Alice's sending device to all of Bob's devices, and also to Alice's other devices (so they receive a copy of the message)"* ([Sesame §2.1](https://signal.org/docs/specifications/sesame/#overview)). `[evidence: spec]` Each device tracks an **active** session per remote device and switches to whichever session last received a message, so devices converge on a single matching session per peer device ([Sesame §2.1, §3.1–3.4](https://signal.org/docs/specifications/sesame/#sesame)). `[evidence: spec]` A peer-reviewed analysis summarizes it the same way: *"Sesame consists in establishing Signal sessions between all devices"* and notes the design is heavier than necessary precisely because "all devices belong to a single user" is not exploited ([Campion, Devigne, Duguey, Fouque, "Multi-Device for Signal," ACNS 2020 / IACR ePrint 2019/1363](https://eprint.iacr.org/2019/1363.pdf)). `[evidence: secondary-analysis]`

**Fan-out on send.** The Sesame sending input is *"some plaintext and a set of recipient UserIDs [that] includes the device's own UserID"* ([Sesame §3.3](https://signal.org/docs/specifications/sesame/#sending-messages)) `[evidence: spec]`; the server-side reality, per Signal: once a device is linked, *"senders must send multiple copies of any messages for this Signal account, with each message encrypted separately for every device on the account"* ([A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/)). `[evidence: official-doc]` So one logical "send" expands to *N(recipient devices) + (M-1)(your own other devices)* ciphertexts, each into the target device's **mailbox** on the server ([Sesame §2.2 "Mailboxes," §3.3](https://signal.org/docs/specifications/sesame/#preliminaries)). `[evidence: spec]`

**The "Sent" sync transcript** is how your *other* devices learn what *you* sent (and to whom), so all your devices show a consistent conversation:

```proto
message SyncMessage {
  message Sent {
    optional string destinationServiceId = 7;     // who the original msg went to (your own ACI for Note-to-Self)
    optional uint64 timestamp = 2;
    optional DataMessage message = 3;              // the message you sent
    repeated UnidentifiedDeliveryStatus unidentifiedStatus = 5; // per-recipient sealed-sender status
    optional bool isRecipientUpdate = 6 [default = false];
    ...
  }
  oneof content { Sent sent = 1; Contacts contacts = 2; ... Keys keys = 13; ... }
  repeated Read read = 5;       // read receipts synced across your devices
  repeated Viewed viewed = 16;
}
```
([SignalService.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/SignalService.proto)) `[evidence: source]` The same `SyncMessage` channel also syncs read/viewed state, contacts, blocked list, configuration, and account keys (`accountEntropyPool`) across devices ([SignalService.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/SignalService.proto)). `[evidence: source]`

**How the "send to self is degenerate" problem is avoided at the protocol layer.** The addressable unit is **(serviceId, deviceId)**, and a device holds **no record/session for itself** ([Sesame §3.1](https://signal.org/docs/specifications/sesame/#device-state)). Therefore "send to my own account" is mechanically identical to "send to a set of *other* device IDs": there is never a 1-element recipient set equal to the sender, never a same-key-to-same-key encryption, and (single-device case) the per-device list is simply empty. A Note-to-Self with `destinationServiceId = <your ACI>` rides this exact path. `[evidence: spec; framing = inference]`

---

## Q4 — Device linking + revocation: provisioning, history backfill, revocation semantics

**Linking** = the QR provisioning handshake of Q1: new device shows QR (provisioning address + Curve25519 public key) → primary scans, authenticates, sends the ECIES-boxed `ProvisionMessage` (identity keys, account info, one-time linking token) to the provisioning mailbox → new device adopts the keys and stands up its own mailbox ([A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/); [PrimaryProvisioningCipher.java](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/java/org/whispersystems/signalservice/internal/crypto/PrimaryProvisioningCipher.java)). `[evidence: official-doc + source]` Only Desktop/iPad can be linked as secondaries; the primary stays the phone. `[evidence: official-doc]`

**History backfill.** *Historically there was none* — Signal describes the old behavior as *"starting fresh, and having only new messages show up"* ([A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/)). `[evidence: official-doc]` Since the **"Link-and-Sync"** work (announced 2025-01-27, beta-rolling), linking optionally transfers history:
- The primary **compresses all message history** (text plus *"stickers, call history, group updates, quotes, reactions, and delivery/read receipts"*) into one archive, **encrypts it with a one-time-use 256-bit AES key delivered inside the provisioning message**, uploads it as an attachment, and the new device downloads + decrypts then discards the key. *"not even Signal employees can access its content."* ([A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/)) `[evidence: official-doc]`
- **Media is bounded to ~45 days**, because the server deletes encrypted attachments 45 days after upload; the archive ships **attachment pointers**, and the new device fetches media on demand within that window. ([A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/)) `[evidence: official-doc]` The support article confirms the user-facing result: *"All your chats and the last 45 days of media can be synchronized from your mobile [device]"* and the linking flow now offers *"Transfer Message History"* vs *"Don't Transfer."* ([Linked Devices](https://support.signal.org/hc/en-us/articles/360007320551-Linked-Devices)) `[evidence: official-doc]` Corroborated by independent coverage ([BleepingComputer](https://www.bleepingcomputer.com/news/security/signal-will-let-you-sync-old-messages-when-linking-new-devices/), [Android Police](https://www.androidpolice.com/signal-linked-desktop-ipad-chat-history-transfer/)). `[evidence: secondary-analysis]`

**Revocation.** A user unlinks a device from **Settings → Linked Devices** on the primary ([Linked Devices](https://support.signal.org/hc/en-us/articles/360007320551-Linked-Devices)). `[evidence: official-doc]` At the protocol layer, removing a device makes peers' records for it go **stale**: *"A UserRecord or DeviceRecord might be marked stale, meaning the record corresponds to a deleted user or device but is being kept around to decrypt delayed messages,"* deletable after a `MAXLATENCY` window ([Sesame §3.1–3.2, §6.4](https://signal.org/docs/specifications/sesame/#device-state)). `[evidence: spec]` Because the server gatekeeps mailboxes, it can refuse delivery to a removed device and inform senders of the changed device set so they update their records ([Sesame §3.3 steps 4–8](https://signal.org/docs/specifications/sesame/#sending-messages)). `[evidence: spec; the precise server-enforcement is inference from the spec's server assumptions]` Identity-key changes (e.g., reinstall) trigger safety-number / key-change warnings ([Sesame §6.1](https://signal.org/docs/specifications/sesame/#security-considerations)). `[evidence: spec]`

---

## Q5 — Sealed sender × multi-device (brief)

Sealed sender hides *who sent* a message from the server: the message ciphertext is wrapped in an envelope **encrypted to the recipient's identity key**, carrying a short-lived **sender certificate** (*"the client's phone number, public identity key, and an expiration timestamp"*), and handed to the server **without sender authentication**, gated by a profile-key-derived **delivery token** for abuse control ([Sealed sender](https://signal.org/blog/sealed-sender/)). `[evidence: official-doc]` Multi-device interplay:
- The sender certificate is bound to the **account** identity key, which is **shared across the sender's devices**, so it is valid no matter which of your devices emitted the message. `[evidence: inference, grounded in Q1's per-user identity key]`
- Fan-out to many devices uses a **"Multi-Recipient Sealed Sender Format"** (an `Envelope` `UNIDENTIFIED_SENDER` subtype) so one ciphertext payload can target multiple recipient devices efficiently ([SignalService.proto `Envelope.Type`](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/SignalService.proto)). `[evidence: source]` The `Sent` transcript even records per-recipient `UnidentifiedDeliveryStatus` ([SignalService.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/SignalService.proto)). `[evidence: source]`
- **Relevance to humm: essentially none, and it's worth saying why.** Sealed sender is a *server-metadata-minimization* trick. Holochain is agent-centric: authorship is a signed, validated, publicly visible property of every source-chain action. There is no server to blind, and hiding "who authored this" is the opposite of what the DHT + deterministic validators do. `[evidence: inference]`

---

## Sanity check: does our approach align?

Recall **our current state** (from the context block):
- Our `DirectMessage` validator **rejects** a recipient set `[me, me]` (duplicate-pubkey rule) **and** requires `recipients.len() >= 2`. A literal "DM to myself" is therefore **invalid at commit-time validation**.
- The same keypair on two devices cannot send a literal self-DM (fails validation).
- **Candidate A:** model "note to self" as a degenerate **personal group** (single-member group the user authored), writing the note as group-scoped content with an **empty recipient list + app-layer self-encryption**, *or* as public-scoped content encrypted only to oneself.
- **Candidate B:** multi-device for one logical user is unsolved; one keypair = one source chain (chain-fork hazard), so the practical paths are (a) one logical user = **N device keypairs** linked at the app layer, or (b) one keypair shared across devices (fragile).

**Headline finding: our validator rule is *not* the bug, and it actually agrees with Signal.** Signal **also never** encrypts a message from an identity to the *exact same* identity+device, and **never** routes "note to self" through a 1:1 "DM to yourself-as-a-peer." In Signal, "self" expands to your *other* device sessions (distinct `(serviceId, deviceId)` endpoints) plus a local thread; the recipient set is never "you, twice," and a single-device note is a **local write with an empty network recipient set** ([Sesame §3.1, §3.3](https://signal.org/docs/specifications/sesame/#device-state)). `[evidence: spec]` Our `[me, me]`-rejection and `recipients >= 2` invariants are a sane DM contract; the mismatch is only that we were *tempted to express note-to-self through the DM path*. Signal sidesteps exactly that by making **Note to Self its own conversation type** ([Note to Self](https://support.signal.org/hc/en-us/articles/360043272451-Note-to-Self)). `[evidence: official-doc]`

**Candidate A aligns with Signal's *single-device* Note-to-Self — almost exactly.** Signal single-device note-to-self = "store locally; there are no other devices to fan out to" ([Sesame §3.3](https://signal.org/docs/specifications/sesame/#sending-messages)). `[evidence: spec]` Candidate A's "single-member personal group, empty PKA / recipient list, self-encrypted content" **is the Holochain spelling of that same behavior**: a self-scoped entry, encrypted to your own key, with no second recipient — so it doesn't trip `recipients >= 2` because it never pretends to be a DM. **Keep the DM validator strict; give note-to-self its own scope/entry type.** This is the right shape and mirrors Signal. `[evidence: inference]`

**Candidate B aligns with Signal's *multi-device* fan-out — but three divergences are *forced* by Holochain and must be designed around:**

1. **Identity-key sharing is impossible → use per-device keys (Sesame's *other* documented model).** Signal copies the **identity private key** onto every device (`ProvisionMessage.aciIdentityKeyPrivate`, [Provisioning.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/Provisioning.proto)). `[evidence: source]` In Holochain, putting one keypair on two source chains is a **chain-fork hazard** — Candidate B(b) ("shared keypair") is the fragile path for exactly this reason. So humm must adopt **distinct device-agent keypairs**, i.e. Sesame's *"per-device identity keys"* model, which the spec explicitly sanctions ([Sesame §3.1](https://signal.org/docs/specifications/sesame/#device-state)). We are not off-roading; we are taking Sesame's documented fork. **Divergence forced by: one keypair = one source chain.** `[evidence: spec + inference]`

2. **No server mailboxes → fan-out is via the public DHT, not pushed copies.** Signal's multi-device relies on a server holding a per-device mailbox and accepting *"multiple copies … encrypted separately for every device"* ([A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/)). `[evidence: official-doc]` humm has no server: the "Sent-transcript-to-my-other-devices" analogue must be **DHT entries/links encrypted to each device-agent's key and discovered by them**, not delivered to mailboxes. **Divergence forced by: no central transcript fan-out server.** `[evidence: inference]`

3. **Public, deterministic validators → recipient structure and authorship are visible by design.** Signal can hide the sender (sealed sender) and shows the server only ciphertext + destination. humm's validators are public WASM re-run by every peer; *who authored what entry* is intrinsic and validated. You can encrypt payloads, but you cannot hide the membership/shape the way a server-mediated system can. **Divergence forced by: public DHT + public deterministic validators.** `[evidence: inference]`

**Net:** ship **A now** (it matches Signal's single-device case and respects our DM invariants); design **B** as a **per-device-agent "device set"** (Sesame's per-device-identity-key model) for true multi-device later. The `[me,me]` / `recipients>=2` rule even *helps* in B: a note that must reach your other device-agents has recipients = {deviceB, deviceC} (the author excluded) — non-duplicate, ≥1 — which is precisely Signal's "exclude self device, encrypt to the others." `[evidence: inference]`

---

## Inspiration: concrete ideas worth stealing (mapped to a Holochain DNA + Tauri client)

- **Note-to-Self as a first-class conversation type, not a self-DM.** Signal keeps it a dedicated thread keyed to your own account ([Note to Self](https://support.signal.org/hc/en-us/articles/360043272451-Note-to-Self)). **DNA:** a distinct `PersonalNote` / self-scope entry type (not `DirectMessage`), so the DM validator stays strict and untouched. **Tauri:** a "Note to Self" thread keyed to the user's *device-set id* (see next). This is just a clean encoding of Candidate A.
- **Separate per-user *identity* from per-device *sessions* — adopt Sesame's per-device-identity-key model.** **DNA:** a signed `DeviceSet` entry = the set of device-agent public keys that constitute one logical user; "the user" is the set (or a designated root agent), and each device is its own agent with its own source chain. This avoids the chain-fork hazard while giving you the "one logical user, many devices" abstraction. ([Sesame §3.1](https://signal.org/docs/specifications/sesame/#device-state)) `[evidence: spec drives the model; mapping = inference]`
- **A device-linking handshake that *authorizes a key* instead of *copying a key*.** Mirror Signal's QR flow ([A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/)) but invert the secret: **Tauri:** the *new* device generates its own agent keypair (its own source chain) and shows a QR containing its **public key + a provisioning nonce** (Signal puts a Curve25519 pubkey + provisioning address in the QR). The existing device scans and, instead of shipping a private key, **signs a `DeviceLinkCertificate`** over the new pubkey. **DNA:** validators admit a device-agent's content iff a valid, non-revoked link certificate from an already-trusted device-agent exists in the `DeviceSet`. (The encrypt-to-the-QR-pubkey channel from `PrimaryProvisioningCipher` is still useful for privately handing over *shared app secrets* — e.g. a content-encryption key — just not the agent's signing key.)
- **One-time-use linking token → a single-use capability grant.** Signal's `ProvisionMessage.provisioningCode` proves enrollment permission ([Provisioning.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/Provisioning.proto)). **DNA:** a Holochain capability grant or single-use signed nonce that gates `DeviceLinkCertificate` issuance, so a stranger can't self-enroll.
- **The "Sent" transcript pattern.** Signal's `SyncMessage.Sent` lets your *other* devices reconstruct your outbox ([SignalService.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/SignalService.proto)). **DNA:** when a device-agent authors a message, also write a DHT-discoverable `SentRecord` **encrypted to the user's other device-agents**, linked so they can render a unified conversation. For note-to-self this collapses to "a self-scoped entry every device-agent in the set can decrypt." `[evidence: source drives the pattern; mapping = inference]`
- **Optional, *bounded* history backfill at link time.** Signal ships an E2EE archive under a one-time key, media bounded to 45 days ([A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/)). **DNA/Tauri:** on linking, an existing device-agent can hand the new one an encrypted archive of prior notes (one-time key delivered in the link ceremony), **bounded** (last N entries / a time window) so you don't replay the entire chain or re-walk the whole DHT. `[evidence: official-doc drives it; mapping = inference]`
- **Never encrypt a payload to the authoring key twice.** Signal always excludes the sending device and encrypts to the *others* ([Sesame §3.1](https://signal.org/docs/specifications/sesame/#device-state)). Keep our duplicate-pubkey rejection; in B, fan a note out to the *other* device-agents only. `[evidence: spec + inference]`

---

## What does NOT translate (server- or shared-key-dependent mechanisms with no clean Holochain analogue)

- **Per-user identity key sharing** (the entire `ProvisionMessage` "copy the identity private key" model, [Provisioning.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/Provisioning.proto)). Impossible without a chain fork; must be replaced by per-device-agent keys. `[evidence: source + inference]`
- **Server mailboxes + push fan-out + 45-day retention** ([Sesame §2.2](https://signal.org/docs/specifications/sesame/#preliminaries); [A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/)). There is no "account mailbox" a new device drains, and no central component to hold/expire ciphertext. The DHT + per-agent source chains are the substrate instead. `[evidence: spec/official-doc + inference]`
- **Sealed sender + server-issued sender certificates + profile-key delivery tokens + server rate-limiting** ([Sealed sender](https://signal.org/blog/sealed-sender/)). No server to issue/verify certificates or rate-limit anonymously; agent-centric authorship is public and validated, the antithesis of sender-anonymity. Metadata hiding on humm would need a fundamentally different design (out of scope here). `[evidence: official-doc + inference]`
- **Double-Ratchet "single converging active session per device" + ratchet-derived forward secrecy / post-compromise security** over a relay ([Double Ratchet](https://signal.org/docs/specifications/doubleratchet/); [Sesame §2.1](https://signal.org/docs/specifications/sesame/#overview)). Holochain entries are append-only and public; per-entry encryption to recipient keys is feasible, but you do **not** get ratchet FS/PCS for free, and a relay-style ratchet doesn't map cleanly onto append-only DHT semantics. `[evidence: spec + inference]`
- **Link-and-Sync's "archive as a server-hosted attachment, available 45 days"** ([A Synchronized Start…](https://signal.org/blog/a-synchronized-start-for-linked-devices/)). No server-side attachment store; backfill must be agent-to-agent / DHT, and the 45-day affordance has no equivalent. `[evidence: official-doc + inference]`

---

## Open questions / follow-ups

1. **Do notes-to-self need forward secrecy / PCS?** Per-entry encryption to your device-agent keys gives confidentiality but not ratchet FS. If a device-agent key leaks, all past self-notes encrypted to it are exposed. Decide whether that's acceptable for "note to self"; if not, an app-side ratchet or key-rotation scheme is a separate project. `[evidence: inference]`
2. **A vs B sequencing.** Confirm we ship **A** (self-scope, single key) first and that A's stored content is **re-keyable** to a future `DeviceSet` so enabling B later doesn't strand existing notes. `[evidence: inference]`
3. **Validator strategy (decision needed).** Recommended: **keep `DirectMessage` strict and add a dedicated self/personal-scope entry type**, rather than relaxing `recipients >= 2` on DMs. Confirm. `[evidence: inference]`
4. **Revocation on a public DHT with no kill switch.** Without a server to stop delivery, how do deterministic validators treat content authored by a device-agent *near* its revocation time? We need a causal/temporal rule that every peer evaluates identically (a Sesame-`MAXLATENCY`-style grace window, but deterministic). `[evidence: spec analogue + inference]`
5. **Device-set transparency / safety-number analogue.** How does a *correspondent* learn and verify your current device-agent set, and detect a maliciously-added device-agent? Signal leans on safety numbers + key-change warnings ([Sesame §6.1](https://signal.org/docs/specifications/sesame/#security-considerations)). What is humm's equivalent on the DHT? `[evidence: spec + inference]`
6. **Stable account id (ACI analogue).** Signal's ACI is a stable account UUID *decoupled* from any single device key, so rotating/removing a device doesn't change "who you are." Do we want a stable logical-user identifier separate from any one device-agent's key, so device churn doesn't break identity or existing DMs? ([Provisioning.proto](https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/Provisioning.proto)) `[evidence: source + inference]`
7. **Source re-verification.** This snapshot reflects `main` on 2026-06-01; "Link-and-Sync" and PNI/usernames were actively evolving. Re-pull `Provisioning.proto`, `SignalService.proto`, and `PrimaryProvisioningCipher.java` before implementation. `[evidence: inference]`

---

## Sources

### Primary — Signal protocol specifications (signal.org/docs)
- The Sesame Algorithm (Rev 2, 2017-04-14) — https://signal.org/docs/specifications/sesame/
- X3DH (Rev 1, 2016-11-04) — https://signal.org/docs/specifications/x3dh/
- PQXDH — https://signal.org/docs/specifications/pqxdh/
- Double Ratchet — https://signal.org/docs/specifications/doubleratchet/
- Docs index — https://signal.org/docs/

### Primary — Signal engineering blog (signal.org/blog)
- A Synchronized Start for Linked Devices (2025-01-27; linking handshake, per-user identity key + per-device sessions, Link-and-Sync, 45-day media) — https://signal.org/blog/a-synchronized-start-for-linked-devices/
- Keep your phone number private with Signal usernames (2024-02-20; PNI / phone-number privacy) — https://signal.org/blog/phone-number-privacy-usernames/
- Technology preview: Sealed sender for Signal — https://signal.org/blog/sealed-sender/

### Primary — Signal source code & protobuf (github.com/signalapp)
- libsignal (Rust protocol core: double_ratchet.rs, identity_key.rs, ratchet.rs, sealed_sender.rs, pqxdh.rs) — https://github.com/signalapp/libsignal
- Provisioning.proto (`ProvisionEnvelope`, `ProvisionMessage`) — https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/Provisioning.proto
- SignalService.proto (`Envelope`, `Content`, `SyncMessage.Sent`, multi-recipient sealed sender) — https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/protowire/SignalService.proto
- PrimaryProvisioningCipher.java (ECDH + HKDF "TextSecure Provisioning Message" + AES-256-CBC + HMAC-SHA256) — https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/java/org/whispersystems/signalservice/internal/crypto/PrimaryProvisioningCipher.java
- SecondaryProvisioningCipher.kt (decrypt side, not quoted but corroborates the handshake) — https://github.com/signalapp/Signal-Android/blob/main/lib/libsignal-service/src/main/java/org/whispersystems/signalservice/internal/crypto/SecondaryProvisioningCipher.kt

### Primary — Signal support center (support.signal.org)
- Note to Self — https://support.signal.org/hc/en-us/articles/360043272451-Note-to-Self
- Linked Devices — https://support.signal.org/hc/en-us/articles/360007320551-Linked-Devices

### Secondary — academic & press (corroboration only)
- Campion, Devigne, Duguey, Fouque, "Multi-Device for Signal," ACNS 2020 — https://link.springer.com/chapter/10.1007/978-3-030-57878-7_9 ; preprint IACR ePrint 2019/1363 — https://eprint.iacr.org/2019/1363.pdf
- BleepingComputer, "Signal will let you sync old messages when linking new devices" (2025-01-28) — https://www.bleepingcomputer.com/news/security/signal-will-let-you-sync-old-messages-when-linking-new-devices/
- Android Police, "Signal's new sync feature will finally transfer your existing messages" (2025-01-27) — https://www.androidpolice.com/signal-linked-desktop-ipad-chat-history-transfer/
- The Intercept, "New Signal Usernames Help Stymie Subpoenas" (2024-03-04) — https://theintercept.com/2024/03/04/signal-app-username-phone-number-privacy/
- TechCrunch, "Signal now lets you keep your phone number private with the launch of usernames" (2024-02-20) — https://techcrunch.com/2024/02/20/signal-now-lets-you-keep-your-phone-number-private-with-the-launch-of-usernames/
