use content_integrity::{EncryptedContent, EncryptedContentHeader};
use hdk::prelude::*;

use super::markers::{MigrationMarkerV1, MigrationMarkerV2, MIGRATION_MARKER_CONTENT_TYPE_PREFIX};

pub(crate) fn marker_content_type(original_content_type: &str) -> String {
    if original_content_type.starts_with(MIGRATION_MARKER_CONTENT_TYPE_PREFIX) {
        original_content_type.to_string()
    } else {
        format!("{MIGRATION_MARKER_CONTENT_TYPE_PREFIX}{original_content_type}")
    }
}

fn build_marker_payload_from_bytes(
    original: &EncryptedContent,
    bytes: SerializedBytes,
) -> EncryptedContent {
    EncryptedContent {
        header: EncryptedContentHeader {
            content_type: marker_content_type(&original.header.content_type),
            ..original.header.clone()
        },
        bytes,
    }
}

pub fn build_marker_payload(
    original: &EncryptedContent,
    marker: &MigrationMarkerV1,
) -> ExternResult<EncryptedContent> {
    let bytes = SerializedBytes::try_from(marker.clone())
        .map_err(|err| wasm_error!(WasmErrorInner::Serialize(err)))?;
    Ok(build_marker_payload_from_bytes(original, bytes))
}

pub fn build_marker_v2_payload(
    original: &EncryptedContent,
    marker: &MigrationMarkerV2,
) -> ExternResult<EncryptedContent> {
    let bytes = SerializedBytes::try_from(marker.clone())
        .map_err(|err| wasm_error!(WasmErrorInner::Serialize(err)))?;
    Ok(build_marker_payload_from_bytes(original, bytes))
}
