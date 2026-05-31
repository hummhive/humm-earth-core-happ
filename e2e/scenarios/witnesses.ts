/**
 * Pass-4 headline coverage:
 *  - G-6.2 recipient-witness verification on AclSpec::HiveGroup
 *    (the closure for attack #5 — recipient-list forgery).
 *  - G-4.4 grant-window containment at BOTH the group layer (pass-3)
 *    and the hive layer (pass-4 back-port).
 *
 * These are the fetch-dependent branches that host-side `cargo test`
 * cannot reach (each needs `must_get_valid_record` against a real
 * GroupMembership / HiveMembership on the DHT).
 */
import { type Agent, waitFor } from "../conductor.js";
import {
  aclSpecHiveGroup,
  assert,
  emptyAcl,
  expectReject,
  groupAcl,
  PAST_MICROS,
  readerAcl,
  step,
  witness,
} from "../acl.js";
import {
  createContent,
  createGroupGenesis,
  createGroupMembership,
  createHiveGenesis,
  createHiveMembership,
  getLatestGroupMembership,
  getLatestMembership,
  listGroupMembers,
} from "../ops.js";

export async function run(alice: Agent, bob: Agent, carol: Agent): Promise<void> {
  console.log("\n# G-6.2 recipient witnesses + G-4.4 grant windows");

  // ---- G-6.2 recipient witnesses ----

  await step("witnessed HiveGroup write commits (every PKA pubkey backed)", async () => {
    const hive = await createHiveGenesis(alice, "w-hive-1");
    const group = await createGroupGenesis(alice, hive.hash, "w-g-1");
    await createGroupMembership(alice, group.hash, bob.key, "Reader", null, null, null);
    const bobMem = await waitFor(() =>
      getLatestGroupMembership(alice, bob.key, group.hash).then((m) => m ?? null),
    );
    const res = await createContent(alice, {
      revision_author_signing_public_key: alice.b64,
      acl_spec: aclSpecHiveGroup({
        hiveGenesisHash: hive.hash,
        groupAcl: groupAcl(group.hash),
        recipientWitnesses: [witness(bob.key, "Reader", bobMem.hash)],
      }),
      public_key_acl: readerAcl([bob.key]),
      content_type: "humm-sidecar-group-message-v1",
    });
    assert(res.hash, "fully-witnessed write must commit");
    // The cryptographic roster (humm-tauri's source of truth for "who
    // is in this group") must surface bob.
    const roster = await waitFor(() =>
      listGroupMembers(alice, group.hash).then((m) => (m.length > 0 ? m : null)),
    );
    assert(
      roster.some((m) => m.membership.for_agent.toString() === bob.key.toString()),
      "list_group_members must include the granted member",
    );
  });

  await step("HiveGroup write REJECTED when a PKA pubkey has no witness (attack #5)", async () => {
    const hive = await createHiveGenesis(alice, "w-hive-2");
    const group = await createGroupGenesis(alice, hive.hash, "w-g-2");
    // bob is listed in public_key_acl.reader but NO witness is stamped —
    // the modified-coordinator recipient-list forgery. Must reject.
    await expectReject(
      createContent(alice, {
        revision_author_signing_public_key: alice.b64,
        acl_spec: aclSpecHiveGroup({
          hiveGenesisHash: hive.hash,
          groupAcl: groupAcl(group.hash),
          recipientWitnesses: [],
        }),
        public_key_acl: readerAcl([bob.key]),
        content_type: "humm-sidecar-group-message-v1",
      }),
      "not backed by any dominating recipient_witness",
    );
  });

  await step("HiveGroup write REJECTED on witness over-claim (witness not in PKA)", async () => {
    const hive = await createHiveGenesis(alice, "w-hive-3");
    const group = await createGroupGenesis(alice, hive.hash, "w-g-3");
    await createGroupMembership(alice, group.hash, bob.key, "Reader", null, null, null);
    const bobMem = await waitFor(() =>
      getLatestGroupMembership(alice, bob.key, group.hash).then((m) => m ?? null),
    );
    // PKA lists only bob; an extra witness over-claims carol (carol not
    // in PKA.reader). The bidirectional check rejects pre-fetch, so the
    // bogus carol membership_hash (reusing bob's) never gets fetched.
    await expectReject(
      createContent(alice, {
        revision_author_signing_public_key: alice.b64,
        acl_spec: aclSpecHiveGroup({
          hiveGenesisHash: hive.hash,
          groupAcl: groupAcl(group.hash),
          recipientWitnesses: [
            witness(bob.key, "Reader", bobMem.hash),
            witness(carol.key, "Reader", bobMem.hash),
          ],
        }),
        public_key_acl: readerAcl([bob.key]),
        content_type: "humm-sidecar-group-message-v1",
      }),
      "not in public_key_acl",
    );
  });

  await step("bucket dominance: an Admin-role member backs a Reader PKA entry", async () => {
    const hive = await createHiveGenesis(alice, "w-hive-4");
    const group = await createGroupGenesis(alice, hive.hash, "w-g-4");
    // bob holds group Admin; he is stamped as a READER-bucket witness.
    // Admin role satisfies the Reader requirement (role dominance), and
    // the group sits in the owner bucket which the Reader bucket accepts.
    await createGroupMembership(alice, group.hash, bob.key, "Admin", null, null, null);
    const bobMem = await waitFor(() =>
      getLatestGroupMembership(alice, bob.key, group.hash).then((m) => m ?? null),
    );
    const res = await createContent(alice, {
      revision_author_signing_public_key: alice.b64,
      acl_spec: aclSpecHiveGroup({
        hiveGenesisHash: hive.hash,
        groupAcl: groupAcl(group.hash),
        recipientWitnesses: [witness(bob.key, "Reader", bobMem.hash)],
      }),
      public_key_acl: readerAcl([bob.key]),
      content_type: "humm-sidecar-group-message-v1",
    });
    assert(res.hash, "Admin-role member as Reader witness must commit");
  });

  await step("HiveGroup write REJECTED when the witness membership is expired", async () => {
    const hive = await createHiveGenesis(alice, "w-hive-5");
    const group = await createGroupGenesis(alice, hive.hash, "w-g-5");
    // Grant bob a Reader membership that is ALREADY expired, capturing
    // its hash directly (get_latest_group_membership would filter it).
    const expired = await createGroupMembership(
      alice,
      group.hash,
      bob.key,
      "Reader",
      null,
      null,
      PAST_MICROS,
    );
    await expectReject(
      createContent(alice, {
        revision_author_signing_public_key: alice.b64,
        acl_spec: aclSpecHiveGroup({
          hiveGenesisHash: hive.hash,
          groupAcl: groupAcl(group.hash),
          recipientWitnesses: [witness(bob.key, "Reader", expired.hash)],
        }),
        public_key_acl: readerAcl([bob.key]),
        content_type: "humm-sidecar-group-message-v1",
      }),
      "expired",
    );
  });

  // ---- G-4.4 grant-window containment (group layer, pass-3) ----

  await step("group G-4.4: expiring Admin grantor cannot mint a permanent membership", async () => {
    const { group, bobAdminHash } = await expiringGroupAdmin(alice, bob, "w-hive-6");
    // bob (group Admin via an EXPIRING Path-C membership) grants carol a
    // PERMANENT membership → rejected.
    await expectReject(
      createGroupMembership(bob, group, carol.key, "Writer", bobAdminHash, null, null),
      "permanent",
    );
  });

  await step("group G-4.4: expiring grantor cannot extend the window", async () => {
    const { group, bobAdminHash, expiry } = await expiringGroupAdmin(alice, bob, "w-hive-7");
    await expectReject(
      createGroupMembership(bob, group, carol.key, "Writer", bobAdminHash, null, expiry + 1_000_000),
      "exceeds",
    );
  });

  await step("group G-4.4: a grant within the window is allowed", async () => {
    const { group, bobAdminHash, expiry } = await expiringGroupAdmin(alice, bob, "w-hive-8");
    const res = await createGroupMembership(
      bob,
      group,
      carol.key,
      "Writer",
      bobAdminHash,
      null,
      expiry,
    );
    assert(res.hash, "grant at-or-before the grantor window must commit");
  });

  // ---- G-4.4 grant-window containment (hive layer, pass-4 back-port) ----

  await step("hive G-4.4: expiring Admin grantor cannot mint a permanent membership", async () => {
    const { hive, bobAdminHash } = await expiringHiveAdmin(alice, bob, "w-hive-9");
    await expectReject(
      createHiveMembership(bob, hive, carol.key, "Writer", bobAdminHash, null),
      "permanent",
    );
  });

  await step("hive G-4.4: expiring grantor cannot extend the window", async () => {
    const { hive, bobAdminHash, expiry } = await expiringHiveAdmin(alice, bob, "w-hive-10");
    await expectReject(
      createHiveMembership(bob, hive, carol.key, "Writer", bobAdminHash, expiry + 1_000_000),
      "exceeds",
    );
  });

  await step("hive G-4.4: a grant within the window is allowed", async () => {
    const { hive, bobAdminHash, expiry } = await expiringHiveAdmin(alice, bob, "w-hive-11");
    const res = await createHiveMembership(bob, hive, carol.key, "Writer", bobAdminHash, expiry);
    assert(res.hash, "hive grant at-or-before the grantor window must commit");
  });
}



