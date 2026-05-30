# humm-tauri × pass-3 feature enablement

Feature-by-feature implementation reference for the pass-3 DNA. Where
[`HUMM_TAURI_ACLSPEC_INTEGRATION.md`](./HUMM_TAURI_ACLSPEC_INTEGRATION.md)
documents *the wire shape*, this doc documents *what each
capability pass-3 enables looks like inside humm-tauri* — which
existing files change, which new files/sidecars/components are
needed, and the migration + acceptance criteria.

Each section follows the same template:

1. **Capability** — one-sentence product framing.
2. **DNA primitives consumed** — `AclSpec` variants, externs,
   signals from pass-3 (or pass-1/2 ephemeral signal channels still
   in use).
3. **humm-tauri existing files touched** — concrete file paths in
   `humm-tauri/src/**` that need code changes for this feature to
   work post-pass-3 (with brief "what changes" note).
4. **humm-tauri new files / components needed** — net-new sidecars,
   stores, modals, hooks the feature requires.
5. **Migration story** — data migration via the pass-3 script
   (`scripts/migrate-dna.ts`) + any UX migration / launch notes.
6. **Acceptance / smoke tests** — observable behaviours that pin
   the feature as working end-to-end.

> **Status note.** This doc lists the feature surfaces pass-3
> enables; humm-tauri implementation work is downstream and lives in
> the humm-tauri repo. This doc is the contract from humm-earth-core.

---

## E.4.a — Cross-hive DMs (preserve + harden)

**Capability.** Two agents in different hives exchange DMs with
either party able to delete. Threads, read-state, soft-delete via
ephemeral signal — all preserved.

**DNA primitives.** `AclSpec::DirectMessage { recipients: [me, peer]
}`. Pre-existing `DmRemoteSignal::DmDeleteRequest` (pass-1 C6) for
soft-delete; existing inbox `DmCreate`/`DmDelete` bytes unchanged.

**humm-tauri existing files touched.**
- `src/sidecars/direct-messages/wire/content.ts:sendDirectMessage` —
  swap `{ acl, publicKeyAcl }` for `{ acl_spec: { DirectMessage: {
  recipients: [me, peer] } }, public_key_acl: readerOnly(recipients) }`.
- `src/sidecars/direct-messages/wire/types.ts` — ship the new wire
  shape for DM messages.
- `src/sidecars/direct-messages/state/DmStore.ts` — ingestion +
  read-state logic stays unchanged at the UX level (DM ID, thread
  ID, content unchanged; only the wire envelope changes).
- `src/api/core/holochain/zomeSignals.ts` — already carries
  `from_agent` from pass-1 C1; no further change.

**humm-tauri new files / components needed.** None. The DM sidecar
ships verbatim post-migration.

**Migration story.** `scripts/migrate-dna.ts import` re-stamps every
`direct_message` content type to `DirectMessage` via the classifier
(see `DNA_MIGRATION_GUIDE.md` § Pass-3 wire-shape migration). The
classifier splices the new agent into the legacy reader bucket if
absent (the integrity validator requires `author ∈ recipients`).

**Acceptance / smoke tests.**
- Alice (HIVE_A) sends DM to Bob (HIVE_B); both see the thread.
- Either party can delete (`validate_delete_encrypted_content` accepts
  any pubkey in `public_key_acl.reader`).
- Modified-coordinator attempts to spoof recipient list → rejected at
  commit with `does not match recipients`.
- Modified-coordinator attempts to impersonate Alice → rejected at
  commit with `does not match action.author` (pass-1
  `check_author_matches_header`).

---

## E.4.b — Group chat with cryptographically enforced membership

**Capability.** Multi-party group chat with cryptographic membership
gating. Cross-hive group members supported (hive owner grants
`GroupMembership` to a pubkey regardless of which hive that pubkey
lives in).

**DNA primitives.** New sidecar content type
`humm-sidecar-group-message-v1` with `AclSpec::HiveGroup`.
`create_group_genesis`, `create_group_membership` for setup.
`list_group_members(group_genesis_hash)` for the authoritative roster.

