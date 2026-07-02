use hdi::prelude::*;

use super::common::{
    fetch_target_encrypted_content, recompute_base, require_link_author_is_target_author,
};

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
