use hdi::prelude::*;

use super::common::{
    decode_utf8_tag, fetch_target_encrypted_content, recompute_base,
    require_link_author_is_target_author,
};

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
    let (target_action, target_entry) = match fetch_target_encrypted_content(&target_address)? {
        Ok(pair) => pair,
        Err(invalid) => return Ok(invalid),
    };
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