**humm-tauri existing files touched.**
- `src/state/group/index.ts` — observe BOTH the display cache (legacy
  `GroupMemberList` entry) and the cryptographic roster
  (`list_group_members`). Expose a
  `useGroupMembersAuthoritative(groupGenesisHash)` hook.
- `src/api/content/group/index.ts` — `GroupApi.add` now calls
  `create_group_genesis` first, then writes the display `Group` entry
  with `acl_spec: HiveGroup` referencing the new genesis. Rename via a
  new content entry (group identity is immutable).
- `src/api/content/groupMemberList/index.ts` — demoted to display
  cache; writes still allowed but authoritative reads use
  `list_group_members`.

**humm-tauri new files / components needed.**
- NEW sidecar: `src/sidecars/group-chat/` (per
  `.newTasks/20260516_PeerMessagingSidecarPlatform.md`).
  Wire shape `humm-sidecar-group-message-v1` with
  `AclSpec::HiveGroup`.

**Migration story.** Group chat is forward-looking; no pass-2
shipped data to migrate. The hive owner creates groups via
`create_group_genesis` and grants memberships via
`create_group_membership` (including cross-hive peers).

**Acceptance / smoke tests.**
- 3 agents across 2 hives. Group owner (HIVE_A) grants Group Writer
  to all 3 (cross-hive grants supported because `for_agent` is just
  a holohash).
- All 3 can send group-chat messages.
- A 4th agent (no membership) attempts to send → rejected at commit
  with `agent X is not the group author... and supplied no
  authorising GroupMembership`.

---

## E.4.c — User-controlled hive discovery (signed, cross-network)

**Capability.** Cross-network publishing of hive discovery anchors.
Owner controls visibility by publishing (or omitting) the anchor.

**DNA primitives.** `AclSpec::OpenWrite { target_hive_genesis_hash:
None }`. Existing `hummhive-core-hive-discovery-v1` content type.

**humm-tauri existing files touched.**
- `src/api/content/hiveDiscovery/index.ts` — write path swaps from
  legacy `{ acl, hive_id: '' }` to `{ acl_spec: { OpenWrite: {
  target_hive_genesis_hash: null } } }`. Reads via `listAllByAuthor`
  unchanged.

**humm-tauri new files / components needed.**
- NEW UI possible (future): `src/containers/HiveDirectory/` — a
  cross-network hive browser surfacing public discovery anchors.

**Migration story.** The classifier in `scripts/migrate-dna.ts` maps
`hummhive-core-hive-discovery-v1` to `OpenWrite { target: None }`
automatically (default mapping ships in the script). No operator
action needed.

**Acceptance / smoke tests.**
- Alice publishes a discovery anchor; Bob (on a different network
  bootstrap, no shared hive) queries her pubkey and sees the anchor.
- Modified-coordinator attempts to forge an anchor claiming Alice's
  signing pubkey → rejected by `check_author_matches_header`.

---

## E.4.d — Member-request (outsider knock)

**Capability.** Outsider with no hive membership submits a
member-request entry that the hive owner sees as a verifiable inbox
entry. Accept flow grants the requester a HiveMembership.

**DNA primitives.** `AclSpec::OpenWrite { target_hive_genesis_hash:
Some(target_hive) }`. Pre-existing `hummhive-core-member-request-v1`
content type. Hive owner accept uses
`create_hive_membership` (pass-2).

**humm-tauri existing files touched.**
- `src/api/content/memberRequest/index.ts` — write path swaps to
  `AclSpec::OpenWrite { target_hive_genesis_hash:
  Some(target_hive_genesis_hash) }`. Requester does NOT need hive
  membership to write.
- The hive owner's inbox UI consumes the verifiable inbox of
  requests (currently stubbed: `MemberRequests` pane returns `[]`).
  Wire this to read from `OpenWrite` entries targeting your hive.

**humm-tauri new files / components needed.**
- NEW UI: `src/containers/MemberRequests/` (replacing the stubbed
  pane) — list of pending requests + accept/decline modal that
  issues `create_hive_membership` on accept.

**Migration story.** Classifier maps `member-request-v1` to
`OpenWrite { target: Some(hive_genesis_hash) }` automatically.

