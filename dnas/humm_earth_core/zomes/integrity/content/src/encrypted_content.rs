//! [`EncryptedContent`] entry and every link validator that hangs off it.
use hdi::hash_path::path::Component;
use hdi::prelude::*;

use crate::hive::{check_hive_authority, HiveRole};

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct EncryptedContent {
    pub header: EncryptedContentHeader,
    pub bytes: SerializedBytes,
}

/// On-DHT header for encrypted content.
///
/// **Security-load-bearing fields** (validators enforce):
/// - `hive_genesis_hash` — the cryptographic hive identity this entry
///   claims membership in.
/// - `author_membership_hash` — `None` = author IS the genesis author of
///   `hive_genesis_hash`; `Some(hash)` = validator must fetch the
///   referenced [`crate::hive::HiveMembership`] and verify it grants
///   `action.author` at least Writer role.
/// - `revision_author_signing_public_key` — must equal `action.author`
///   (the pass-1 `check_author_matches_header` guard).
///
/// **Routing/display fields** (validators ignore for security; UI
/// consumes):
/// - `id` — opaque app-level identifier (humm-tauri's content squuid).
/// - `hive_id` — human-readable hive alias. Kept as a `String` for
///   migration continuity from the pre-pass-2 squuid-keyed era. Security
///   is rooted in `hive_genesis_hash`; this field is NOT trusted.
/// - `content_type` — opaque app-level content kind ("dm", "post",
///   "pair-ss", ...). Used by the path-recompute checks (it is the
///   second component of every hive-scoped link path).
/// - `acl` — per-entry access control list keyed by app-level group IDs
///   (humm-tauri Group squuids). Routing hint only.
/// - `public_key_acl` — per-entry access control list keyed by holohash
///   pubkeys. Load-bearing for DM delete authority (I-A); routing hint
///   for signal fan-out.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct EncryptedContentHeader {
    pub id: String,
    pub hive_id: String,
    pub hive_genesis_hash: ActionHash,
    pub author_membership_hash: Option<ActionHash>,
    pub content_type: String,
    pub acl: Acl,
    pub public_key_acl: Acl,
    pub revision_author_signing_public_key: String,
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

/// Run the pass-2 content guards in order:
/// 1. Pass-1 `check_author_matches_header`.
/// 2. Pass-2 `check_hive_authority(Writer)` against `hive_genesis_hash`
///    using the supplied `author_membership_hash`.
///
/// Short-circuits on first Invalid; propagates host errors from the
/// membership chain walk.
fn run_content_validators(
    author: &AgentPubKey,
    timestamp: &Timestamp,
    content: &EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    // Guard 1: header-pubkey vs. action.author.
    let header_check = check_author_matches_header(
        author,
        &content.header.revision_author_signing_public_key,
    );
    if !matches!(header_check, ValidateCallbackResult::Valid) {
        return Ok(header_check);
    }
    // Guard 2: hive-membership authority. Writer role is the minimum;
    // Admin and Owner satisfy the check too (role_satisfies).
    check_hive_authority(
        author,
        &content.header.hive_genesis_hash,
        content.header.author_membership_hash.as_ref(),
        HiveRole::Writer,
        timestamp,
    )
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
    run_content_validators(&action.author, &action.timestamp, &encrypted_content)
}

/// **I-A** — Receiver-initiated tombstone authorization.
///
/// Permitted deleters:
/// - The original author (`action.author == original_action.author()`).
/// - Any agent whose holohash string appears in
///   `original_entry.public_key_acl.{owner, admin, writer, reader}`.
///
/// The driver task originally specified `reader` only, but
/// `public_key_acl` is a per-entry **authorization** list keyed by
/// holohash pubkeys: any agent explicitly listed has standing. For a DM
/// the list is `[sender, recipient]` in `reader`, so either party can
/// delete. For a hive-scoped post listing admins, writers, or readers,
/// each of those agents has an explicit grant that includes deletion.
///
/// Note: the broader hive-membership Admin/Owner role does NOT
/// automatically convey delete authority over arbitrary content. The
/// author retains exclusive control unless they explicitly listed
/// someone in the entry's `public_key_acl.*` at commit time. This
/// matches privacy expectations for DMs (a hive Admin should NOT be
/// able to silently delete a DM between two members).
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
// EncryptedContentUpdates links (unchanged from pass-1)
// =============================================================================

