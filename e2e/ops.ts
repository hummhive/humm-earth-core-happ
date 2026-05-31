/**
 * Typed `content`-zome operation wrappers over an {@link Agent}. These
 * mirror the exact externs + payloads humm-tauri calls, so the scenario
 * files read as the real product flow (create hive → grant → create
 * group → grant → write witnessed content).
 */
import { type ActionHash, type AgentPubKey } from "@holochain/client";

import { type Agent } from "./conductor.js";
import { type Acl, type AclBucketName, type AclByGroupGenesis, type RecipientWitness } from "./acl.js";

export type HashResponse = { hash: ActionHash };
export type GroupMembershipResponse = {
  membership: {
    for_agent: AgentPubKey;
    role: AclBucketName;
    group_genesis_hash: ActionHash;
    expiry: number | null;
  };
  hash: ActionHash;
};
export type HiveMembershipResponse = {
  membership: { for_agent: AgentPubKey; role: AclBucketName; expiry: number | null };
  hash: ActionHash;
};
export type EncryptedContentResponse = {
  encrypted_content: unknown;
  hash: ActionHash;
  original_hash: string;
};

export const createHiveGenesis = (a: Agent, displayId = "hive") =>
  a.call<HashResponse>("create_hive_genesis", { display_id: displayId });

export const createHiveMembership = (
  a: Agent,
  hiveGenesisHash: ActionHash,
  forAgent: AgentPubKey,
  role: AclBucketName,
  grantorMembershipHash: ActionHash | null = null,
  expiry: number | null = null,
) =>
  a.call<HiveMembershipResponse>("create_hive_membership", {
    hive_genesis_hash: hiveGenesisHash,
    for_agent: forAgent,
    role,
    grantor_membership_hash: grantorMembershipHash,
    expiry,
  });

export const getLatestMembership = (a: Agent, agent: AgentPubKey, hiveGenesisHash: ActionHash) =>
  a.call<HiveMembershipResponse | null>("get_latest_membership", {
    agent,
    hive_genesis_hash: hiveGenesisHash,
  });

export const createGroupGenesis = (
  a: Agent,
  hiveGenesisHash: ActionHash,
  displayId = "group",
  hiveWideRole: AclBucketName | null = null,
  creatorHiveMembershipHash: ActionHash | null = null,
) =>
  a.call<HashResponse>("create_group_genesis", {
    hive_genesis_hash: hiveGenesisHash,
    display_id: displayId,
    hive_wide_role: hiveWideRole,
    creator_hive_membership_hash: creatorHiveMembershipHash,
  });

export const createGroupMembership = (
  a: Agent,
  groupGenesisHash: ActionHash,
  forAgent: AgentPubKey,
  role: AclBucketName,
  grantorMembershipHash: ActionHash | null = null,
  grantorHiveMembershipHash: ActionHash | null = null,
  expiry: number | null = null,
) =>
  a.call<GroupMembershipResponse>("create_group_membership", {
    group_genesis_hash: groupGenesisHash,
    for_agent: forAgent,
    role,
    grantor_membership_hash: grantorMembershipHash,
    grantor_hive_membership_hash: grantorHiveMembershipHash,
    expiry,
  });

export const getLatestGroupMembership = (a: Agent, agent: AgentPubKey, groupGenesisHash: ActionHash) =>
  a.call<GroupMembershipResponse | null>("get_latest_group_membership", {
    agent,
    group_genesis_hash: groupGenesisHash,
  });

export const listGroupMembers = (a: Agent, groupGenesisHash: ActionHash) =>
  a.call<GroupMembershipResponse[]>("list_group_members", groupGenesisHash);

export const createContent = (
  a: Agent,
  payload: {
    id?: string;
    display_hive_id?: string;
    content_type?: string;
    revision_author_signing_public_key: string;
    bytes?: Uint8Array;
    acl_spec: unknown;
    public_key_acl: Acl;
    dynamic_links?: string[] | null;
  },
) =>
  a.call<EncryptedContentResponse>("create_encrypted_content", {
    id: payload.id ?? `id-${Math.random().toString(36).slice(2, 10)}`,
    display_hive_id: payload.display_hive_id ?? "",
    content_type: payload.content_type ?? "test-content-type",
    revision_author_signing_public_key: payload.revision_author_signing_public_key,
    bytes: payload.bytes ?? Buffer.from("e2e-bytes"),
    acl_spec: payload.acl_spec,
    public_key_acl: payload.public_key_acl,
    dynamic_links: payload.dynamic_links ?? null,
  });

export const updateContent = (
  a: Agent,
  previousHash: ActionHash,
  updatedEncryptedContent: unknown,
) =>
  a.call<EncryptedContentResponse>("update_encrypted_content", {
    previous_encrypted_content_hash: previousHash,
    updated_encrypted_content: updatedEncryptedContent,
  });

export const getEncryptedContent = (a: Agent, actionHash: ActionHash) =>
  a.call<unknown | null>("get_encrypted_content", actionHash);

export type { AclByGroupGenesis, RecipientWitness };
