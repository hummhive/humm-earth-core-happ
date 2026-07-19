//! Caller-authored service-meter and opt-in node-spec record mutators.

use std::{collections::BTreeMap, str::FromStr};

use base64::Engine;
use content_integrity::{Acl, AclSpec, EncryptedContent, EncryptedContentHeader};
use hdk::prelude::*;

use super::crud::{create_encrypted_content, update_encrypted_content};
use super::paging::{canonical_lowest_hash, content_id_records_by_author};
use super::{CreateEncryptedContentInput, EncryptedContentResponse, UpdateEncryptedContentInput};

pub const SERVICE_METER_CONTENT_TYPE: &str = "hummhive-core-service-meter-v1";
pub const SERVICE_METER_ID_PREFIX: &str = "service-meter-v1:";
pub const SERVICE_METER_SCHEMA_TAG: &str = "hummhive-service-meter/1";
pub const NODE_SPEC_CONTENT_TYPE: &str = "hummhive-core-node-spec-v1";
pub const NODE_SPEC_CONTENT_ID: &str = "node-spec-v1";
pub const NODE_SPEC_SCHEMA_TAG: &str = "hummhive-node-spec/1";
pub(crate) const ACCEPTED_APP_SIGNING_KEYS_B64: &[&str] = &[];

const MAX_METER_DIMENSIONS: usize = 16;
const MAX_NODE_SPEC_ENTRIES: usize = 32;
const MIN_RECORD_KEY_CHARS: usize = 1;
const MAX_RECORD_KEY_CHARS: usize = 64;
const MIN_NODE_SPEC_VALUE_CHARS: usize = 1;
const MAX_NODE_SPEC_VALUE_CHARS: usize = 256;

const INVALID_METER_PERIOD: &str = "service meter period must be YYYY-MM-DD";
const TOO_MANY_METER_DIMENSIONS: &str = "service meter accepts at most 16 counter dimensions";
const METER_UNION_EXCEEDS_DIMENSIONS: &str =
    "service meter counter union with the prior record exceeds 16 dimensions";
const INVALID_METER_KEY: &str =
    "service meter counter keys must be 1-64 printable ASCII chars without | ; =";
const INVALID_METER_COUNTER: &str = "service meter counters must be canonical u128 decimal strings";
const INVALID_METER_SNAPSHOT: &str = "service meter payload is not a ServiceMeterSnapshot";
const INVALID_NODE_SPEC_ENTRIES: &str = "node spec accepts at most 32 entries";
const INVALID_NODE_SPEC_KEY: &str =
    "node spec keys must be 1-64 printable ASCII chars without | ; =";
const INVALID_NODE_SPEC_VALUE: &str =
    "node spec values must be 1-256 chars without control characters or | ;";
const INVALID_NODE_SPEC_SNAPSHOT: &str = "node spec payload is not a NodeSpecSnapshot";
const UNRECOGNIZED_APP_SIGNING_KEY: &str = "unrecognized app signing key";
const MALFORMED_APP_ATTESTATION: &str = "app attestation signature malformed";
const INVALID_APP_ATTESTATION: &str = "app attestation signature invalid";
const INVALID_METER_ACTION_HASH: &str = "service meter prior hash is not a valid action hash";
const INVALID_NODE_SPEC_ACTION_HASH: &str = "node spec prior hash is not a valid action hash";

#[hdk_entry_helper]
#[derive(Clone, PartialEq, Eq)]
pub struct ServiceMeterSnapshot {
    pub schema: String,
    pub period: String,
    pub counters: BTreeMap<String, String>,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq, Eq)]
