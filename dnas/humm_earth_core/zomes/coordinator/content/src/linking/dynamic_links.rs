use content_integrity::{EncryptedContentHeader, LinkTypes};
use hdi::hash_path::path::Component;
use hdk::prelude::*;

/// Create `Dynamic` links: one per supplied label, each with base
/// `Path([hive_genesis_hash_b64, content_type, dynamic_label])` → target.
///
/// **Pass-3.** The hive context now comes from
/// `target.header.hive_context()` rather than the removed
/// `header.hive_genesis_hash` field. Caller
/// (`create_encrypted_content`) gates this function on
/// `hive_context().is_some()`; this function asserts the invariant so
/// a misuse fails fast.
///
/// The integrity validator recomputes the base from
/// `hive_context()`, `header.content_type`, AND the `dynamic_label`
/// carried in the `LinkTag` (UTF-8 bytes). The tag is therefore
/// load-bearing — without it the validator cannot reconstruct the
/// third path component and would have to reject every dynamic link
/// by construction.
pub fn create_dynamic_links(
    header: &EncryptedContentHeader,
    action_hash: &ActionHash,
    dynamic_links: &[String],
) -> ExternResult<()> {
    let hive_hash = header.hive_context().ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(
            "create_dynamic_links called on a header with no hive_context \
             (DirectMessage or OpenWrite without target); the integrity \
             validator would reject the resulting links"
                .into(),
        ))
    })?;
    let hive_b64 = hive_hash.to_string();
    for label in dynamic_links {
        let hive_path = Path::from(vec![
            Component::from(hive_b64.clone()),
            Component::from(header.content_type.clone()),
            Component::from(label.clone()),
        ]);
        create_link(
            hive_path.path_entry_hash()?,
            action_hash.clone(),
            LinkTypes::Dynamic,
            // Tag carries the dynamic_label as UTF-8 bytes so the
            // integrity validator can recompute the base path.
            LinkTag::from(label.clone()),
        )?;
    }
    Ok(())
}
