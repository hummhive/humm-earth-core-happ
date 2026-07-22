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

use std::collections::{HashMap, HashSet};

use crate::linking::acl_links::{acl_fanout, create_acl_link, discovery_path_hash};
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
        tombstoned: None,
    };

    emit_create_signals(&response, &encrypted_content.header.public_key_acl)?;
    publish_create_links(&encrypted_content, &action_hash, input.dynamic_links)?;

    Ok(response)
}

/// Local self-emit + best-effort cross-host fan-out to public_key_acl.reader.
/// The reader bucket is the validated recipient list for DirectMessage, a
/// routing hint for HiveGroup/Public, and usually empty for OpenWrite.
/// `from_agent` stays None on both; the receiver stamps conductor provenance.
fn emit_create_signals(
    response: &EncryptedContentResponse,
    public_key_acl: &Acl,
) -> ExternResult<()> {
    emit_signal(EncryptedContentSignal {
        action_type: EncryptedContentSignalType::Create,
        data: response.clone(),
        from_agent: None,
    })?;
    remote_signal_acl_readers(
        public_key_acl,
        EncryptedContentSignal {
            action_type: EncryptedContentSignalType::Create,
            data: response.clone(),
            from_agent: None,
        },
    );
    Ok(())
}

/// Publish every discovery/index link a create earns: the self-pointer +
/// author-shape Hive link (all variants), plus the hive-scoped bundle,
/// group ACL links, and lineage link when the header carries that context.
fn publish_create_links(
    encrypted_content: &EncryptedContent,
    action_hash: &ActionHash,
    dynamic_links: Option<Vec<String>>,
) -> ExternResult<()> {
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
        Component::from(encrypted_content.header.content_type.clone()),
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
        if let Some(dynamic_links) = dynamic_links {
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

    if let Some(lineage) = &encrypted_content.header.lineage {
        create_link(
            Path::from(vec![
                Component::from(lineage.prior_dna_hash_b64.clone()),
                Component::from(lineage.prior_action_hash_b64.clone()),
            ])
            .path_entry_hash()?,
            action_hash.clone(),
            LinkTypes::Lineage,
            LinkTag::from(lineage.prior_action_hash_b64.clone()),
        )?;
    }

    Ok(())
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
        lineage: input.lineage.clone(),
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
        tombstoned: None,
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
    let mut resolved: HashMap<ActionHash, Option<EncryptedContentResponse>> = HashMap::new();
    let mut out = Vec::with_capacity(ahs.len());
    for ah in ahs {
        let response = match resolved.get(&ah) {
            Some(cached) => cached.clone(),
            None => {
                let response = get_encrypted_content(ah.clone()).ok();
                resolved.insert(ah, response.clone());
                response
            }
        };
        if let Some(response) = response {
            out.push(response);
        }
    }
    Ok(out)
}

/// Fetch an update-chain action's record and its decoded `EncryptedContent`,
/// erroring if the action is unresolvable or references a different entry type.
fn get_encrypted_content_chain_action(
    action_hash: &ActionHash,
) -> ExternResult<(Record, EncryptedContent)> {
    let record = get(action_hash.clone(), GetOptions::network())?.ok_or_else(|| {
        wasm_error!(WasmErrorInner::Guest(
            "Could not resolve EncryptedContent update-chain action".into(),
        ))
    })?;
    let content: EncryptedContent = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(
                "Update-chain action does not reference an EncryptedContent".into(),
            ))
        })?;
    Ok((record, content))
}

