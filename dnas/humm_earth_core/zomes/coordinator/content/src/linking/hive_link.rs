use content_integrity::{EncryptedContent, LinkTypes};
use hdi::hash_path::path::Component;
use hdk::prelude::*;

/// Create the HIVE-shape `Hive` discovery link:
/// base = `Path([hive_genesis_hash_b64, content_type])` → `target`.
///
/// The integrity validator recomputes the base from
/// `target.header.hive_genesis_hash` + `target.header.content_type` and
/// rejects any link whose claimed base diverges, so the components used
/// here MUST match those header fields exactly.
pub fn create_hive_link(
    encrypted_content: EncryptedContent,
    action_hash: ActionHash,
) -> ExternResult<ActionHash> {
    let hive_path = Path::from(vec![
        Component::from(encrypted_content.header.hive_genesis_hash.to_string()),
        Component::from(encrypted_content.header.content_type.clone()),
    ]);
    let hive_ah = create_link(
        hive_path.path_entry_hash()?,
        action_hash.clone(),
        LinkTypes::Hive,
        (),
    )?;

    Ok(hive_ah)
}
