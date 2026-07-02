use hdi::prelude::*;

use crate::encrypted_content::EncryptedContent;

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
