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
use super::paging::{canonical_lowest_hash, content_id_records_by_author};
use super::signals::{
    remote_signal_acl_readers, EncryptedContentSignal, EncryptedContentSignalType,
};
use super::{CreateEncryptedContentInput, EncryptedContentResponse, UpdateEncryptedContentInput};

#[hdk_extern]
pub fn create_encrypted_content(
    input: CreateEncryptedContentInput,
) -> ExternResult<EncryptedContentResponse> {
    let encrypted_content = EncryptedContent {
        header: header_from_input(&input),
        bytes: input.bytes.clone(),
    };
    let action_hash = create_entry(&EntryTypes::EncryptedContent(encrypted_content.clone()))?;
    let response = EncryptedContentResponse {
        encrypted_content: encrypted_content.clone(),
        hash: action_hash.clone().to_string(),
        original_hash: action_hash.to_string(),
        latest_action_micros: None,
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

/// Assemble the integrity [`EncryptedContentHeader`] from a create
/// input. Single source of truth shared by [`create_encrypted_content`],
/// [`find_or_create_encrypted_content`], and the hiveless remediation
/// extern — the header decides `hive_context()` and therefore which
/// discovery links a write earns.
pub(crate) fn header_from_input(input: &CreateEncryptedContentInput) -> EncryptedContentHeader {
    EncryptedContentHeader {
        id: input.id.clone(),
        display_hive_id: input.display_hive_id.clone(),
        content_type: input.content_type.clone(),
        revision_author_signing_public_key: input.revision_author_signing_public_key.clone(),
        acl_spec: input.acl_spec.clone(),
        public_key_acl: input.public_key_acl.clone(),
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FindOrCreateContentResponse {
    pub response: EncryptedContentResponse,
    pub was_created: bool,
}

/// Idempotent create keyed on `(hive_genesis_hash, input.id)`,
/// caller-authored only. If the caller already wrote an entry on that
/// content-id path, return it (`was_created: false`) — NO write, NO
/// signal, and content differences between `input` and the found entry
/// are ignored by design (find wins: crash-resume semantics).
/// Otherwise delegate to [`create_encrypted_content`].
///
/// Canonical pick when multiple caller-authored candidates exist:
/// lowest-b64 hash, matching humm-tauri's `selectCanonicalByHash.ts`.
/// Cross-agent duplicate prevention is NOT provided here (author-scoped
/// find); that is the pass-7 A11 uniqueness-validator work.
/// NOT cap-granted: mutator — a remote grant would let peers write to
/// the callee's chain.
#[hdk_extern]
pub fn find_or_create_encrypted_content(
    input: CreateEncryptedContentInput,
) -> ExternResult<FindOrCreateContentResponse> {
    let header = header_from_input(&input);
    let Some(hive) = header.hive_context() else {
        return Err(wasm_error!(WasmErrorInner::Guest(String::from(
            "find_or_create_encrypted_content requires a hive-scoped acl_spec (HiveGroup or OpenWrite with target)"
        ))));
    };
    let me = agent_info()?.agent_initial_pubkey;
    let (records, _truncated) = content_id_records_by_author(hive, &input.id, &me)?;
    if let Some(existing) = canonical_lowest_hash(records) {
        return Ok(FindOrCreateContentResponse {
            response: existing,
            was_created: false,
        });
    }
    let response = create_encrypted_content(input)?;
    Ok(FindOrCreateContentResponse {
        response,
        was_created: true,
    })
}

#[hdk_extern]
pub fn get_encrypted_content(content_hash: ActionHash) -> ExternResult<EncryptedContentResponse> {
    let ah = get_eh(content_hash.clone())?;
    let Some((entry, hash, _, ts)) = get_latest_typed_from_eh(ah)? else {
        return Err(wasm_error!(WasmErrorInner::Guest(String::from(
            "Could not find the EncryptedContent"
        ))));
    };
    Ok(EncryptedContentResponse {
        encrypted_content: entry,
        hash: hash.to_string(),
        original_hash: content_hash.to_string(),
        latest_action_micros: Some(ts.as_micros()),
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

/// Resolve the root `EncryptedContent` action by walking native update metadata.
///
/// `OriginalHashPointer` links are still written as an index/compatibility aid,
/// but update authority must not depend on network-fetched pointer links: a
/// hostile or stale pointer can race ahead of validation and poison `[0]`
/// selection. The action chain is the authoritative root.
fn encrypted_content_root_hash(mut action_hash: ActionHash) -> ExternResult<ActionHash> {
    loop {
        let record = get(action_hash.clone(), GetOptions::network())?.ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(
                "Could not resolve EncryptedContent update-chain action".into(),
            ))
        })?;
        let _: EncryptedContent = record
            .entry()
            .to_app_option()
            .map_err(|e| wasm_error!(e))?
            .ok_or_else(|| {
                wasm_error!(WasmErrorInner::Guest(
                    "Update-chain action does not reference an EncryptedContent".into(),
                ))
            })?;
        match record.action() {
            Action::Create(_) => return Ok(action_hash),
            Action::Update(update) => action_hash = update.original_action_address.clone(),
            _ => {
                return Err(wasm_error!(WasmErrorInner::Guest(
                    "EncryptedContent update-chain action must be a Create or Update".into(),
                )));
            }
        }
    }
}

#[hdk_extern]
pub fn update_encrypted_content(
    input: UpdateEncryptedContentInput,
) -> ExternResult<EncryptedContentResponse> {
    let original_content_hash =
        encrypted_content_root_hash(input.previous_encrypted_content_hash.clone())?;
    let updated_encrypted_content_hash = update_entry(
        input.previous_encrypted_content_hash.clone(),
        &input.updated_encrypted_content,
    )?;
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
