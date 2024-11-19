use content_integrity::{EncryptedContent, LinkTypes};
use hdk::prelude::*;
use hdi::hash_path::path::Component;

pub fn create_dynamic_links(
    encrypted_content: EncryptedContent,
    action_hash: ActionHash,
    dynamic_links: Vec<String>,
) -> ExternResult<Vec<ActionHash>> {
    // let my_agent_pub_key = agent_info()?.agent_latest_pubkey;

    let mut ahs = vec![];
    dynamic_links.iter().for_each(|link| {
        // TODO: update this to use the acl to find all writers of this content and create corresponding links
        // do we need this or will we always fetch content by dynamic links in the context of a hive and not an author?
        // let author_path = Path::from(vec![
        //     Component::from(my_agent_pub_key.to_string()),
        //     Component::from(encrypted_content.header.content_type.clone()),
        //     Component::from(link.clone()),
        // ]);
        let hive_path = Path::from(vec![
            Component::from(encrypted_content.header.hive_id.clone()),
            Component::from(encrypted_content.header.content_type.clone()),
            Component::from(link),
        ]);

        // let author_path_entry_hash = author_path.path_entry_hash().expect(
        //     format!(
        //         "could not get path entry hash for author: '{}'",
        //         my_agent_pub_key
        //     )
        //     .as_str(),
        // );
        let hive_path_entry_hash = hive_path.path_entry_hash().expect(
            format!(
                "could not get path entry hash for hive: '{}'",
                encrypted_content.header.hive_id
            )
            .as_str(),
        );

        // let author_ah_res = create_link(
        //     author_path_entry_hash,
        //     action_hash.clone(),
        //     LinkTypes::HummContentWriter,
        //     (),
        // );

        let hive_ah_res = create_link(
            hive_path_entry_hash,
            action_hash.clone(),
            LinkTypes::Dynamic,
            (),
        );

        // let author_ah = author_ah_res
        //     .expect(format!("could not create link for author: '{}'", my_agent_pub_key).as_str());
        let hive_ah = hive_ah_res.expect(
            format!(
                "could not create link for hive: '{}'",
                encrypted_content.header.hive_id
            )
            .as_str(),
        );

        ahs.push(hive_ah);
    });

    Ok(ahs)
}
