/**
 * AclSpec variant scenarios (DirectMessage / Public / OpenWrite) +
 * M-1 update author-binding. Covers the per-variant accept/reject
 * contracts humm-tauri relies on for DMs, public posts, member-request
 * / hive-discovery, and update-chain integrity.
 */
import { type Agent } from "../conductor.js";
import {
  aclSpecDirectMessage,
  aclSpecOpenWrite,
  aclSpecPublic,
  assert,
  emptyAcl,
  expectReject,
  readerAcl,
  step,
} from "../acl.js";
import { createContent, createHiveGenesis, updateContent } from "../ops.js";

export async function run(alice: Agent, bob: Agent, carol: Agent): Promise<void> {
  console.log("\n# acl-spec variants + update author-binding");

  // --- DirectMessage ---

  await step("DM with author in recipients + matching reader bucket commits", async () => {
    const recipients = [alice.key, bob.key];
    const res = await createContent(alice, {
      revision_author_signing_public_key: alice.b64,
      acl_spec: aclSpecDirectMessage(recipients),
      public_key_acl: readerAcl(recipients),
      content_type: "direct_message",
    });
    assert(res.hash, "valid DM must commit");
  });

  await step("DM REJECTED when author is not in recipients", async () => {
    // Alice tries to forge a DM between bob and carol.
    await expectReject(
      createContent(alice, {
        revision_author_signing_public_key: alice.b64,
        acl_spec: aclSpecDirectMessage([bob.key, carol.key]),
        public_key_acl: readerAcl([bob.key, carol.key]),
        content_type: "direct_message",
      }),
    );
  });

  await step("DM REJECTED when reader bucket != recipients (routing forgery)", async () => {
    // recipients = [alice, bob], but reader bucket injects carol.
    await expectReject(
      createContent(alice, {
        revision_author_signing_public_key: alice.b64,
        acl_spec: aclSpecDirectMessage([alice.key, bob.key]),
        public_key_acl: readerAcl([alice.key, carol.key]),
        content_type: "direct_message",
      }),
    );
  });

  await step("DM REJECTED with a single recipient (cardinality floor)", async () => {
    await expectReject(
      createContent(alice, {
        revision_author_signing_public_key: alice.b64,
        acl_spec: aclSpecDirectMessage([alice.key]),
        public_key_acl: readerAcl([alice.key]),
        content_type: "direct_message",
      }),
    );
  });

  // --- Public ---

  await step("Public post by hive Writer commits; reader bucket is a free hint", async () => {
    const hive = await createHiveGenesis(alice, "var-hive-pub");
    const res = await createContent(alice, {
      revision_author_signing_public_key: alice.b64,
      acl_spec: aclSpecPublic(hive.hash, null),
      public_key_acl: { owner: "", admin: [], writer: [], reader: ["*"] },
      content_type: "humm-addon-text-post-v1",
    });
    assert(res.hash, "Public post must commit");
  });

  // --- OpenWrite ---

  await step("OpenWrite with target=None commits for any author (hive discovery)", async () => {
    const res = await createContent(carol, {
      revision_author_signing_public_key: carol.b64,
      acl_spec: aclSpecOpenWrite(null),
      public_key_acl: emptyAcl(),
      content_type: "hummhive-core-hive-discovery-v1",
    });
    assert(res.hash, "OpenWrite target=None must commit for an outsider");
  });

  await step("OpenWrite with a real target HiveGenesis commits (member-request)", async () => {
    const hive = await createHiveGenesis(alice, "var-hive-ow");
    // Carol is NOT a member of the hive — OpenWrite only needs the
    // target to resolve to a real HiveGenesis + author identity.
    const res = await createContent(carol, {
      revision_author_signing_public_key: carol.b64,
      acl_spec: aclSpecOpenWrite(hive.hash),
      public_key_acl: emptyAcl(),
      content_type: "hummhive-core-member-request-v1",
    });
    assert(res.hash, "OpenWrite to a real hive must commit");
  });

  await step("OpenWrite REJECTED when target is not a real HiveGenesis", async () => {
    // Use a real content-entry action hash as the bogus target: it
    // resolves on the DHT but is NOT a HiveGenesis, so fetch_genesis
    // fails the variant validator.
    const decoy = await createContent(alice, {
      revision_author_signing_public_key: alice.b64,
      acl_spec: aclSpecOpenWrite(null),
      public_key_acl: emptyAcl(),
      content_type: "decoy",
    });
    await expectReject(
      createContent(carol, {
        revision_author_signing_public_key: carol.b64,
        acl_spec: aclSpecOpenWrite(decoy.hash),
        public_key_acl: emptyAcl(),
        content_type: "hummhive-core-member-request-v1",
      }),
    );
  });

  // --- M-1: update author binding ---

  await step("author can update their own content (M-1 allows)", async () => {
    const hive = await createHiveGenesis(alice, "var-hive-upd");
    const created = await createContent(alice, {
      id: "upd-1",
      revision_author_signing_public_key: alice.b64,
      acl_spec: aclSpecPublic(hive.hash, null),
      public_key_acl: emptyAcl(),
      content_type: "humm-addon-text-post-v1",
    });
    const updated = {
      header: {
        id: "upd-1",
        display_hive_id: "",
        content_type: "humm-addon-text-post-v1",
        acl_spec: aclSpecPublic(hive.hash, null),
        public_key_acl: emptyAcl(),
        revision_author_signing_public_key: alice.b64,
      },
      bytes: Buffer.from("e2e-bytes-v2"),
    };
    const res = await updateContent(alice, created.hash, updated);
    assert(res.hash, "self-update must commit");
  });

  await step("M-1 REJECTS an update authored by a different agent", async () => {
    // Use OpenWrite content so bob's update entry PASSES the variant
    // validator (OpenWrite only checks author-vs-header). That isolates
    // M-1 (action.author == original_action.author) as the sole failing
    // rule — a Public/HiveGroup update would instead be rejected earlier
    // by the hive/group authority check and never reach M-1.
    const created = await createContent(alice, {
      id: "upd-2",
      revision_author_signing_public_key: alice.b64,
      acl_spec: aclSpecOpenWrite(null),
      public_key_acl: emptyAcl(),
      content_type: "hummhive-core-hive-discovery-v1",
    });
    // Bob commits an Update on HIS chain pointing at Alice's create.
    // His header pubkey is his own (clears the pass-1 author-vs-header
    // guard); M-1 then rejects because the original author is Alice.
    const forged = {
      header: {
        id: "upd-2",
        display_hive_id: "",
        content_type: "hummhive-core-hive-discovery-v1",
        acl_spec: aclSpecOpenWrite(null),
        public_key_acl: emptyAcl(),
        revision_author_signing_public_key: bob.b64,
      },
      bytes: Buffer.from("hijack"),
    };
    await expectReject(updateContent(bob, created.hash, forged), "original action author");
  });
}
