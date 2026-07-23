//! ACL discovery links for [`EncryptedContentHeader`].
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

use content_integrity::{AclByGroupGenesis, EncryptedContentHeader, LinkTypes};
use hdi::hash_path::path::Component;
use hdk::prelude::*;

/// Build a discovery link's base path hash `[hive_genesis_hash_b64,
/// content_type, key]` — shared by the ACL fan-out (key = group-genesis
/// entity id) and the update-time Dynamic reindex (key = dynamic label),
/// which are structurally identical three-component paths.
pub(crate) fn discovery_path_hash(
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

/// Create the validator-pinned ACL link shape at a precomputed discovery base.
pub(crate) fn create_acl_link_at(
    path_hash: EntryHash,
    target: &ActionHash,
    entity_id: &str,
    link_type: LinkTypes,
) -> ExternResult<()> {
    create_link(
        path_hash,
        target.clone(),
        link_type,
        LinkTag::from(entity_id.to_string()),
    )?;
    Ok(())
}

/// Create a single ACL link of the given variant. The `entity_id` is
/// stamped into the `LinkTag` as UTF-8 bytes so the integrity validator
/// can recompute the third path component (see this module's header).
pub(crate) fn create_acl_link(
    hive_b64: &str,
    content_type: &str,
    target: &ActionHash,
    entity_id: &str,
    link_type: LinkTypes,
) -> ExternResult<()> {
    create_acl_link_at(
        discovery_path_hash(hive_b64, content_type, entity_id)?,
        target,
        entity_id,
        link_type,
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
    header: &EncryptedContentHeader,
    action_hash: &ActionHash,
) -> ExternResult<()> {
    let hive_hash = header.hive_context().ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(
            "create_acl_links called on a header with no hive_context".into(),
        ))
    })?;
    let group_acl = header.group_acl().ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(
            "create_acl_links called on a header whose acl_spec is not \
                 HiveGroup; HummContent* links anchor only to HiveGroup \
                 content"
                .into(),
        ))
    })?;
    let hive_b64 = hive_hash.to_string();

    for (link_type, entity_hashes) in acl_fanout(group_acl) {
        for entity_hash in entity_hashes {
            let entity_id = entity_hash.to_string();
            create_acl_link(
                &hive_b64,
                &header.content_type,
                action_hash,
                &entity_id,
                link_type,
            )?;
        }
    }
    Ok(())
}

/// A zero-allocation iterator over the group hashes in one ACL link class.
pub(crate) type AclGroupHashIter<'a> = std::iter::Chain<
    std::iter::Chain<std::slice::Iter<'a, ActionHash>, std::slice::Iter<'a, ActionHash>>,
    std::slice::Iter<'a, ActionHash>,
>;

/// The full per-link-type entity fan-out for a `group_acl`, encoding the
/// dominance rule once: Owner→[owner]; Admin→admin; Writer→admin∪writer;
/// Reader→admin∪writer∪reader. Shared by [`create_acl_links`] and the
/// update-time ACL reindex so both compute the identical link set.
pub(crate) fn acl_fanout<'a>(
    group_acl: &'a AclByGroupGenesis,
) -> [(LinkTypes, AclGroupHashIter<'a>); 4] {
    let empty: &'a [ActionHash] = &[];
    let owner = std::slice::from_ref(&group_acl.owner);
    [
        (
            LinkTypes::HummContentOwner,
            owner.iter().chain(empty.iter()).chain(empty.iter()),
        ),
        (
            LinkTypes::HummContentAdmin,
            group_acl
                .admin
                .iter()
                .chain(empty.iter())
                .chain(empty.iter()),
        ),
        (
            LinkTypes::HummContentWriter,
            group_acl
                .admin
                .iter()
                .chain(group_acl.writer.iter())
                .chain(empty.iter()),
        ),
        (
            LinkTypes::HummContentReader,
            group_acl
                .admin
                .iter()
                .chain(group_acl.writer.iter())
                .chain(group_acl.reader.iter()),
        ),
    ]
}
