use hdi::prelude::*;

use super::types::{GroupGenesis, GroupMembership};
use crate::hive::{check_hive_authority, role_satisfies, Role};

/// Fetch and decode a [`GroupGenesis`] by action hash, returning
/// `(group_author, group)`. Mirrors [`crate::hive::fetch_genesis`].
pub fn fetch_group_genesis(
    group_genesis_hash: &ActionHash,
) -> ExternResult<(AgentPubKey, GroupGenesis)> {
    let record = must_get_valid_record(group_genesis_hash.clone())?;
    let author = record.action().author().clone();
    let entry: GroupGenesis = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "group_genesis_hash {group_genesis_hash} does not reference a GroupGenesis entry",
            )))
        })?;
    Ok((author, entry))
}

/// Fetch and decode a [`GroupMembership`] by action hash, returning
/// `(membership_author, membership)`.
pub fn fetch_group_membership(
    membership_hash: &ActionHash,
) -> ExternResult<(AgentPubKey, GroupMembership)> {
    let record = must_get_valid_record(membership_hash.clone())?;
    let author = record.action().author().clone();
    let entry: GroupMembership = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "membership_hash {membership_hash} does not reference a GroupMembership entry",
            )))
        })?;
    Ok((author, entry))
}

/// Verify `agent` holds at least `required_role` in the group at
/// `timestamp`, via Path A (group author), Path B (hive sovereignty),
/// or Path C (explicit group membership). Returns `Valid` on the first
/// satisfied route, a contextual `Invalid` if none hold, and propagates
/// host errors from the chain walk.
///
/// `membership_hash` backs Path C (the agent's own group membership);
/// `hive_membership_hash` backs Path B (the agent's hive membership in
/// the group's parent hive). Either may be `None`.
///
/// Evaluation order is A → B → C, matching the conceptual primacy of the
/// hive sovereign. Path B is attempted even when `hive_membership_hash`
/// is `None` because that is how the hive *genesis author* (the root
/// sovereign, who holds no membership entry) is recognised.
pub fn check_group_authority(
    agent: &AgentPubKey,
    group_genesis_hash: &ActionHash,
    membership_hash: Option<&ActionHash>,
    hive_membership_hash: Option<&ActionHash>,
    required_role: Role,
    timestamp: &Timestamp,
) -> ExternResult<ValidateCallbackResult> {
    // Anchor on the group genesis. Catches "group_genesis_hash points at
    // a non-GroupGenesis entry" before anything else, and yields the
    // parent hive needed for Path B.
    let (group_author, group) = fetch_group_genesis(group_genesis_hash)?;

    // Path A — group author is the implicit group Owner.
    if &group_author == agent {
        return Ok(ValidateCallbackResult::Valid);
    }

    // Path B — hive sovereignty. A hive Admin+ of the parent hive (or the
    // hive genesis author, caught inside check_hive_authority even with a
    // None witness) controls every group in their hive. `required_role` is
    // intentionally NOT forwarded here: hive Admin+ confers FULL group
    // authority, so it satisfies every group-level role including Owner.
    // Forwarding it would wrongly demand hive Owner to grant a group Owner.
    let hive_check = check_hive_authority(
        agent,
        &group.hive_genesis_hash,
        hive_membership_hash,
        Role::Admin,
        timestamp,
    )?;
    if matches!(hive_check, ValidateCallbackResult::Valid) {
        return Ok(ValidateCallbackResult::Valid);
    }

    // Path C — explicit group membership.
    let Some(hash) = membership_hash else {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "agent {agent} is neither the group author of {group_genesis_hash}, \
             nor a hive Admin+ of its parent hive, and supplied no authorising \
             GroupMembership",
        )));
    };
    let (_membership_author, membership) = fetch_group_membership(hash)?;
    if &membership.group_genesis_hash != group_genesis_hash {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "group membership {hash} is for group {} but caller claimed group {group_genesis_hash}",
            membership.group_genesis_hash,
        )));
    }
    if &membership.for_agent != agent {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "group membership {hash} grants role to {} but action author is {agent}",
            membership.for_agent,
        )));
    }
    if let Some(expiry) = membership.expiry {
        if timestamp > &expiry {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "group membership {hash} expired at {expiry:?}; action timestamp {timestamp:?}",
            )));
        }
    }
    if !role_satisfies(membership.role, required_role) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "group membership {hash} grants role {:?}, required {:?}",
            membership.role, required_role,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}
