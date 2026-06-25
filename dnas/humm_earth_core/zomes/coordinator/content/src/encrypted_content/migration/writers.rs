use content_integrity::EncryptedContent;
use hdk::prelude::*;

use super::markers::{MigrationMarkerV1, MigrationMarkerV2};
use super::payload::{build_marker_payload, build_marker_v2_payload};
use crate::encrypted_content::crud::{get_encrypted_content, update_encrypted_content};
use crate::encrypted_content::{EncryptedContentResponse, UpdateEncryptedContentInput};

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
