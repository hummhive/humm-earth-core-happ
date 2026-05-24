use hdi::prelude::*;

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct EncryptedContent {
    pub header: EncryptedContentHeader,
    pub bytes: SerializedBytes,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct EncryptedContentHeader {
    pub id: String,
    pub hive_id: String,
    pub content_type: String,
    pub acl: Acl,
    pub public_key_acl: Acl,
    pub revision_author_signing_public_key: String,
    // revisionauthor
    // add hash?
    // add signature?
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct Acl {
    pub owner: String,
    pub admin: Vec<String>,
    pub writer: Vec<String>,
    pub reader: Vec<String>,
}

// Cross-check the sender-controlled `revision_author_signing_public_key`
// field against the cryptographically-attested `action.author`. Without
// this guard the field is forgeable: any peer with a custom DNA can
// commit an entry claiming another agent's signing pubkey, and every
// downstream consumer (DMs, member entries, audit trails) trusts the
// header value as the sender identity. Comparing the string form is
// sound because `AgentPubKey::to_string()` emits the same multibase
// holohash form (`'u' + URL_SAFE_NO_PAD(39 bytes)`) that
// `@holochain/client::encodeHashToBase64` writes into the header.
fn check_author_matches_header(
    action_author: &AgentPubKey,
    header_pubkey: &str,
) -> ValidateCallbackResult {
    let attested = action_author.to_string();
    if attested != header_pubkey {
        return ValidateCallbackResult::Invalid(format!(
            "revision_author_signing_public_key ({}) does not match action.author ({})",
            header_pubkey, attested,
        ));
    }
    ValidateCallbackResult::Valid
}

pub fn validate_create_encrypted_content(
    action: EntryCreationAction,
    encrypted_content: EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    Ok(check_author_matches_header(
        action.author(),
        &encrypted_content.header.revision_author_signing_public_key,
    ))
}
pub fn validate_update_encrypted_content(
    action: Update,
    encrypted_content: EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    Ok(check_author_matches_header(
        &action.author,
        &encrypted_content.header.revision_author_signing_public_key,
    ))
}
pub fn validate_delete_encrypted_content(
    _action: Delete,
    _original_action: EntryCreationAction,
    _original_encrypted_content: EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}
pub fn validate_create_link_encrypted_content_updates(
    _action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    let action_hash = base_address
        .into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest(String::from(
            "No action hash associated with link"
        ))))?;
    let record = must_get_valid_record(action_hash)?;
    let _encrypted_content: crate::EncryptedContent = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(String::from(
            "Linked action must reference an entry"
        ))))?;
    let action_hash =
        target_address
            .into_action_hash()
            .ok_or(wasm_error!(WasmErrorInner::Guest(String::from(
                "No action hash associated with link"
            ))))?;
    let record = must_get_valid_record(action_hash)?;
    let _encrypted_content: crate::EncryptedContent = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(String::from(
            "Linked action must reference an entry"
        ))))?;
    Ok(ValidateCallbackResult::Valid)
}
pub fn validate_delete_link_encrypted_content_updates(
    _action: DeleteLink,
    _original_action: CreateLink,
    _base: AnyLinkableHash,
    _target: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(String::from(
        "EncryptedContentUpdates links cannot be deleted",
    )))
}
pub fn validate_create_link_all_encrypted_content(
    _action: CreateLink,
    _base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    // Check the entry type for the given action hash
    let action_hash =
        target_address
            .into_action_hash()
            .ok_or(wasm_error!(WasmErrorInner::Guest(String::from(
                "No action hash associated with link"
            ))))?;
    let record = must_get_valid_record(action_hash)?;
    let _encrypted_content: crate::EncryptedContent = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(String::from(
            "Linked action must reference an entry"
        ))))?;
    // TODO: add the appropriate validation rules
    Ok(ValidateCallbackResult::Valid)
}
pub fn validate_delete_link_all_encrypted_content(
    _action: DeleteLink,
    _original_action: CreateLink,
    _base: AnyLinkableHash,
    _target: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(String::from(
        "AllEncryptedContent links cannot be deleted",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_pubkey(byte: u8) -> AgentPubKey {
        AgentPubKey::from_raw_36(vec![byte; 36])
    }

    fn sample_header(pk_b64: &str) -> EncryptedContentHeader {
        EncryptedContentHeader {
            id: "id".into(),
            hive_id: "hive".into(),
            content_type: "ct".into(),
            acl: Acl {
                owner: "owner".into(),
                admin: vec![],
                writer: vec![],
                reader: vec![],
            },
            public_key_acl: Acl {
                owner: "owner".into(),
                admin: vec![],
                writer: vec![],
                reader: vec![],
            },
            revision_author_signing_public_key: pk_b64.into(),
        }
    }

    #[test]
    fn check_rejects_when_header_pubkey_does_not_match_action_author() {
        let alice = agent_pubkey(1);
        let bob = agent_pubkey(2);
        let bob_b64 = bob.to_string();
        let result = check_author_matches_header(&alice, &bob_b64);
        match result {
            ValidateCallbackResult::Invalid(msg) => {
                assert!(msg.contains("revision_author_signing_public_key"));
                assert!(msg.contains(&bob_b64));
                assert!(msg.contains(&alice.to_string()));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn check_accepts_when_header_pubkey_matches_action_author() {
        let alice = agent_pubkey(1);
        let alice_b64 = alice.to_string();
        let result = check_author_matches_header(&alice, &alice_b64);
        assert!(
            matches!(result, ValidateCallbackResult::Valid),
            "expected Valid, got {result:?}",
        );
    }

    #[test]
    fn check_rejects_empty_header_pubkey() {
        let alice = agent_pubkey(1);
        let result = check_author_matches_header(&alice, "");
        assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
    }

    #[test]
    fn check_rejects_legacy_placeholder_header() {
        let alice = agent_pubkey(1);
        let result = check_author_matches_header(
            &alice,
            "test-revision-author-signing-public-key",
        );
        assert!(matches!(result, ValidateCallbackResult::Invalid(_)));
    }

    #[test]
    fn header_struct_round_trips_through_the_check() {
        let alice = agent_pubkey(7);
        let valid_header = sample_header(&alice.to_string());
        assert!(matches!(
            check_author_matches_header(&alice, &valid_header.revision_author_signing_public_key),
            ValidateCallbackResult::Valid,
        ));
        let forged_header = sample_header(&agent_pubkey(8).to_string());
        assert!(matches!(
            check_author_matches_header(&alice, &forged_header.revision_author_signing_public_key),
            ValidateCallbackResult::Invalid(_),
        ));
    }
}
