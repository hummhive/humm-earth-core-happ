use content_integrity::{Acl, AclSpec, EncryptedContent, HiveGenesis};
use hdk::prelude::*;

use super::markers::{
    MigrationMarkerV1, MigrationMarkerV2, HIVE_GENESIS_MARKER_ORIGINAL_TYPE,
    HIVE_MIGRATION_MARKER_CONTENT_ID,
};
use super::payload::{build_marker_payload, build_marker_v2_payload, marker_content_type};
use crate::encrypted_content::crud::{
    create_encrypted_content, get_encrypted_content, header_from_input, update_encrypted_content,
};
use crate::encrypted_content::paging::{canonical_lowest_hash, content_id_records_by_author};
use crate::encrypted_content::{
    CreateEncryptedContentInput, EncryptedContentResponse, UpdateEncryptedContentInput,
};

/// Input for [`mark_migrated`].
#[derive(Serialize, Deserialize, Debug)]
pub struct MarkMigratedInput {
    /// Action hash of the original entry. Typically the original
    /// `Create` action hash the host stored at first ingest.
    pub original_action_hash: ActionHash,
    /// The forward-pointer payload to write onto the original entry's
    /// chain.
    pub marker: MigrationMarkerV1,
}

/// Write a forward-pointer marker onto an original entry by delegating
/// to `update_encrypted_content`. Returns the full
/// [`EncryptedContentResponse`] for the marker update so callers can
/// inspect the new action hash and signal-emitted state.
///
/// Side-effects (inherited from `update_encrypted_content`):
/// local `emit_signal` carrying the marker payload to the author's own
/// UI; cross-host `send_remote_signal` fan-out via
/// `remote_signal_acl_readers` to every agent in the entry's
/// `public_key_acl.reader` (minus self), with `from_agent` stamped on
/// the receiver by `recv_remote_signal`; standard `EncryptedContentUpdates`
/// + `OriginalHashPointer` link plumbing.
///
/// **Not in `set_cap_tokens`** — local-only by design. A remote-callable
/// `mark_migrated` would let attackers pollute another agent's chain
/// AND fan out a spurious cross-host migration signal in that agent's
/// name. Local-only matches the precedent of `update_encrypted_content`
/// itself.
#[hdk_extern]
pub fn mark_migrated(input: MarkMigratedInput) -> ExternResult<EncryptedContentResponse> {
    let original = get_encrypted_content(input.original_action_hash.clone())?;
    let marker_payload = build_marker_payload(&original.encrypted_content, &input.marker)?;
    update_marker_entry(input.original_action_hash, marker_payload)
}

fn update_marker_entry(
    original_action_hash: ActionHash,
    marker_payload: EncryptedContent,
) -> ExternResult<EncryptedContentResponse> {
    update_encrypted_content(UpdateEncryptedContentInput {
        previous_encrypted_content_hash: original_action_hash,
        updated_encrypted_content: marker_payload,
    })
}

/// Input for [`mark_migrated_v2`].
#[derive(Debug, Serialize, Deserialize)]
pub struct MarkMigratedV2Input {
    /// Action hash of the original entry. Typically the original
    /// `Create` action hash the host stored at first ingest.
    pub original_action_hash: ActionHash,
    /// The forward-pointer payload to write onto the original entry's
    /// chain.
    pub marker: MigrationMarkerV2,
}