**Acceptance / smoke tests.**
- Outsider Dave creates a member-request targeting HIVE_A.
- Hive owner Alice sees the request in her verifiable inbox.
- Dave attempts to forge `target_hive_genesis_hash` to a fake hash →
  rejected at commit (`fetch_genesis` fails on the bogus target).

---

## E.4.e — Local media library with selective sharing

**Capability.** A user has a single media library where each item
chooses its own sharing scope (personal / per-peer / per-group /
per-hive / public). Blob bytes live in the Rust content-addressed
store; only metadata is on the DHT.

**DNA primitives.** Polymorphic per item:
- Personal → `AclSpec::HiveGroup` with a singleton personal group
- Per-peer → `AclSpec::DirectMessage { recipients: [me, peer] }`
- Per-group → `AclSpec::HiveGroup { group_acl: [target_group_genesis_hash] }`
- Per-hive → `AclSpec::HiveGroup` with the hive's hive-wide reader group
- Public → `AclSpec::Public { hive_genesis_hash }`

**humm-tauri existing files touched.**
- `src/api/content/blob/index.ts` — `BlobApi.add` (currently a stub)
  finished; accepts an `AclSpec` parameter the call site selects.
- `src/api/core/hummContent/hummContentWrites.ts:addEntry` — carries
  through whatever `AclSpec` variant the caller supplies (instead of
  always calling `derrivePublicKeyAcl`).
- `src/api/core/acl/index.ts` — new
  `getAclSpecCreatorDefault({ scope: 'personal' | 'group' | 'hive' |
  'public' | 'peer' }) => AclSpec` helper.
- `src/state/blob/*` — new MobX store for the media library
  (observable list, upload state, sharing-scope per item).

**humm-tauri new files / components needed.**
- NEW UI: `src/containers/MediaLibrary/` — list + upload + per-item
  sharing-picker modal.
- NEW UI: `src/containers/ShareSelector/` — the picker that maps to
  AclSpec variants.

**Migration story.** Pre-existing blob entries (if any) restamp
through the classifier default (`Public` for unknown content types).
humm-tauri can re-stamp specific items via the UI once real groups
exist.

**Acceptance / smoke tests.**
- Upload one item to media library; share with one peer via DM;
  share another with a Marketing group; share another publicly.
- A third party (no group membership) sees only the public item.
- The per-peer item appears in the recipient's DM inbox.

---

## E.4.f — Per-content ACL picker (the "ACL UI is a later slice" promise)

**Capability.** Compose lets the user pick how a post is shared
(public / in-group / DMs / openWrite) on a per-item basis.

**DNA primitives.** All four `AclSpec` variants.

**humm-tauri existing files touched.**
- `src/containers/Compose/index.tsx` — swap the hardcoded
  `buildPublicAcl(ownerSigningKey)` for a `<ShareSelector>` choice
  that resolves to one of the four `AclSpec` variants. The current
  comment ("Public ACL today; ACL UI is a later slice") is the marker
  to find this site.

**humm-tauri new files / components needed.**
- Reuses the `ShareSelector` from § E.4.e.

**Migration story.** No data migration; UX work only.

**Acceptance / smoke tests.**
- Compose a post; pick "share with Marketing group".
- Only Marketing members see + decrypt.
- A non-member's commit attempt is rejected.

---

## E.4.g — Forgery-proof groups / roles / ACLs (headline win)

**Capability.** Every group/role/ACL claim cryptographically
attributable to the hive owner via the `GroupMembership` chain. A
modified coordinator cannot self-promote, inject members, or forge
privileged invites.

**DNA primitives.** Phase A integrity foundation. Phase B coordinator
externs. Phase C `AclSpec::HiveGroup` validators.

**humm-tauri existing files touched.**
- `src/api/content/group/index.ts` — `GroupApi.add` first calls
  `create_group_genesis`, then creates the display `Group` entry with
  `acl_spec: HiveGroup` referencing the new `group_genesis_hash`.
  `GroupApi.update` for display-only changes (rename) via a new
  `Group` content entry (`GroupGenesis` itself is immutable).
- `src/api/content/groupMemberList/index.ts` — demoted to display
  cache; writes still allowed (display) but authoritative reads use
  `list_group_members`.
