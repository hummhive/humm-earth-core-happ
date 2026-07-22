use hdi::prelude::*;

use super::types::{GroupGenesis, GroupMembership};

/// Resolve a link target that MUST be action-addressed. A non-action
/// target is a structural defect of the link itself → deterministic
/// `Invalid`, never `Err` (an `Err` parks the link in validation-retry
/// limbo instead of rejecting it).
pub(crate) fn require_action_target(
    target_address: &AnyLinkableHash,
) -> Result<ActionHash, ValidateCallbackResult> {
    target_address.clone().into_action_hash().ok_or_else(|| {
        ValidateCallbackResult::Invalid(format!(
            "link target {target_address} must be an ActionHash"
        ))
    })
}

/// `link_author == target_author` guard shared by the three group link
/// create validators.
pub(crate) fn require_link_author_is(
    link_author: &AgentPubKey,
    target_author: &AgentPubKey,
) -> ValidateCallbackResult {
    if link_author != target_author {
        return ValidateCallbackResult::Invalid(format!(
            "link author {link_author} does not match target entry author {target_author}",
        ));
    }
    ValidateCallbackResult::Valid
}

/// Decoded link target, or the `Invalid` verdict when the link author is not
/// the target entry's author.
pub(crate) fn link_authors_target_entry<T>(
    link_action: &CreateLink,
    target_address: &AnyLinkableHash,
) -> ExternResult<Result<T, ValidateCallbackResult>>
where
    T: TryFrom<SerializedBytes, Error = SerializedBytesError>,
{
    let target_ah = match require_action_target(target_address) {
        Ok(hash) => hash,
        Err(invalid) => return Ok(Err(invalid)),
    };
    let record = must_get_valid_record(target_ah)?;
    if let invalid @ ValidateCallbackResult::Invalid(_) =
        require_link_author_is(&link_action.author, record.action().author())
    {
        return Ok(Err(invalid));
    }
    let Some(entry) = record.entry().to_app_option().map_err(|e| wasm_error!(e))? else {
        return Ok(Err(ValidateCallbackResult::Invalid(format!(
            "link target {} references an unexpected entry type",
            record.action_address(),
        ))));
    };
    Ok(Ok(entry))
}