/// V2 twin of [`mark_migrated`]. Same security envelope: local-only by
/// design (NOT in `set_cap_tokens`); see the V1 doc-comment for the
/// rationale.
///
/// Use this for the **hive-identity** migration markers that carry
/// `new_hive_genesis_hash_base64`. For per-entry content markers, V1
/// is sufficient when V1-only readers still exist in the wild; pass-2.5
/// hosts can write V2 here too without breaking V2-aware readers.
///
/// ## Original entry types (pass-6-idempotent-writes)
///
/// - `EncryptedContent` original → marker rides the update chain
///   (unchanged V1 mechanism).
/// - `HiveGenesis` original → marker is a fresh `EncryptedContent`
///   CREATE on the content-id path `[genesis_b64,
///   "hive-migration-marker-v2"]` with `OpenWrite{target = genesis}`:
///   the frozen integrity update gate rejects cross-entry-type
///   updates, so the update mechanism can never serve hives. Founder
///   only; re-marking updates the one marker entry (no duplicates).
///   V1 readers structurally never see hive markers.
/// - Anything else → explicit `Err`, never the silent dormant path.
///
/// ## Dormant / unresolvable original entry (pass-4 migration rescue)
///
/// Returns `Ok(None)` if the original entry is not readable from this
/// cell (e.g. the post-cutover pass-4 `@4` cell is peerless and the
/// network `get` cannot resolve the action hash). The marker is
/// COURTESY metadata for other pass-4 readers — irrelevant on a
/// dormant cell — so skipping it is correct, not a failure. The miss
/// is logged via `warn!` (never silently swallowed). On success
/// returns `Ok(Some(response))` for the marker update so callers can
/// inspect the new action hash and signal state.
#[hdk_extern]
pub fn mark_migrated_v2(
    input: MarkMigratedV2Input,
) -> ExternResult<Option<EncryptedContentResponse>> {
    let Some(record) = get(input.original_action_hash.clone(), GetOptions::network())? else {
        warn!(
            "mark_migrated_v2: original entry {} not readable; skipping forward-pointer marker (dormant/absent cell)",
            input.original_action_hash
        );
        return Ok(None);
    };
    // Entry-def-index discrimination, NOT msgpack shape: GroupGenesis is
    // a field-superset of HiveGenesis and would false-positive here.
    if let Some(genesis) = crate::hive::queries::try_decode_hive_genesis(&record) {
        return mark_hive_genesis_migrated(input, &genesis, record.action().author());
    }
    // No superset hazard for this shape probe: no other entry type
    // carries EncryptedContent's nested `header` + `bytes` fields.
    if record
        .entry()
        .to_app_option::<EncryptedContent>()
        .ok()
        .flatten()
        .is_none()
    {
        return Err(wasm_error!(WasmErrorInner::Guest(String::from(
            "mark_migrated_v2: original must be an EncryptedContent or HiveGenesis entry"
        ))));
    }
    let original = match get_encrypted_content(input.original_action_hash.clone()) {
        Ok(o) => o,
        Err(e) => {
            warn!(
                "mark_migrated_v2: original entry {} not readable ({e:?}); skipping forward-pointer marker (dormant/absent cell)",
                input.original_action_hash
            );
            return Ok(None);
        }
    };
    let marker_payload = build_marker_v2_payload(&original.encrypted_content, &input.marker)?;
    let response = update_marker_entry(input.original_action_hash, marker_payload)?;
    Ok(Some(response))
}

/// Founder-only create-based hive marker: first mark CREATEs the one
/// marker entry; a re-mark UPDATEs it (same entry type, so the frozen
/// update gate allows it).
fn mark_hive_genesis_migrated(
    input: MarkMigratedV2Input,
    genesis: &HiveGenesis,
    genesis_author: &AgentPubKey,
) -> ExternResult<Option<EncryptedContentResponse>> {
    let me = agent_info()?.agent_initial_pubkey;
    if genesis_author != &me {
        return Err(wasm_error!(WasmErrorInner::Guest(String::from(
            "mark_migrated_v2: only the hive founder can mark a HiveGenesis migrated"
        ))));
    }
    let marker_bytes = SerializedBytes::try_from(input.marker)
        .map_err(|err| wasm_error!(WasmErrorInner::Serialize(err)))?;
    let marker_input = CreateEncryptedContentInput {
        id: HIVE_MIGRATION_MARKER_CONTENT_ID.into(),
        display_hive_id: genesis.display_id.clone(),
        content_type: marker_content_type(HIVE_GENESIS_MARKER_ORIGINAL_TYPE),
        revision_author_signing_public_key: me.to_string(),
        bytes: marker_bytes,
        acl_spec: AclSpec::OpenWrite {
            target_hive_genesis_hash: Some(input.original_action_hash.clone()),
        },
        public_key_acl: Acl {
            owner: me.to_string(),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        },
        dynamic_links: None,
    };

    let (existing, _truncated) = content_id_records_by_author(
        &input.original_action_hash,
        HIVE_MIGRATION_MARKER_CONTENT_ID,
        &me,
    )?;
    if let Some(found) = canonical_lowest_hash(existing) {
        let previous = ActionHash::try_from(found.hash.as_str()).map_err(|err| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "mark_migrated_v2: malformed marker action hash {}: {err:?}",
                found.hash
            )))
        })?;
        let response = update_encrypted_content(UpdateEncryptedContentInput {
            previous_encrypted_content_hash: previous,
            updated_encrypted_content: EncryptedContent {
                header: header_from_input(&marker_input),
                bytes: marker_input.bytes,
            },
        })?;
        return Ok(Some(response));
    }
    let response = create_encrypted_content(marker_input)?;
    Ok(Some(response))
}