- `src/api/content/member/index.ts` and `src/api/content/invite/*` —
  invite-creation captures `inviter_group_authority_hashes` map;
  accept iterates groupIds and calls `create_group_membership` per
  group.
- `src/api/core/acl/index.ts:derrivePublicKeyAcl` — calls
  `list_group_members(group_genesis_hash)` for `HiveGroup` content.
- `src/state/group/index.ts`, `src/state/groupMemberList/index.ts` —
  observe both the display cache and the cryptographic roster;
  expose `useGroupMembersAuthoritative(groupGenesisHash)` hook.
- `src/containers/MembersAndGroups/Groups/ManageGroup/index.tsx`,
  `.../Members/ManageMember/index.jsx`,
  `.../Invites/ManageInvite/index.tsx` — submit paths replaced per
  the per-modal wiring in
  [`HUMM_TAURI_ACLSPEC_INTEGRATION.md`](./HUMM_TAURI_ACLSPEC_INTEGRATION.md)
  § 3.

**humm-tauri new files / components needed.** None for this feature
itself (the existing Members & Groups pane IS the UI); only the
backing API calls change.

**Migration story.** Per `DNA_MIGRATION_GUIDE.md` § Pass-3 wire-shape
migration: the classifier defaults legacy group entries to `Public`
(safer than HiveGroup-with-unmapped-groups). humm-tauri post-
migration UI workflow:
1. On first launch under pass-3, walk legacy groups (squuid-keyed)
   and call `create_group_genesis` to materialise each as a real
   `GroupGenesis`.
2. For each member in the legacy `GroupMemberList`, call
   `create_group_membership` granting the recorded role.
3. Re-emit affected entries via `create_encrypted_content` with
   `acl_spec: AclSpec::HiveGroup { ... }` referencing the new
   genesis hashes.

This step is humm-tauri UX (could be a one-shot post-migration
wizard); the integrity zome enforces correctness at commit time.

**Acceptance / smoke tests.**
- A modified-coordinator agent attempts every attack from the
  attack-coverage matrix (rows 1-14 in
  [`HUMM_TAURI_ACLSPEC_INTEGRATION.md`](./HUMM_TAURI_ACLSPEC_INTEGRATION.md)
  § 6); every attempt fails at commit-time validation.
- Self-promote: writer attempts to grant themselves Admin → blocked
  by Rule 1 (no self-grant).
- Cross-hive group claim: writer in HIVE_A attempts to commit
  HiveGroup content with `group_acl.owner` pointing at a group in
  HIVE_B → blocked by cross-hive consistency check.
- Expired admin grant: admin's group membership expires; new commits
  authored after expiry are rejected.

---

## E.4.h — WebRTC + iroh streaming meta-layer

**Capability.** Streaming transports (WebRTC for call AV; iroh-roq /
iroh-live for live streams) hardened at the discovery /
authentication layer. Stream manifests are cryptographically signed;
recipients verifiable per scope.

**DNA primitives.** None for transport (network/SDK layer). Stream
manifest entries use:
- `AclSpec::Public` for open broadcast streams.
- `AclSpec::HiveGroup` for group-gated streams.
- `AclSpec::DirectMessage` for private 1:1 video calls.
- Call signaling continues via ephemeral
  `DmRemoteSignal::DmCall(InitRequest|InitAccept|SdpData)` from
  pass-1 C7 (no DHT footprint).

**humm-tauri existing files touched.**
- `src/sidecars/dm-webrtc/` (planned per
  `.newTasks/T_DM_MEDIA_AND_WEBRTC_AV_FUTURE_SCOPE.md`) — call
  signaling continues unchanged. `from_agent` already attested.
- `src/types/index.ts` — `TextPost.liveStream` schema field is
  already declared with `transport: 'iroh-roq' | 'iroh-live'`; the
  stream manifest is the entry referenced from the post.

**humm-tauri new files / components needed.**
- NEW sidecar: `src/sidecars/streaming/` — stream manifest entries
  (URL, iroh node ID, encryption key reference). Per-scope variant.

**Migration story.** Forward-looking; no migration needed. Once
streaming sidecars ship, they inherit pass-3's authority model for
free.

