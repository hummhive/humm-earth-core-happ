# Pass-5 — Hive Owner role + ACL hardening: humm-tauri integration

Handoff for the humm-tauri team to integrate the pass-5 integrity-zome fork.
Pass-5 **intentionally bumps the DNA hash** (first integrity change since pass-4)
and bundles the first-class hive **Owner** role, the reader-read-only bugfix,
role-grant hardening, and several adjacent coordinator additions.

**DNA hash:** `uhC0k2dXMIa1yI-V4ibCWMiTY5G6-p0laq6IOAVQ2F8XXReDHSxyS`
(was pass-4 `uhC0k26bYG0qmTCFk4_D996GRCTecEtMdL5pXyvCUu0ACJN12omCV`).
Coordinator hot-swap does NOT apply — this is a new cell; pass-4 data migrates
forward via `scripts/migrate-dna.ts` (see Migration below). Toolchain also moves
to holochain 0.6.1 (hdk 0.6.1 / hdi 0.7.1), matching the runtime you already run.

---

## TL;DR — what changed for the frontend

1. **Hive ownership is now transferable** by a two-party handshake (offer →
   accept). One owner per hive; admins cannot demote the owner.
2. **`Role::Owner` is NO LONGER grantable via `HiveMembership`** — the validator
   rejects it. Remove 'Owner' from the grantable role picker; add a "Transfer
   ownership" action wired to the handshake.
3. **Reader is read-only** — the content-delete validator no longer authorizes
   the `reader` bucket on non-DM content (the long-standing reader-can-delete
   bug). DM recipients can still delete their DMs.
