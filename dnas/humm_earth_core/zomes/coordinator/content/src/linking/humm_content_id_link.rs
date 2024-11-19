use content_integrity::{EncryptedContent, LinkTypes};
use hdk::prelude::*;
use hdi::hash_path::path::Component;

pub fn create_humm_content_id_link(
    encrypted_content: EncryptedContent,
    action_hash: ActionHash,
) -> ExternResult<ActionHash> {
    let path = Path::from(vec![
        Component::from(encrypted_content.header.hive_id),
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
