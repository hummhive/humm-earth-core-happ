use hdi::prelude::*;

use crate::encrypted_content::EncryptedContent;

fn action_hash_from_link_address(
    address: AnyLinkableHash,
    label: &str,
) -> Result<ActionHash, ValidateCallbackResult> {
    address.into_action_hash().ok_or_else(|| {
        ValidateCallbackResult::Invalid(format!(
            "OriginalHashPointer {label} must be an ActionHash"
        ))
    })
}

fn require_encrypted_content_record(record: &Record, label: &str) -> ExternResult<()> {
    let _: EncryptedContent = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "OriginalHashPointer {label} does not reference an EncryptedContent"
            )))
        })?;
    Ok(())
}

fn encrypted_content_root_hash(mut action_hash: ActionHash) -> ExternResult<ActionHash> {
    loop {
        let record = must_get_valid_record(action_hash.clone())?;
        require_encrypted_content_record(&record, "base chain action")?;
        match record.action() {
            Action::Create(_) => return Ok(action_hash),
            Action::Update(update) => action_hash = update.original_action_address.clone(),
            _ => {
                return Err(wasm_error!(WasmErrorInner::Guest(
                    "OriginalHashPointer base chain action must be a Create or Update".into(),
                )));
            }
        }
    }
}

pub fn validate_create_link_original_hash_pointer(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    if !tag.0.is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "OriginalHashPointer tag must be empty".into(),
        ));
    }
    let base_hash = match action_hash_from_link_address(base_address, "base") {
        Ok(hash) => hash,
        Err(result) => return Ok(result),
    };
    let target_hash = match action_hash_from_link_address(target_address, "target") {
        Ok(hash) => hash,
        Err(result) => return Ok(result),
    };

    let base_record = must_get_valid_record(base_hash.clone())?;
    require_encrypted_content_record(&base_record, "base")?;
    if &action.author != base_record.action().author() {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "OriginalHashPointer link author {} does not match base entry author {}",
            action.author,
            base_record.action().author(),
        )));
    }

    let target_record = must_get_valid_record(target_hash.clone())?;
    require_encrypted_content_record(&target_record, "target")?;
    if !matches!(target_record.action(), Action::Create(_)) {
        return Ok(ValidateCallbackResult::Invalid(
            "OriginalHashPointer target must be the root Create action".into(),
        ));
    }
    if base_record.action().author() != target_record.action().author() {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "OriginalHashPointer base author {} does not match target root author {}",
            base_record.action().author(),
            target_record.action().author(),
        )));
    }

    let root_hash = encrypted_content_root_hash(base_hash)?;
    if root_hash != target_hash {
        return Ok(ValidateCallbackResult::Invalid(
            "OriginalHashPointer target must match the native update-chain root".into(),
        ));
    }

    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_delete_link_original_hash_pointer(
    action: DeleteLink,
    original_action: CreateLink,
    _base: AnyLinkableHash,
    _target: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    if action.author == original_action.author {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "OriginalHashPointer links may only be deleted by the link author (creator: {}, attempted by: {})",
        original_action.author, action.author,
    )))
}