4. **Governance owner-gate must repoint** from `authorMembershipHash === null`
   to `get_member_hive_role(me) === 'Owner'` (HAZARD #1 below).
5. New externs: the owner handshake + role reads + `delete_group_genesis`,
   `revoke_hive_membership`, `redeem_invite_grant`, and the four read helpers
   you requested (`get_hive_owner`, `content_summary`,
   `my_pair_shared_secret_exists`, `changes_since`).

---

## 1. Owner handshake protocol

Hive ownership starts with the genesis author (the founder) as the implicit
owner. It transfers only when the current owner publishes an OFFER and the named
recipient publishes an ACCEPT. Resolution of "who is the owner now" is a
deterministic fold over the offer→accept lineage.

### Entries (integrity, immutable)
- `HiveOwnerHandoffOffer { hive_genesis_hash: ActionHash, to_agent: AgentPubKey,
  offerer_owner_accept_hash: Option<ActionHash>, created_at_microseconds: i64 }`
  — `offerer_owner_accept_hash` is `None` when the offerer is the genesis author
  (lineage root) or `Some(accept)` citing the offerer's own prior acceptance.
- `HiveOwnerHandoffAccept { offer_hash: ActionHash }`.

### Externs (coordinator)
| Extern | Shape | Notes |
|---|---|---|
| `initiate_owner_handoff` | `{ hive_genesis_hash, to_agent, offerer_owner_accept_hash: Option<ActionHash> } -> ActionHash` | commits the offer + a recipient-keyed discovery link; returns the offer hash |
| `accept_owner_handoff` | `{ offer_hash } -> ActionHash` | recipient commits the accept + the hive-keyed resolution link; returns the accept hash |
| `cancel_owner_handoff` | `offer_hash: ActionHash -> ()` | offerer deletes their discovery link (soft cancel; if already accepted, the accept stands) |
| `list_pending_owner_handoffs` | `() -> Vec<{ offer_hash, offer }>` | offers addressed to the CALLER (recipient pending-offer surface) |
| `get_member_hive_role` | `{ hive_genesis_hash, agent } -> Option<HiveRole>` | `Some(Owner)` if `agent` is the resolved owner; else the latest non-Owner membership role; else `None` |
| `list_member_hive_roles` | `{ hive_genesis_hash, agents: Vec<AgentPubKey> } -> Vec<(AgentPubKey, Option<HiveRole>)>` | resolves the owner ONCE; batched (no per-row N+1) |
| `get_hive_owner` | `hive_genesis_hash: ActionHash -> AgentPubKey` | the current owner pubkey (resolved lineage, not merely the genesis author) |

### The transfer ceremony (3 steps, for the UI)
1. Current owner calls `initiate_owner_handoff(H, B)` AND grants B a permanent
   **Admin** `HiveMembership` (so B can run the hive operationally).
2. B sees the offer via `list_pending_owner_handoffs` and calls
   `accept_owner_handoff(offer)`.
3. From the accept onward, `get_member_hive_role(B) === 'Owner'` and B may grant
   Admins (citing B's accept as `grantor_owner_accept_hash`).
The offerer remains owner until the accept lands — no limbo window.

### Guarantees and honest residuals
- **Guarantee:** integrity proves "X is a validly-descended owner" by O(1)
  induction over the lineage and FORBIDS Owner-via-membership; the coordinator
  deterministically resolves the single CURRENT owner (`resolve_current_owner`)
  and gates the owner-only mutators (Admin grant, owner-revoke) on it.
- **Residual (documented, accepted by the team):** integrity's `is_lineage_owner`
  is an EVER-owner predicate (a validator cannot detect a completed downstream
  transfer — acceptance/cancel live off the offerer's chain + are chain-fork
  evadable). So a malicious PAST owner (genesis author OR any prior handoff
  recipient) can fork the lineage — author a competing offer to a colluder and
  win the deterministic resolution — to RE-SEIZE current ownership. This is an
  irreducible cross-chain double-spend (no global consensus order on an
  agent-centric DHT); confirmed by the security review + an oracle. Blast radius
  is GOVERNANCE only (Admin-grant gate, revoke-protect, owner UI badge) — NOT
  content decryption. Transfer is final against admins + third parties, just not
  against a former owner. "Ownership handed off, not cryptographically taken away."
- **Determinism + detection:** honest nodes never split-brain (links sorted before
  the bound; an unresolved-parent offer is excluded, not promoted to root); forks
  resolve to the smallest offer `ActionHash` (raw bytes) identically on every node.
  A fork is `warn!`ed and surfaced by `is_ownership_contested(hive) -> bool` so the
  UI can flag a contested transfer for out-of-band review.

### BDD
- **Given** founder A's hive, **When** A `initiate_owner_handoff(H,B)` + grants B
  Admin, then B `accept_owner_handoff`, **Then** `get_member_hive_role(B)=Owner`
  and `get_member_hive_role(A)≠Owner`.
- **Given** the transfer above, **When** A (former owner) `create_hive_membership(role:Admin)`,
  **Then** rejected `"only the current hive owner may grant the Admin role"`;
  **When** B (current owner) does it citing B's accept, **Then** succeeds.
- **Given** any hive, **When** `create_hive_membership(role:Owner)`, **Then**
  rejected `"the Owner role cannot be granted via membership; use the owner-handoff handshake"`.
- **Given** A→B→C transfers, **When** any node calls `get_member_hive_role(C)`,
  **Then** all nodes agree `Owner`.

---

## 2. Frontend cutover — HAZARDS

1. **#1 — the governance owner gate.** `isActiveHiveOwner = authorMembershipHash
   === null` (Members/index.tsx ~56) MISLABELS owners after a transfer. Repoint
   the GOVERNANCE gate (`isActiveHiveOwner` / `isHiveOwner` / `canManageHiveRole`)
   to `get_member_hive_role(me) === 'Owner'`. KEEP `authorMembershipHash === null`
   ONLY for `author_membership_hash` stamping (that path is unchanged —
   `get_latest_membership` / `list_my_hives` still return `None`/`null` for the
   implicit owner).
2. **Owner offers are NOT an Inbox event.** Poll `list_pending_owner_handoffs`;
   do NOT expect a new `InboxEvent`. (We deliberately avoided the inbox because
   `dmSweep.ts probeAndDrainInbox` consumes unknown events and would destroy the
   offer pointer.)
3. **`get_member_hive_role` returns `Option<HiveRole>`** — `None` = no governance
   role (render "—", not an error). For the Members roster, prefer
   `list_member_hive_roles(agents)` (one owner-resolve, batched).
4. **Role picker:** remove 'Owner' from the ManageMember `HIVE_ROLE_OPTIONS`
   grantable list; CreateInvite `ROLES` already excludes it. KEEP 'Owner' in the
   `HiveRole` type union + all display/label paths. Add a separate "Transfer
   ownership" action → the handshake.
5. **Reject-string contract** (Section 4) — your `groupMembershipRejections.ts`
   regexes must add the new substrings.
6. **Migration** of pass-4 Owner-via-membership hives (Section 5).
7. **Reader-read-only** (Section 3) — any UI that offered reader-delete on hive
   content will now get a reject.
8. **Bonus:** the `HAPP-WORKAROUND(list_my_hives-decode)` poll in
   joinedHiveWatch.ts can be retired on pass-5 (the decode bug was fixed in
   pass-4).

---

## 3. Reader read-only + the edit/delete asymmetry

`validate_delete_encrypted_content` is now variant-aware on `acl_spec`:
- `DirectMessage` → the `reader` bucket IS the recipient set; recipients may
  delete their copy.
- `HiveGroup | Public | OpenWrite` → only `owner ∪ admin ∪ writer` may delete
  (reader DROPPED). The original author may always delete, any variant.

In-place **update stays author-only** (unchanged). This asymmetry is intentional:
a Delete carries no witness payload, so "edit ≡ delete" cannot be made safe.
"Writers manage all hive docs" is delivered by **re-author + delete**, which
yields a NEW action hash authored by the editor — permalinks, links, and comments
do NOT carry, and authorship shifts. Do NOT surface an in-place "Edit" on peers'
docs; a moderation/"Replace" action must warn that attribution + comments + links
won't carry. True collaborative in-place edit is out of scope for pass-5.

---

## 4. Reject-string contract (verbatim, wire-stable)

Assert with `expect(String(err)).toContain(<substr>)`:
- `the Owner role cannot be granted via membership; use the owner-handoff handshake`
- `only the hive Owner may grant the Admin role` (integrity floor)
- `only the current hive owner may grant the Admin role` (coordinator precheck)
- `cannot assign a membership role to the hive's founding owner`
- `cannot hand off ownership to yourself`
- `offer author is not an owner of the hive`
- `accept author is not the offer's to_agent`
- `refusing to revoke the current hive owner's membership`
- `refusing to delete a group with live members`
- `invite max_uses exhausted`

---

## 5. Migration — pass-4 → pass-5

`scripts/migrate-dna.ts` export → import. The new constraint: the pass-5
validator REJECTS `HiveMembership{role:Owner}`. So the migration MUST transform
or skip any pass-4 secondary Owner-via-membership grant on import:
- The genesis author becomes the lineage root automatically (no membership
  needed; `is_lineage_owner(author, hive, None)` is true).
- Any secondary `HiveMembership{role:Owner}` is DROPPED or down-converted to
  Admin (re-issued as Admin by the current owner during/after migration).
Post-migration, `get_member_hive_role` only ever emits `Owner` from the lineage,
never from a membership — so legacy Owner memberships are inert.

---

## 6. Read helpers requested by humm-tauri (RC)

| Extern | Shape | Granted? |
|---|---|---|
| `get_hive_owner` | `ActionHash -> AgentPubKey` | yes (public DHT) |
| `content_summary` | `{ hive_genesis_hash, content_types: Vec<String> } -> Vec<{ content_type, count, latest_action_micros: Option<i64>, latest_action_hash: Option<ActionHash> }>` | yes (public DHT) |
| `my_pair_shared_secret_exists` | `{ active_hive_genesis_hash, content_type, group_id } -> bool` | NO (local-chain read) |
| `changes_since` | `{ hive_genesis_hash, content_types: Vec<String>, since_seq: u32 } -> { new_action_count, latest_seq }` | NO (local-chain read) |
| `is_ownership_contested` | `ActionHash -> bool` | yes (public DHT) |

Deviations from the original request: `content_summary` + `changes_since` take an
explicit `content_types` list (no DHT index enumerates a hive's content-types);
`my_pair_shared_secret_exists` takes the opaque `group_id` (as
`fetch_pair_ss_with_hive_check` does) and checks the caller's LOCAL chain
(authoritative, immune to the 30s propagation gap). The two local-chain readers
are NOT cap-granted (same policy as `get_messages_since` / `get_last_probe`); the
local UI calls them directly over the app interface. `changes_since` reflects the
caller's OWN writes — for peers' new content use `content_summary`'s latest hint.

---

## 7. Other coordinator additions

- `delete_group_genesis(group_genesis_hash) -> ActionHash` — author-gated
  cosmetic tombstone; refuses `"refusing to delete a group with live members"`;
  sweeps the caller's own discovery links. A deleted GroupGenesis still resolves
  via `must_get_valid_record`, so it is NOT a security revocation.
- `revoke_hive_membership({ membership_hash, new_expiry: i64,
  grantor_membership_hash, grantor_owner_accept_hash }) -> HiveMembershipResponse`
  — revocation = re-issue the same grant with a past expiry (mirrors
  `revoke_group_membership`); refuses to revoke the current owner's membership.
- `redeem_invite_grant({ invite_action_hash, max_uses: Option<u32>, membership })`
  — advisory `max_uses` soft-cap via idempotent redemption markers; the count is
  ADVISORY (approver-authored, griefable) — the real authority is the validated
  `HiveMembership`. Exhaustion rejects `"invite max_uses exhausted"`.
- `list_by_author` gains `since_ts: Option<Timestamp>` + `limit: Option<usize>`
  (oldest-first; truncation drops the NEWEST so a bounded page keeps the range
  start). Unblocks the C2 placeholders.

---

## 8. Honest microcopy

- Transfer confirm: "Hand off hive ownership to X? You remain a member; the
  hive's founder keeps a recovery key and cannot be fully removed."
- Recipient prompt: "X offers you ownership of this hive. Accept?"
- FORBIDDEN: "sole cryptographic owner" / "X can no longer touch the hive" —
  untrue (see the documented residual). "Undemotable" means the Owner role +
  Admin-grant power; a current owner can still revoke a member's ops-Admin.

---

## 9. Glossary — hive-Owner vs group-Owner

- **hive Owner** = the single transferable hive authority (this doc). Resolved by
  `get_hive_owner` / `get_member_hive_role`.
- **group Owner** = the `Role::Owner` bucket within a `GroupGenesis` ACL
  (unchanged; group ownership is NOT single-owner and is not handshake-transferred).
The `HiveRole` enum (`Owner > Admin > Writer > Reader`) is shared by both; context
(hive vs group) disambiguates. Update `../humm-tauri/GLOSSARY.md` accordingly.
