use content_integrity::*;
use hdk::prelude::*;
use hdi::hash_path::path::Component;
use zome_utils::*;

use crate::{
    dynamic_links::create_dynamic_links, hive_link::create_hive_link,
    humm_content_id_link::create_humm_content_id_link, linking::acl_links::create_acl_links,
    time_indexed_links::*,
};

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct EncryptedContentResponse {
    pub encrypted_content: EncryptedContent,
    pub hash: String,
    pub original_hash: String,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct CreateEncryptedContentInput {
    pub id: String,
    pub hive_id: String,
    pub content_type: String,
    pub revision_author_signing_public_key: String,
    pub bytes: SerializedBytes,
    pub acl: Acl,
    pub public_key_acl: Acl,
    pub dynamic_links: Option<Vec<String>>,
}

#[hdk_extern]
pub fn create_encrypted_content(
    input: CreateEncryptedContentInput,
) -> ExternResult<EncryptedContentResponse> {
    let encrypted_content = EncryptedContent {
        header: EncryptedContentHeader {
            id: input.id,
            hive_id: input.hive_id.clone(),
            content_type: input.content_type.clone(),
            revision_author_signing_public_key: input.revision_author_signing_public_key,
            acl: input.acl,
            public_key_acl: input.public_key_acl,
        },
        bytes: input.bytes,
    };
    let action_hash = create_entry(&EntryTypes::EncryptedContent(encrypted_content.clone()))?;
    // let record = get(action_hash.clone(), GetOptions::default())?.ok_or(wasm_error!(
    //     WasmErrorInner::Guest(String::from(
    //         "Could not find the newly created EncryptedContent"
    //     ))
    // ))?;

    // create original hash pointer link pointing to itslef
    create_link(
        action_hash.clone(),
        action_hash.clone(),
        LinkTypes::OriginalHashPointer,
        (),
    )?;

    // create link to the author
    let my_agent_pub_key = agent_info()?.agent_latest_pubkey;
    let author_link_path = Path::from(vec![
        Component::from(my_agent_pub_key.to_string()),
        Component::from(input.content_type),
    ]);
    create_link(
        author_link_path.path_entry_hash()?,
        action_hash.clone(),
        LinkTypes::Hive,
        (),
    )?;

    // acl links
    create_acl_links(encrypted_content.clone(), action_hash.clone())?;

    // hive link - ignore empty hive_id which is used by hive discovery entries
    if input.hive_id != "" {
        create_hive_link(encrypted_content.clone(), action_hash.clone())?;
    }

    // content ID link
    create_humm_content_id_link(encrypted_content.clone(), action_hash.clone())?;

    // dynamic links
    if let Some(dynamic_links) = input.dynamic_links {
        create_dynamic_links(
            encrypted_content.clone(),
            action_hash.clone(),
            dynamic_links,
        )?;
    }

    // time indexing links
    time_index_encrypted_content(action_hash.clone(), &encrypted_content.header.content_type)?;

    Ok(EncryptedContentResponse {
        encrypted_content,
        hash: action_hash.clone().to_string(),
        original_hash: action_hash.to_string(),
    })
}

#[hdk_extern]
pub fn get_encrypted_content(content_hash: ActionHash) -> ExternResult<EncryptedContentResponse> {
    let ah = get_eh(content_hash.clone())?;
    let Some((entry, hash, _)) = get_latest_typed_from_eh(ah)? else {
        return Err(wasm_error!(WasmErrorInner::Guest(String::from(
            "Could not find the EncryptedContent"
        ))));
    };
    Ok(EncryptedContentResponse {
        encrypted_content: entry,
        hash: hash.to_string(),
        original_hash: content_hash.to_string(),
    })
}

#[hdk_extern]
pub fn get_many_encrypted_content(
    ahs: Vec<ActionHash>,
) -> ExternResult<Vec<EncryptedContentResponse>> {
    return ahs
        .into_iter()
        .map(|ah| get_encrypted_content(ah))
        .collect();
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetEncryptedContentByTimeAndAuthorInput {
    author: AgentPubKey,
    content_type: String,
    start_time: Option<Timestamp>,
    end_time: Option<Timestamp>,
    limit: Option<usize>,
}

#[hdk_extern]
pub fn get_encrypted_content_by_time_and_author(
    input: GetEncryptedContentByTimeAndAuthorInput,
) -> ExternResult<Vec<EncryptedContentResponse>> {
    let res = get_encrypted_content_time_index_links(
        input.author,
        &input.content_type,
        input.start_time,
        input.end_time,
        input.limit,
    )?;
    let hashes: Vec<ActionHash> = res
        .1
        .into_iter()
        .map(|(_, link)| link.target.into_action_hash())
        .filter_map(|x| x)
        .collect();
    get_many_encrypted_content(hashes)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListByDynamicLinkInput {
    pub hive_id: String,
    pub content_type: String,
    pub dynamic_link: String,
}

#[hdk_extern]
pub fn list_by_dynamic_link(
    input: ListByDynamicLinkInput,
) -> ExternResult<Vec<EncryptedContentResponse>> {
    let path = Path::from(vec![
        Component::from(input.hive_id),
        Component::from(input.content_type),
        Component::from(input.dynamic_link.clone()),
    ]);
    let get_links_input = GetLinksInputBuilder::try_new(path.path_entry_hash()?, LinkTypes::Dynamic)?.build();
    let links = get_links(get_links_input)?;
    let hashes: Vec<ActionHash> = links
        .into_iter()
        .map(|link| link.target.into_action_hash())
        .filter_map(|x| x)
        .collect();
    get_many_encrypted_content(hashes)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListByHiveInput {
    pub hive_id: String,
    pub content_type: String,
}

#[hdk_extern]
pub fn list_by_hive_link(input: ListByHiveInput) -> ExternResult<Vec<EncryptedContentResponse>> {
    let path = Path::from(vec![
        Component::from(input.hive_id),
        Component::from(input.content_type),
    ]);
    let get_links_input = GetLinksInputBuilder::try_new(path.path_entry_hash()?, LinkTypes::Hive)?.build();
    let links = get_links(get_links_input)?;
    let hashes: Vec<ActionHash> = links
        .into_iter()
        .map(|link| link.target.into_action_hash())
        .filter_map(|x| x)
        .collect();
    get_many_encrypted_content(hashes)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListByContentIdInput {
    pub hive_id: String,
    pub content_id: String,
}

#[hdk_extern]
pub fn get_by_content_id_link(
    input: ListByContentIdInput,
) -> ExternResult<EncryptedContentResponse> {
    let path = Path::from(vec![
        Component::from(input.hive_id.clone()),
        Component::from(input.content_id.clone()),
    ]);
    
    let get_links_input = GetLinksInputBuilder::try_new(path.path_entry_hash()?, LinkTypes::HummContentId)?.build();
    let links = get_links(get_links_input)?;

    let hashes: Vec<ActionHash> = links
        .into_iter()
        .map(|link| link.target.into_action_hash())
        .filter_map(|x| x)
        .collect();

    if hashes.len() == 0 {
        return Err(wasm_error!(WasmErrorInner::Guest(format!(
            "Could not find the content with id: \"{}\"",
            input.content_id
        ))));
    }
    get_encrypted_content(hashes[0].clone())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListByAclInput {
    pub hive_id: String,
    pub content_type: String,
    pub acl_role: String, // cant get enum to work with serde
    // pub acl_role: AclRole,
    pub entity_id: String,
}
// #[derive(Serialize, Deserialize, Debug)]
// pub enum AclRole {
//     Owner,
//     Admin,
//     Writer,
//     Reader,
// }

#[hdk_extern]
pub fn list_by_acl_link(input: ListByAclInput) -> ExternResult<Vec<EncryptedContentResponse>> {
    let path = Path::from(vec![
        Component::from(input.hive_id),
        Component::from(input.content_type),
        Component::from(input.entity_id.clone()),
    ]);

    let links = match input.acl_role.as_str() {
        "Owner" => get_links(GetLinksInputBuilder::try_new(path.path_entry_hash()?, LinkTypes::HummContentOwner)?.build())?,
        "Admin" => get_links(GetLinksInputBuilder::try_new(path.path_entry_hash()?, LinkTypes::HummContentAdmin)?.build())?,
        "Writer" => get_links(GetLinksInputBuilder::try_new(path.path_entry_hash()?, LinkTypes::HummContentWriter)?.build())?,
        "Reader" => get_links(GetLinksInputBuilder::try_new(path.path_entry_hash()?, LinkTypes::HummContentReader)?.build())?,
        _ => {
            return Err(wasm_error!(WasmErrorInner::Guest(String::from(
                "Invalid acl_role"
            ))))
        }
    };

    let hashes: Vec<ActionHash> = links
        .into_iter()
        .map(|link| link.target.into_action_hash())
        .filter_map(|x| x)
        .collect();
    get_many_encrypted_content(hashes)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListByAuthorInput {
    pub author: String,
    pub content_type: String,
}
#[hdk_extern]
pub fn list_by_author(input: ListByAuthorInput) -> ExternResult<Vec<EncryptedContentResponse>> {
    let path = Path::from(vec![
        Component::from(input.author),
        Component::from(input.content_type),
    ]);

    let links = get_links(GetLinksInputBuilder::try_new(path.path_entry_hash()?, LinkTypes::Hive)?.build())?;
    let hashes: Vec<ActionHash> = links
        .into_iter()
        .map(|link| link.target.into_action_hash())
        .filter_map(|x| x)
        .collect();
    let a = get_many_encrypted_content(hashes);
    a
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdateEncryptedContentInput {
    pub previous_encrypted_content_hash: ActionHash,
    pub updated_encrypted_content: EncryptedContent,
}

#[hdk_extern]
pub fn update_encrypted_content(
    input: UpdateEncryptedContentInput,
) -> ExternResult<EncryptedContentResponse> {
    let updated_encrypted_content_hash = update_entry(
        input.previous_encrypted_content_hash.clone(),
        &input.updated_encrypted_content,
    )?;

    let original_hash_link = get_links(GetLinksInputBuilder::try_new(input.previous_encrypted_content_hash.clone(), LinkTypes::OriginalHashPointer)?.build())?;

    if original_hash_link.is_empty() {
        return Err(wasm_error!(WasmErrorInner::Guest(format!(
            "Could not find the hash of the original EncryptedContent that is trying to be updated"
        ))));
    }
    create_link(
        original_hash_link[0]
            .clone()
            .target
            .into_action_hash()
            .unwrap(),
        updated_encrypted_content_hash.clone(),
        LinkTypes::EncryptedContentUpdates,
        (),
    )?;
    create_link(
        updated_encrypted_content_hash.clone(),
        original_hash_link[0]
            .clone()
            .target
            .into_action_hash()
            .unwrap(),
        LinkTypes::OriginalHashPointer,
        (),
    )?;

    // TODO: create time link. get rid of default links and update links?
    let record = get_encrypted_content(updated_encrypted_content_hash.clone())?;
    Ok(record)
}

#[hdk_extern]
pub fn delete_encrypted_content(
    original_encrypted_content_hash: ActionHash,
) -> ExternResult<ActionHash> {
    delete_entry(original_encrypted_content_hash)
    // TODO: delete links
}
