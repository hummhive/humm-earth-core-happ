//! Create / read / update / delete externs for `EncryptedContent`.
//!
//! Each mutating extern follows the same pattern:
//!   1. Mutate DHT state (create_entry / update_entry / delete_entry).
//!   2. Emit local signal via `emit_signal` (always — author's own UI).
//!   3. Fan out cross-host via `remote_signal_acl_readers` (best-effort to
//!      every agent in `public_key_acl.reader` minus self).
//!
//! `from_agent` is always set to `None` on outbound emissions; the
//! conductor-attested provenance is stamped on the RECEIVER side by
//! the `recv_remote_signal` dispatcher in `lib.rs`.

use content_integrity::*;
use hdi::hash_path::path::Component;
use hdk::prelude::*;

use crate::{
    dynamic_links::create_dynamic_links, hive_link::create_hive_link,
    humm_content_id_link::create_humm_content_id_link, linking::acl_links::create_acl_links,
};

use super::get_helpers::{get_eh, get_latest_typed_from_eh};
use super::signals::{
    remote_signal_acl_readers, EncryptedContentSignal, EncryptedContentSignalType,
};
use super::{CreateEncryptedContentInput, EncryptedContentResponse, UpdateEncryptedContentInput};

#[hdk_extern]
pub fn create_encrypted_content(
    input: CreateEncryptedContentInput,
) -> ExternResult<EncryptedContentResponse> {
    let encrypted_content = EncryptedContent {
        header: EncryptedContentHeader {
            id: input.id,
            display_hive_id: input.display_hive_id,
            content_type: input.content_type.clone(),
            revision_author_signing_public_key: input.revision_author_signing_public_key,
            acl_spec: input.acl_spec,
            public_key_acl: input.public_key_acl,
        },
        bytes: input.bytes,
    };
    let action_hash = create_entry(&EntryTypes::EncryptedContent(encrypted_content.clone()))?;
    let response = EncryptedContentResponse {
        encrypted_content: encrypted_content.clone(),
        hash: action_hash.clone().to_string(),
        original_hash: action_hash.to_string(),
    };

    // Local emit (every variant) + best-effort cross-host fan-out to
    // every agent in public_key_acl.reader. For DirectMessage the
    // reader bucket IS the validated recipient list; for HiveGroup +
    // Public it is the routing hint; for OpenWrite it is typically
    // empty (the entry is its own announcement).
    emit_signal(EncryptedContentSignal {
        action_type: EncryptedContentSignalType::Create,
        data: response.clone(),
        from_agent: None,
    })?;
    remote_signal_acl_readers(
        &encrypted_content.header.public_key_acl,
        EncryptedContentSignal {
            action_type: EncryptedContentSignalType::Create,
            data: response.clone(),
            from_agent: None,
        },
    );

    // OriginalHashPointer (self-link) — every entry.
    create_link(
        action_hash.clone(),
        action_hash.clone(),
        LinkTypes::OriginalHashPointer,
        (),
    )?;

    // Author-shape Hive link [author_pubkey, content_type] — every
    // entry. The integrity validator accepts this path for ALL variants.
    let my_agent_pub_key = agent_info()?.agent_initial_pubkey;
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

    // The hive-scoped link bundle (hive-shape Hive link, HummContentId,
    // Dynamic, HummContent* ACL links) is only meaningful when the
    // entry binds a hive context. DirectMessage and OpenWrite-without-
    // target intentionally skip these — the integrity validator
    // rejects them for those variants. We mirror the contract here.
    if encrypted_content.header.hive_context().is_some() {
        create_hive_link(encrypted_content.clone(), action_hash.clone())?;
        create_humm_content_id_link(encrypted_content.clone(), action_hash.clone())?;
        if let Some(dynamic_links) = input.dynamic_links {
            create_dynamic_links(
                encrypted_content.clone(),
                action_hash.clone(),
                dynamic_links,
            )?;
        }
    }

    // HummContent{Owner,Admin,Writer,Reader} links require a
    // group_acl, which only AclSpec::HiveGroup carries. Skip for the
    // other three variants.
    if encrypted_content.header.group_acl().is_some() {
        create_acl_links(encrypted_content.clone(), action_hash.clone())?;
    }

    Ok(response)
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

/// Resolve many `EncryptedContent` action hashes at once.
///
/// LIST SEMANTICS — tolerant by design: a target whose record is not
/// resolvable (a link that gossiped ahead of its entry, or a tombstoned
/// target) is DROPPED, not fatal. This extern backs `list_by_hive_link`,
/// `list_by_dynamic_link`, `list_by_acl_link`, and `list_by_author`; an
/// all-or-nothing `collect()` here let a single dangling link poison
/// every hive-scoped list read (live-confirmed: the fresh-public-media
/// foreign-resolve hard-fail when a link gossips ahead of its record,
/// and the group-discovery tombstone throw). Callers dedupe by action
/// hash and re-sweep, so the resolvable subset is the correct answer.
#[hdk_extern]
pub fn get_many_encrypted_content(
    ahs: Vec<ActionHash>,
) -> ExternResult<Vec<EncryptedContentResponse>> {
    Ok(ahs
        .into_iter()
        .filter_map(|ah| get_encrypted_content(ah).ok())
        .collect())
}

/// Resolve an `OriginalHashPointer` link target to an `ActionHash`.
///
/// The integrity zome validates `OriginalHashPointer` link targets as
/// unconditionally `Valid` (no target-type or author check), so a non-member
/// peer can plant a link whose target is an `EntryHash` rather than an
/// `ActionHash`. Returning an `Err` keeps such a poison link from trapping the
/// author's update with a `.unwrap()` panic — a cheap griefing vector that
/// would otherwise abort the victim's update on every retry until the link is
/// gossiped away. Mirrors the `let Some(ah) = …into_action_hash() else { … }`
/// guard used in `probe_inbox`.
fn original_pointer_action_hash(target: AnyLinkableHash) -> ExternResult<ActionHash> {
    let Some(ah) = target.into_action_hash() else {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "OriginalHashPointer target is not an ActionHash".to_string(),
        )));
    };
    Ok(ah)
}

