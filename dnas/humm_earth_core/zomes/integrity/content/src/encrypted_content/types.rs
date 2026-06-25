use hdi::prelude::*;

use crate::hive::{role_satisfies, Role};

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
