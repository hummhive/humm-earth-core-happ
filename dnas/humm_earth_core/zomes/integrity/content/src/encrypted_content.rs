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
//! ## What is NOT enforced this pass (G-6.2 deferred)
//!
//! The recipient-set integrity check — "every pubkey in
//! `public_key_acl.{owner,admin,writer,reader}` must hold a matching
//! `GroupMembership` in the same-or-higher bucket of `group_acl`" —
//! is documented but DEFERRED to a follow-up sub-commit (Phase C.1).
//! Until then, `public_key_acl` on `HiveGroup` content is treated as
//! an unauthenticated routing hint at commit time. Decryption gating
//! (via SharedSecrets) is unaffected; recipient-list FORGERY (Mallory
//! adds her pubkey to the reader list to receive remote-signal
//! notifications even without group membership) is the residual
//! attack pending G-6.2.
use hdi::hash_path::path::Component;
use hdi::prelude::*;

use crate::group::check_group_authority;
use crate::hive::{check_hive_authority, Role};

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
    },
    /// Direct sender↔recipient(s). Pair or small-group; cross-hive
    /// viable (no hive/group membership check on recipients). The
    /// validator enforces:
    /// - `action.author` ∈ `recipients`
    /// - `2 <= recipients.len() <= DM_MAX_RECIPIENTS`
    /// - `public_key_acl.reader` equals `recipients` (for I-A delete
    ///   authority symmetry — either party may delete; routing fanout
    ///   matches).
    DirectMessage {
        recipients: Vec<AgentPubKey>,
    },
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
    let header_check = check_author_matches_header(
        author,
        &content.header.revision_author_signing_public_key,
    );
    if !matches!(header_check, ValidateCallbackResult::Valid) {
        return Ok(header_check);
    }
    match &content.header.acl_spec {
        AclSpec::HiveGroup {
            hive_genesis_hash,
            author_membership_hash,
            group_acl,
            author_group_membership_hash,
        } => validate_hivegroup_acl(
            author,
            timestamp,
            hive_genesis_hash,
            author_membership_hash.as_ref(),
            group_acl,
            author_group_membership_hash.as_ref(),
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
/// **Recipient-set integrity** (G-6.2) is documented but NOT
/// enforced this commit; see the module doc comment for the deferral.
/// Until G-6.2 lands, `public_key_acl` on HiveGroup content is an
/// unauthenticated routing hint.
fn validate_hivegroup_acl(
    author: &AgentPubKey,
    timestamp: &Timestamp,
    hive_genesis_hash: &ActionHash,
    author_membership_hash: Option<&ActionHash>,
    group_acl: &AclByGroupGenesis,
    author_group_membership_hash: Option<&ActionHash>,
) -> ExternResult<ValidateCallbackResult> {
    // Cardinality bound — same amplification mitigation as
    // DM_MAX_RECIPIENTS. Each group hash forces network calls;
    // unbounded fan-out is a validator-DoS surface.
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
    // Hive authority: Writer+ in the named hive (mirrors pass-2).
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
    // For each group listed in group_acl, the author must hold
    // Writer+. The same author_group_membership_hash witness is reused
    // across all groups; this is a deliberate simplification — the
    // common humm-tauri pattern publishes content under groups the
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
    let mut sorted_expected: Vec<String> =
        recipients.iter().map(|r| r.to_string()).collect();
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

/// **I-A** — Receiver-initiated tombstone authorization.
///
/// Permitted deleters:
/// - The original author (`action.author == original_action.author()`).
/// - Any agent whose holohash string appears in
///   `original_entry.public_key_acl.{owner, admin, writer, reader}`.
///
/// Across all four `AclSpec` variants this rule is uniform. For
/// `DirectMessage` the reader bucket carries the recipient set
/// (validated at create-time), so both parties retain delete authority.
/// For `HiveGroup` and `Public` the rule preserves the pass-2 contract.
/// For `OpenWrite` only the original author (or any pubkey the author
/// chose to list) can delete — useful for member-request retraction.
pub fn validate_delete_encrypted_content(
    action: Delete,
    original_action: EntryCreationAction,
    original_entry: EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    if &action.author == original_action.author() {
        return Ok(ValidateCallbackResult::Valid);
    }
    let author_str = action.author.to_string();
    let pka = &original_entry.header.public_key_acl;
    let listed = pka.owner == author_str
        || pka.admin.iter().any(|a| a == &author_str)
        || pka.writer.iter().any(|a| a == &author_str)
        || pka.reader.iter().any(|a| a == &author_str);
    if listed {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "delete by {} is not authorised: not the original author \
         ({}) and not listed in public_key_acl",
        action.author,
        original_action.author(),
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
        ValidateCallbackResult::Invalid(format!(
            "{tag_label} link tag is not valid UTF-8: {e}",
        ))
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
        AclLinkClass::Admin => group_acl
            .admin
            .iter()
            .any(|h| h.to_string() == entity_id),
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

    fn sample_content_with_acl(public_key_acl: Acl) -> EncryptedContent {
        let header = EncryptedContentHeader {
            id: "id".into(),
            display_hive_id: "hive".into(),
            content_type: "ct".into(),
            acl_spec: sample_acl_spec(),
            public_key_acl,
            revision_author_signing_public_key: agent_pubkey(1).to_string(),
        };
        EncryptedContent {
            header,
            bytes: SerializedBytes::from(UnsafeBytes::from(vec![])),
        }
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
        let result = check_author_matches_header(
            &alice,
            "test-revision-author-signing-public-key",
        );
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
    fn delete_accepts_recipient_in_public_key_acl_reader() {
        // DM scenario: sender = alice (entry author), recipient = bob in
        // public_key_acl.reader. Bob deletes; should be Valid.
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let action = make_delete(bob.clone());
        let original = EntryCreationAction::Create(make_create(alice));
        let content = sample_content_with_acl(Acl {
            owner: "x".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![bob.to_string()],
        });
        let result = validate_delete_encrypted_content(action, original, content)
            .expect("validator should not error in test");
        assert!(matches!(result, ValidateCallbackResult::Valid));
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
    fn delete_reader_acl_accept_reject_pair() {
        // Side-by-side accept/reject pin: the same delete-author with
        // the same original-action, differing only in whether the
        // entry's `public_key_acl.reader` contains the author's pubkey.
        // Catches any future regression where the validator stops
        // consulting the ACL it was given.
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let action = make_delete(bob.clone());
        let original = EntryCreationAction::Create(make_create(alice));
        let content_with_bob = sample_content_with_acl(Acl {
            owner: "x".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![bob.to_string()],
        });
        let accept = validate_delete_encrypted_content(action, original, content_with_bob)
            .expect("validator should not error in test");
        assert!(matches!(accept, ValidateCallbackResult::Valid));

        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let action = make_delete(bob);
        let original = EntryCreationAction::Create(make_create(alice));
        let content_without_bob = sample_content_with_acl(Acl {
            owner: "x".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        });
        let reject = validate_delete_encrypted_content(action, original, content_without_bob)
            .expect("validator should not error in test");
        assert!(matches!(reject, ValidateCallbackResult::Invalid(_)));
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
        let recomputed = recompute_base(&["a", "b", "c"])
            .expect("recompute_base should compute in test");
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
        let content = dm_content(alice.clone(), vec![alice.clone()], reader_acl(&[alice.clone()]));
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
        let result = run_content_validators(
            &agent_pubkey(1),
            &Timestamp(0),
            &content,
        )
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
}