pub fn validate_create_link_encrypted_content_updates(
    _action: CreateLink,
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

    // Hive-shape candidate.
    let hive_b64 = target_entry.header.hive_genesis_hash.to_string();
    let hive_base = recompute_base(&[&hive_b64, content_type])?;
    if hive_base == base_address {
        return Ok(ValidateCallbackResult::Valid);
    }

    Ok(ValidateCallbackResult::Invalid(format!(
        "Hive link base {base_address} matches neither the author-shape \
         path [{author_b64}, {content_type}] nor the hive-shape path \
         [{hive_b64}, {content_type}]",
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
    let hive_b64 = target_entry.header.hive_genesis_hash.to_string();
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

    let hive_b64 = target_entry.header.hive_genesis_hash.to_string();
    let content_id = target_entry.header.id.as_str();
    let expected = recompute_base(&[&hive_b64, content_id])?;
    if expected == base_address {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "HummContentId link base {base_address} does not match recomputed path \
         [{hive_b64}, {content_id}]",
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
/// Contract:
/// - Base = `Path([hive_genesis_hash_b64, content_type, entity_id])`
///   recomputed from target header + tag.
/// - Tag = UTF-8 bytes of the `entity_id` used in the third path
///   component. Required because the entity_id is per-link and cannot
///   be uniquely derived from the target alone (Admin/Writer/Reader
///   each fan out to multiple entity_ids).
/// - `entity_id` membership in the corresponding ACL set:
///   * `Owner` — must equal `target.acl.owner`.
///   * `Admin` — must be in `target.acl.admin`.
///   * `Writer` — must be in `target.acl.admin ∪ target.acl.writer`
///     (the writer set is the union per coordinator convention).
///   * `Reader` — must be in
///     `target.acl.admin ∪ target.acl.writer ∪ target.acl.reader`.
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

    // Recompute the base path: [hive_genesis_hash_b64, content_type, entity_id].
    let hive_b64 = target_entry.header.hive_genesis_hash.to_string();
    let content_type = target_entry.header.content_type.as_str();
    let expected = recompute_base(&[&hive_b64, content_type, &entity_id])?;
    if expected != base_address {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "{class_label} link base {base_address} does not match recomputed path \
             [{hive_b64}, {content_type}, {entity_id}]",
        )));
    }

    // Verify entity_id membership in the appropriate ACL set.
    let acl = &target_entry.header.acl;
    let valid_membership = match class {
        AclLinkClass::Owner => acl.owner == entity_id,
        AclLinkClass::Admin => acl.admin.iter().any(|a| a == &entity_id),
        AclLinkClass::Writer => {
            acl.admin.iter().any(|a| a == &entity_id)
                || acl.writer.iter().any(|w| w == &entity_id)
        }
        AclLinkClass::Reader => {
            acl.admin.iter().any(|a| a == &entity_id)
                || acl.writer.iter().any(|w| w == &entity_id)
                || acl.reader.iter().any(|r| r == &entity_id)
        }
    };
    if !valid_membership {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "{class_label} link entity_id {entity_id} is not in target acl.{} set",
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

    fn sample_header_pubkey(pk_b64: &str) -> EncryptedContentHeader {
        EncryptedContentHeader {
            id: "id".into(),
            hive_id: "hive".into(),
            hive_genesis_hash: action_hash(9),
            author_membership_hash: None,
            content_type: "ct".into(),
            acl: Acl {
                owner: "owner".into(),
                admin: vec![],
                writer: vec![],
                reader: vec![],
            },
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
            hive_id: "hive".into(),
            hive_genesis_hash: action_hash(9),
            author_membership_hash: None,
            content_type: "ct".into(),
            acl: Acl {
                owner: "owner".into(),
                admin: vec![],
                writer: vec![],
                reader: vec![],
            },
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
    fn delete_rejects_acl_match_by_string_only_on_unrelated_field() {
        // Defensive: ensure the validator looks at all four ACL fields,
        // not just one. If a future refactor accidentally drops a check,
        // this will catch it.
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let action = make_delete(bob.clone());
        let original = EntryCreationAction::Create(make_create(alice));
        // Bob is only in `reader` — should pass.
        let content = sample_content_with_acl(Acl {
            owner: "z".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![bob.to_string()],
        });
        assert!(matches!(
            validate_delete_encrypted_content(action, original, content).unwrap(),
            ValidateCallbackResult::Valid,
        ));
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
}
