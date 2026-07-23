use content_integrity::{EncryptedContentHeader, LinkTypes};
use hdi::hash_path::path::Component;
use hdk::prelude::*;

/// Create the HIVE-shape `Hive` discovery link:
/// base = `Path([hive_genesis_hash_b64, content_type])` → `target`.
///
/// **Pass-3.** The hive context now comes from
/// `target.header.hive_context()` rather than the removed
/// `header.hive_genesis_hash` field. The accessor returns `Some(hash)`
/// for `AclSpec::HiveGroup`, `AclSpec::Public`, and
/// `AclSpec::OpenWrite { target: Some(...) }`; `None` for
/// `AclSpec::DirectMessage` and `AclSpec::OpenWrite { target: None }`.
/// Caller (`create_encrypted_content`) gates this function on
/// `hive_context().is_some()`; this function asserts the invariant
/// rather than silently skipping so a misuse fails fast at commit
/// time rather than rotting into an unfindable entry.
pub fn create_hive_link(
    header: &EncryptedContentHeader,
    action_hash: &ActionHash,
) -> ExternResult<()> {
    let hive_hash = header.hive_context().ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(
            "create_hive_link called on a header with no hive_context \
             (DirectMessage or OpenWrite without target); the integrity \
             validator would reject the resulting link"
                .into(),
        ))
    })?;
    let hive_path = Path::from(vec![
        Component::from(hive_hash.to_string()),
        Component::from(header.content_type.clone()),
    ]);
    create_link(
        hive_path.path_entry_hash()?,
        action_hash.clone(),
        LinkTypes::Hive,
        (),
    )?;
    Ok(())
}
