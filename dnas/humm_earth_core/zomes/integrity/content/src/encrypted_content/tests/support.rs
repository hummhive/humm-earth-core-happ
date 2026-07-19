use super::super::*;
use hdi::prelude::*;

pub(crate) fn agent_pubkey(byte: u8) -> AgentPubKey {
    AgentPubKey::from_raw_36(vec![byte; 36])
}

pub(crate) fn action_hash(byte: u8) -> ActionHash {
    ActionHash::from_raw_36(vec![byte; 36])
}

pub(crate) fn sample_acl_spec() -> AclSpec {
    AclSpec::HiveGroup {
        hive_genesis_hash: action_hash(9),
        author_membership_hash: None,
        group_acl: AclByGroupGenesis {
            owner: action_hash(10),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        },
        author_group_membership_hash: None,
        recipient_witnesses: vec![],
    }
}

pub(crate) fn sample_header_pubkey(pk_b64: &str) -> EncryptedContentHeader {
    EncryptedContentHeader {
        id: "id".into(),
        display_hive_id: "hive".into(),
        content_type: "ct".into(),
        acl_spec: sample_acl_spec(),
        public_key_acl: Acl {
            owner: "owner".into(),
            admin: vec![],
            writer: vec![],
            reader: vec![],
        },
        revision_author_signing_public_key: pk_b64.into(),
        lineage: None,
    }
}

pub(crate) fn content_with_spec(acl_spec: AclSpec, public_key_acl: Acl) -> EncryptedContent {
    EncryptedContent {
        header: EncryptedContentHeader {
            id: "id".into(),
            display_hive_id: "hive".into(),
            content_type: "ct".into(),
            acl_spec,
            public_key_acl,
            revision_author_signing_public_key: agent_pubkey(1).to_string(),
            lineage: None,
        },
        bytes: SerializedBytes::from(UnsafeBytes::from(vec![])),
    }
}

pub(crate) fn sample_content_with_acl(public_key_acl: Acl) -> EncryptedContent {
    content_with_spec(sample_acl_spec(), public_key_acl)
}

pub(crate) fn make_delete(author: AgentPubKey) -> Delete {
    Delete {
        author,
        timestamp: Timestamp(0),
        action_seq: 1,
        prev_action: action_hash(0),
        deletes_address: action_hash(1),
        deletes_entry_address: EntryHash::from_raw_36(vec![0u8; 36]),
        weight: Default::default(),
    }
}

pub(crate) fn make_create(author: AgentPubKey) -> Create {
    Create {
        author,
        timestamp: Timestamp(0),
        action_seq: 0,
        prev_action: action_hash(0),
        entry_type: EntryType::App(AppEntryDef {
            entry_index: 0.into(),
            zome_index: 0.into(),
            visibility: EntryVisibility::Public,
        }),
        entry_hash: EntryHash::from_raw_36(vec![0u8; 36]),
        weight: Default::default(),
    }
}

pub(crate) fn make_create_link(author: AgentPubKey) -> CreateLink {
    CreateLink {
        author,
        timestamp: Timestamp(0),
        action_seq: 0,
        prev_action: action_hash(0),
        base_address: AnyLinkableHash::from(action_hash(1)),
        target_address: AnyLinkableHash::from(action_hash(2)),
        zome_index: 0.into(),
        link_type: 0.into(),
        tag: LinkTag::new(vec![]),
        weight: Default::default(),
    }
}

pub(crate) fn make_delete_link(author: AgentPubKey) -> DeleteLink {
    DeleteLink {
        author,
        timestamp: Timestamp(0),
        action_seq: 1,
        prev_action: action_hash(0),
        base_address: AnyLinkableHash::from(action_hash(1)),
        link_add_address: action_hash(3),
    }
}

pub(crate) fn link_args() -> (AnyLinkableHash, AnyLinkableHash, LinkTag) {
    (
        AnyLinkableHash::from(action_hash(1)),
        AnyLinkableHash::from(action_hash(2)),
        LinkTag::new(vec![]),
    )
}