#[hdk_extern]
pub fn update_encrypted_content(
    input: UpdateEncryptedContentInput,
) -> ExternResult<EncryptedContentResponse> {
    let updated_encrypted_content_hash = update_entry(
        input.previous_encrypted_content_hash.clone(),
        &input.updated_encrypted_content,
    )?;
    let original_hash_link = get_links(
        LinkQuery::try_new(
            input.previous_encrypted_content_hash.clone(),
            LinkTypes::OriginalHashPointer,
        )?,
        GetStrategy::Network,
    )?;

    if original_hash_link.is_empty() {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "Could not find the hash of the original EncryptedContent that is trying to be updated"
                .to_string(),
        )));
    }
    let original_content_hash = original_pointer_action_hash(original_hash_link[0].target.clone())?;
    create_link(
        original_content_hash.clone(),
        updated_encrypted_content_hash.clone(),
        LinkTypes::EncryptedContentUpdates,
        (),
    )?;
    create_link(
        updated_encrypted_content_hash.clone(),
        original_content_hash,
        LinkTypes::OriginalHashPointer,
        (),
    )?;

    let record = get_encrypted_content(updated_encrypted_content_hash.clone())?;

    emit_signal(EncryptedContentSignal {
        action_type: EncryptedContentSignalType::Update,
        data: record.clone(),
        from_agent: None,
    })?;
    remote_signal_acl_readers(
        &record.encrypted_content.header.public_key_acl,
        EncryptedContentSignal {
            action_type: EncryptedContentSignalType::Update,
            data: record.clone(),
            from_agent: None,
        },
    );

    Ok(record)
}

#[hdk_extern]
pub fn delete_encrypted_content(
    original_encrypted_content_hash: ActionHash,
) -> ExternResult<ActionHash> {
    let record = get_encrypted_content(original_encrypted_content_hash.clone())?;
    let ah = delete_entry(original_encrypted_content_hash.clone())?;
    emit_signal(EncryptedContentSignal {
        action_type: EncryptedContentSignalType::Delete,
        data: record.clone(),
        from_agent: None,
    })?;
    let acl_for_remote = record.encrypted_content.header.public_key_acl.clone();
    remote_signal_acl_readers(
        &acl_for_remote,
        EncryptedContentSignal {
            action_type: EncryptedContentSignalType::Delete,
            data: record,
            from_agent: None,
        },
    );

    crate::delete_own_links_targeting(AnyLinkableHash::from(original_encrypted_content_hash))?;

    Ok(ah)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A poison pointer whose target is an `EntryHash` (not an `ActionHash`)
    /// returns a clean `Err` instead of trapping. The integrity zome validates
    /// `OriginalHashPointer` targets as unconditionally `Valid`, so a remote
    /// peer can plant such a link; this is the regression guard for the
    /// `.unwrap()` griefing vector in `update_encrypted_content`.
    #[test]
    fn original_pointer_rejects_non_action_hash_target() {
        let poison = AnyLinkableHash::from(EntryHash::from_raw_36(vec![2u8; 36]));
        let err = original_pointer_action_hash(poison).unwrap_err();
        assert!(format!("{err:?}").contains("not an ActionHash"));
    }

    /// A pointer whose target is an `ActionHash` resolves to that hash.
    #[test]
    fn original_pointer_resolves_action_hash_target() {
        let ah = ActionHash::from_raw_36(vec![7u8; 36]);
        let resolved =
            original_pointer_action_hash(AnyLinkableHash::from(ah.clone())).expect("resolves");
        assert_eq!(resolved, ah);
    }
}
