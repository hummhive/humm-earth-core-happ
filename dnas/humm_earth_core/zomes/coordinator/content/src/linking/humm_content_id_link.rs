use content_integrity::{EncryptedContentHeader, LinkTypes};
use hdi::hash_path::path::Component;
use hdk::prelude::*;

/// Create the `HummContentId` lookup link:
/// base = `Path([hive_genesis_hash_b64, header.id])` → `target`.
///
/// **Pass-3.** Hive context recovered via `header.hive_context()`.
/// Caller gates on `hive_context().is_some()`; this function asserts
/// the invariant so a misuse fails fast.
pub fn create_humm_content_id_link(
    header: &EncryptedContentHeader,
    action_hash: &ActionHash,
) -> ExternResult<()> {
    let hive_hash = header.hive_context().ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(
            "create_humm_content_id_link called on a header with no \
             hive_context (DirectMessage or OpenWrite without target); \
             the integrity validator would reject the resulting link"
                .into(),
        ))
    })?;
    let path = Path::from(vec![
        Component::from(hive_hash.to_string()),
        Component::from(header.id.clone()),
    ]);
    create_link(
        path.path_entry_hash()?,
        action_hash.clone(),
        LinkTypes::HummContentId,
        (),
    )?;
    Ok(())
}
