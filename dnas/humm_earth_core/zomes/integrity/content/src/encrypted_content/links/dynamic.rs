use hdi::prelude::*;

use super::common::{
    decode_utf8_tag, fetch_target_encrypted_content, recompute_base,
    require_link_author_is_target_author,
};

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
    let (target_action, target_entry) = match fetch_target_encrypted_content(&target_address)? {
        Ok(pair) => pair,
        Err(invalid) => return Ok(invalid),
    };
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
