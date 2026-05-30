use content_integrity::{EncryptedContent, LinkTypes};
use hdi::hash_path::path::Component;
use hdk::prelude::*;

/// Create the `HummContentId` lookup link:
/// base = `Path([hive_genesis_hash_b64, header.id])` → `target`.
///
/// The integrity validator recomputes the base from
/// `target.header.hive_genesis_hash` + `target.header.id`; the link
/// author must equal the target entry's author.
pub fn create_humm_content_id_link(
    encrypted_content: EncryptedContent,
    action_hash: ActionHash,
) -> ExternResult<ActionHash> {
    let path = Path::from(vec![
        Component::from(encrypted_content.header.hive_genesis_hash.to_string()),
        Component::from(encrypted_content.header.id),
    ]);

    let hive_ah = create_link(
        path.path_entry_hash()?,
        action_hash.clone(),
        LinkTypes::HummContentId,
        (),
    )?;

    Ok(hive_ah)
}
