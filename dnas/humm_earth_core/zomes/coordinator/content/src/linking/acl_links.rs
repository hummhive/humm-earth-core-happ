//! ACL discovery links for [`EncryptedContent`].
//!
//! Pass-2 changes:
//! - Base path's first component is now `hive_genesis_hash.to_string()`
//!   (was `hive_id`). The integrity validator recomputes against
//!   `target.header.hive_genesis_hash` and rejects any divergence.
//! - The link's `LinkTag` now carries the `entity_id` (the third path
//!   component) as UTF-8 bytes. The integrity validator needs this to
//!   rebuild the full base path; without the tag, ACL link validation
//!   would have to enumerate every entity in the relevant ACL set
//!   to find which one produced the supplied base hash.
//! - `.expect()` panics replaced with `?` so a failed link write
//!   surfaces as a validation error instead of crashing the wasm.
//!
//! The convention preserved from pass-1: writers receive both an
//! `Admin` and a `Writer` link (admin ⊆ writer); readers receive
//! Admin + Writer + Reader links. The integrity validator's per-class
//! ACL-set check matches this fan-out exactly.

use content_integrity::{EncryptedContent, LinkTypes};
use hdi::hash_path::path::Component;
use hdk::prelude::*;

/// Build the ACL link's base path: `[hive_genesis_hash_b64,
/// content_type, entity_id]`. Returns the path hash a `create_link`
/// call would use as `base_address`.
fn acl_path_hash(
    encrypted_content: &EncryptedContent,
    entity_id: &str,
) -> ExternResult<holo_hash::EntryHash> {
    let path = Path::from(vec![
        Component::from(encrypted_content.header.hive_genesis_hash.to_string()),
        Component::from(encrypted_content.header.content_type.clone()),
        Component::from(entity_id.to_string()),
    ]);
    path.path_entry_hash()
}

/// Create a single ACL link of the given variant. The `entity_id` is
/// stamped into the `LinkTag` as UTF-8 bytes so the integrity validator
/// can recompute the third path component (see this module's header).
fn create_acl_link(
    encrypted_content: &EncryptedContent,
    target: &ActionHash,
    entity_id: &str,
    link_type: LinkTypes,
) -> ExternResult<ActionHash> {
    create_link(
        acl_path_hash(encrypted_content, entity_id)?,
        target.clone(),
        link_type,
        LinkTag::from(entity_id.to_string()),
    )
}

/// Create the full ACL link fan-out for an `EncryptedContent` entry:
/// one Owner link + one Admin link per admin + one Writer link per
/// member of (admin ∪ writer) + one Reader link per member of
/// (admin ∪ writer ∪ reader).
pub fn create_acl_links(
    encrypted_content: EncryptedContent,
    action_hash: ActionHash,
) -> ExternResult<Vec<ActionHash>> {
    let mut acl_link_action_hashes: Vec<ActionHash> = Vec::new();

    let owner = encrypted_content.header.acl.owner.clone();
    let admins: Vec<String> = encrypted_content.header.acl.admin.clone();
    // Writer set = admin ∪ writer (admins inherit writer rights).
    let writers: Vec<String> = encrypted_content
        .header
        .acl
        .admin
        .iter()
        .chain(encrypted_content.header.acl.writer.iter())
        .cloned()
        .collect();
    // Reader set = (admin ∪ writer) ∪ reader.
    let readers: Vec<String> = writers
        .iter()
        .chain(encrypted_content.header.acl.reader.iter())
        .cloned()
        .collect();

    acl_link_action_hashes.push(create_acl_link(
        &encrypted_content,
        &action_hash,
        &owner,
        LinkTypes::HummContentOwner,
    )?);

    for id in &admins {
        acl_link_action_hashes.push(create_acl_link(
            &encrypted_content,
            &action_hash,
            id,
            LinkTypes::HummContentAdmin,
        )?);
    }

    for id in &writers {
        acl_link_action_hashes.push(create_acl_link(
            &encrypted_content,
            &action_hash,
            id,
            LinkTypes::HummContentWriter,
        )?);
    }

    for id in &readers {
        acl_link_action_hashes.push(create_acl_link(
            &encrypted_content,
            &action_hash,
            id,
            LinkTypes::HummContentReader,
        )?);
    }

    Ok(acl_link_action_hashes)
}
