use hdi::prelude::*;

use super::common::{
    fetch_target_encrypted_content, recompute_base, require_link_author_is_target_author,
};

/// Validate a `Lineage` link create. The base must equal
/// `Path([prior_dna_hash_b64, prior_action_hash_b64])` recomputed from the
/// TARGET entry's own `header.lineage` claim (both components are typed
/// header fields, so no tag decode is needed), and the link author must be
/// the target entry's author — closing forged-index poisoning of
/// `resolve_by_prior_generation`.
pub fn validate_create_link_lineage(
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

    let Some(lineage) = target_entry.header.lineage.as_ref() else {
        return Ok(ValidateCallbackResult::Invalid(
            "Lineage link target has no lineage claim in its header".to_string(),
        ));
    };
    let expected = recompute_base(&[&lineage.prior_dna_hash_b64, &lineage.prior_action_hash_b64])?;
    if expected == base_address {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(
        "Lineage link base does not match the target's lineage claim".to_string(),
    ))
}

pub fn validate_delete_link_lineage(
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
        "Lineage link delete must be authored by the link creator".into(),
    ))
}
