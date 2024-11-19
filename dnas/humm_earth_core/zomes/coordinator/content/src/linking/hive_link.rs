use content_integrity::{EncryptedContent, LinkTypes};
use hdk::prelude::*;
use hdi::hash_path::path::Component;

pub fn create_hive_link(
    encrypted_content: EncryptedContent,
    action_hash: ActionHash,
) -> ExternResult<ActionHash> {
    let hive_path = Path::from(vec![
        Component::from(encrypted_content.header.hive_id),
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