**Acceptance / smoke tests.**
- Alice starts a Public stream; Bob (different hive) discovers the
  manifest, fetches the iroh node ID, connects.
- Modified-coordinator's attempt to forge Alice's stream URL is
  rejected via author check at the manifest entry.
- Private 1:1 video call: SDP signaling carries via ephemeral remote
  signals (no DHT); call connects.

---

## E.4.i — Sidecar marketplace + agent directory (planned)

**Capability.** Cross-network publishing of sidecar manifests and
agent directory entries.

**DNA primitives.** `AclSpec::OpenWrite { target_hive_genesis_hash:
None }`. Existing infrastructure.

**humm-tauri existing files touched.** N/A (planned features).

**humm-tauri new files / components needed.**
- NEW: `src/api/content/agentDirectory/` and
  `src/api/content/sidecarManifest/` modules.
- NEW UI possible: a directory pane (absent today).

**Migration story.** Forward-looking; no migration.

**Acceptance / smoke tests.**
- Any agent can publish a directory entry; any agent can list by
  author.
- Forged-author entries are rejected at commit.

---

## E.4.j — Personal vault (today via singleton group; future dedicated variant)

**Capability.** Per-user local + encrypted-on-DHT vault for personal
content. Only the author can read; no other party even sees it
exists.

**DNA primitives.** Today: `AclSpec::HiveGroup` with a singleton
personal group (only member: the user). Future (deferred D12):
dedicated `AclSpec::PersonalVault` variant (private entry,
source-chain-only — no DHT publish).

**humm-tauri existing files touched.**
- The existing personal-group pattern continues to work; no code
  change beyond pass-3 wire-shape migration.

**humm-tauri new files / components needed.** None today. A future
`PersonalVault` variant (D12 in the plan) would add a dedicated
private entry type.

**Migration story.** Singleton personal groups migrate the same as
other HiveGroup content (the classifier defaults to Public, then
humm-tauri post-migration re-stamps to HiveGroup with the migrated
personal group_genesis).

**Acceptance / smoke tests.**
- User creates a personal entry; only they can read it.
- A second agent in the same hive cannot see the entry's content
  (decryption gated by shared secret; routing intentionally absent
  via empty `public_key_acl.reader`).

---

## E.4.k — Subscription / paid content (deferred)

**Capability.** Subscription-gated content (StoryFuel whitepaper
roadmap). Releases content to a paying subscriber set.

**DNA primitives.** Documented as a future `AclSpec::SubscriptionGated`
variant (D13 in the plan); not implemented this pass. humm-tauri
work waits on that future DNA bump.

**humm-tauri existing files touched.** N/A (deferred).

**humm-tauri new files / components needed.** N/A (deferred).

**Migration story.** Future migration; not pass-3.

**Acceptance / smoke tests.** N/A (deferred).

---

## E.4.l — Pre-signed invite links (Discord-style one-click join)

**Capability.** Hive Owner/Admin generates a shareable URL like
`humm://hive/invite?hive=<b64>&invite=<b64>&token=<HMAC>`. The
recipient clicks; the humm-tauri Tauri app opens; the user sees
"Join <hive name> as <role>?"; one click → they're in. Closes the
UX gap between "send an invite" (existing flow, requires inviter
to manually grant a pre-existing pubkey) and "share a link"
(Discord/Slack/Notion-style frictionless onboarding).

**DNA primitives — pass-3 already provides everything; no DNA
changes needed.** The flow composes existing variants and externs:

