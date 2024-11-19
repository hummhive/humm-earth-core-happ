use std::vec;

use content_integrity::*;
use hdk::prelude::*;
use hdi::hash_path::path::Component;
use time_indexing::*;

pub fn time_index_encrypted_content(ah: ActionHash, content_type: &str) -> ExternResult<TypedPath> {
    let agent_info = agent_info()?;
    let time = get(ah.clone(), GetOptions::default())?
        .unwrap()
        .action()
        .timestamp();
    let path = Path::from(vec![
        Component::from(ENCRYPTED_CONTENT_TIME_INDEX),
        Component::from(agent_info.agent_latest_pubkey.to_string()), // TODO: how to handle agent public key changes?
    ]);
    let index = index_item(
        path.typed(LinkTypes::TimePath)?,
        ah.clone().into(),
        content_type,
        LinkTypes::TimeItem.try_into().unwrap(),
        time,
        &vec![], // todo: what is this used for? a vec of u8s
    )?;

    Ok(index.0)
}

// TODO
// pub fn update_index_encrypted_content(
//     ah: ActionHash,
//     content_type: &str,
//     index_time: Timestamp,
// ) -> ExternResult<TypedPath> {
//     let agent_info = agent_info()?;
//     let path = Path::from(vec![
//         Component::from(ENCRYPTED_CONTENT_TIME_INDEX),
//         Component::from(agent_info.agent_latest_pubkey.to_string()), // TODO: how to handle agent public key changes?
//     ]);
//     let index = index_item(
//         path.typed(LinkTypes::TimePath)?,
//         ah.clone().into(),
//         content_type,
//         LinkTypes::TimeItem.try_into().unwrap(),
//         index_time,
//         &vec![], // todo: what is this used for? a vec of u8s
//     )?;

//     Ok(index.0)
// }

pub fn get_encrypted_content_time_index_links(
    author: AgentPubKey,
    content_type: &str,
    start_time: Option<Timestamp>,
    end_time: Option<Timestamp>,
    limit: Option<usize>,
) -> ExternResult<(SweepInterval, Vec<(Timestamp, Link)>)> {
    let begin = start_time.unwrap_or(Timestamp::HOLOCHAIN_EPOCH); // FIXME use dna_info.origin_time when available
    let end = end_time.unwrap_or(sys_time().unwrap());
    let limit = limit.unwrap_or(usize::MAX);
    debug!("start = {} | target_count = {}", begin, limit);
    let sweep_interval = SweepInterval::new(begin, end)?;
    let tag = LinkTag::new(content_type.to_string());
    let path = Path::from(vec![
        Component::from(ENCRYPTED_CONTENT_TIME_INDEX),
        Component::from(author.to_string()),
    ]);
    let response = get_latest_time_indexed_links(
        path.typed(LinkTypes::TimePath)?,
        sweep_interval,
        limit,
        Some(tag),
        LinkTypes::TimeItem,
    )?;

    debug!("links.len = {}", response.1.len());
    // / Convert links to BeadLinks
    // let bls: Vec<BeadLink> = response
    //     .1
    //     .into_iter()
    //     .map(|(_index_time, link)| {
    //         let bt: TimedItemTag = SerializedBytes::from(UnsafeBytes::from(link.tag.0))
    //             .try_into()
    //             .unwrap();
    //         BeadLink {
    //             //index_time,
    //             creation_time: bt.devtest_timestamp,
    //             //creation_time: link.timestamp,
    //             bead_ah: ActionHash::try_from(link.target).unwrap(),
    //             bead_type: bt.item_type,
    //             //bead_type: tag2str(&link.tag).unwrap(),
    //         }
    //     })
    //     .collect();
    // debug!("bls.len = {}", bls.len());

    Ok(response)
}
