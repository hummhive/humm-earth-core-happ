//! [`EncryptedContent`] entry and every link validator that hangs off it.
//!
//! ## Pass-3 wire shape: [`AclSpec`] variant model
//!
//! Pass-2 carried four security-load-bearing fields at the top of the
//! header (`hive_id`, `hive_genesis_hash`, `author_membership_hash`,
//! `acl`). Pass-3 collapses them into a single `acl_spec: AclSpec`
//! discriminated union so every communication pattern in the
//! humm-tauri product (intra-hive group content, cross-hive DMs,
//! world-readable public content, open-write outsider-knock and
//! cross-network discovery) maps to a scope variant whose validator
//! has the right contract for that scope.
//!
//! Variants:
//! - [`AclSpec::HiveGroup`] — group-scoped content under a hive. The
//!   author must hold Writer+ in the hive AND Writer+ in every group
//!   listed in `group_acl.*`. This is the headline cryptographic
//!   enforcement; closes the group/role/ACL poisoning class.
//! - [`AclSpec::DirectMessage`] — pair or small-group DM, cross-hive
//!   viable. Author must be in `recipients`; cardinality bounded.
//! - [`AclSpec::Public`] — world-readable content authored under a
//!   hive context. Author must hold Writer+ in the hive; recipient
//!   set unconstrained.
//! - [`AclSpec::OpenWrite`] — outsider knock + cross-network
//!   discovery. No hive/group membership required; only author
//!   identity (pass-1 `check_author_matches_header`) + optional
//!   target HiveGenesis existence.
//!
//! ## Pass-4 — G-6.2 recipient-set integrity SHIPPED
//!
//! `AclSpec::HiveGroup` now carries a `recipient_witnesses:
//! Vec<RecipientWitness>` field. Every pubkey listed in
//! `public_key_acl.{owner,admin,writer,reader}` MUST be backed by a
//! witness that points at a real, validated [`crate::group::GroupMembership`]
//! in a group that lives in the entry's `group_acl` at a dominating
//! bucket. The validator checks the cross-reference at commit time
//! (cardinality bound + bidirectional set-equality + per-witness
//! membership fetch + bucket-dominance + role-satisfaction + expiry).
//! Closes attack #5: a modified coordinator can no longer inject a
//! foreign pubkey into the reader bucket of a private group post to
//! receive remote-signal notifications. Decryption gating via
//! SharedSecrets was always intact; this hardens the routing-fan-out
//! attribution to match.
use hdi::hash_path::path::Component;
use hdi::prelude::*;

use crate::group::{check_group_authority, fetch_group_membership};
use crate::hive::{check_hive_authority, role_satisfies, Role};

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct EncryptedContent {
    pub header: EncryptedContentHeader,
    pub bytes: SerializedBytes,
}

/// On-DHT header for encrypted content.
///
/// **Security-load-bearing fields** (validators enforce):
/// - `acl_spec` — the per-scope authority contract; see [`AclSpec`].
/// - `revision_author_signing_public_key` — must equal `action.author`
///   (the pass-1 `check_author_matches_header` guard).
///
/// **Routing/display fields** (validators ignore for security; UI
/// consumes):
/// - `id` — opaque app-level identifier (humm-tauri's content squuid).
/// - `display_hive_id` — human-readable hive alias (was `hive_id` in
///   pass-1/2). Kept as a `String` for migration continuity. Security
///   is rooted in `acl_spec`; this field is NOT trusted.
/// - `content_type` — opaque app-level content kind ("dm", "post",
///   "pair-ss", ...). Used by the path-recompute checks (it is the
///   second component of every hive-scoped link path).
/// - `public_key_acl` — per-entry access control list keyed by
///   holohash pubkeys. Load-bearing for I-A delete authority and for
///   `DirectMessage` recipient binding (validator pins this == DM
///   recipients); routing hint for signal fan-out for other variants.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct EncryptedContentHeader {
    pub id: String,
    pub display_hive_id: String,
    pub content_type: String,
    pub acl_spec: AclSpec,
    pub public_key_acl: Acl,
    pub revision_author_signing_public_key: String,
}

/// Per-entry access control by `GroupGenesis` action hash (pass-3
/// cryptographic replacement for pass-2's `Acl` keyed by humm-tauri
/// group squuid strings). Every hash listed here MUST resolve to a
/// real `GroupGenesis` in the same hive as the entry's
/// `hive_genesis_hash`; the integrity validator enforces this.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct AclByGroupGenesis {
    pub owner: ActionHash,
    pub admin: Vec<ActionHash>,
    pub writer: Vec<ActionHash>,
    pub reader: Vec<ActionHash>,
}

/// ACL bucket discriminator for [`RecipientWitness`] and the
/// bucket-dominance rule in [`validate_hivegroup_acl`].
///
/// Order-derived dominance: `Owner > Admin > Writer > Reader`. A
/// witness in a higher bucket satisfies every lower bucket too — if
/// Alice holds group-Admin in an admin-bucket group, humm-tauri may
/// stamp her as the Writer- or Reader-bucket witness using the same
/// admin-bucket membership. Matches the pass-3 link-validator
/// semantics for `HummContent{Owner,Admin,Writer,Reader}` (admin ⊆
/// writer ⊆ reader).
///
/// The variant set + ordering MUST stay byte-stable across releases;
/// see [`bucket_required_role`] for the witness-role-satisfaction
/// mapping that depends on it.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AclBucket {
    Owner,
    Admin,
    Writer,
    Reader,
}

impl AclBucket {
    /// Does `self` dominate `other`? `self` dominates `other` iff its
    /// role is at least as high — an Owner-bucket witness satisfies
    /// Admin/Writer/Reader; an Admin-bucket witness satisfies
    /// Writer/Reader; etc. Used by the bidirectional cross-check in
    /// [`validate_hivegroup_acl`] step 5.
    pub fn dominates(self, other: AclBucket) -> bool {
        role_satisfies(bucket_required_role(self), bucket_required_role(other))
    }
}

/// The role a [`crate::group::GroupMembership`] must satisfy to back a
/// witness in `bucket`. Owner-bucket → group Owner; Admin → group
/// Admin; Writer → group Writer; Reader → group Reader.
///
/// Centralised so the validator + future helpers cannot drift from the
/// single source-of-truth ordering on [`AclBucket`].
pub(crate) fn bucket_required_role(bucket: AclBucket) -> Role {
    match bucket {
        AclBucket::Owner => Role::Owner,
        AclBucket::Admin => Role::Admin,
        AclBucket::Writer => Role::Writer,
        AclBucket::Reader => Role::Reader,
    }
}

/// Per-recipient membership witness stamped on every
/// `AclSpec::HiveGroup` entry by the writer. The writer asserts:
///
/// - `pubkey` holds the membership at `membership_hash`,
/// - that membership grants `bucket`-level role (or higher), AND
/// - the membership's `group_genesis_hash` is in the corresponding
///   bucket of the entry's `group_acl` (Owner-bucket → `group_acl.owner`;
///   Admin → `admin ∪ owner`; Writer → `admin ∪ writer ∪ owner`;
///   Reader → all four).
///
/// The validator re-verifies every assertion at commit time via
/// `must_get_valid_record(membership_hash)`. A modified coordinator
/// stamping a witness for a pubkey that is not a real group member
/// (attack #5) fails at the per-witness fetch / role / group-match
/// check; the entry is rejected and never reaches the DHT.
///
/// Why per-witness, not per-pubkey on a map: keeps the witness
/// authoring locally cheap (one `get_latest_group_membership` per
/// pubkey + bucket the humm-tauri helper already pre-computes for the
/// roster) and validator iteration cost bounded by
/// [`HIVEGROUP_MAX_WITNESSES`].
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct RecipientWitness {
    pub pubkey: AgentPubKey,
    pub bucket: AclBucket,
    pub membership_hash: ActionHash,
}

/// Maximum recipient count on a `DirectMessage`. Bounds the
/// per-content fanout amplification surface (a modified coordinator
/// otherwise could write a DM with thousands of recipients,
/// triggering one `send_remote_signal` per recipient on every
/// receiver). 32 covers small-group DMs (Slack-style threads,
/// pair-and-up patterns); larger group chats use
/// `AclSpec::HiveGroup` with a custom group.
pub const DM_MAX_RECIPIENTS: usize = 32;

/// Maximum total group references on a single `AclSpec::HiveGroup`
/// entry (the sum of `group_acl.owner` + `admin.len()` + `writer.len()`
/// + `reader.len()`). Bounds the per-content amplification surface:
/// every group hash forces one `must_get_valid_record` for the group
/// genesis plus potentially two more for the per-group authority chain
/// walk. Without this bound a modified coordinator could commit
/// content with thousands of group hashes, forcing every validating
/// peer to issue O(N) network calls per validate. 64 covers realistic
/// hive structures (a hive with 64 distinct groups attached to one
/// piece of content is already an outlier); revisit if production
/// usage demands more.
pub const GROUP_ACL_MAX_GROUPS: usize = 64;

/// Maximum recipient_witnesses count per HiveGroup entry. Each witness
/// triggers one `must_get_valid_record` at commit time; unbounded
/// would be a validator-DoS surface (a modified coordinator could
/// stamp thousands of witnesses, forcing every validating peer to
/// issue O(N) network calls per entry). 256 covers any realistic
/// group-scoped content (a 256-recipient group is already an outlier;
/// mirrors the [`DM_MAX_RECIPIENTS`] rationale at higher cardinality
/// since group fan-out is naturally larger than DM fan-out).
pub const HIVEGROUP_MAX_WITNESSES: usize = 256;

/// First-class per-scope authority contract on every `EncryptedContent`
/// entry. Variant-dispatched at commit time.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub enum AclSpec {
    /// In-hive group-scoped content. Author and recipients are
    /// constrained by hive-and-group membership. This is the
    /// load-bearing pass-3 variant — closes the modified-coordinator
    /// poisoning class for group content.
    HiveGroup {
        /// Cryptographic hive identity.
        hive_genesis_hash: ActionHash,
        /// `None` = author IS the hive genesis author (implicit
        /// Owner). `Some(hash)` = author's authorising `HiveMembership`.
        author_membership_hash: Option<ActionHash>,
        /// Per-bucket group allowlist by `GroupGenesis` action hash.
        /// All groups MUST belong to `hive_genesis_hash`.
        group_acl: AclByGroupGenesis,
        /// `None` = author authority via the group-author (Path A) or
        /// hive-sovereign (Path B) route in
        /// [`crate::group::check_group_authority`]. `Some(hash)` =
        /// author's authorising `GroupMembership` (Path C).
        author_group_membership_hash: Option<ActionHash>,
        /// G-6.2: per-recipient membership witnesses. MUST cover every
        /// pubkey in [`EncryptedContentHeader::public_key_acl`]
        /// `.{owner, admin, writer, reader}` exactly once (validator-
        /// checked one-to-one). Each witness must point at a real
        /// [`crate::group::GroupMembership`] for the named pubkey,
        /// granting a role that satisfies the bucket, in a group that
        /// is in the corresponding (or higher) bucket of `group_acl`,
        /// and unexpired at the entry's `action.timestamp`.
        /// Cardinality bounded by [`HIVEGROUP_MAX_WITNESSES`].
        recipient_witnesses: Vec<RecipientWitness>,
    },
    /// Direct sender↔recipient(s). Pair or small-group; cross-hive
    /// viable (no hive/group membership check on recipients). The
    /// validator enforces:
    /// - `action.author` ∈ `recipients`
    /// - `2 <= recipients.len() <= DM_MAX_RECIPIENTS`
    /// - `public_key_acl.reader` equals `recipients` (for I-A delete
    ///   authority symmetry — either party may delete; routing fanout
    ///   matches).
    DirectMessage { recipients: Vec<AgentPubKey> },
    /// World-readable content authored under a hive context. Author
    /// must hold Writer+ in the named hive; recipient set
    /// unconstrained (humm-tauri may use `public_key_acl.reader =
    /// ['*']` or empty interchangeably as a routing hint).
    Public {
        hive_genesis_hash: ActionHash,
        author_membership_hash: Option<ActionHash>,
    },
    /// Open-write content authored without prior hive membership in
    /// the target hive. For: member-request (outsider knocks),
    /// cross-network hive-discovery (no hive scope). Validator only
    /// runs `check_author_matches_header`; if `target_hive_genesis_hash`
    /// is `Some`, that hash MUST resolve to a real `HiveGenesis`.
    OpenWrite {
        target_hive_genesis_hash: Option<ActionHash>,
    },
}

