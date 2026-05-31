/**
 * Hive + group authority scenarios — the fetch-dependent
 * `check_hive_authority` (Path 1/2) and `check_group_authority`
 * (Path A/B/C) branches that host-side `cargo test` cannot reach.
 *
 * Real humm-tauri flow: found a hive, grant memberships, found a group,
 * grant group memberships, then write scoped content as various agents.
 */
import { type Agent, waitFor } from "../conductor.js";
import {
  aclSpecHiveGroup,
  aclSpecPublic,
  assert,
  emptyAcl,
  expectReject,
  groupAcl,
  step,
} from "../acl.js";
import {
  createContent,
  createGroupGenesis,
  createGroupMembership,
  createHiveGenesis,
  createHiveMembership,
  getLatestGroupMembership,
  getLatestMembership,
} from "../ops.js";

export async function run(alice: Agent, bob: Agent, carol: Agent): Promise<void> {
  console.log("\n# hive + group authority");

  // --- check_hive_authority via AclSpec::Public writes ---

  await step("hive genesis author publishes Public content (Path 1)", async () => {
    const hive = await createHiveGenesis(alice, "auth-hive-1");
    const res = await createContent(alice, {
      revision_author_signing_public_key: alice.b64,
      acl_spec: aclSpecPublic(hive.hash, null),
      public_key_acl: emptyAcl(),
      content_type: "humm-addon-text-post-v1",
    });
    assert(res.hash, "Path-1 author write must commit");
  });

  await step("non-member Public write is REJECTED (no Path 2 witness)", async () => {
    const hive = await createHiveGenesis(alice, "auth-hive-2");
    // Bob is not the genesis author and supplies no membership witness.
    // check_hive_authority fetches the genesis (immediately available on
    // the shared conductor) and rejects with the authority message.
    await expectReject(
      createContent(bob, {
        revision_author_signing_public_key: bob.b64,
        acl_spec: aclSpecPublic(hive.hash, null),
        public_key_acl: emptyAcl(),
        content_type: "humm-addon-text-post-v1",
      }),
      "authorising HiveMembership",
    );
  });

  await step("granted hive Writer publishes Public content (Path 2)", async () => {
    const hive = await createHiveGenesis(alice, "auth-hive-3");
    await createHiveMembership(alice, hive.hash, bob.key, "Writer", null, null);
    const mine = await waitFor(() =>
      getLatestMembership(bob, bob.key, hive.hash).then((m) => m ?? null),
    );
    const res = await createContent(bob, {
      revision_author_signing_public_key: bob.b64,
      acl_spec: aclSpecPublic(hive.hash, mine.hash),
      public_key_acl: emptyAcl(),
      content_type: "humm-addon-text-post-v1",
    });
    assert(res.hash, "Path-2 granted Writer write must commit");
  });

  await step("Public write with a DIFFERENT-hive witness is REJECTED", async () => {
    const hiveA = await createHiveGenesis(alice, "auth-hive-4a");
    const hiveB = await createHiveGenesis(alice, "auth-hive-4b");
    await createHiveMembership(alice, hiveB.hash, bob.key, "Writer", null, null);
    const inB = await waitFor(() =>
      getLatestMembership(bob, bob.key, hiveB.hash).then((m) => m ?? null),
    );
    // Bob holds a hive-B witness but claims hive A → check_hive_authority
    // rejects: the membership's hive != the claimed hive.
    await expectReject(
      createContent(bob, {
        revision_author_signing_public_key: bob.b64,
        acl_spec: aclSpecPublic(hiveA.hash, inB.hash),
        public_key_acl: emptyAcl(),
        content_type: "humm-addon-text-post-v1",
      }),
      "claimed hive",
    );
  });

  // --- check_group_authority via AclSpec::HiveGroup writes ---

  await step("group author writes HiveGroup content (Path A)", async () => {
    const hive = await createHiveGenesis(alice, "auth-hive-5");
    const group = await createGroupGenesis(alice, hive.hash, "g-A");
    const res = await createContent(alice, {
      revision_author_signing_public_key: alice.b64,
      acl_spec: aclSpecHiveGroup({
        hiveGenesisHash: hive.hash,
        groupAcl: groupAcl(group.hash),
      }),
      public_key_acl: emptyAcl(),
      content_type: "humm-sidecar-group-message-v1",
    });
    assert(res.hash, "group author (Path A) write must commit");
  });

  await step("hive Admin writes content for a group they did NOT author (Path B)", async () => {
    const hive = await createHiveGenesis(alice, "auth-hive-6");
    await createHiveMembership(alice, hive.hash, bob.key, "Admin", null, null);
    const group = await createGroupGenesis(alice, hive.hash, "g-B");
    const bobHive = await waitFor(() =>
      getLatestMembership(bob, bob.key, hive.hash).then((m) => m ?? null),
    );
    const res = await createContent(bob, {
      revision_author_signing_public_key: bob.b64,
      acl_spec: aclSpecHiveGroup({
        hiveGenesisHash: hive.hash,
        authorMembershipHash: bobHive.hash,
        groupAcl: groupAcl(group.hash),
      }),
      public_key_acl: emptyAcl(),
      content_type: "humm-sidecar-group-message-v1",
    });
    assert(res.hash, "hive Admin (Path B) write must commit");
  });

  await step("granted group Writer writes content (Path C)", async () => {
    // The HiveGroup validator requires BOTH hive Writer+ (step 2) AND
    // per-group Writer+ (step 3). Bob gets a hive Writer membership to
    // clear the hive precheck, then his group authority is proven via
    // Path C (explicit GroupMembership) — NOT Path A (he didn't found
    // the group) and NOT Path B (he is hive Writer, not Admin+).
    const hive = await createHiveGenesis(alice, "auth-hive-7");
    await createHiveMembership(alice, hive.hash, bob.key, "Writer", null, null);
    const group = await createGroupGenesis(alice, hive.hash, "g-C");
    await createGroupMembership(alice, group.hash, bob.key, "Writer", null, null, null);
    const bobHive = await waitFor(() =>
      getLatestMembership(bob, bob.key, hive.hash).then((m) => m ?? null),
    );
    const bobGroup = await waitFor(() =>
      getLatestGroupMembership(bob, bob.key, group.hash).then((m) => m ?? null),
    );
    const res = await createContent(bob, {
      revision_author_signing_public_key: bob.b64,
      acl_spec: aclSpecHiveGroup({
        hiveGenesisHash: hive.hash,
        authorMembershipHash: bobHive.hash,
        groupAcl: groupAcl(group.hash),
        authorGroupMembershipHash: bobGroup.hash,
      }),
      public_key_acl: emptyAcl(),
      content_type: "humm-sidecar-group-message-v1",
    });
    assert(res.hash, "group Writer (Path C) write must commit");
  });

  await step("hive Writer who is NOT a group member is REJECTED", async () => {
    const hive = await createHiveGenesis(alice, "auth-hive-8");
    await createHiveMembership(alice, hive.hash, carol.key, "Writer", null, null);
    const group = await createGroupGenesis(alice, hive.hash, "g-D");
    const carolHive = await waitFor(() =>
      getLatestMembership(carol, carol.key, hive.hash).then((m) => m ?? null),
    );
    // Carol: hive Writer (no Path-B sovereignty), not group author, no
    // group membership → check_group_authority rejects.
    await expectReject(
      createContent(carol, {
        revision_author_signing_public_key: carol.b64,
        acl_spec: aclSpecHiveGroup({
          hiveGenesisHash: hive.hash,
          authorMembershipHash: carolHive.hash,
          groupAcl: groupAcl(group.hash),
        }),
        public_key_acl: emptyAcl(),
        content_type: "humm-sidecar-group-message-v1",
      }),
      "authorising GroupMembership",
    );
  });
}