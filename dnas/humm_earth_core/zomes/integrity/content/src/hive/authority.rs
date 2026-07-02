use hdi::hash_path::path::Component;
use hdi::prelude::*;

use super::types::{HiveGenesis, HiveMembership, Role};

/// Permission containment: does `held` satisfy `required`? Higher roles
/// satisfy lower ones; equal satisfies equal. The order
/// `Owner > Admin > Writer > Reader` is hard-coded here; this function
/// is THE single source of truth for the ordering.
pub fn role_satisfies(held: Role, required: Role) -> bool {
    fn rank(r: Role) -> u8 {
        match r {
            Role::Owner => 4,
            Role::Admin => 3,
            Role::Writer => 2,
            Role::Reader => 1,
        }
    }
    rank(held) >= rank(required)
}

/// Fetch + decode an entry by action hash into `(author, entry)`, with an
/// explicit "wrong entry type" message (`type_label`) rather than a bare
/// deserialization error. Shared by the typed fetchers below.
pub(super) fn fetch_authored_entry<T: TryFrom<SerializedBytes, Error = SerializedBytesError>>(
    action_hash: &ActionHash,
    type_label: &str,
) -> ExternResult<(AgentPubKey, T)> {
    let record = must_get_valid_record(action_hash.clone())?;
    let author = record.action().author().clone();
    let entry = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(e))?
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "{action_hash} does not reference a {type_label} entry"
            )))
        })?;
    Ok((author, entry))
}

/// `(genesis_author, genesis_entry)` for a [`HiveGenesis`] action hash.
pub fn fetch_genesis(genesis_hash: &ActionHash) -> ExternResult<(AgentPubKey, HiveGenesis)> {
    fetch_authored_entry(genesis_hash, "HiveGenesis")
}

/// `(membership_author, membership_entry)` for a [`HiveMembership`] action hash.
pub fn fetch_membership(
    membership_hash: &ActionHash,
) -> ExternResult<(AgentPubKey, HiveMembership)> {
    fetch_authored_entry(membership_hash, "HiveMembership")
}

/// Verify `agent` holds at least `required_role` for `genesis_hash` at
/// `timestamp`, where the claim is backed either by:
///
/// - `membership_hash.is_none()` AND `agent` IS the genesis author
///   (implicit Owner), OR
/// - `membership_hash.is_some()` AND that membership grants `agent` a
///   role >= `required_role`, is for the correct hive, and has not
///   expired at `timestamp`.
///
/// Returns `Valid` on success and a contextual `Invalid` on every
/// failure branch. Caller is expected to short-circuit on `Invalid` /
/// propagate `Err`.
pub fn check_hive_authority(
    agent: &AgentPubKey,
    genesis_hash: &ActionHash,
    membership_hash: Option<&ActionHash>,
    required_role: Role,
    timestamp: &Timestamp,
) -> ExternResult<ValidateCallbackResult> {
    // Always anchor to the genesis. This catches "genesis_hash points at
    // a non-HiveGenesis entry" cleanly before we look at anything else.
    let (genesis_author, _genesis) = fetch_genesis(genesis_hash)?;

    // Path 1: implicit-Owner. The genesis author always has Owner role,
    // no membership entry needed.
    if &genesis_author == agent {
        return Ok(ValidateCallbackResult::Valid);
    }

    // Path 2: explicit-membership. Caller must supply a membership hash;
    // we fetch it, verify it's for the right hive + right grantee, and
    // check role + expiry.
    let Some(hash) = membership_hash else {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "agent {agent} is not the genesis author of {genesis_hash} \
             and supplied no authorising HiveMembership",
        )));
    };
    let (_membership_author, membership) = fetch_membership(hash)?;
    if &membership.hive_genesis_hash != genesis_hash {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "membership {hash} is for hive {} but caller claimed hive {genesis_hash}",
            membership.hive_genesis_hash,
        )));
    }
    if &membership.for_agent != agent {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "membership {hash} grants role to {} but action author is {agent}",
            membership.for_agent,
        )));
    }
    if let Some(expiry) = membership.expiry {
        if timestamp > &expiry {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "membership {hash} expired at {expiry:?}; action timestamp {timestamp:?}",
            )));
        }
    }
    if !role_satisfies(membership.role, required_role) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "membership {hash} grants role {:?}, required {:?}",
            membership.role, required_role,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Recompute the hash of a path constructed from string components.
///
/// Used by link validators to verify the link's claimed `base_address`
/// matches the path the link CLAIMS to be off of, after recomputing that
/// path from validated data (the target entry's header fields).
pub fn recompute_path_hash(components: &[&str]) -> ExternResult<AnyLinkableHash> {
    let path = Path::from(
        components
            .iter()
            .map(|c| Component::from(*c))
            .collect::<Vec<_>>(),
    );
    Ok(path.path_entry_hash()?.into())
}