impl EncryptedContentHeader {
    /// Return the hive context this header binds to, if any. Used by
    /// hive-scoped link validators to recover the path's first
    /// component without variant-matching at each call site.
    /// - `HiveGroup` / `Public` → the cryptographic hive identity.
    /// - `OpenWrite { target_hive_genesis_hash: Some(h) }` → the
    ///   target hive (member-request flow; entry lives in that hive's
    ///   discovery index).
    /// - `DirectMessage` / `OpenWrite { target: None }` → `None`.
    pub fn hive_context(&self) -> Option<&ActionHash> {
        match &self.acl_spec {
            AclSpec::HiveGroup {
                hive_genesis_hash, ..
            } => Some(hive_genesis_hash),
            AclSpec::Public {
                hive_genesis_hash, ..
            } => Some(hive_genesis_hash),
            AclSpec::OpenWrite {
                target_hive_genesis_hash,
            } => target_hive_genesis_hash.as_ref(),
            AclSpec::DirectMessage { .. } => None,
        }
    }

    /// Return the per-group ACL for `HiveGroup` content, or `None` for
    /// the other variants (which have no group_acl by design). Used by
    /// the `HummContent{Owner,Admin,Writer,Reader}` link validators.
    pub fn group_acl(&self) -> Option<&AclByGroupGenesis> {
        match &self.acl_spec {
            AclSpec::HiveGroup { group_acl, .. } => Some(group_acl),
            _ => None,
        }
    }
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct Acl {
    pub owner: String,
    pub admin: Vec<String>,
    pub writer: Vec<String>,
    pub reader: Vec<String>,
}

// Cross-check the sender-controlled `revision_author_signing_public_key`
// field against the cryptographically-attested `action.author`. Without
// this guard the field is forgeable: any peer with a custom DNA can
// commit an entry claiming another agent's signing pubkey, and every
// downstream consumer (DMs, member entries, audit trails) trusts the
// header value as the sender identity. Comparing the string form is
// sound because `AgentPubKey::to_string()` emits the same multibase
// holohash form (`'u' + URL_SAFE_NO_PAD(39 bytes)`) that
// `@holochain/client::encodeHashToBase64` writes into the header.
fn check_author_matches_header(
    action_author: &AgentPubKey,
    header_pubkey: &str,
) -> ValidateCallbackResult {
    let attested = action_author.to_string();
    if attested != header_pubkey {
        return ValidateCallbackResult::Invalid(format!(
            "revision_author_signing_public_key ({}) does not match action.author ({})",
            header_pubkey, attested,
        ));
    }
    ValidateCallbackResult::Valid
}

/// Variant-dispatch entrypoint for create + update validation. Runs the
/// pass-1 author-vs-header guard once, then delegates to the
/// per-variant validator that carries the right authority contract.
fn run_content_validators(
    author: &AgentPubKey,
    timestamp: &Timestamp,
    content: &EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    let header_check =
        check_author_matches_header(author, &content.header.revision_author_signing_public_key);
    if !matches!(header_check, ValidateCallbackResult::Valid) {
        return Ok(header_check);
    }
    match &content.header.acl_spec {
        AclSpec::HiveGroup {
            hive_genesis_hash,
            author_membership_hash,
            group_acl,
            author_group_membership_hash,
            recipient_witnesses,
        } => validate_hivegroup_acl(
            author,
            timestamp,
            hive_genesis_hash,
            author_membership_hash.as_ref(),
            group_acl,
            author_group_membership_hash.as_ref(),
            recipient_witnesses,
            &content.header.public_key_acl,
        ),
        AclSpec::DirectMessage { recipients } => {
            validate_directmessage_acl(author, recipients, &content.header.public_key_acl)
        }
        AclSpec::Public {
            hive_genesis_hash,
            author_membership_hash,
        } => check_hive_authority(
            author,
            hive_genesis_hash,
            author_membership_hash.as_ref(),
            Role::Writer,
            timestamp,
        ),
        AclSpec::OpenWrite {
            target_hive_genesis_hash,
        } => validate_openwrite_acl(target_hive_genesis_hash.as_ref()),
    }
}

/// `AclSpec::HiveGroup` validator. The author must hold Writer+ in
/// the hive AND Writer+ in every group listed in `group_acl.*`. Every
/// group hash in `group_acl` MUST resolve to a `GroupGenesis` in the
/// same hive (closes cross-hive group claim, attack #9).
///
/// **G-6.2 recipient-set integrity (pass-4).** The `recipient_witnesses`
/// list MUST cover every pubkey in `public_key_acl` exactly once
/// (bidirectional cross-check) and each witness MUST resolve to a
/// real, unexpired, role-sufficient [`crate::group::GroupMembership`]
/// for the named pubkey in a group present in the corresponding (or
/// higher) bucket of `group_acl`. Closes attack #5: recipient-list
/// forgery for routing fan-out.
///
/// ## Step order (fail-fast)
///
/// 1. Cardinality bound on `group_acl` (cheap, pre-fetch).
/// 2. Hive Writer+ authority (1 fetch).
/// 3. Per-group cross-hive consistency + per-group Writer+ authority
///    (≤ 3 fetches per group, bounded by [`GROUP_ACL_MAX_GROUPS`]).
/// 4. Cardinality bound on `recipient_witnesses` (cheap, pre-fetch).
/// 5. Bidirectional set-equality between `public_key_acl` buckets and
///    witnesses with bucket-dominance check (cheap, pre-fetch — pure
///    string compares + role-table lookups).
/// 6. Per-witness [`crate::group::GroupMembership`] verification
///    (1 fetch per witness, bounded by [`HIVEGROUP_MAX_WITNESSES`]).
///
/// Steps 1-3 stay pre-witness so hive/group authority failure short-
/// circuits the entire validator before any witness-fetch cost.
/// Steps 4-5 are pre-fetch within the witness suite so the cheap
/// cross-check rejections fire before the per-witness fetches.
fn validate_hivegroup_acl(
    author: &AgentPubKey,
    timestamp: &Timestamp,
    hive_genesis_hash: &ActionHash,
    author_membership_hash: Option<&ActionHash>,
    group_acl: &AclByGroupGenesis,
    author_group_membership_hash: Option<&ActionHash>,
    recipient_witnesses: &[RecipientWitness],
    public_key_acl: &Acl,
) -> ExternResult<ValidateCallbackResult> {
    // Step 1 — group_acl cardinality bound. Same amplification
    // mitigation as DM_MAX_RECIPIENTS. Each group hash forces network
    // calls; unbounded fan-out is a validator-DoS surface.
    let total_groups = 1usize
        .saturating_add(group_acl.admin.len())
        .saturating_add(group_acl.writer.len())
        .saturating_add(group_acl.reader.len());
    if total_groups > GROUP_ACL_MAX_GROUPS {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveGroup group_acl references {total_groups} groups; \
             maximum is GROUP_ACL_MAX_GROUPS = {GROUP_ACL_MAX_GROUPS}",
        )));
    }
    // Step 2 — hive authority: Writer+ in the named hive (mirrors pass-2).
    let hive_check = check_hive_authority(
        author,
        hive_genesis_hash,
        author_membership_hash,
        Role::Writer,
        timestamp,
    )?;
    if !matches!(hive_check, ValidateCallbackResult::Valid) {
        return Ok(hive_check);
    }
    // Step 3 — for each group listed in group_acl, the author must
    // hold Writer+. The same author_group_membership_hash witness is
    // reused across all groups; this is a deliberate simplification —
    // the common humm-tauri pattern publishes content under groups the
    // author is a member of via one specific membership. Cross-group
    // multi-membership writes are still possible by issuing separate
    // entries.
    for group_hash in std::iter::once(&group_acl.owner)
        .chain(group_acl.admin.iter())
        .chain(group_acl.writer.iter())
        .chain(group_acl.reader.iter())
    {
        // Per-group cross-hive consistency check. fetch_group_genesis
        // returns the parent hive; reject if it does not match the
        // entry's hive (attack #9).
        let (_group_author, group) = crate::group::fetch_group_genesis(group_hash)?;
        if &group.hive_genesis_hash != hive_genesis_hash {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "HiveGroup acl references group {group_hash} in hive {} \
                 but entry claims hive {hive_genesis_hash}",
                group.hive_genesis_hash,
            )));
        }
        let group_check = check_group_authority(
            author,
            group_hash,
            author_group_membership_hash,
            author_membership_hash,
            Role::Writer,
            timestamp,
        )?;
        if !matches!(group_check, ValidateCallbackResult::Valid) {
            return Ok(group_check);
        }
    }
    // Steps 4-6 — G-6.2 recipient-witness verification
    // (cardinality → bidirectional PKA cross-check → per-witness fetch).
    validate_recipient_witnesses(recipient_witnesses, public_key_acl, group_acl, timestamp)
}

