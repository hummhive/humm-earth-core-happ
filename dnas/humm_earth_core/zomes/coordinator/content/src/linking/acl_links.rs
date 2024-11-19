use content_integrity::{EncryptedContent, LinkTypes};
use hdk::prelude::*;
use hdi::hash_path::path::Component;

// TODO: use the public key acl instead of the entity acl

pub fn create_acl_links(
    encrypted_content: EncryptedContent,
    action_hash: ActionHash,
) -> ExternResult<Vec<ActionHash>> {
    // add a link for each author based on the ACK admin and write fields
    let mut acl_link_action_hashes: Vec<ActionHash> = vec![];
    let owner = encrypted_content.header.acl.owner;
    let admins: Vec<String> = encrypted_content.header.acl.admin.clone();
    let writers: Vec<String> = encrypted_content
        .header
        .acl
        .admin
        .iter()
        .chain(encrypted_content.header.acl.writer.clone().iter())
        .cloned()
        .collect();
    let readers: Vec<String> = writers
        .iter()
        .chain(encrypted_content.header.acl.reader.clone().iter())
        .cloned()
        .collect();

    // owner
    let owner_path = Path::from(vec![
        Component::from(encrypted_content.header.hive_id.clone()),
        Component::from(encrypted_content.header.content_type.clone()),
        Component::from(owner.to_string()),
    ]);

    let owner_ah = create_link(
        owner_path
            .path_entry_hash()
            .expect(format!("could not get path entry hash for owner: '{}'", owner).as_str()),
        action_hash.clone(),
        LinkTypes::HummContentOwner,
        (),
    );
    acl_link_action_hashes
        .push(owner_ah.expect(format!("could not create link for owner: '{}'", owner).as_str()));

    admins.iter().for_each(|id| {
        let path = Path::from(vec![
            Component::from(encrypted_content.header.hive_id.clone()),
            Component::from(encrypted_content.header.content_type.clone()),
            Component::from(id.to_string()),
        ]);

        let ah = create_link(
            path.path_entry_hash()
                .expect(format!("could not get path entry hash for admin: '{}'", id).as_str()),
            action_hash.clone(),
            LinkTypes::HummContentAdmin,
            (),
        );
        acl_link_action_hashes
            .push(ah.expect(format!("could not create link for admin: '{}'", id).as_str()));
    });

    writers.iter().for_each(|id| {
        let path = Path::from(vec![
            Component::from(encrypted_content.header.hive_id.clone()),
            Component::from(encrypted_content.header.content_type.clone()),
            Component::from(id.to_string()),
        ]);

        let ah = create_link(
            path.path_entry_hash()
                .expect(format!("could not get path entry hash for writer: '{}'", id).as_str()),
            action_hash.clone(),
            LinkTypes::HummContentWriter,
            (),
        );
        acl_link_action_hashes
            .push(ah.expect(format!("could not create link for writer: '{}'", id).as_str()));
    });

    readers.iter().for_each(|id| {
        let path = Path::from(vec![
            Component::from(encrypted_content.header.hive_id.clone()),
            Component::from(encrypted_content.header.content_type.clone()),
            Component::from(id.to_string()),
        ]);

        let ah = create_link(
            path.path_entry_hash()
                .expect(format!("could not get path entry hash for reader: '{}'", id).as_str()),
            action_hash.clone(),
            LinkTypes::HummContentReader,
            (),
        );
        acl_link_action_hashes
            .push(ah.expect(format!("could not create link for reader: '{}'", id).as_str()));
    });

    Ok(acl_link_action_hashes)
}
