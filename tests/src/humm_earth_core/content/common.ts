import { CallableCell } from "@holochain/tryorama";
import {
  NewEntryAction,
  ActionHash,
  Record,
  AppBundleSource,
  fakeActionHash,
  fakeAgentPubKey,
  fakeEntryHash,
  fakeDnaHash,
  encodeHashToBase64,
} from "@holochain/client";
import { decode } from "@msgpack/msgpack";

export type EncryptedContentResponse = {
  encrypted_content: any;
  hash: ActionHash;
};

export enum AclRole {
  Owner = "Owner",
  Admin = "Admin",
  Writer = "Writer",
  Reader = "Reader",
}

// Pull the cell's agent pubkey out of `cell_id` and render it as the
// `'u' + URL_SAFE_NO_PAD` holohash form the integrity zome compares
// against `action.author`. Tests that need to match the validation rule
// thread this value through the sample builders.
export function cellPubkeyB64(cell: CallableCell): string {
  return encodeHashToBase64(cell.cell_id[1]);
}

export function sampleAcl() {
  return {
    owner: "test-entity-acl-id",
    admin: [],
    writer: [],
    reader: [],
  };
}

export function sampleEncryptedContent(
  partialEncryptedContent: any = {},
  agentPubKeyB64?: string,
) {
  return {
    bytes: Buffer.from("test-bytes"),
    ...partialEncryptedContent,
    header: {
      id: "test-id",
      hive_id: "test-hive-id",
      content_type: "test-content-type",
      revision_author_signing_public_key:
        agentPubKeyB64 ?? "test-revision-author-signing-public-key",
      acl: sampleAcl(),
      public_key_acl: {
        owner: "test-entity-acl-public-key",
        admin: [],
        writer: [],
        reader: [],
      },
      ...((partialEncryptedContent as any).header || {}),
    },
  };
}

export async function sampleCreateEncryptedContentInput(
  partialEncryptedContent: any = {},
  dynamicLinks: any[] = [],
  agentPubKeyB64?: string,
) {
  const sample = sampleEncryptedContent(partialEncryptedContent, agentPubKeyB64);
  return {
    id: sample.header.id,
    hive_id: sample.header.hive_id,
    content_type: sample.header.content_type,
    revision_author_signing_public_key:
      sample.header.revision_author_signing_public_key,
    bytes: sample.bytes,
    acl: sample.header.acl,
    public_key_acl: sample.header.public_key_acl,
    dynamic_links: dynamicLinks,
  };
}

export async function createEncryptedContent(
  cell: CallableCell,
  createEncryptedContentInput: any = undefined,
): Promise<EncryptedContentResponse> {
  const content =
    createEncryptedContentInput ||
    (await sampleCreateEncryptedContentInput({}, [], cellPubkeyB64(cell)));
  return cell.callZome({
    zome_name: "content",
    fn_name: "create_encrypted_content",
    payload: content,
  });
}