/// G-6.2 — recipient-witness verification. Three sub-checks:
///
/// - **Cardinality** (step 4): bounded by [`HIVEGROUP_MAX_WITNESSES`]
///   so a modified coordinator cannot DoS the validator with thousands
///   of witnesses.
/// - **Bidirectional cross-check** (step 5, pre-fetch): every pubkey
///   in `public_key_acl.{owner,admin,writer,reader}` must be backed by
///   exactly one witness whose `bucket` dominates the PKA bucket the
///   pubkey lives in, AND every witness's `pubkey` must appear in the
///   PKA bucket it claims. No over-claiming, no under-claiming, no
///   duplicates.
/// - **Per-witness verification** (step 6, fetch): each witness's
///   `membership_hash` resolves to a real [`crate::group::GroupMembership`]
///   for the named pubkey, in a group present in the bucket
///   (or any dominating bucket) of `group_acl`, granting a role that
///   satisfies the witness bucket, unexpired at `timestamp`.
///
/// Extracted as a free function so the host-side test module can
/// exercise the pre-fetch branches (steps 4 + 5) without a DHT.
fn validate_recipient_witnesses(
    witnesses: &[RecipientWitness],
    public_key_acl: &Acl,
    group_acl: &AclByGroupGenesis,
    timestamp: &Timestamp,
) -> ExternResult<ValidateCallbackResult> {
    // Step 4 — cardinality.
    if witnesses.len() > HIVEGROUP_MAX_WITNESSES {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveGroup recipient_witnesses.len() = {} exceeds \
             HIVEGROUP_MAX_WITNESSES = {}",
            witnesses.len(),
            HIVEGROUP_MAX_WITNESSES,
        )));
    }
    // Step 5 — bidirectional cross-check with bucket dominance.
    if let Some(invalid) = check_witness_pka_bidirectional(witnesses, public_key_acl) {
        return Ok(invalid);
    }
    // Step 6 — per-witness fetch + verification.
    for witness in witnesses {
        let result = verify_recipient_witness(witness, group_acl, timestamp)?;
        if !matches!(result, ValidateCallbackResult::Valid) {
            return Ok(result);
        }
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Pre-fetch bidirectional cross-check between PKA buckets and the
/// witness list. Returns `Some(Invalid)` on any forgery (PKA pubkey
/// without dominating witness, or witness pubkey not in claimed PKA
/// bucket, or duplicate witnesses for the same pubkey). Pure string
/// compares + role-table lookups — host-reachable, no DHT.
///
/// The dominance rule (witness bucket ≥ PKA bucket) lets humm-tauri
/// stamp ONE witness per pubkey at the highest role the pubkey holds:
/// an Admin-bucket witness automatically satisfies Writer + Reader
/// PKA entries for the same pubkey, matching the pass-3 link
/// validator's admin ⊆ writer ⊆ reader semantics.
fn check_witness_pka_bidirectional(
    witnesses: &[RecipientWitness],
    public_key_acl: &Acl,
) -> Option<ValidateCallbackResult> {
    // Combined worst-case cost: O(N_witnesses * N_pka) string
    // comparisons across the forward + reverse passes; with
    // HIVEGROUP_MAX_WITNESSES = 256 and a similarly-bounded PKA, this
    // is ≤ 2 * 256 * 256 = 131_072 ~50-char compares per validate call.
    // Bounded; acceptable as a one-shot commit-time check.
    // Pre-compute every witness's pubkey-string + bucket exactly once.
    // The forward and reverse passes both iterate witnesses N_pka and
    // N_witnesses times respectively; without this pre-pass each
    // iteration would re-`to_string()` the pubkey, producing
    // O(N_pka * N_witnesses) String allocations. With it the allocator
    // pressure is O(N_witnesses).
    let witness_strings: Vec<(String, AclBucket)> = witnesses
        .iter()
        .map(|w| (w.pubkey.to_string(), w.bucket))
        .collect();
    // Reject duplicate witnesses for the same pubkey — would let a
    // coordinator over-stamp a pubkey across multiple buckets to mask
    // an over-claim. One canonical witness per pubkey, period.
    {
        let mut seen: std::collections::HashSet<&str> =
            std::collections::HashSet::with_capacity(witness_strings.len());
        for (pubkey_str, _bucket) in &witness_strings {
            if !seen.insert(pubkey_str.as_str()) {
                return Some(ValidateCallbackResult::Invalid(format!(
                    "HiveGroup recipient_witnesses contains duplicate \
                     pubkey {pubkey_str} (one canonical witness per pubkey)",
                )));
            }
        }
    }
    // Forward direction — every PKA pubkey backed by a dominating
    // witness. Iterates the chained PKA buckets lazily (no Vec).
    // Owner bucket is a single string (may be empty for non-Owner-
    // bearing headers); admin/writer/reader are vecs.
    let pka_iter = std::iter::once((AclBucket::Owner, public_key_acl.owner.as_str()))
        .filter(|(_, s)| !s.is_empty())
        .chain(
            public_key_acl
                .admin
                .iter()
                .map(|s| (AclBucket::Admin, s.as_str())),
        )
        .chain(
            public_key_acl
                .writer
                .iter()
                .map(|s| (AclBucket::Writer, s.as_str())),
        )
        .chain(
            public_key_acl
                .reader
                .iter()
                .map(|s| (AclBucket::Reader, s.as_str())),
        );
    for (bucket, pubkey_str) in pka_iter {
        let backed = witness_strings
            .iter()
            .any(|(wp, wb)| wp == pubkey_str && wb.dominates(bucket));
        if !backed {
            return Some(ValidateCallbackResult::Invalid(format!(
                "HiveGroup public_key_acl.{:?} entry {} is not backed by \
                 any dominating recipient_witness",
                bucket, pubkey_str,
            )));
        }
    }
    // Reverse direction — every witness's pubkey appears in the PKA
    // bucket it claims (no over-claim).
    for (pubkey_str, bucket) in &witness_strings {
        let pka_bucket = match bucket {
            AclBucket::Owner => std::slice::from_ref(&public_key_acl.owner),
            AclBucket::Admin => public_key_acl.admin.as_slice(),
            AclBucket::Writer => public_key_acl.writer.as_slice(),
            AclBucket::Reader => public_key_acl.reader.as_slice(),
        };
        if !pka_bucket.iter().any(|s| s == pubkey_str) {
            return Some(ValidateCallbackResult::Invalid(format!(
                "HiveGroup recipient_witness for {pubkey_str} claims bucket {:?} \
                 but pubkey is not in public_key_acl.{:?}",
                bucket, bucket,
            )));
        }
    }
    None
}

/// Per-witness commit-time verification: fetch the cited
/// [`crate::group::GroupMembership`], confirm grantee identity, group
/// containment in a dominating `group_acl` bucket, role satisfaction,
/// and unexpired timing. One `must_get_valid_record` per call.
fn verify_recipient_witness(
    witness: &RecipientWitness,
    group_acl: &AclByGroupGenesis,
    timestamp: &Timestamp,
) -> ExternResult<ValidateCallbackResult> {
    let (_membership_author, membership) = fetch_group_membership(&witness.membership_hash)?;
    if membership.for_agent != witness.pubkey {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveGroup recipient_witness membership {} grants role to {} \
             but witness claims pubkey {}",
            witness.membership_hash, membership.for_agent, witness.pubkey,
        )));
    }
    // The membership's group MUST live in a `group_acl` bucket that is
    // dominated by — or equal to — the witness bucket. Owner-bucket
    // witnesses only accept Owner-bucket groups; Reader-bucket
    // witnesses accept any bucket (admin ∪ writer ∪ reader ∪ owner).
    // Group-bucket acceptance follows AclBucket dominance
    // (Owner > Admin > Writer > Reader): a witness in bucket X
    // accepts a membership in any group_acl bucket Y where
    // X.dominates(Y). Owner-bucket witnesses require the owner group
    // exactly. Admin accepts owner∪admin; Writer accepts
    // owner∪admin∪writer; Reader accepts all four. This intentionally
    // diverges from the per-class link validator semantics in
    // `validate_create_link_humm_content_acl` (which gates on the
    // bucket only, e.g. HummContentAdmin → group_acl.admin only):
    // witnesses use dominance because a single high-bucket witness
    // must cover the corresponding lower-bucket PKA entries for the
    // same pubkey under humm-tauri's inclusive-listing convention.
    //
    // The match arms inline `.any()` over the chained iterator to
    // avoid materialising a Vec<&ActionHash> per witness call (bounded
    // worst case ~16KB of pointer allocations at HIVEGROUP_MAX_WITNESSES *
    // GROUP_ACL_MAX_GROUPS); the dominance ordering is preserved
    // because each higher bucket adds groups from the immediately
    // higher-authority bucket onto the same iterator chain.
    let group_in_accepted_bucket = match witness.bucket {
        AclBucket::Owner => group_acl.owner == membership.group_genesis_hash,
        AclBucket::Admin => std::iter::once(&group_acl.owner)
            .chain(group_acl.admin.iter())
            .any(|g| g == &membership.group_genesis_hash),
        AclBucket::Writer => std::iter::once(&group_acl.owner)
            .chain(group_acl.admin.iter())
            .chain(group_acl.writer.iter())
            .any(|g| g == &membership.group_genesis_hash),
        AclBucket::Reader => std::iter::once(&group_acl.owner)
            .chain(group_acl.admin.iter())
            .chain(group_acl.writer.iter())
            .chain(group_acl.reader.iter())
            .any(|g| g == &membership.group_genesis_hash),
    };
    if !group_in_accepted_bucket {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveGroup recipient_witness membership {} is for group {} \
             which is not in group_acl bucket {:?} or any dominating bucket",
            witness.membership_hash, membership.group_genesis_hash, witness.bucket,
        )));
    }
    // Role satisfies the bucket (Admin-bucket → group Admin+ etc.).
    let required = bucket_required_role(witness.bucket);
    if !role_satisfies(membership.role, required) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveGroup recipient_witness membership {} grants role {:?}, \
             required {:?} for bucket {:?}",
            witness.membership_hash, membership.role, required, witness.bucket,
        )));
    }
    // Unexpired at the entry's timestamp.
    if let Some(expiry) = membership.expiry {
        if timestamp > &expiry {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "HiveGroup recipient_witness membership {} expired at {:?}; \
                 action timestamp {:?}",
                witness.membership_hash, expiry, timestamp,
            )));
        }
    }
    Ok(ValidateCallbackResult::Valid)
}

/// `AclSpec::DirectMessage` validator. The DM author must be one of
/// the recipients (no impersonation); the recipient set must be
/// 2..=`DM_MAX_RECIPIENTS` (no zero-recipient DMs, no over-cap
/// fanout) AND unique (no degenerate self-DM with duplicate keys);
/// `public_key_acl.reader` (as string-form pubkeys) MUST equal the
/// recipient set so I-A delete authority remains symmetric (either
/// party can delete by pubkey-match against `public_key_acl.reader`).
fn validate_directmessage_acl(
    author: &AgentPubKey,
    recipients: &[AgentPubKey],
    public_key_acl: &Acl,
) -> ExternResult<ValidateCallbackResult> {
    if recipients.len() < 2 {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "DirectMessage recipients.len() = {} (must be >= 2)",
            recipients.len(),
        )));
    }
    if recipients.len() > DM_MAX_RECIPIENTS {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "DirectMessage recipients.len() = {} exceeds DM_MAX_RECIPIENTS = {}",
            recipients.len(),
            DM_MAX_RECIPIENTS,
        )));
    }
    if !recipients.iter().any(|r| r == author) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "DirectMessage author {author} is not in recipients",
        )));
    }
    // Uniqueness: reject duplicate recipients. Degenerate self-DM
    // (recipients = [me, me]) would otherwise pass the cardinality +
    // reader-equality checks and produce ambiguous on-DHT shape.
    let mut seen: std::collections::HashSet<&AgentPubKey> =
        std::collections::HashSet::with_capacity(recipients.len());
    for r in recipients.iter() {
        if !seen.insert(r) {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "DirectMessage recipients contains duplicate pubkey {r}",
            )));
        }
    }
    // Reader bucket MUST equal recipients (set equality on string
    // form). Other buckets MUST be empty — DM has no owner/admin/writer
    // semantics, just the symmetric reader bucket.
    if !public_key_acl.owner.is_empty()
        || !public_key_acl.admin.is_empty()
        || !public_key_acl.writer.is_empty()
    {
        return Ok(ValidateCallbackResult::Invalid(
            "DirectMessage public_key_acl owner/admin/writer must be empty".into(),
        ));
    }
    let mut sorted_expected: Vec<String> = recipients.iter().map(|r| r.to_string()).collect();
    sorted_expected.sort();
    let mut sorted_actual = public_key_acl.reader.clone();
    sorted_actual.sort();
    if sorted_expected != sorted_actual {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "DirectMessage public_key_acl.reader {:?} does not match recipients {:?}",
            sorted_actual, sorted_expected,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}
