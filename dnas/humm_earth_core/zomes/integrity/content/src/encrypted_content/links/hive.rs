use hdi::prelude::*;

use super::common::{
    fetch_target_encrypted_content, recompute_base, require_link_author_is_target_author,
};

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
    let (target_action, target_entry) = match fetch_target_encrypted_content(&target_address)? {
        Ok(pair) => pair,
        Err(invalid) => return Ok(invalid),
    };
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