/** A finite, still-valid expiry ~1000s in the future (micros). */
function windowExpiry(): number {
  return Date.now() * 1000 + 1_000_000_000;
}

/** Found a hive+group and grant bob an EXPIRING group-Admin membership
 * (Path C). Returns the group hash + bob's membership hash + the expiry
 * so the caller can probe the grant-window boundary. */
async function expiringGroupAdmin(alice: Agent, bob: Agent, hiveId: string) {
  const expiry = windowExpiry();
  const hive = await createHiveGenesis(alice, hiveId);
  const group = await createGroupGenesis(alice, hive.hash, `${hiveId}-g`);
  await createGroupMembership(alice, group.hash, bob.key, "Admin", null, null, expiry);
  const bobAdmin = await waitFor(() =>
    getLatestGroupMembership(bob, bob.key, group.hash).then((m) => m ?? null),
  );
  return { group: group.hash, bobAdminHash: bobAdmin.hash, expiry };
}

/** Found a hive and grant bob an EXPIRING hive-Admin membership (Path 2). */
async function expiringHiveAdmin(alice: Agent, bob: Agent, hiveId: string) {
  const expiry = windowExpiry();
  const hive = await createHiveGenesis(alice, hiveId);
  await createHiveMembership(alice, hive.hash, bob.key, "Admin", null, expiry);
  const bobAdmin = await waitFor(() =>
    getLatestMembership(bob, bob.key, hive.hash).then((m) => m ?? null),
  );
  return { hive: hive.hash, bobAdminHash: bobAdmin.hash, expiry };
}