/// `AgentToGroupMemberships`: base = grantee `AgentPubKey`, target =
/// [`GroupMembership`]. Forward index ("my group memberships"). Base must
/// equal `membership.for_agent`; link author must be the membership
/// author (the grantor).
pub fn validate_create_link_agent_to_group_memberships(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    _tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    let target_ah = match require_action_target(&target_address) {
        Ok(hash) => hash,
        Err(invalid) => return Ok(invalid),
    };
    let record = must_get_valid_record(target_ah)?;
    let target_author = record.action().author().clone();
    let author_check = require_link_author_is(&action.author, &target_author);
    if !matches!(author_check, ValidateCallbackResult::Valid) {
        return Ok(author_check);
    }
    let Some(membership): Option<GroupMembership> =
        record.entry().to_app_option().map_err(|e| wasm_error!(e))?
    else {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "AgentToGroupMemberships target {target_address} is not a GroupMembership",
        )));
    };
    let expected_base = AnyLinkableHash::from(membership.for_agent.clone());
    if base_address != expected_base {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "AgentToGroupMemberships base {base_address} does not match membership.for_agent {}",
            membership.for_agent,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// `GroupToGroupMemberships`: base = `group_genesis_hash`, target =
/// [`GroupMembership`], tag = `for_agent` (multibase string bytes).
/// Reverse index — the cryptographic roster. Base must equal
/// `membership.group_genesis_hash`; tag must equal `membership.for_agent`;
/// link author must be the membership author.
pub fn validate_create_link_group_to_group_memberships(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    let target_ah = match require_action_target(&target_address) {
        Ok(hash) => hash,
        Err(invalid) => return Ok(invalid),
    };
    let record = must_get_valid_record(target_ah)?;
    let target_author = record.action().author().clone();
    let author_check = require_link_author_is(&action.author, &target_author);
    if !matches!(author_check, ValidateCallbackResult::Valid) {
        return Ok(author_check);
    }
    let Some(membership): Option<GroupMembership> =
        record.entry().to_app_option().map_err(|e| wasm_error!(e))?
    else {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "GroupToGroupMemberships target {target_address} is not a GroupMembership",
        )));
    };
    let expected_base = AnyLinkableHash::from(membership.group_genesis_hash.clone());
    if base_address != expected_base {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "GroupToGroupMemberships base {base_address} does not match \
             membership.group_genesis_hash {}",
            membership.group_genesis_hash,
        )));
    }
    let tag_str = match String::from_utf8(tag.0) {
        Ok(s) => s,
        Err(e) => {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "GroupToGroupMemberships tag is not valid UTF-8: {e}",
            )))
        }
    };
    if tag_str != membership.for_agent.to_string() {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "GroupToGroupMemberships tag {tag_str} does not match membership.for_agent {}",
            membership.for_agent,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// `HiveToGroups`: base = `hive_genesis_hash`, target = [`GroupGenesis`].
/// Enumerate a hive's groups. Base must equal `genesis.hive_genesis_hash`;
/// link author must be the genesis author (the group creator); tag MUST
/// be empty (reserved for future use; constrained now so a rogue group
/// creator cannot poison the field for display/routing consumers).
pub fn validate_create_link_hive_to_groups(
    action: CreateLink,
    base_address: AnyLinkableHash,
    target_address: AnyLinkableHash,
    tag: LinkTag,
) -> ExternResult<ValidateCallbackResult> {
    let target_ah = match require_action_target(&target_address) {
        Ok(hash) => hash,
        Err(invalid) => return Ok(invalid),
    };
    let record = must_get_valid_record(target_ah)?;
    let target_author = record.action().author().clone();
    let author_check = require_link_author_is(&action.author, &target_author);
    if !matches!(author_check, ValidateCallbackResult::Valid) {
        return Ok(author_check);
    }
    if !tag.0.is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "HiveToGroups link tag must be empty (reserved for future use)".into(),
        ));
    }
    let Some(genesis): Option<GroupGenesis> =
        record.entry().to_app_option().map_err(|e| wasm_error!(e))?
    else {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveToGroups target {target_address} is not a GroupGenesis",
        )));
    };
    let expected_base = AnyLinkableHash::from(genesis.hive_genesis_hash.clone());
    if base_address != expected_base {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveToGroups base {base_address} does not match genesis.hive_genesis_hash {}",
            genesis.hive_genesis_hash,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Author-gated delete shared by the group index, owner-handoff, and
/// invite-redemption links: only the link creator may delete.
///
/// **Index-vs-entry contract (security-relevant).** A link's `delete`
/// is the link author's prerogative, which means a grantor (who is the
/// only legal author of `AgentToGroupMemberships` /
/// `GroupToGroupMemberships`) can later remove the discovery links
/// pointing at a grantee's still-valid `GroupMembership` entry. The
/// `GroupMembership` entry itself is immutable and remains cryptographically
/// valid — only the index loses the row. Coordinator + downstream
/// consumers MUST treat the discovery links as a *cache*, not as the
/// authoritative roster: every authority decision MUST be made by
/// `must_get_valid_record` against the entry hash directly. The links
/// exist solely to enumerate "which membership hashes does this group /
/// agent currently advertise"; a missing link does NOT prove a missing
/// membership. (Mirrors the pass-2 hive-link discipline; documented
/// here so future consumers in this repo and humm-tauri don't
/// mistakenly index-gate access.)
pub fn validate_delete_group_link(
    action: DeleteLink,
    original_action: CreateLink,
    link_label: &str,
) -> ExternResult<ValidateCallbackResult> {
    if action.author == original_action.author {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "{link_label} link delete must be authored by the link creator \
         (creator: {}, attempted by: {})",
        original_action.author, action.author,
    )))
}