| Step | What humm-tauri does | Pass-3 primitive |
|---|---|---|
| 1 | Alice writes the invite | `create_encrypted_content` with `content_type: 'hummhive-core-pre-signed-invite-v1'` + `AclSpec::Public { hive_genesis_hash }`. Validator requires Alice holds Writer+ in the hive. Payload (plaintext-readable JSON): `{intended_role, intended_group_memberships, expiry, max_uses, hmac_secret}`. |
| 2 | Alice generates the URL | Pure app-level construction: `humm://hive/invite?hive=<hive_genesis_b64>&invite=<invite_action_hash_b64>&token=<HMAC(invite_action_hash, hmac_secret)>`. |
| 3 | Alice shares the URL | DM, email, Discord, QR code, etc. — no DNA layer. |
| 4 | Bob clicks the URL → humm-tauri opens | Tauri URL handler (`tauri.conf.json` → `allowlist`/`scheme`) dispatches to a new Accept-Invite modal. Modal calls `get_encrypted_content(invite_action_hash)`. Public entries are world-readable; Bob does NOT need hive membership. |
| 5 | Bob's app verifies the HMAC | Pure TS/JS: recompute HMAC(invite.action_hash, invite.payload.hmac_secret) and compare to the `token` query param. Detects post-publication tampering. |
| 6 | Bob signals acceptance | `create_encrypted_content` with `content_type: 'hummhive-core-invite-redemption-v1'` + `AclSpec::OpenWrite { target_hive_genesis_hash: Some(hive_genesis) }`. Bob does NOT need pre-existing hive membership; the OpenWrite validator only checks author identity + target HiveGenesis existence. Payload references `{invite_action_hash, opaque_token}`. |
| 7 | Alice's app sees Bob's redemption | Existing `Inbox::HiveInvite` link (byte 2) OR polling pattern (mirrors the existing member-request flow). Alice's running humm-tauri process auto-detects via signal or periodic poll. |
| 8 | Alice mints Bob's membership | Existing `create_hive_membership` (pass-2). Bonus: `create_group_membership` per `intended_group_memberships` from the invite payload. The `create_hive_membership` extern already self-writes an `Inbox::HiveInvite` link from Bob's pubkey → Bob's `list_my_hives` surfaces the new hive automatically. |

**Bob's experience**: one click → wait a few seconds → "you joined
<hive name>". **Alice's experience**: invisible. If Alice's app is
online, the redemption is processed in the background. If Alice is
offline, Bob's redemption waits in Alice's inbox until she comes
online (same as member-request flow).

**Modified-coordinator resistance.** Pass-3's
`check_author_matches_header` + the action-author signature mean
the invite's signing pubkey is cryptographically attributable to
Alice; Bob's HMAC verification confirms the invite content hasn't
been tampered with after Alice published it. Alice's app verifies
the redemption's `invite_action_hash` matches an invite she
actually published (`action.author == her pubkey`) before minting
Bob's membership.

**humm-tauri existing files touched.**
- `src-tauri/tauri.conf.json` — register the `humm://` URL scheme
  + corresponding handler. Tauri docs:
  https://tauri.app/v2/guides/distribution/sign-up
- `src-tauri/src/lib.rs` (or equivalent) — wire the URL handler to
  emit a Tauri event the React side picks up.
- `src/api/content/hiveInvite/index.ts` (or new) — `createInvite`
  helper that posts the Public entry + computes the URL.
- `src/api/core/hummContent/hummContentWrites.ts:addEntry` — only
  matters that it accepts `AclSpec::Public` + `AclSpec::OpenWrite`
  payloads (pass-3 ACLSpec wire shape, already documented).

**humm-tauri new files / components needed.**
- NEW UI: `src/containers/CreateInvite/` — modal: pick role +
  groups + expiry; show generated URL + copy-to-clipboard.
- NEW UI: `src/containers/AcceptInvite/` — landing page reached
  via URL handler; shows "Join <hive name> as <role>?" + accept
  button. Bob clicks → emits the OpenWrite redemption.
- NEW background processor: `src/sidecars/invite-redemption/` —
  polls Alice's own inbox for `hummhive-core-invite-redemption-v1`
  entries targeting her hives, verifies the invite-action-hash
  exists + is hers, processes the redemption (mint
  HiveMembership + optional GroupMemberships).
- NEW content schema entries in `src/types/contentSchema.ts`:
  ```ts
  export type PreSignedInvitePayload = {
    intended_role: HiveRole;
    intended_group_memberships: Array<{
      group_genesis_hash_base64: string;
      role: HiveRole;
    }>;
    expiry_microseconds: number | null;   // null = no expiry
    max_uses: number | null;              // null = unlimited
    hmac_secret_base64: string;           // random per-invite
  };

  export type InviteRedemptionPayload = {
    invite_action_hash_base64: string;
    opaque_token_base64: string; // recomputed HMAC, for auditing
  };
  ```

