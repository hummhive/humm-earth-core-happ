//! Batch remediation for legacy hive-context-less entries (pass-6
//! idempotent-writes generation).
//!
//! Legacy humm-tauri SharedSecret writes with an empty TS `hive_id`
//! produced `OpenWrite { target: None }` headers, which earn NO hive /
//! content-id / dynamic discovery links — invisible to the C4
//! `fetch_pair_ss_with_hive_check` intersection forever. An update can
//! never fix this: `update_encrypted_content` writes only
//! `EncryptedContentUpdates` + `OriginalHashPointer` links, and a
//! retroactive Dynamic link on the original create fails the frozen
//! integrity link validator (the old header lacks hive context). The
//! only path is recreate-with-corrected-header + tombstone-original,
//! which this module batches server-side in one zome call.

use content_integrity::*;
use hdk::prelude::*;

use super::crud::{create_encrypted_content, delete_encrypted_content, header_from_input};
use super::paging::{canonical_lowest_hash, content_id_records_by_author};
use super::queries::{list_by_author, ListByAuthorInput};
use super::{CreateEncryptedContentInput, EncryptedContentResponse};

const REMEDIATE_MAX_ITEMS: usize = 64;

/// List the caller's own entries of `content_type` whose header lacks
/// hive context (legacy empty-hive-id writes) — the remediation
/// candidates for [`remediate_hiveless_content`]. Delegates to
/// [`list_by_author`], inheriting its tolerant per-target skip
/// semantics. NOT cap-granted: own-content enumeration.
#[hdk_extern]
pub fn list_my_hiveless_content(
    content_type: String,
) -> ExternResult<Vec<EncryptedContentResponse>> {
    let me = agent_info()?.agent_initial_pubkey;
    let mut records = list_by_author(ListByAuthorInput {
        author: me.to_string(),
        content_type,
        since_ts: None,
        limit: None,
    })?;
    records.retain(|record| record.encrypted_content.header.hive_context().is_none());
    Ok(records)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RemediateHivelessItem {
    pub original_action_hash: ActionHash,
    /// Client-supplied corrected create input: the zome cannot decrypt
    /// payloads to recover the group id, so the caller rebuilds the
    /// header (hive-scoped acl_spec) and `dynamic_links` per item.
    pub corrected: CreateEncryptedContentInput,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RemediateHivelessInput {
    pub items: Vec<RemediateHivelessItem>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RemediationStatus {
    Recreated,
    SkippedAlreadyCorrect,
    SkippedAlreadyRemediated,
    Failed,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RemediationOutcome {
    /// b64 echo of the input's `original_action_hash`.
    pub original_hash: String,
    pub status: RemediationStatus,
    /// b64 create-action hash of the corrected entry, when known.
    pub new_hash: Option<String>,
    /// Failure reason / delete-retry info for humans and logs.
    pub detail: Option<String>,
}

fn check_items_bound(len: usize) -> ExternResult<()> {
    if len > REMEDIATE_MAX_ITEMS {
        return Err(wasm_error!(WasmErrorInner::Guest(String::from(
            "remediate_hiveless_content: at most 64 items per call"
        ))));
    }
    Ok(())
}

/// Batch recreate+tombstone of hiveless originals, max 64 items. One
/// outcome per input item in input order; business conditions
/// (unresolvable / foreign / already-correct / already-remediated /
/// hiveless-corrected) NEVER abort the batch. A create failure DOES
/// abort the whole call: catching it would commit a partial scratch
/// (entry on the content-id path without its Dynamic links) that the
/// re-run probe would mistake for a completed remediation — whole-call
/// Err rolls every write back atomically. Idempotent: a re-run finds
/// the corrected entry on its content-id path and reports
/// `skipped_already_remediated`, retrying the original's tombstone if
/// the first run's delete failed. Create/delete emit their normal
/// signals. NOT cap-granted (mutator).
#[hdk_extern]
pub fn remediate_hiveless_content(
    input: RemediateHivelessInput,
) -> ExternResult<Vec<RemediationOutcome>> {
    check_items_bound(input.items.len())?;
    let me = agent_info()?.agent_initial_pubkey;
    input
        .items
        .into_iter()
        .map(|item| remediate_item(item, &me))
        .collect()
}

/// The already-remediated probe runs BEFORE resolving the original:
/// a fully-successful prior run tombstones the original, so requiring
/// it to resolve first would misreport idempotent re-runs as failures.
fn remediate_item(
    item: RemediateHivelessItem,
    me: &AgentPubKey,
) -> ExternResult<RemediationOutcome> {
    let original_hash = item.original_action_hash.to_string();

    let corrected_header = header_from_input(&item.corrected);
    let Some(corrected_hive) = corrected_header.hive_context() else {
        return Ok(failed(original_hash, "corrected input lacks hive context"));
    };

    let (existing, _truncated) = content_id_records_by_author(corrected_hive, &item.corrected.id, me)?;
    if let Some(existing) = canonical_lowest_hash(existing) {
        let detail = retry_original_tombstone(&item.original_action_hash, me)?;
        return Ok(RemediationOutcome {
            original_hash,
            status: RemediationStatus::SkippedAlreadyRemediated,
            new_hash: Some(existing.hash),
            detail,
        });
    }

    let Some(record) = get(item.original_action_hash.clone(), GetOptions::network())? else {
        return Ok(failed(original_hash, "original not resolvable"));
    };
    let Some(original) = decode_encrypted_content(&record) else {
        return Ok(failed(
            original_hash,
            "original is not an EncryptedContent entry",
        ));
    };
    if record.action().author() != me {
        return Ok(failed(original_hash, "caller is not the original author"));
    }
    if original.header.hive_context().is_some() {
        return Ok(RemediationOutcome {
            original_hash,
            status: RemediationStatus::SkippedAlreadyCorrect,
            new_hash: None,
            detail: None,
        });
    }

    let created = create_encrypted_content(item.corrected)?;
    let detail = match delete_encrypted_content(item.original_action_hash) {
        Ok(_) => None,
        Err(err) => Some(format!("original delete failed (re-run remediates): {err:?}")),
    };
    Ok(RemediationOutcome {
        original_hash,
        status: RemediationStatus::Recreated,
        new_hash: Some(created.hash),
        detail,
    })
}

/// Self-heal for a prior run whose create landed but whose delete
/// failed: tombstone the leftover original if it still resolves, is
/// caller-authored, and is still hiveless. Returns the detail string to
/// surface, `None` when nothing needed doing.
fn retry_original_tombstone(
    original: &ActionHash,
    me: &AgentPubKey,
) -> ExternResult<Option<String>> {
    let Some(record) = get(original.clone(), GetOptions::network())? else {
        return Ok(None);
    };
    if record.action().author() != me {
        debug!("remediate self-heal: original {original} not caller-authored; leaving it");
        return Ok(None);
    }
    let Some(entry) = decode_encrypted_content(&record) else {
        debug!("remediate self-heal: original {original} is not an EncryptedContent; leaving it");
        return Ok(None);
    };
    if entry.header.hive_context().is_some() {
        return Ok(None);
    }
    Ok(Some(match delete_encrypted_content(original.clone()) {
        Ok(_) => String::from("original tombstoned on retry"),
        Err(err) => format!("original delete retry failed: {err:?}"),
    }))
}

fn decode_encrypted_content(record: &Record) -> Option<EncryptedContent> {
    record.entry().to_app_option::<EncryptedContent>().ok().flatten()
}

fn failed(original_hash: String, detail: impl Into<String>) -> RemediationOutcome {
    RemediationOutcome {
        original_hash,
        status: RemediationStatus::Failed,
        new_hash: None,
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// humm-tauri decodes these exact strings; a variant rename is a
    /// wire break.
    #[test]
    fn remediation_status_serde_snake_case() {
        for (status, expected) in [
            (RemediationStatus::Recreated, "recreated"),
            (
                RemediationStatus::SkippedAlreadyCorrect,
                "skipped_already_correct",
            ),
            (
                RemediationStatus::SkippedAlreadyRemediated,
                "skipped_already_remediated",
            ),
            (RemediationStatus::Failed, "failed"),
        ] {
            let io = ExternIO::encode(&status).expect("encode status");
            let decoded: String = io.decode().expect("decode as plain string");
            assert_eq!(decoded, expected);
        }
    }

    #[test]
    fn remediate_bounds_error_literal() {
        assert!(check_items_bound(REMEDIATE_MAX_ITEMS).is_ok());
        let err = check_items_bound(REMEDIATE_MAX_ITEMS + 1).expect_err("over-cap must reject");
        assert!(format!("{err:?}").contains("at most 64 items per call"));
    }
}