/// `AclSpec::OpenWrite` validator. No hive/group membership required —
/// this is the outsider-knock + cross-network-discovery contract. If
/// `target_hive_genesis_hash` is `Some`, that hash must resolve to a
/// real `HiveGenesis` (closes the fake-target attack #13).
fn validate_openwrite_acl(
    target_hive_genesis_hash: Option<&ActionHash>,
) -> ExternResult<ValidateCallbackResult> {
    let Some(target) = target_hive_genesis_hash else {
        return Ok(ValidateCallbackResult::Valid);
    };
    // fetch_genesis errors if the hash doesn't reference a real
    // HiveGenesis; we only care that it resolves, not WHO authored.
    crate::hive::fetch_genesis(target).map(|_| ValidateCallbackResult::Valid)
}

pub fn validate_create_encrypted_content(
    action: EntryCreationAction,
    encrypted_content: EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    run_content_validators(action.author(), action.timestamp(), &encrypted_content)
}

pub fn validate_update_encrypted_content(
    action: Update,
    encrypted_content: EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    // **M-1 fix.** Without this guard, any agent with Writer+ in the
    // same hive could commit an Update on their own source chain
    // pointing at *another agent's* EncryptedContent. The Holochain-
    // native update chain would then report the rogue update as part
    // of the original entry's revision graph, letting consumers that
    // resolve updates via app-level discovery be poisoned. The fix:
    // fetch the original record and reject if action.author does not
    // match the original action's author. This mirrors the
    // dispatch_delete_entry pattern in lib.rs (delete authority is
    // bound to the original-author + public_key_acl rule).
    let original = must_get_valid_record(action.original_action_address.clone())?;
    let original_author = original.action().author();
    if &action.author != original_author {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "EncryptedContent update author {} does not match original action author {}",
            action.author, original_author,
        )));
    }
    run_content_validators(&action.author, &action.timestamp, &encrypted_content)
}

/// Author may always delete. Otherwise a `DirectMessage`'s `reader` bucket is
/// its recipient set (they may delete their copy); for every other scope only
/// owner/admin/writer may delete — a pure reader is read-only.
pub fn validate_delete_encrypted_content(
    action: Delete,
    original_action: EntryCreationAction,
    original_entry: EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    if &action.author == original_action.author() {
        return Ok(ValidateCallbackResult::Valid);
    }
    let deleter = action.author.to_string();
    let pka = &original_entry.header.public_key_acl;
    let authorized = match &original_entry.header.acl_spec {
        AclSpec::DirectMessage { .. } => pka.reader.contains(&deleter),
        AclSpec::HiveGroup { .. } | AclSpec::Public { .. } | AclSpec::OpenWrite { .. } => {
            pka.owner == deleter || pka.admin.contains(&deleter) || pka.writer.contains(&deleter)
        }
    };
    if authorized {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "delete by {} is not authorised for this content's ACL scope",
        action.author,
    )))
}

// =============================================================================
// EncryptedContentUpdates links — pass-3 author binding (L-1)
// =============================================================================

/// Validate an `EncryptedContentUpdates` link create.
///
/// **Pass-3 hardening (L-1).** Pass-1 left this validator with no
/// author binding: any agent could publish a link claiming "entry A
/// updates to entry B" against any other agent's entries. Combined
/// with the pass-1 update-author gap (now closed by the M-1 fix in
/// `validate_update_encrypted_content`), this allowed app-level
/// update-chain poisoning via the link index even when the Holochain-
/// native update chain was correctly bound to the original author.
///
/// Contract:
/// - `base` must reference an `EncryptedContent` entry authored by the
///   link author.
/// - `target` must reference an `EncryptedContent` entry authored by
///   the link author. (The M-1 fix means the only valid Update for a
///   chain rooted at `base` is itself authored by the base author, so
///   any valid pair (base, target) under that constraint shares the
///   same author. The link-author binding here is the matching
///   integrity-zome rule.)
/// - Delete is permanently rejected (the chain index is immutable; see
///   `validate_delete_link_encrypted_content_updates`).
pub fn validate_create_link_encrypted_content_updates(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    let base_ah = base_address.into_action_hash().ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(
            "EncryptedContentUpdates link base must be an ActionHash".into(),
        ))
    })?;
    let base_record = must_get_valid_record(base_ah)?;
    let base_author = base_record.action().author().clone();
    if action.author != base_author {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "EncryptedContentUpdates link author {} does not match base \
             entry author {}",
            action.author, base_author,
        )));
    }
    let _: EncryptedContent = base_record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(
                "EncryptedContentUpdates link base does not reference an EncryptedContent".into(),
            ))
        })?;
    let target_ah = target_address.into_action_hash().ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(
            "EncryptedContentUpdates link target must be an ActionHash".into(),
        ))
    })?;
    let target_record = must_get_valid_record(target_ah)?;
    let target_author = target_record.action().author().clone();
    if action.author != target_author {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "EncryptedContentUpdates link author {} does not match target \
             entry author {}",
            action.author, target_author,
        )));
    }
    let _: EncryptedContent = target_record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(
                "EncryptedContentUpdates link target does not reference an EncryptedContent".into(),
            ))
        })?;
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_delete_link_encrypted_content_updates(
    _action: DeleteLink,
    _original_action: CreateLink,
    _base: AnyLinkableHash,
    _target: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(String::from(
        "EncryptedContentUpdates links cannot be deleted",
    )))
}

// =============================================================================
// Pass-2 link recompute helpers
// =============================================================================

/// Fetch the target `EncryptedContent` referenced by a link's
/// `target_address`, returning the action and the typed entry.
///
/// Every hive-scoped link validator (`Hive`, `Dynamic`, `HummContent*`,
/// `HummContentId`) starts here to recover the authoritative header
/// fields used for path recomputation.
fn fetch_target_encrypted_content(
    target_address: &AnyLinkableHash,
) -> ExternResult<(SignedActionHashed, EncryptedContent)> {
    let target_ah = target_address.clone().into_action_hash().ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "link target {target_address} must be an ActionHash",
        )))
    })?;
    let record = must_get_valid_record(target_ah)?;
    let entry: EncryptedContent = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "link target {target_address} does not reference an EncryptedContent",
            )))
        })?;
    Ok((record.signed_action().clone(), entry))
}

/// Recompute a path hash from string components and return it as the
/// `AnyLinkableHash` form a link's `base_address` carries.
fn recompute_base(components: &[&str]) -> ExternResult<AnyLinkableHash> {
    let path = Path::from(
        components
            .iter()
            .map(|c| Component::from(*c))
            .collect::<Vec<_>>(),
    );
    Ok(path.path_entry_hash()?.into())
}

/// Verify the link's author IS the target entry's author. Discovery
/// links may only be published by the author of the entry they point at;
/// this prevents Mallory from indexing alice's content under mallory's
/// chosen paths.
fn require_link_author_is_target_author(
    link_action: &CreateLink,
    target_action: &SignedActionHashed,
) -> ValidateCallbackResult {
    let target_author = target_action.action().author();
    if &link_action.author != target_author {
        return ValidateCallbackResult::Invalid(format!(
            "link author {} does not match target entry author {}",
            link_action.author, target_author,
        ));
    }
    ValidateCallbackResult::Valid
}

/// Decode a UTF-8 string from a link tag. Returns Invalid on non-UTF-8
/// bytes instead of erroring; non-UTF-8 tag content is a malformed
/// publish, not a host failure.
fn decode_utf8_tag(tag: &LinkTag, tag_label: &str) -> Result<String, ValidateCallbackResult> {
    String::from_utf8(tag.0.clone()).map_err(|e| {
        ValidateCallbackResult::Invalid(format!("{tag_label} link tag is not valid UTF-8: {e}",))
    })
}

// =============================================================================
// Hive link validator (both author-shape AND hive-shape)
// =============================================================================

/// Validate a `Hive` link create.
///
/// `LinkTypes::Hive` is **overloaded** across two path shapes the
/// coordinator publishes:
///
/// - **Author-shape**: base = `Path([author_pubkey, content_type])` →
///   author's discovery index ("all my content of this type"). Created
///   by every `create_encrypted_content` call in
///   `coordinator/.../crud.rs`.
/// - **Hive-shape**: base =
///   `Path([hive_genesis_hash_b64, content_type])` → hive-wide
///   discovery index. Created by
///   `coordinator/.../linking/hive_link.rs`.
///
/// The validator recomputes BOTH possible bases from the target entry's
/// validated header fields and accepts the link if EITHER matches the
/// claimed `base_address`. The author-shape path is implicitly tied to
/// the link author (= target author); the hive-shape path is tied to
/// the cryptographic hive identity. Any other base is a forgery.
pub fn validate_create_link_hive(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    let (target_action, target_entry) = fetch_target_encrypted_content(&target_address)?;
    let author_check = require_link_author_is_target_author(&action, &target_action);
    if !matches!(author_check, ValidateCallbackResult::Valid) {
        return Ok(author_check);
    }

    // Author-shape candidate.
    let author_b64 = target_action.action().author().to_string();
    let content_type = &target_entry.header.content_type;
    let author_base = recompute_base(&[&author_b64, content_type])?;
    if author_base == base_address {
        return Ok(ValidateCallbackResult::Valid);
    }

    // Hive-shape candidate (only meaningful for variants that bind a
    // hive context — HiveGroup, Public, or OpenWrite with a target).
    // For DirectMessage and OpenWrite { target: None }, hive_context()
    // returns None; only the author-shape path is acceptable, and we
    // fall through to the Invalid path below.
    if let Some(hive_hash) = target_entry.header.hive_context() {
        let hive_b64 = hive_hash.to_string();
        let hive_base = recompute_base(&[&hive_b64, content_type])?;
        if hive_base == base_address {
            return Ok(ValidateCallbackResult::Valid);
        }
        return Ok(ValidateCallbackResult::Invalid(format!(
            "Hive link base {base_address} matches neither the author-shape \
             path [{author_b64}, {content_type}] nor the hive-shape path \
             [{hive_b64}, {content_type}]",
        )));
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "Hive link base {base_address} does not match the author-shape \
         path [{author_b64}, {content_type}]; target entry has no hive \
         context (DirectMessage or OpenWrite without target) so the \
         hive-shape path is not available",
    )))
}

/// `Hive` link delete is the link author's prerogative.
pub fn validate_delete_link_hive(
    action: DeleteLink,
    original_action: CreateLink,
    _base: AnyLinkableHash,
    _target: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    if action.author == original_action.author {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(
        "Hive link delete must be authored by the link creator".into(),
    ))
}

