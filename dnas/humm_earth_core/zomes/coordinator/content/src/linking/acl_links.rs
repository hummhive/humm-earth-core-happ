//! ACL discovery links for [`EncryptedContent`].
//!
//! Pass-3 changes:
//! - The link bundle is published **only for `AclSpec::HiveGroup`**.
//!   The other three variants (`DirectMessage`, `Public`, `OpenWrite`)
//!   have no `group_acl` field; the integrity link validator rejects
//!   HummContent* links targeting non-HiveGroup entries, so the
//!   coordinator must mirror the contract and skip publication.
//! - `entity_id` is now the string form of a `GroupGenesis` action
//!   hash (was a humm-tauri group squuid). The link's `LinkTag`
//!   carries this string verbatim so the integrity validator can
//!   recompute the third path component.
//! - Reading the per-bucket group lists comes from
//!   `header.group_acl()`, not the removed `header.acl`.
//!
//! Pass-2 invariants preserved:
//! - Writers receive both an `Admin` and a `Writer` link (admin ⊆
//!   writer). Readers receive Admin + Writer + Reader links. The
//!   integrity validator's per-class group_acl-set check matches this
//!   fan-out exactly.

use content_integrity::{AclByGroupGenesis, EncryptedContent, LinkTypes};
use hdi::hash_path::path::Component;
use hdk::prelude::*;

/// Build the ACL link's base path: `[hive_genesis_hash_b64,
/// content_type, entity_id]`. Returns the path hash a `create_link`
/// call would use as `base_address`.
fn acl_path_hash(
    hive_b64: &str,
    content_type: &str,
    entity_id: &str,
) -> ExternResult<holo_hash::EntryHash> {
    let path = Path::from(vec![
        Component::from(hive_b64.to_string()),
        Component::from(content_type.to_string()),
        Component::from(entity_id.to_string()),
    ]);
    path.path_entry_hash()
}

/// Create a single ACL link of the given variant. The `entity_id` is
/// stamped into the `LinkTag` as UTF-8 bytes so the integrity validator
/// can recompute the third path component (see this module's header).
fn create_acl_link(
    hive_b64: &str,
    content_type: &str,
    target: &ActionHash,
    entity_id: &str,
    link_type: LinkTypes,
) -> ExternResult<ActionHash> {
    create_link(
        acl_path_hash(hive_b64, content_type, entity_id)?,
        target.clone(),
        link_type,
        LinkTag::from(entity_id.to_string()),
    )
}

/// Create the full ACL link fan-out for an `EncryptedContent` entry
/// whose `acl_spec` is `AclSpec::HiveGroup`. Caller
/// (`create_encrypted_content`) gates on `group_acl().is_some()`;
/// this function asserts the invariant so a misuse fails fast.
///
/// Fan-out (matches the integrity validator's per-class group_acl-set
/// check):
/// - one `Owner` link for `group_acl.owner`
/// - one `Admin` link per `group_acl.admin`
/// - one `Writer` link per member of (`admin ∪ writer`)
/// - one `Reader` link per member of (`admin ∪ writer ∪ reader`)
pub fn create_acl_links(
    encrypted_content: EncryptedContent,
    action_hash: ActionHash,
) -> ExternResult<Vec<ActionHash>> {
    let hive_hash = encrypted_content.header.hive_context().ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(
            "create_acl_links called on a header with no hive_context".into(),
        ))
    })?;
    let group_acl: &AclByGroupGenesis = encrypted_content.header.group_acl().ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(
            "create_acl_links called on a header whose acl_spec is not \
                 HiveGroup; HummContent* links anchor only to HiveGroup \
                 content"
                .into(),
        ))
    })?;
    let hive_b64 = hive_hash.to_string();
    let content_type = encrypted_content.header.content_type.clone();

    // entity_id is the string form of a GroupGenesis ActionHash.
    let owner_id = group_acl.owner.to_string();
    let admin_ids: Vec<String> = group_acl.admin.iter().map(|h| h.to_string()).collect();
    // Writer set = admin ∪ writer (admins inherit writer rights).
    let writer_ids: Vec<String> = group_acl
        .admin
        .iter()
        .chain(group_acl.writer.iter())
        .map(|h| h.to_string())
        .collect();
    // Reader set = (admin ∪ writer) ∪ reader.
    let reader_ids: Vec<String> = group_acl
        .admin
        .iter()
        .chain(group_acl.writer.iter())
        .chain(group_acl.reader.iter())
        .map(|h| h.to_string())
        .collect();

    let mut acl_link_action_hashes: Vec<ActionHash> = Vec::new();
    acl_link_action_hashes.push(create_acl_link(
        &hive_b64,
        &content_type,
        &action_hash,
        &owner_id,
        LinkTypes::HummContentOwner,
    )?);
    for id in &admin_ids {
        acl_link_action_hashes.push(create_acl_link(
            &hive_b64,
            &content_type,
            &action_hash,
            id,
            LinkTypes::HummContentAdmin,
        )?);
    }
    for id in &writer_ids {
        acl_link_action_hashes.push(create_acl_link(
            &hive_b64,
            &content_type,
            &action_hash,
            id,
            LinkTypes::HummContentWriter,
        )?);
    }
    for id in &reader_ids {
        acl_link_action_hashes.push(create_acl_link(
            &hive_b64,
            &content_type,
            &action_hash,
            id,
            LinkTypes::HummContentReader,
        )?);
    }
    Ok(acl_link_action_hashes)
}