**Migration story.** No data migration (forward-looking feature;
pass-1/2 had no invite-link concept). humm-tauri can ship this
**right now** against the pass-3 DNA hash — no need to wait for
pass-4. The pass-4 wire-shape change to `AclSpec::HiveGroup`
doesn't affect this flow because both the invite entry (`Public`)
and redemption entry (`OpenWrite`) live in variants pass-4 leaves
unchanged.

**Acceptance / smoke tests.**
- Alice (hive Owner) creates an invite for "Writer" role. URL
  generated. Bob (no hive membership, different network) clicks.
  His humm-tauri opens; Accept-Invite modal shows "Join <hive>
  as Writer". Bob clicks accept. Within 30s, Bob's `list_my_hives`
  shows the new hive.
- HMAC tampering: a modified URL with a flipped byte in the token
  → Bob's verifier rejects pre-redemption with "invite content has
  been modified".
- Modified-coordinator attempt to forge an invite: Mallory tries
  to write an invite claiming Alice as author. Rejected by
  `check_author_matches_header` (pass-1 invariant).
- Stale invite redemption: Alice receives a redemption for an
  `invite_action_hash` she never published. App-side check
  rejects; no `HiveMembership` is minted.
- Expired invite: invite payload `expiry_microseconds` is past.
  Alice's processor rejects the redemption without minting.

---

## Quick reference — which features need new humm-tauri files

| Feature | NEW files |
|---|---|
| E.4.a — Cross-hive DMs | None (existing DM sidecar) |
| E.4.b — Group chat | `src/sidecars/group-chat/` |
| E.4.c — Hive discovery | NEW UI possible: `src/containers/HiveDirectory/` |
| E.4.d — Member-request | NEW UI: `src/containers/MemberRequests/` (replaces stub) |
| E.4.e — Media library | NEW UI: `src/containers/MediaLibrary/`, `src/containers/ShareSelector/`; NEW store: `src/state/blob/*` |
| E.4.f — ACL picker | Reuses `ShareSelector` from E.4.e |
| E.4.g — Forgery-proof groups | No new UI; existing MembersAndGroups pane is the UI |
| E.4.h — Streaming | NEW sidecar: `src/sidecars/streaming/` |
| E.4.i — Sidecar marketplace | NEW: `src/api/content/agentDirectory/`, `.../sidecarManifest/`, NEW UI optional |
| E.4.j — Personal vault | None today |
| E.4.k — Paid content | N/A (deferred) |
| E.4.l — Pre-signed invite links | NEW UI: `src/containers/CreateInvite/`, `src/containers/AcceptInvite/`; NEW sidecar: `src/sidecars/invite-redemption/`; Tauri URL-scheme handler in `src-tauri/`; new content schema for invite + redemption payloads |

## Quick reference — which features carry data migration

| Feature | Migration step |
|---|---|
| E.4.a — DMs | Classifier auto-restamps to `DirectMessage` |
| E.4.b — Group chat | Forward-looking; no migration |
| E.4.c — Hive discovery | Classifier auto-restamps to `OpenWrite { target: None }` |
| E.4.d — Member-request | Classifier auto-restamps to `OpenWrite { target: Some }` |
| E.4.e — Media library | Default `Public`; humm-tauri post-migration re-stamps |
| E.4.f — ACL picker | No migration; UX only |
| E.4.g — Forgery-proof groups | Default `Public`; humm-tauri post-migration runs `create_group_genesis` + `create_group_membership` per legacy group, then re-stamps |
| E.4.h — Streaming | Forward-looking; no migration |
| E.4.i — Sidecar marketplace | Forward-looking; no migration |
| E.4.j — Personal vault | Same as E.4.g (singleton personal group via classifier default + re-stamp) |
| E.4.k — Paid content | N/A (deferred) |
| E.4.l — Pre-signed invite links | Forward-looking; no data migration. Ship against pass-3 DNA (no need to wait for pass-4 — uses `Public` + `OpenWrite` variants that pass-4 leaves unchanged). |

This doc is the contract; humm-tauri implementation lives downstream
and will reference these section IDs in commit messages and code
comments.
