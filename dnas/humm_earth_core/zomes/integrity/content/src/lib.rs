pub mod encrypted_content;
pub mod globals;
pub mod group;
pub mod hive;
pub mod inbox;
pub mod invite;
mod validation_dispatch;

pub use encrypted_content::*;
pub use globals::*;
pub use group::*;
pub use hive::*;
pub use inbox::*;
pub use invite::*;

use hdi::prelude::*;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
#[hdk_entry_types]
#[unit_enum(EntryTypesUnit)]
pub enum EntryTypes {
    EncryptedContent(EncryptedContent),
    HiveGenesis(HiveGenesis),
    HiveMembership(HiveMembership),
    /// Private source-chain entry — never DHT-published. Validators
    /// allow any commit (a host should only ever write its own
    /// probe-log; the entry is invisible to peers anyway).
    #[entry_type(visibility = "private")]
    DmProbeLog(DmProbeLog),
    /// Group root-of-trust + role-grant entries. Appended at the END to
    /// keep existing entry-type indices (0..=3) stable post-pass-2.
    GroupGenesis(GroupGenesis),
    GroupMembership(GroupMembership),
    /// Pass-5 owner-handoff + invite-redemption entries. Appended at the END
    /// to keep existing entry-type indices (0..=5) stable.
    HiveOwnerHandoffOffer(HiveOwnerHandoffOffer),
    HiveOwnerHandoffAccept(HiveOwnerHandoffAccept),
    InviteRedemption(InviteRedemption),
}

#[derive(Serialize, Deserialize)]
#[hdk_link_types]
pub enum LinkTypes {
    OriginalHashPointer,
    EncryptedContentUpdates,
    TimePath,
    TimeItem,
    Hive,
    Dynamic,
    HummContentId,
    HummContentOwner,
    HummContentAdmin,
    HummContentWriter,
    HummContentReader,
    /// Inbox — recipient AgentPubKey → content ActionHash; tag = 1 byte
    /// [`InboxEvent`] discriminator. Appended at the END to keep all
    /// existing variant indices stable post-pass-2.
    Inbox,
    /// Group discovery links. Appended at the END to keep existing
    /// link-type indices (0..=11) stable post-pass-2.
    AgentToGroupMemberships,
    GroupToGroupMemberships,
    HiveToGroups,
    /// Pass-5 owner-handoff + invite-redemption links. Appended at the END
    /// to keep existing link-type indices (0..=14) stable.
    AgentToOwnerHandoffs,
    HiveToOwnerHandoffs,
    InviteToRedemptions,
}

#[hdk_extern]
pub fn genesis_self_check(_data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_agent_joining(
    agent_pub_key: AgentPubKey,
    membrane_proof: &Option<MembraneProof>,
) -> ExternResult<ValidateCallbackResult> {
    validation_dispatch::validate_agent_joining(agent_pub_key, membrane_proof)
}

#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    validation_dispatch::validate_op(op)
}
