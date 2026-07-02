use hdi::prelude::*;

use crate::{EntryTypesUnit, LinkTypes};

pub(crate) fn validate_agent_joining(
    _agent_pub_key: AgentPubKey,
    _membrane_proof: &Option<MembraneProof>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}

pub(super) fn validate_register_agent_activity(
    agent_activity: OpActivity<EntryTypesUnit, LinkTypes>,
) -> ExternResult<ValidateCallbackResult> {
    match agent_activity {
        OpActivity::CreateAgent { agent, action } => {
            let previous_action = must_get_action(action.prev_action)?;
            match previous_action.action() {
                Action::AgentValidationPkg(AgentValidationPkg { membrane_proof, .. }) => {
                    validate_agent_joining(agent, membrane_proof)
                }
                _ => Ok(ValidateCallbackResult::Invalid(
                    "The previous action for a `CreateAgent` action must be an `AgentValidationPkg`"
                        .to_string(),
                )),
            }
        }
        _ => Ok(ValidateCallbackResult::Valid),
    }
}