pub struct NodeSpecSnapshot {
    pub schema: String,
    pub spec: BTreeMap<String, String>,
    pub declared_at_micros: i64,
    pub verified_by_app_key: Option<String>,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct UpsertServiceMeterInput {
    pub hive_genesis_hash: ActionHash,
    pub period: String,
    pub counters: BTreeMap<String, String>,
    pub display_hive_id: String,
    pub revision_author_signing_public_key: String,
    pub public_key_acl: Acl,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct NodeSpecAttestation {
    pub app_signing_key_b64: String,
    pub signature_b64: String,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct PublishNodeSpecInput {
    pub hive_genesis_hash: ActionHash,
    pub spec: BTreeMap<String, String>,
    pub declared_at_micros: i64,
    pub app_attestation: Option<NodeSpecAttestation>,
    pub display_hive_id: String,
    pub revision_author_signing_public_key: String,
    pub public_key_acl: Acl,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UpsertContentResponse {
    pub response: EncryptedContentResponse,
    pub was_created: bool,
    pub was_updated: bool,
}

/// Upserts caller-authored cumulative service counters for one Hive day bucket.
/// Every call converges the stored header (display alias, revision signing key,
/// public-key ACL) to the caller's values — a header-only change is a real
/// update. NOT cap-granted: a remote caller must not write to the callee's
/// source chain.
#[hdk_extern]
pub fn upsert_service_meter(input: UpsertServiceMeterInput) -> ExternResult<UpsertContentResponse> {
    let UpsertServiceMeterInput {
        hive_genesis_hash,
        period,
        counters,
        display_hive_id,
        revision_author_signing_public_key,
        public_key_acl,
    } = input;
    validate_period(&period)?;
    let counters = canonicalize_counters(counters)?;
    let id = format!("{SERVICE_METER_ID_PREFIX}{period}");
    let me = agent_info()?.agent_initial_pubkey;
    let (records, _truncated) = content_id_records_by_author(&hive_genesis_hash, &id, &me)?;
    let header = build_record_header(
        id,
        SERVICE_METER_CONTENT_TYPE,
        hive_genesis_hash,
        display_hive_id,
        revision_author_signing_public_key,
        public_key_acl,
    );

    match canonical_lowest_hash(records) {
        None => create_service_meter(header, period, counters),
        Some(prior) => update_service_meter(prior, header, &period, counters),
    }
}

/// Publishes or replaces the caller's opt-in node specification for one Hive.
/// Every call converges the stored header to the caller's values — a
/// header-only change is a real update. NOT cap-granted: a remote caller must
/// not write to the callee's source chain.
#[hdk_extern]
pub fn publish_node_spec(input: PublishNodeSpecInput) -> ExternResult<UpsertContentResponse> {
    let PublishNodeSpecInput {
        hive_genesis_hash,
        spec,
        declared_at_micros,
        app_attestation,
        display_hive_id,
        revision_author_signing_public_key,
        public_key_acl,
    } = input;
    validate_spec_entries(&spec)?;
    let me = agent_info()?.agent_initial_pubkey;
    let verified_by_app_key =
        verify_app_attestation(app_attestation.as_ref(), &me, declared_at_micros, &spec)?;
    let snapshot = NodeSpecSnapshot {
        schema: NODE_SPEC_SCHEMA_TAG.to_string(),
        spec,
        declared_at_micros,
        verified_by_app_key,
    };
    let (records, _truncated) =
        content_id_records_by_author(&hive_genesis_hash, NODE_SPEC_CONTENT_ID, &me)?;
    let header = build_record_header(
        NODE_SPEC_CONTENT_ID.to_string(),
        NODE_SPEC_CONTENT_TYPE,
        hive_genesis_hash,
        display_hive_id,
        revision_author_signing_public_key,
        public_key_acl,
    );

    match canonical_lowest_hash(records) {
        None => create_node_spec(header, snapshot),
        Some(prior) => update_node_spec(prior, header, snapshot),
    }
}

fn create_service_meter(
    header: EncryptedContentHeader,
    period: String,
    counters: BTreeMap<String, String>,
) -> ExternResult<UpsertContentResponse> {
    let bytes = serialize_snapshot(ServiceMeterSnapshot {
        schema: SERVICE_METER_SCHEMA_TAG.to_string(),
        period: period.clone(),
        counters,
    })?;
    let response = create_encrypted_content(build_create_input(header, bytes, Some(vec![period])))?;
    Ok(build_upsert_response(response, true, false))
}

fn update_service_meter(
    prior: EncryptedContentResponse,
    header: EncryptedContentHeader,
    period: &str,
    counters: BTreeMap<String, String>,
) -> ExternResult<UpsertContentResponse> {
    let previous_snapshot = decode_service_meter_snapshot(&prior, period)?;
    let merged = merge_counters(&previous_snapshot.counters, &counters)?;
    if merged == previous_snapshot.counters && header == prior.encrypted_content.header {
        return Ok(build_upsert_response(prior, false, false));
    }
    let bytes = serialize_snapshot(ServiceMeterSnapshot {
        schema: SERVICE_METER_SCHEMA_TAG.to_string(),
        period: period.to_string(),
        counters: merged,
    })?;
    let response = update_prior_payload(prior, header, bytes, INVALID_METER_ACTION_HASH)?;
    Ok(build_upsert_response(response, false, true))
}

fn create_node_spec(
    header: EncryptedContentHeader,
    snapshot: NodeSpecSnapshot,
) -> ExternResult<UpsertContentResponse> {
    let bytes = serialize_snapshot(snapshot)?;
    let response = create_encrypted_content(build_create_input(header, bytes, None))?;
    Ok(build_upsert_response(response, true, false))
}

fn update_node_spec(
    prior: EncryptedContentResponse,
    header: EncryptedContentHeader,
    snapshot: NodeSpecSnapshot,
) -> ExternResult<UpsertContentResponse> {
    let previous_snapshot = decode_node_spec_snapshot(&prior)?;
    if snapshot == previous_snapshot && header == prior.encrypted_content.header {
        return Ok(build_upsert_response(prior, false, false));
    }
    let bytes = serialize_snapshot(snapshot)?;
    let response = update_prior_payload(prior, header, bytes, INVALID_NODE_SPEC_ACTION_HASH)?;
    Ok(build_upsert_response(response, false, true))
}

fn build_upsert_response(
    response: EncryptedContentResponse,
    was_created: bool,
    was_updated: bool,
) -> UpsertContentResponse {
    UpsertContentResponse {
        response,
        was_created,
        was_updated,
    }
}

/// Builds the converged header every upsert stamps on both create and update.
fn build_record_header(
    id: String,
    content_type: &str,
    hive_genesis_hash: ActionHash,
    display_hive_id: String,
    revision_author_signing_public_key: String,
    public_key_acl: Acl,
) -> EncryptedContentHeader {
    EncryptedContentHeader {
        id,
        display_hive_id,
        content_type: content_type.to_string(),
        revision_author_signing_public_key,
        acl_spec: AclSpec::OpenWrite {
            target_hive_genesis_hash: Some(hive_genesis_hash),
        },
        public_key_acl,
        lineage: None,
    }
}

fn build_create_input(
    header: EncryptedContentHeader,
    bytes: SerializedBytes,
    dynamic_links: Option<Vec<String>>,
) -> CreateEncryptedContentInput {
    let EncryptedContentHeader {
        id,
        display_hive_id,
        content_type,
        revision_author_signing_public_key,
        acl_spec,
        public_key_acl,
        lineage,
    } = header;
    CreateEncryptedContentInput {
        id,
        display_hive_id,
        content_type,
        revision_author_signing_public_key,
        bytes,
        acl_spec,
        public_key_acl,
        dynamic_links,
        lineage,
    }
}

fn update_prior_payload(
    prior: EncryptedContentResponse,
    header: EncryptedContentHeader,
    bytes: SerializedBytes,
    malformed_hash_error: &'static str,
) -> ExternResult<EncryptedContentResponse> {
    let previous_encrypted_content_hash =
        ActionHash::try_from(prior.hash.as_str()).map_err(|_| guest_error(malformed_hash_error))?;
    update_encrypted_content(UpdateEncryptedContentInput {
        previous_encrypted_content_hash,
        updated_encrypted_content: EncryptedContent { header, bytes },
        dynamic_links: None,
        remove_dynamic_links: None,
    })
}

fn decode_service_meter_snapshot(
    prior: &EncryptedContentResponse,
    period: &str,
) -> ExternResult<ServiceMeterSnapshot> {
    let snapshot: ServiceMeterSnapshot =
        holochain_serialized_bytes::decode(prior.encrypted_content.bytes.bytes())
            .map_err(|_| guest_error(INVALID_METER_SNAPSHOT))?;
    if snapshot.schema != SERVICE_METER_SCHEMA_TAG || snapshot.period != period {
        return Err(guest_error(INVALID_METER_SNAPSHOT));
    }
    Ok(snapshot)
}

fn decode_node_spec_snapshot(prior: &EncryptedContentResponse) -> ExternResult<NodeSpecSnapshot> {
    let snapshot: NodeSpecSnapshot =
        holochain_serialized_bytes::decode(prior.encrypted_content.bytes.bytes())
            .map_err(|_| guest_error(INVALID_NODE_SPEC_SNAPSHOT))?;
    if snapshot.schema != NODE_SPEC_SCHEMA_TAG {
        return Err(guest_error(INVALID_NODE_SPEC_SNAPSHOT));
    }
    Ok(snapshot)
}

fn serialize_snapshot<T>(snapshot: T) -> ExternResult<SerializedBytes>
where
    SerializedBytes: TryFrom<T, Error = SerializedBytesError>,
{
    SerializedBytes::try_from(snapshot).map_err(|err| wasm_error!(WasmErrorInner::Serialize(err)))
}

fn validate_period(period: &str) -> ExternResult<()> {
    let bytes = period.as_bytes();
    let shape_is_valid = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(|byte| byte.is_ascii_digit())
        && bytes[5..7].iter().all(|byte| byte.is_ascii_digit())
        && bytes[8..].iter().all(|byte| byte.is_ascii_digit());
    if !shape_is_valid {
        return Err(guest_error(INVALID_METER_PERIOD));
    }
    let month = (bytes[5] - b'0') * 10 + bytes[6] - b'0';
    let day = (bytes[8] - b'0') * 10 + bytes[9] - b'0';
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(guest_error(INVALID_METER_PERIOD));
    }
    Ok(())
}

fn canonicalize_counters(
    counters: BTreeMap<String, String>,
) -> ExternResult<BTreeMap<String, String>> {
    if counters.len() > MAX_METER_DIMENSIONS {
        return Err(guest_error(TOO_MANY_METER_DIMENSIONS));
    }
    validate_keys(counters.keys(), INVALID_METER_KEY)?;
    counters
        .into_iter()
        .map(|(key, value)| {
            if !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(guest_error(INVALID_METER_COUNTER));
            }
            u128::from_str(&value)
                .map(|parsed| (key, parsed.to_string()))
                .map_err(|_| guest_error(INVALID_METER_COUNTER))
        })
        .collect()
}

fn validate_spec_entries(spec: &BTreeMap<String, String>) -> ExternResult<()> {
    if spec.len() > MAX_NODE_SPEC_ENTRIES {
        return Err(guest_error(INVALID_NODE_SPEC_ENTRIES));
    }
    validate_keys(spec.keys(), INVALID_NODE_SPEC_KEY)?;
    let values_are_valid = spec.values().all(|value| {
        let char_count = value.chars().count();
        (MIN_NODE_SPEC_VALUE_CHARS..=MAX_NODE_SPEC_VALUE_CHARS).contains(&char_count)
            && !value.contains(['|', ';'])
            && !value.chars().any(char::is_control)
    });
    if !values_are_valid {
        return Err(guest_error(INVALID_NODE_SPEC_VALUE));
    }
    Ok(())
}

fn merge_counters(
    prior: &BTreeMap<String, String>,
    new: &BTreeMap<String, String>,
) -> ExternResult<BTreeMap<String, String>> {
    let mut merged = prior
        .iter()
        .map(|(key, value)| {
            u128::from_str(value)
                .map(|parsed| (key.clone(), parsed))
                .map_err(|_| guest_error(INVALID_METER_SNAPSHOT))
        })
        .collect::<ExternResult<BTreeMap<_, _>>>()?;
    new.iter()
        .try_for_each(|(key, value)| -> ExternResult<()> {
            let incoming = u128::from_str(value).map_err(|_| guest_error(INVALID_METER_COUNTER))?;
            let maximum = merged
                .get(key)
                .copied()
                .map_or(incoming, |stored| stored.max(incoming));
            merged.insert(key.clone(), maximum);
            Ok(())
        })?;
    if merged.len() > MAX_METER_DIMENSIONS {
        return Err(guest_error(METER_UNION_EXCEEDS_DIMENSIONS));
    }
    Ok(merged
        .into_iter()
        .map(|(key, value)| (key, value.to_string()))
        .collect())
}

fn validate_keys<'a>(
    mut keys: impl Iterator<Item = &'a String>,
    error_message: &'static str,
) -> ExternResult<()> {
    let keys_are_valid = keys.all(|key| {
        (MIN_RECORD_KEY_CHARS..=MAX_RECORD_KEY_CHARS).contains(&key.len())
            && key
                .bytes()
                .all(|byte| (b' '..=b'~').contains(&byte) && !matches!(byte, b'|' | b';' | b'='))
    });
    if !keys_are_valid {
        return Err(guest_error(error_message));
    }
    Ok(())
}

fn attestation_canonical_string(
    author_b64: &str,
    declared_at_micros: i64,
    spec: &BTreeMap<String, String>,
) -> String {
    let joined = spec
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(";");
    format!("{NODE_SPEC_SCHEMA_TAG}|{author_b64}|{declared_at_micros}|{joined}")
}

fn verify_app_attestation(
    attestation: Option<&NodeSpecAttestation>,
    author: &AgentPubKey,
    declared_at_micros: i64,
    spec: &BTreeMap<String, String>,
) -> ExternResult<Option<String>> {
    let Some(attestation) = attestation else {
        return Ok(None);
    };
    let key_is_accepted =
        ACCEPTED_APP_SIGNING_KEYS_B64.contains(&attestation.app_signing_key_b64.as_str());
    if !key_is_accepted {
        return Err(guest_error(UNRECOGNIZED_APP_SIGNING_KEY));
    }
    let key = AgentPubKey::try_from(attestation.app_signing_key_b64.as_str())
        .map_err(|_| guest_error(MALFORMED_APP_ATTESTATION))?;
    let decoded_signature = base64::engine::general_purpose::STANDARD
        .decode(&attestation.signature_b64)
        .map_err(|_| guest_error(MALFORMED_APP_ATTESTATION))?;
    let signature_bytes: [u8; SIGNATURE_BYTES] = decoded_signature
        .try_into()
        .map_err(|_| guest_error(MALFORMED_APP_ATTESTATION))?;
    let canonical = attestation_canonical_string(&author.to_string(), declared_at_micros, spec);
    if !verify_signature_raw(
        key,
        Signature::from(signature_bytes),
        canonical.into_bytes(),
    )? {
        return Err(guest_error(INVALID_APP_ATTESTATION));
    }
    Ok(Some(attestation.app_signing_key_b64.clone()))
}

fn guest_error(message: &'static str) -> WasmError {
    wasm_error!(WasmErrorInner::Guest(String::from(message)))
}

#[cfg(test)]
mod tests;