/// Resolve the root `EncryptedContent` action by walking native update metadata.
///
/// `OriginalHashPointer` links are still written as an index/compatibility aid,
/// but update authority must not depend on network-fetched pointer links: a
/// hostile or stale pointer can race ahead of validation and poison `[0]`
/// selection. The action chain is the authoritative root.
fn encrypted_content_root_hash(mut action_hash: ActionHash) -> ExternResult<ActionHash> {
    loop {
        let (record, _) = get_encrypted_content_chain_action(&action_hash)?;
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

/// Fetch `previous` once, returning the update-chain ROOT action hash and
/// `previous`'s decoded header. One fetch replaces the prior walk-then-refetch
/// over the same predecessor.
fn resolve_update_base(previous: ActionHash) -> ExternResult<(ActionHash, EncryptedContentHeader)> {
    let (record, content) = get_encrypted_content_chain_action(&previous)?;
    let root = match record.action() {
        Action::Create(_) => previous,
        Action::Update(update) => {
            encrypted_content_root_hash(update.original_action_address.clone())?
        }
        _ => {
            return Err(wasm_error!(WasmErrorInner::Guest(
                "EncryptedContent update-chain action must be a Create or Update".into(),
            )));
        }
    };
    Ok((root, content.header))
}

#[hdk_extern]
pub fn update_encrypted_content(
    input: UpdateEncryptedContentInput,
) -> ExternResult<EncryptedContentResponse> {
    let (original_content_hash, prior_header) =
        resolve_update_base(input.previous_encrypted_content_hash.clone())?;
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

    reindex_dynamic_links(
        &input.updated_encrypted_content,
        &updated_encrypted_content_hash,
        input.dynamic_links,
        input.remove_dynamic_links,
    )?;
    reindex_acl_links(
        &prior_header,
        &input.updated_encrypted_content,
        &updated_encrypted_content_hash,
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

/// Relink Dynamic discovery to the update action per the caller's label
/// contract: `dynamic_links` retargets each named label to `updated_hash`
/// (deleting the caller's own older links on that path); then
/// `remove_dynamic_links` deletes the caller's own links on those paths
/// outright. No-op for headers without a hive context.
fn reindex_dynamic_links(
    updated_content: &EncryptedContent,
    updated_hash: &ActionHash,
    dynamic_links: Option<Vec<String>>,
    remove_dynamic_links: Option<Vec<String>>,
) -> ExternResult<()> {
    let Some(hive_hash) = updated_content.header.hive_context() else {
        return Ok(());
    };
    let hive_b64 = hive_hash.to_string();
    let content_type = &updated_content.header.content_type;
    let me = agent_info()?.agent_initial_pubkey;
    if let Some(labels) = dynamic_links {
        create_dynamic_links(
            updated_content.clone(),
            updated_hash.clone(),
            labels.clone(),
        )?;
        for label in &labels {
            delete_own_links_on_path(
                discovery_path_hash(&hive_b64, content_type, label)?,
                LinkTypes::Dynamic,
                &me,
                Some(updated_hash),
            )?;
        }
    }
    if let Some(labels) = remove_dynamic_links {
        for label in &labels {
            delete_own_links_on_path(
                discovery_path_hash(&hive_b64, content_type, label)?,
                LinkTypes::Dynamic,
                &me,
                None,
            )?;
        }
    }
    Ok(())
}

fn delete_own_links_on_path(
    base: EntryHash,
    link_type: LinkTypes,
    me: &AgentPubKey,
    keep_target: Option<&ActionHash>,
) -> ExternResult<()> {
    for link in get_links(LinkQuery::try_new(base, link_type)?, GetStrategy::Network)? {
        if link.author != *me {
            continue;
        }
        if keep_target
            .is_some_and(|keep| link.target.clone().into_action_hash().as_ref() == Some(keep))
        {
            continue;
        }
        delete_link(link.create_link_hash, GetOptions::network())?;
    }
    Ok(())
}

/// Converge HummContent* ACL discovery links to the update action when the
/// group_acl changed: entities newly present in a bucket's fan-out get a
/// link to `updated_hash`; entities that dropped out have the caller's own
/// bucket links deleted. No-op unless both headers are HiveGroup with a
/// changed group_acl.
fn reindex_acl_links(
    prior_header: &EncryptedContentHeader,
    updated_content: &EncryptedContent,
    updated_hash: &ActionHash,
) -> ExternResult<()> {
    let (Some(old_acl), Some(new_acl)) =
        (prior_header.group_acl(), updated_content.header.group_acl())
    else {
        return Ok(());
    };
    if old_acl == new_acl {
        return Ok(());
    }
    let Some(hive_hash) = updated_content.header.hive_context() else {
        return Ok(());
    };
    let hive_b64 = hive_hash.to_string();
    let content_type = &updated_content.header.content_type;
    let me = agent_info()?.agent_initial_pubkey;
    for ((link_type, old_ids), (_, new_ids)) in
        acl_fanout(old_acl).into_iter().zip(acl_fanout(new_acl))
    {
        let old_set: HashSet<&String> = old_ids.iter().collect();
        let new_set: HashSet<&String> = new_ids.iter().collect();
        for id in new_ids.iter().filter(|id| !old_set.contains(*id)) {
            create_acl_link(&hive_b64, content_type, updated_hash, id, link_type)?;
        }
        for id in old_ids.iter().filter(|id| !new_set.contains(*id)) {
            delete_own_links_on_path(
                discovery_path_hash(&hive_b64, content_type, id)?,
                link_type,
                &me,
                None,
            )?;
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DeleteContentResponse {
    pub was_deleted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete_action_hash: Option<ActionHash>,
}

/// Idempotent tombstone: deleting an already-deleted or absent target is
/// a no-op success (`was_deleted: false`), so client retry loops and
/// remediation re-runs never error on an already-met goal. Any error
/// other than the two wire-stable absent literals still propagates.
///
/// Contract: a network `get` cannot distinguish tombstoned from
/// not-yet-propagated, so `was_deleted: false` means "goal met OR target
/// currently unresolvable from this node" — callers deleting content they
/// did not author SHOULD re-probe later rather than treat it as terminal.
#[hdk_extern]
pub fn delete_encrypted_content(
    original_encrypted_content_hash: ActionHash,
) -> ExternResult<DeleteContentResponse> {
    let record = match get_encrypted_content(original_encrypted_content_hash.clone()) {
        Ok(record) => record,
        Err(e) if is_absent_content_error(&e) => {
            return Ok(DeleteContentResponse {
                was_deleted: false,
                delete_action_hash: None,
            });
        }
        Err(e) => return Err(e),
    };
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

    Ok(DeleteContentResponse {
        was_deleted: true,
        delete_action_hash: Some(ah),
    })
}

/// The two wire-stable "target already absent" reject literals
/// (`get_eh` / `get_encrypted_content`); anything else is a real failure.
fn is_absent_content_error(e: &WasmError) -> bool {
    let msg = format!("{e:?}");
    msg.contains("no Record found at given hash")
        || msg.contains("Could not find the EncryptedContent")
}