// =============================================================================
// Dynamic link validator
// =============================================================================

/// Validate a `Dynamic` link create.
///
/// Base must equal `Path([hive_genesis_hash_b64, content_type,
/// dynamic_label])` recomputed from the target entry's header plus the
/// `dynamic_label` carried in the link's tag (UTF-8 bytes). The link
/// author must be the target entry's author.
pub fn validate_create_link_dynamic(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    let (target_action, target_entry) = fetch_target_encrypted_content(&target_address)?;
    let author_check = require_link_author_is_target_author(&action, &target_action);
    if !matches!(author_check, ValidateCallbackResult::Valid) {
        return Ok(author_check);
    }

    let dynamic_label = match decode_utf8_tag(&tag, "Dynamic") {
        Ok(s) => s,
        Err(invalid) => return Ok(invalid),
    };
    let Some(hive_hash) = target_entry.header.hive_context() else {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "Dynamic link target has no hive context (acl_spec is \
             DirectMessage or OpenWrite without target); Dynamic links \
             require a hive-scoped path",
        )));
    };
    let hive_b64 = hive_hash.to_string();
    let content_type = target_entry.header.content_type.as_str();
    let expected = recompute_base(&[&hive_b64, content_type, &dynamic_label])?;
    if expected == base_address {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "Dynamic link base {base_address} does not match recomputed path \
         [{hive_b64}, {content_type}, {dynamic_label}]",
    )))
}

pub fn validate_delete_link_dynamic(
    action: DeleteLink,
    original_action: CreateLink,
    _base: AnyLinkableHash,
    _target: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    if action.author == original_action.author {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(
        "Dynamic link delete must be authored by the link creator".into(),
    ))
}

// =============================================================================
// HummContentId link validator
// =============================================================================

/// `HummContentId` link base = `Path([hive_genesis_hash_b64, header.id])`.
/// No tag. Provides "lookup by content_id within a hive".
pub fn validate_create_link_humm_content_id(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    let (target_action, target_entry) = fetch_target_encrypted_content(&target_address)?;
    let author_check = require_link_author_is_target_author(&action, &target_action);
    if !matches!(author_check, ValidateCallbackResult::Valid) {
        return Ok(author_check);
    }

    let Some(hive_hash) = target_entry.header.hive_context() else {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HummContentId link target has no hive context (acl_spec is \
             DirectMessage or OpenWrite without target); HummContentId \
             links require a hive-scoped path",
        )));
    };
    let hive_b64 = hive_hash.to_string();
    let content_id = target_entry.header.id.as_str();
    let expected = recompute_base(&[&hive_b64, content_id])?;
    if expected == base_address {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "HummContentId link base {base_address} does not match recomputed \
         path [{hive_b64}, {content_id}]",
    )))
}

pub fn validate_delete_link_humm_content_id(
    action: DeleteLink,
    original_action: CreateLink,
    _base: AnyLinkableHash,
    _target: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    if action.author == original_action.author {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(
        "HummContentId link delete must be authored by the link creator".into(),
    ))
}

// =============================================================================
// HummContentOwner / Admin / Writer / Reader link validators
// =============================================================================

/// Classes of ACL link, mapped to the field of `acl` they index.
/// Drives the `validate_create_link_humm_content_acl` dispatch.
#[derive(Clone, Copy, Debug)]
pub enum AclLinkClass {
    Owner,
    Admin,
    Writer,
    Reader,
}

