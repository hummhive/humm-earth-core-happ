use content_integrity::{EncryptedContent, LinkTypes};
use hdi::hash_path::path::Component;
use hdk::prelude::*;

/// Create `Dynamic` links: one per supplied label, each with base
/// `Path([hive_genesis_hash_b64, content_type, dynamic_label])` → target.
///
/// The integrity validator recomputes the base from
/// `target.header.hive_genesis_hash`, `target.header.content_type`, AND
/// the `dynamic_label` carried in the `LinkTag` (UTF-8 bytes). The tag
/// is therefore load-bearing — without it the validator cannot
/// reconstruct the third path component and would have to reject every
/// dynamic link by construction.
pub fn create_dynamic_links(
    encrypted_content: EncryptedContent,
    action_hash: ActionHash,
    dynamic_links: Vec<String>,
) -> ExternResult<Vec<ActionHash>> {
    let mut ahs = Vec::with_capacity(dynamic_links.len());
    let hive_b64 = encrypted_content.header.hive_genesis_hash.to_string();
    let content_type = encrypted_content.header.content_type.clone();
    for label in dynamic_links {
        let hive_path = Path::from(vec![
            Component::from(hive_b64.clone()),
            Component::from(content_type.clone()),
            Component::from(label.clone()),
        ]);
        let hive_ah = create_link(
            hive_path.path_entry_hash()?,
            action_hash.clone(),
            LinkTypes::Dynamic,
            // Tag carries the dynamic_label as UTF-8 bytes so the
            // integrity validator can recompute the base path.
            LinkTag::from(label),
        )?;
        ahs.push(hive_ah);
    }
    Ok(ahs)
}
