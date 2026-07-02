use hdi::hash_path::path::Component;
use hdi::prelude::*;

use crate::encrypted_content::EncryptedContent;

/// Fetch the target `EncryptedContent` referenced by a link's
/// `target_address`, returning the action and the typed entry.
///
/// Every hive-scoped link validator (`Hive`, `Dynamic`, `HummContent*`,
/// `HummContentId`) starts here to recover the authoritative header
/// fields used for path recomputation.
pub(super) fn fetch_target_encrypted_content(
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
pub(crate) fn recompute_base(components: &[&str]) -> ExternResult<AnyLinkableHash> {
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
pub(super) fn require_link_author_is_target_author(
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
pub(super) fn decode_utf8_tag(
    tag: &LinkTag,
    tag_label: &str,
) -> Result<String, ValidateCallbackResult> {
    String::from_utf8(tag.0.clone()).map_err(|e| {
        ValidateCallbackResult::Invalid(format!("{tag_label} link tag is not valid UTF-8: {e}",))
    })
}