/// Validate an ACL link create (`HummContentOwner` / `HummContentAdmin`
/// / `HummContentWriter` / `HummContentReader`).
///
/// **Pass-3 reshape.** These links index `EncryptedContent` entries by
/// the `GroupGenesis` action hash assigned to each ACL bucket in the
/// header's `acl_spec`. The link is only meaningful for the
/// `AclSpec::HiveGroup` variant; for the other three variants this
/// validator rejects (the variants have no `group_acl` field, so no
/// HummContent* link can validly anchor to them).
///
/// Contract:
/// - Base = `Path([hive_genesis_hash_b64, content_type, entity_id])`
///   recomputed from the target's `HiveGroup` variant fields + tag.
/// - Tag = UTF-8 bytes of the `entity_id` = a `GroupGenesis.to_string()`
///   used in the third path component. Required because the entity_id
///   is per-link and cannot be uniquely derived from the target alone
///   (Admin/Writer/Reader each fan out to multiple group hashes).
/// - `entity_id` membership in the corresponding bucket of
///   `acl_spec.HiveGroup.group_acl`:
///   * `Owner`  — must equal `group_acl.owner.to_string()`.
///   * `Admin`  — must be in `group_acl.admin` (string form).
///   * `Writer` — must be in `group_acl.admin ∪ group_acl.writer`
///     (admins inherit writer rights, per coordinator convention).
///   * `Reader` — must be in
///     `group_acl.admin ∪ group_acl.writer ∪ group_acl.reader`.
/// - Link author = target entry author.
pub fn validate_create_link_humm_content_acl(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
    class: AclLinkClass,
) -> ExternResult<ValidateCallbackResult> {
    let (target_action, target_entry) = fetch_target_encrypted_content(&target_address)?;
    let author_check = require_link_author_is_target_author(&action, &target_action);
    if !matches!(author_check, ValidateCallbackResult::Valid) {
        return Ok(author_check);
    }

    let class_label = match class {
        AclLinkClass::Owner => "HummContentOwner",
        AclLinkClass::Admin => "HummContentAdmin",
        AclLinkClass::Writer => "HummContentWriter",
        AclLinkClass::Reader => "HummContentReader",
    };
    let entity_id = match decode_utf8_tag(&tag, class_label) {
        Ok(s) => s,
        Err(invalid) => return Ok(invalid),
    };

    // HummContent* links require AclSpec::HiveGroup — they index by
    // group_acl bucket, which only that variant carries. For the
    // other three variants (DirectMessage, Public, OpenWrite) reject
    // outright.
    let (Some(hive_hash), Some(group_acl)) = (
        target_entry.header.hive_context(),
        target_entry.header.group_acl(),
    ) else {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "{class_label} link target's acl_spec is not HiveGroup; \
             HummContent* links only anchor to HiveGroup content",
        )));
    };
    let hive_b64 = hive_hash.to_string();
    let content_type = target_entry.header.content_type.as_str();
    let expected = recompute_base(&[&hive_b64, content_type, &entity_id])?;
    if expected != base_address {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "{class_label} link base {base_address} does not match recomputed \
             path [{hive_b64}, {content_type}, {entity_id}]",
        )));
    }
    // Verify entity_id membership in the appropriate group_acl bucket.
    // entity_id is the string form of a GroupGenesis ActionHash. We
    // compare lazily (h.to_string() == entity_id, short-circuiting via
    // .any) so an Owner-class link does NOT pre-allocate strings for
    // admin/writer/reader buckets it never inspects.
    let valid_membership = match class {
        AclLinkClass::Owner => group_acl.owner.to_string() == entity_id,
        AclLinkClass::Admin => group_acl.admin.iter().any(|h| h.to_string() == entity_id),
        AclLinkClass::Writer => group_acl
            .admin
            .iter()
            .chain(group_acl.writer.iter())
            .any(|h| h.to_string() == entity_id),
        AclLinkClass::Reader => group_acl
            .admin
            .iter()
            .chain(group_acl.writer.iter())
            .chain(group_acl.reader.iter())
            .any(|h| h.to_string() == entity_id),
    };
    if !valid_membership {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "{class_label} link entity_id {entity_id} is not in target \
             group_acl.{} set",
            match class {
                AclLinkClass::Owner => "owner",
                AclLinkClass::Admin => "admin",
                AclLinkClass::Writer => "admin∪writer",
                AclLinkClass::Reader => "admin∪writer∪reader",
            },
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_delete_link_humm_content_acl(
    action: DeleteLink,
    original_action: CreateLink,
    _base: AnyLinkableHash,
    _target: AnyLinkableHash,
    _tag: LinkTag,
    class_label: &str,
) -> ExternResult<ValidateCallbackResult> {
    if action.author == original_action.author {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "{class_label} link delete must be authored by the link creator",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_pubkey(byte: u8) -> AgentPubKey {
        AgentPubKey::from_raw_36(vec![byte; 36])
    }

    fn action_hash(byte: u8) -> ActionHash {
        ActionHash::from_raw_36(vec![byte; 36])
    }

    fn sample_acl_spec() -> AclSpec {
        AclSpec::HiveGroup {
            hive_genesis_hash: action_hash(9),
            author_membership_hash: None,
            group_acl: AclByGroupGenesis {
                owner: action_hash(10),
                admin: vec![],
                writer: vec![],
                reader: vec![],
            },
            author_group_membership_hash: None,
            recipient_witnesses: vec![],
        }
    }

    fn sample_header_pubkey(pk_b64: &str) -> EncryptedContentHeader {
        EncryptedContentHeader {
            id: "id".into(),
            display_hive_id: "hive".into(),
            content_type: "ct".into(),
            acl_spec: sample_acl_spec(),
            public_key_acl: Acl {
                owner: "owner".into(),
                admin: vec![],
                writer: vec![],
                reader: vec![],
            },
            revision_author_signing_public_key: pk_b64.into(),
        }
    }

    fn content_with_spec(acl_spec: AclSpec, public_key_acl: Acl) -> EncryptedContent {
        EncryptedContent {
            header: EncryptedContentHeader {
                id: "id".into(),
                display_hive_id: "hive".into(),
                content_type: "ct".into(),
                acl_spec,
                public_key_acl,
                revision_author_signing_public_key: agent_pubkey(1).to_string(),
            },
            bytes: SerializedBytes::from(UnsafeBytes::from(vec![])),
        }
    }

    fn sample_content_with_acl(public_key_acl: Acl) -> EncryptedContent {
        content_with_spec(sample_acl_spec(), public_key_acl)
    }

    // ---------------------------------------------------------------------
    // Pass-1 invariants — header pubkey check
    // ---------------------------------------------------------------------

    #[test]
    fn check_rejects_when_header_pubkey_does_not_match_action_author() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let bob_b64 = bob.to_string();
        let result = check_author_matches_header(&alice, &bob_b64);
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.contains("revision_author_signing_public_key"));
                assert!(msg.contains(&bob_b64));
                assert!(msg.contains(&alice.to_string()));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn check_accepts_when_header_pubkey_matches_action_author() {
        let alice = agent_pubkey(1);
        let alice_b64 = alice.to_string();
        let result = check_author_matches_header(&alice, &alice_b64);
        assert!(
            matches!(result, ValidateCallbackResult::Valid),
            "expected Valid, got {result:?}",
        );
    }

    #[test]
    fn check_rejects_empty_header_pubkey() {
        let alice = agent_pubkey(1);
        let result = check_author_matches_header(&alice, "");
        assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
    }

    #[test]
    fn check_rejects_legacy_placeholder_header() {
        let alice = agent_pubkey(1);
        let result = check_author_matches_header(&alice, "test-revision-author-signing-public-key");
        assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
    }

    #[test]
    fn header_struct_round_trips_through_the_check() {
        let alice = agent_pubkey(7);
        let valid_header = sample_header_pubkey(&alice.to_string());
        assert!(matches!(
            check_author_matches_header(&alice, &valid_header.revision_author_signing_public_key),
            ValidateCallbackResult::Valid,
        ));
        let forged_header = sample_header_pubkey(&agent_pubkey(8).to_string());
        assert!(matches!(
            check_author_matches_header(&alice, &forged_header.revision_author_signing_public_key),
            ValidateCallbackResult::Invalid(_),
        ));
    }

    // ---------------------------------------------------------------------
    // I-A — validate_delete_encrypted_content
    // ---------------------------------------------------------------------
    //
    // These tests construct `Delete` and `EntryCreationAction` shapes
    // manually. We avoid faking entire actions; instead we test the
    // function's *decision* logic by setting up the relevant fields the
    // function reads.

    fn make_delete(author: AgentPubKey) -> Delete {
        Delete {
            author,
            timestamp: Timestamp(0),
            action_seq: 1,
            prev_action: action_hash(0),
            deletes_address: action_hash(1),
            deletes_entry_address: EntryHash::from_raw_36(vec![0u8; 36]),
            weight: Default::default(),
        }
    }

    fn make_create(author: AgentPubKey) -> Create {
        Create {
            author,
            timestamp: Timestamp(0),
            action_seq: 0,
            prev_action: action_hash(0),
            entry_type: EntryType::App(AppEntryDef {
                entry_index: 0.into(),
                zome_index: 0.into(),
                visibility: EntryVisibility::Public,
            }),
            entry_hash: EntryHash::from_raw_36(vec![0u8; 36]),
            weight: Default::default(),
        }
    }

    #[test]
    fn delete_accepts_original_author() {
        let alice = agent_pubkey(1);
        let action = make_delete(alice.clone());
        let original = EntryCreationAction::Create(make_create(alice));
        let content = sample_content_with_acl(Acl {
            owner: "x".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        });
        let result = validate_delete_encrypted_content(action, original, content)
            .expect("validator should not error in test");
        assert!(matches!(result, ValidateCallbackResult::Valid));
    }

    #[test]
    fn delete_rejects_stranger_with_empty_public_key_acl() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let action = make_delete(bob);
        let original = EntryCreationAction::Create(make_create(alice));
        let content = sample_content_with_acl(Acl {
            owner: "not-bob".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        });
        let result = validate_delete_encrypted_content(action, original, content)
            .expect("validator should not error in test");
        assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
    }

    #[test]
    fn delete_dm_recipient_in_reader_bucket_is_valid() {
        let bob = agent_pubkey(2);
        let content = content_with_spec(
            AclSpec::DirectMessage {
                recipients: vec![bob.clone()],
            },
            Acl {
                owner: "x".into(),
                admin: vec![],
                writer: vec![],
                reader: vec![bob.to_string()],
            },
        );
        let result = validate_delete_encrypted_content(
            make_delete(bob),
            EntryCreationAction::Create(make_create(agent_pubkey(1))),
            content,
        )
        .expect("validator should not error in test");
        assert!(
            matches!(result, ValidateCallbackResult::Valid),
            "got {result:?}"
        );
    }

    #[test]
    fn delete_hive_group_reader_is_rejected() {
        let bob = agent_pubkey(2);
        let content = sample_content_with_acl(Acl {
            owner: "x".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![bob.to_string()],
        });
        let result = validate_delete_encrypted_content(
            make_delete(bob),
            EntryCreationAction::Create(make_create(agent_pubkey(1))),
            content,
        )
        .expect("validator should not error in test");
        assert!(
            matches!(result, ValidateCallbackResult::Invalid(_)),
            "got {result:?}"
        );
    }

    #[test]
    fn delete_accepts_listed_in_public_key_acl_admin() {
        let alice = agent_pubkey(1);
        let admin = agent_pubkey(3);
        let action = make_delete(admin.clone());
        let original = EntryCreationAction::Create(make_create(alice));
        let content = sample_content_with_acl(Acl {
            owner: "x".into(),
            admin: vec![admin.to_string()],
            writer: vec![],
            reader: vec![],
        });
        let result = validate_delete_encrypted_content(action, original, content)
            .expect("validator should not error in test");
        assert!(matches!(result, ValidateCallbackResult::Valid));
    }

    #[test]
    fn delete_accepts_listed_in_public_key_acl_writer() {
        let alice = agent_pubkey(1);
        let writer = agent_pubkey(4);
        let action = make_delete(writer.clone());
        let original = EntryCreationAction::Create(make_create(alice));
        let content = sample_content_with_acl(Acl {
            owner: "x".into(),
            admin: vec![],
            writer: vec![writer.to_string()],
            reader: vec![],
        });
        let result = validate_delete_encrypted_content(action, original, content)
            .expect("validator should not error in test");
        assert!(matches!(result, ValidateCallbackResult::Valid));
    }

    #[test]
    fn delete_accepts_listed_in_public_key_acl_owner() {
        let alice = agent_pubkey(1);
        let owner = agent_pubkey(5);
        let action = make_delete(owner.clone());
        let original = EntryCreationAction::Create(make_create(alice));
        let content = sample_content_with_acl(Acl {
            owner: owner.to_string(),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        });
        let result = validate_delete_encrypted_content(action, original, content)
            .expect("validator should not error in test");
        assert!(matches!(result, ValidateCallbackResult::Valid));
    }

    #[test]
    fn delete_rejects_when_author_string_is_substring_of_acl_value() {
        // The `public_key_acl.owner` field is a `String` and the
        // admin/writer/reader fields are `Vec<String>`. The validator
        // uses `==` (exact-string) comparison everywhere, so a stranger
        // whose pubkey string happens to be a strict substring of an
        // ACL value MUST NOT false-match. This test pins the exact-
        // match guarantee in every bucket.
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let bob_str = bob.to_string();
        // Bob's pubkey string with an appended suffix — bob is NOT
        // exactly listed; the suffix-bearing string is the ACL value.
        let bob_with_suffix = format!("{bob_str}EXTRA");
        // Note: the inverse direction (ACL value is a strict prefix of
        // bob's pubkey) is covered by
        // `delete_rejects_when_acl_value_is_substring_of_author_string`.
        for acl in [
            // owner field carries a string that CONTAINS bob's pubkey.
            Acl {
                owner: bob_with_suffix.clone(),
                admin: vec![],
                writer: vec![],
                reader: vec![],
            },
            // admin vec carries a containing-string (admin entry holds bob+suffix).
            Acl {
                owner: "z".into(),
                admin: vec![bob_with_suffix.clone()],
                writer: vec![],
                reader: vec![],
            },
            // writer vec carries a containing-string.
            Acl {
                owner: "z".into(),
                admin: vec![],
                writer: vec![bob_with_suffix.clone()],
                reader: vec![],
            },
            // reader vec carries a containing-string (reader entry holds bob+suffix).
            Acl {
                owner: "z".into(),
                admin: vec![],
                writer: vec![],
                reader: vec![bob_with_suffix.clone()],
            },
        ] {
            let action = make_delete(bob.clone());
            let original = EntryCreationAction::Create(make_create(alice.clone()));
            let content = sample_content_with_acl(acl);
            let result = validate_delete_encrypted_content(action, original, content)
                .expect("validator should not error in test");
            assert!(
                matches!(result, ValidateCallbackResult::Invalid(_)),
                "substring-but-not-exact ACL match must NOT permit delete; got {result:?}",
            );
        }
    }

    #[test]
    fn delete_rejects_when_acl_value_is_substring_of_author_string() {
        // Inverse of the previous test: an ACL value that is a strict
        // prefix of the deleter's pubkey string must NOT false-match.
        // Exact-string semantics in both directions.
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let bob_str = bob.to_string();
        let bob_prefix_only = bob_str[..bob_str.len() - 1].to_string();
        let action = make_delete(bob.clone());
        let original = EntryCreationAction::Create(make_create(alice));
        let content = sample_content_with_acl(Acl {
            owner: bob_prefix_only.clone(),
            admin: vec![bob_prefix_only.clone()],
            writer: vec![bob_prefix_only.clone()],
            reader: vec![bob_prefix_only],
        });
        let result = validate_delete_encrypted_content(action, original, content)
            .expect("validator should not error in test");
        assert!(
            matches!(result, ValidateCallbackResult::Invalid(_)),
            "ACL value that is a strict prefix of author pubkey must NOT permit delete; got {result:?}",
        );
    }

    #[test]
    fn delete_hive_group_writer_admin_owner_are_valid() {
        let bob = agent_pubkey(2);
        let bob_b64 = bob.to_string();
        let buckets = [
            Acl {
                owner: bob_b64.clone(),
                admin: vec![],
                writer: vec![],
                reader: vec![],
            },
            Acl {
                owner: "x".into(),
                admin: vec![bob_b64.clone()],
                writer: vec![],
                reader: vec![],
            },
            Acl {
                owner: "x".into(),
                admin: vec![],
                writer: vec![bob_b64.clone()],
                reader: vec![],
            },
        ];
        for acl in buckets {
            let content = sample_content_with_acl(acl);
            let result = validate_delete_encrypted_content(
                make_delete(bob.clone()),
                EntryCreationAction::Create(make_create(agent_pubkey(1))),
                content,
            )
            .expect("validator should not error in test");
            assert!(
                matches!(result, ValidateCallbackResult::Valid),
                "got {result:?}"
            );
        }
    }

    #[test]
    fn delete_dm_non_recipient_is_rejected() {
        let bob = agent_pubkey(2);
        let stranger = agent_pubkey(5);
        let content = content_with_spec(
            AclSpec::DirectMessage {
                recipients: vec![stranger.clone()],
            },
            Acl {
                owner: "x".into(),
                admin: vec![],
                writer: vec![],
                reader: vec![stranger.to_string()],
            },
        );
        let result = validate_delete_encrypted_content(
            make_delete(bob),
            EntryCreationAction::Create(make_create(agent_pubkey(1))),
            content,
        )
        .expect("validator should not error in test");
        assert!(
            matches!(result, ValidateCallbackResult::Invalid(_)),
            "got {result:?}"
        );
    }

    // ---------------------------------------------------------------------
    // Path-recompute sanity
    // ---------------------------------------------------------------------

    #[test]
    fn recompute_base_matches_path_constructed_manually() {
        let manual = Path::from(vec![
            Component::from("a"),
            Component::from("b"),
            Component::from("c"),
        ]);
        let manual_hash: AnyLinkableHash = manual
            .path_entry_hash()
            .expect("path hash should compute in test")
            .into();
        let recomputed =
            recompute_base(&["a", "b", "c"]).expect("recompute_base should compute in test");
        assert_eq!(manual_hash, recomputed);
    }

    // ---------------------------------------------------------------------
    // Delete-link author-equality — Hive / Dynamic / HummContentId /
    // HummContentAcl all share the contract "only the link's author may
    // delete it". Pure logic, no host calls.
    // ---------------------------------------------------------------------

    fn make_create_link(author: AgentPubKey) -> CreateLink {
        CreateLink {
            author,
            timestamp: Timestamp(0),
            action_seq: 0,
            prev_action: action_hash(0),
            base_address: AnyLinkableHash::from(action_hash(1)),
            target_address: AnyLinkableHash::from(action_hash(2)),
            zome_index: 0.into(),
            link_type: 0.into(),
            tag: LinkTag::new(vec![]),
            weight: Default::default(),
        }
    }

    fn make_delete_link(author: AgentPubKey) -> DeleteLink {
        DeleteLink {
            author,
            timestamp: Timestamp(0),
            action_seq: 1,
            prev_action: action_hash(0),
            base_address: AnyLinkableHash::from(action_hash(1)),
            link_add_address: action_hash(3),
        }
    }

    fn link_args() -> (AnyLinkableHash, AnyLinkableHash, LinkTag) {
        (
            AnyLinkableHash::from(action_hash(1)),
            AnyLinkableHash::from(action_hash(2)),
            LinkTag::new(vec![]),
        )
    }

    #[test]
    fn delete_link_hive_accepts_original_author() {
        let alice = agent_pubkey(1);
        let (base, target, tag) = link_args();
        let result = validate_delete_link_hive(
            make_delete_link(alice.clone()),
            make_create_link(alice),
            base,
            target,
            tag,
        )
        .expect("validator should not error in test");
        assert!(matches!(result, ValidateCallbackResult::Valid));
    }

    #[test]
    fn delete_link_hive_rejects_third_party() {
        let alice = agent_pubkey(1);
        let mallory = agent_pubkey(99);
        let (base, target, tag) = link_args();
        let result = validate_delete_link_hive(
            make_delete_link(mallory),
            make_create_link(alice),
            base,
            target,
            tag,
        )
        .expect("validator should not error in test");
        assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
    }

    #[test]
    fn delete_link_dynamic_enforces_author_equality() {
        let alice = agent_pubkey(1);
        let mallory = agent_pubkey(99);
        let (base, target, tag) = link_args();
        let accept = validate_delete_link_dynamic(
            make_delete_link(alice.clone()),
            make_create_link(alice.clone()),
            base.clone(),
            target.clone(),
            tag.clone(),
        )
        .expect("validator should not error in test");
        assert!(matches!(accept, ValidateCallbackResult::Valid));
        let reject = validate_delete_link_dynamic(
            make_delete_link(mallory),
            make_create_link(alice),
            base,
            target,
            tag,
        )
        .expect("validator should not error in test");
        assert!(matches!(reject, ValidateCallbackResult::Invalid(_)));
    }

    #[test]
    fn delete_link_humm_content_id_enforces_author_equality() {
        let alice = agent_pubkey(1);
        let mallory = agent_pubkey(99);
        let (base, target, tag) = link_args();
        let accept = validate_delete_link_humm_content_id(
            make_delete_link(alice.clone()),
            make_create_link(alice.clone()),
            base.clone(),
            target.clone(),
            tag.clone(),
        )
        .expect("validator should not error in test");
        assert!(matches!(accept, ValidateCallbackResult::Valid));
        let reject = validate_delete_link_humm_content_id(
            make_delete_link(mallory),
            make_create_link(alice),
            base,
            target,
            tag,
        )
        .expect("validator should not error in test");
        assert!(matches!(reject, ValidateCallbackResult::Invalid(_)));
    }

    #[test]
    fn delete_link_humm_content_acl_enforces_author_equality_per_class() {
        // ACL delete-link validator takes a `class_label: &str` for
        // error messaging; iterate every ACL class to confirm uniform
        // author-equality across the four variants.
        let alice = agent_pubkey(1);
        let mallory = agent_pubkey(99);
        for class_label in [
            "HummContentOwner",
            "HummContentAdmin",
            "HummContentWriter",
            "HummContentReader",
        ] {
            let (base, target, tag) = link_args();
            let accept = validate_delete_link_humm_content_acl(
                make_delete_link(alice.clone()),
                make_create_link(alice.clone()),
                base.clone(),
                target.clone(),
                tag.clone(),
                class_label,
            )
            .expect("validator should not error in test");
            assert!(
                matches!(accept, ValidateCallbackResult::Valid),
                "{class_label} delete by original author must be Valid; got {accept:?}",
            );
            let reject = validate_delete_link_humm_content_acl(
                make_delete_link(mallory.clone()),
                make_create_link(alice.clone()),
                base,
                target,
                tag,
                class_label,
            )
            .expect("validator should not error in test");
            match reject {
                ValidateCallbackResult::Invalid(msg) => {
                    assert!(
                        msg.contains(class_label),
                        "{class_label}: error message must identify the link class; got {msg:?}",
                    );
                }
                other => panic!("{class_label}: expected Invalid, got {other:?}"),
            }
        }
    }

    #[test]
    fn delete_link_encrypted_content_updates_is_invalid() {
        // EncryptedContentUpdates is the only link type that
        // unconditionally rejects deletes (preserves the update chain
        // integrity).
        let alice = agent_pubkey(1);
        let (base, target, tag) = link_args();
        let result = validate_delete_link_encrypted_content_updates(
            make_delete_link(alice.clone()),
            make_create_link(alice),
            base,
            target,
            tag,
        )
        .expect("validator should not error in test");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.contains("cannot be deleted"));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------
    // Pass-3 — variant-dispatch validators (pre-fetch branches only).
    //
    // Fetch-dependent branches (HiveGroup hive/group authority,
    // OpenWrite target existence) require must_get_valid_record and
    // are covered by Tryorama integration tests once the harness is
    // paired. Same host-side limitation as the pass-2 hive validator
    // suite.
    // ---------------------------------------------------------------------

    fn dm_content(
        author: AgentPubKey,
        recipients: Vec<AgentPubKey>,
        public_key_acl: Acl,
    ) -> EncryptedContent {
        EncryptedContent {
            header: EncryptedContentHeader {
                id: "dm-id".into(),
                display_hive_id: "".into(),
                content_type: "dm".into(),
                acl_spec: AclSpec::DirectMessage { recipients },
                public_key_acl,
                revision_author_signing_public_key: author.to_string(),
            },
            bytes: SerializedBytes::from(UnsafeBytes::from(vec![])),
        }
    }

    fn empty_acl() -> Acl {
        Acl {
            owner: "".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        }
    }

    fn reader_acl(readers: &[AgentPubKey]) -> Acl {
        Acl {
            owner: "".into(),
            admin: vec![],
            writer: vec![],
            reader: readers.iter().map(|r| r.to_string()).collect(),
        }
    }

    #[test]
    fn directmessage_rejects_zero_recipients() {
        let alice = agent_pubkey(1);
        let content = dm_content(alice.clone(), vec![], empty_acl());
        let result = run_content_validators(&alice, &Timestamp(0), &content)
            .expect("DM validator runs pre-fetch on cardinality check");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.contains("recipients.len() = 0"), "got: {msg}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn directmessage_rejects_one_recipient() {
        let alice = agent_pubkey(1);
        let content = dm_content(
            alice.clone(),
            vec![alice.clone()],
            reader_acl(&[alice.clone()]),
        );
        let result = run_content_validators(&alice, &Timestamp(0), &content)
            .expect("DM validator runs pre-fetch on cardinality check");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.contains("recipients.len() = 1"), "got: {msg}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn directmessage_rejects_over_max_recipients() {
        let alice = agent_pubkey(1);
        // DM_MAX_RECIPIENTS = 32; build 33.
        let recipients: Vec<AgentPubKey> = (0u8..33).map(agent_pubkey).collect();
        let pka = reader_acl(&recipients);
        let content = dm_content(alice.clone(), recipients, pka);
        let result = run_content_validators(&alice, &Timestamp(0), &content)
            .expect("DM validator runs pre-fetch on cardinality check");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.contains("exceeds DM_MAX_RECIPIENTS"), "got: {msg}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn directmessage_rejects_author_not_in_recipients() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let carol = agent_pubkey(3);
        // Mallory (alice) tries to spoof a DM between bob and carol.
        let content = dm_content(
            alice.clone(),
            vec![bob.clone(), carol.clone()],
            reader_acl(&[bob, carol]),
        );
        let result = run_content_validators(&alice, &Timestamp(0), &content)
            .expect("DM validator runs pre-fetch on author-in-recipients");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.contains("not in recipients"), "got: {msg}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn directmessage_rejects_reader_bucket_mismatch() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let mallory = agent_pubkey(99);
        // Recipient list is [alice, bob], but reader bucket has
        // [alice, mallory] — modified-coordinator forgery (Mallory
        // injects herself into the routing fan-out).
        let content = dm_content(
            alice.clone(),
            vec![alice.clone(), bob],
            reader_acl(&[alice.clone(), mallory]),
        );
        let result = run_content_validators(&alice, &Timestamp(0), &content)
            .expect("DM validator runs pre-fetch on reader-bucket equality");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.contains("public_key_acl.reader"), "got: {msg}");
                assert!(msg.contains("does not match recipients"), "got: {msg}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn directmessage_rejects_nonempty_non_reader_buckets() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let mut pka = reader_acl(&[alice.clone(), bob.clone()]);
        pka.admin.push("trojan".into());
        let content = dm_content(alice.clone(), vec![alice.clone(), bob], pka);
        let result = run_content_validators(&alice, &Timestamp(0), &content)
            .expect("DM validator runs pre-fetch on owner/admin/writer empty");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(
                    msg.contains("owner/admin/writer must be empty"),
                    "got: {msg}"
                );
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn directmessage_accepts_2_recipients_in_any_order() {
        // Reader bucket sorting normalises order: validator should
        // accept regardless of insertion order. Sorted-equality is the
        // pre-fetch path; no must_get_valid_record fired.
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let pka = reader_acl(&[bob.clone(), alice.clone()]); // reversed
        let content = dm_content(alice.clone(), vec![alice, bob], pka);
        let result = run_content_validators(&agent_pubkey(1), &Timestamp(0), &content)
            .expect("DM validator runs pre-fetch on order-independent equality");
        assert!(matches!(result, ValidateCallbackResult::Valid));
    }

    fn openwrite_content(author: AgentPubKey, target: Option<ActionHash>) -> EncryptedContent {
        EncryptedContent {
            header: EncryptedContentHeader {
                id: "open-id".into(),
                display_hive_id: "".into(),
                content_type: "member-request".into(),
                acl_spec: AclSpec::OpenWrite {
                    target_hive_genesis_hash: target,
                },
                public_key_acl: empty_acl(),
                revision_author_signing_public_key: author.to_string(),
            },
            bytes: SerializedBytes::from(UnsafeBytes::from(vec![])),
        }
    }

    #[test]
    fn openwrite_with_no_target_accepts_without_fetch() {
        // OpenWrite { target: None } is the cross-network-discovery
        // case. Validator returns Valid pre-fetch (only the author-
        // identity check runs, and the header pubkey is built from
        // the author).
        let alice = agent_pubkey(1);
        let content = openwrite_content(alice.clone(), None);
        let result = run_content_validators(&alice, &Timestamp(0), &content)
            .expect("OpenWrite with no target runs pre-fetch");
        assert!(matches!(result, ValidateCallbackResult::Valid));
    }

    #[test]
    fn openwrite_header_pubkey_mismatch_rejects_pre_fetch() {
        // The pass-1 check_author_matches_header guard fires before any
        // OpenWrite-specific logic.
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let mut content = openwrite_content(bob, None);
        // Override the header pubkey to alice's string; action.author
        // (agent_pubkey(99)) will not match.
        content.header.revision_author_signing_public_key = alice.to_string();
        let result = run_content_validators(&agent_pubkey(99), &Timestamp(0), &content)
            .expect("OpenWrite validator runs pre-fetch on author-vs-header");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(
                    msg.contains("revision_author_signing_public_key"),
                    "got: {msg}"
                );
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------
    // Pass-4 — G-6.2 recipient-witness verification (pre-fetch branches).
    //
    // Steps 4 (cardinality) + 5 (bidirectional cross-check) are host-
    // reachable; step 6 (per-witness membership fetch) requires a
    // live conductor and is covered by Tryorama tests
    // (`tryorama-recipient-witnesses.test.ts`).
    // ---------------------------------------------------------------------

    /// Build an `AclByGroupGenesis` from a literal owner + bucket hashes.
    fn group_acl(
        owner: ActionHash,
        admin: Vec<ActionHash>,
        writer: Vec<ActionHash>,
        reader: Vec<ActionHash>,
    ) -> AclByGroupGenesis {
        AclByGroupGenesis {
            owner,
            admin,
            writer,
            reader,
        }
    }

    /// PKA with no owner string + arbitrary per-bucket pubkey strings.
    fn pka_from_buckets(
        admin: Vec<&AgentPubKey>,
        writer: Vec<&AgentPubKey>,
        reader: Vec<&AgentPubKey>,
    ) -> Acl {
        Acl {
            owner: "".into(),
            admin: admin.into_iter().map(|p| p.to_string()).collect(),
            writer: writer.into_iter().map(|p| p.to_string()).collect(),
            reader: reader.into_iter().map(|p| p.to_string()).collect(),
        }
    }

    fn witness(pubkey: AgentPubKey, bucket: AclBucket, membership: ActionHash) -> RecipientWitness {
        RecipientWitness {
            pubkey,
            bucket,
            membership_hash: membership,
        }
    }

    #[test]
    fn witnesses_empty_with_nonempty_pka_rejected() {
        // Forward direction failure: PKA has a reader entry but no
        // witness backs it.
        let bob = agent_pubkey(2);
        let pka = pka_from_buckets(vec![], vec![], vec![&bob]);
        let acl = group_acl(action_hash(10), vec![], vec![], vec![action_hash(11)]);
        let result = validate_recipient_witnesses(&[], &pka, &acl, &Timestamp(0))
            .expect("step 5 is pre-fetch");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(
                    msg.contains("not backed by any dominating recipient_witness"),
                    "got: {msg}"
                );
                assert!(msg.contains(&bob.to_string()), "got: {msg}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn witnesses_missing_pka_entry_rejected() {
        // PKA has two reader entries; only one is backed by a witness.
        // The unbacked entry rejects.
        let bob = agent_pubkey(2);
        let mallory = agent_pubkey(99);
        let pka = pka_from_buckets(vec![], vec![], vec![&bob, &mallory]);
        let acl = group_acl(action_hash(10), vec![], vec![], vec![action_hash(11)]);
        let witnesses = vec![witness(bob.clone(), AclBucket::Reader, action_hash(20))];
        let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0))
            .expect("step 5 is pre-fetch");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(
                    msg.contains("not backed by any dominating recipient_witness"),
                    "got: {msg}"
                );
                // Either bob or mallory message — the iteration order
                // is deterministic (owner, admin, writer, reader);
                // mallory comes after bob in reader vec so bob is
                // checked first. bob IS backed, so mallory is the
                // expected failure.
                assert!(msg.contains(&mallory.to_string()), "got: {msg}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn witnesses_over_claim_without_pka_entry_rejected() {
        // Reverse direction failure: witness claims a bucket for a
        // pubkey that is not in the corresponding PKA bucket.
        let bob = agent_pubkey(2);
        let mallory = agent_pubkey(99);
        // PKA has bob in reader; witness over-claims mallory in reader.
        let pka = pka_from_buckets(vec![], vec![], vec![&bob]);
        let acl = group_acl(action_hash(10), vec![], vec![], vec![action_hash(11)]);
        // Two witnesses: one legitimate (bob), one over-claim (mallory).
        // bob covers the forward-direction check; mallory triggers the
        // reverse-direction check.
        let witnesses = vec![
            witness(bob.clone(), AclBucket::Reader, action_hash(20)),
            witness(mallory.clone(), AclBucket::Reader, action_hash(21)),
        ];
        let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0))
            .expect("step 5 is pre-fetch");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.contains("claims bucket Reader"), "got: {msg}");
                assert!(msg.contains(&mallory.to_string()), "got: {msg}");
                assert!(msg.contains("not in public_key_acl.Reader"), "got: {msg}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn witnesses_step5_passes_when_round_trip_consistent_step6_triggers_fetch() {
        // Step 5 (bidirectional cross-check) accepts a single Reader-
        // bucket witness for bob in PKA.reader: the forward and
        // reverse passes both round-trip cleanly. Step 6 then fires
        // the membership fetch, which fails host-side because there
        // is no DHT — we observe Err, not Ok(Invalid). This pins the
        // step-5 → step-6 boundary for the simplest happy case.
        //
        // The dominance-happy-path (Admin-bucket witness backing a
        // Reader-bucket PKA entry) is covered end-to-end by Tryorama
        // because asserting on dominance requires the membership to
        // actually resolve.
        let bob = agent_pubkey(2);
        let pka = pka_from_buckets(vec![], vec![], vec![&bob]);
        let acl = group_acl(action_hash(10), vec![action_hash(11)], vec![], vec![]);
        let witnesses = vec![witness(bob.clone(), AclBucket::Reader, action_hash(20))];
        let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0));
        assert!(
            result.is_err(),
            "expected host fetch error after pre-fetch checks passed; got Ok({:?})",
            result.ok()
        );
    }

    #[test]
    fn witnesses_reader_cannot_back_admin_pka_step5() {
        // Bucket-dominance violation: bob is in PKA.admin; only a
        // Reader-bucket witness is provided. Reader does NOT dominate
        // Admin — step 5 must reject before any fetch.
        let bob = agent_pubkey(2);
        let pka = pka_from_buckets(vec![&bob], vec![], vec![]);
        let acl = group_acl(action_hash(10), vec![action_hash(11)], vec![], vec![]);
        let witnesses = vec![witness(bob.clone(), AclBucket::Reader, action_hash(20))];
        let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0))
            .expect("step 5 dominance check is pre-fetch");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                // Forward direction message — the Admin-bucket PKA
                // entry has no dominating witness.
                assert!(msg.contains("public_key_acl.Admin"), "got: {msg}");
                assert!(
                    msg.contains("not backed by any dominating recipient_witness"),
                    "got: {msg}"
                );
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn witnesses_exceed_max_count_rejected() {
        // Cardinality bound (step 4) fires before the cross-check
        // (step 5). Build HIVEGROUP_MAX_WITNESSES + 1 witnesses and
        // confirm the cardinality message is the rejection cause.
        let acl = group_acl(action_hash(10), vec![], vec![], vec![]);
        let pka = pka_from_buckets(vec![], vec![], vec![]); // contents irrelevant — step 4 fires first
        let witnesses: Vec<RecipientWitness> = (0..HIVEGROUP_MAX_WITNESSES + 1)
            .map(|i| {
                // Spread `i` across the first 4 bytes of the 36-byte
                // pubkey so no two witnesses collide even if
                // HIVEGROUP_MAX_WITNESSES is raised above 2^32. A
                // collision would surface as a dedup hit (step 5)
                // before the cardinality bound (step 4) fires —
                // changing the rejection reason from
                // "exceeds HIVEGROUP_MAX_WITNESSES" to "duplicate
                // pubkey", which is the wrong invariant to pin in
                // this test.
                let mut bytes = vec![0u8; 36];
                let i_bytes = (i as u32).to_le_bytes();
                bytes[..4].copy_from_slice(&i_bytes);
                let pk = AgentPubKey::from_raw_36(bytes);
                witness(pk, AclBucket::Reader, action_hash(20))
            })
            .collect();
        let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0))
            .expect("step 4 cardinality is pre-fetch");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(
                    msg.contains("HIVEGROUP_MAX_WITNESSES"),
                    "expected cardinality message; got: {msg}"
                );
                assert!(
                    msg.contains(&format!("= {}", HIVEGROUP_MAX_WITNESSES + 1)),
                    "expected actual count in message; got: {msg}"
                );
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn witnesses_duplicate_pubkey_rejected() {
        // Defense-in-depth: a duplicate-witness-for-same-pubkey forge
        // (Mallory stamps her pubkey twice across two buckets to mask
        // an over-claim) is rejected at the dedup check inside step 5.
        let bob = agent_pubkey(2);
        let pka = pka_from_buckets(vec![], vec![], vec![&bob]);
        let acl = group_acl(action_hash(10), vec![], vec![], vec![action_hash(11)]);
        let witnesses = vec![
            witness(bob.clone(), AclBucket::Reader, action_hash(20)),
            witness(bob.clone(), AclBucket::Reader, action_hash(21)),
        ];
        let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0))
            .expect("dedup is pre-fetch");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.contains("duplicate"), "got: {msg}");
                assert!(msg.contains(&bob.to_string()), "got: {msg}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn witnesses_reader_cannot_back_owner_pka_entry() {
        // Owner-bucket PKA entry must be backed by an Owner-bucket
        // (or higher — Owner is the highest) witness. Reader-bucket
        // witness does NOT satisfy the Owner PKA entry. Confirms
        // dominance applies uniformly across all four PKA buckets.
        let alice = agent_pubkey(1);
        let pka = Acl {
            owner: alice.to_string(),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        };
        let acl = group_acl(action_hash(10), vec![], vec![], vec![]);
        // Wrong bucket witness — Reader cannot back Owner.
        let witnesses = vec![witness(alice.clone(), AclBucket::Reader, action_hash(20))];
        let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0))
            .expect("step 5 is pre-fetch");
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.contains("public_key_acl.Owner"), "got: {msg}");
                assert!(
                    msg.contains("not backed by any dominating recipient_witness"),
                    "got: {msg}"
                );
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn acl_bucket_dominance_matrix() {
        // Pin the dominance ordering — Owner > Admin > Writer > Reader.
        // Any change to the AclBucket variant order or
        // bucket_required_role must keep this matrix intact.
        use AclBucket::*;
        for higher in [Owner, Admin, Writer, Reader] {
            assert!(higher.dominates(higher), "{higher:?} dominates self");
        }
        // Owner dominates everything.
        assert!(Owner.dominates(Admin));
        assert!(Owner.dominates(Writer));
        assert!(Owner.dominates(Reader));
        // Admin dominates Writer + Reader, not Owner.
        assert!(!Admin.dominates(Owner));
        assert!(Admin.dominates(Writer));
        assert!(Admin.dominates(Reader));
        // Writer dominates Reader, not above.
        assert!(!Writer.dominates(Owner));
        assert!(!Writer.dominates(Admin));
        assert!(Writer.dominates(Reader));
        // Reader dominates only itself.
        assert!(!Reader.dominates(Owner));
        assert!(!Reader.dominates(Admin));
        assert!(!Reader.dominates(Writer));
    }
}
