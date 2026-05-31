/**
 * Pre-signed invite-link flow (E.4.l) — end-to-end composition of
 * pass-3 primitives, no new DNA surface. Proves the Discord-style
 * one-click-join flow humm-tauri will build works against the real
 * conductor:
 *   1. Alice (hive Writer+) publishes a Public invite entry.
 *   2. Bob (outsider, no membership) reads it — Public is world-readable.
 *   3. Bob publishes an OpenWrite redemption targeting Alice's hive.
 *   4. Alice mints Bob's HiveMembership (the accept step).
 *   5. Bob's get_latest_membership surfaces the new hive membership.
 */
import { type Agent, waitFor } from "../conductor.js";
import { aclSpecOpenWrite, aclSpecPublic, assert, emptyAcl, step } from "../acl.js";
import {
  createContent,
  createHiveGenesis,
  createHiveMembership,
  getEncryptedContent,
  getLatestMembership,
} from "../ops.js";

export async function run(alice: Agent, bob: Agent, _carol: Agent): Promise<void> {
  console.log("\n# pre-signed invite links (E.4.l end-to-end)");

  await step("invite link: publish → outsider read → redeem → mint → joined", async () => {
    // 1. Alice publishes the pre-signed invite as Public content.
    const hive = await createHiveGenesis(alice, "invite-hive");
    const invite = await createContent(alice, {
      id: "invite-1",
      revision_author_signing_public_key: alice.b64,
      acl_spec: aclSpecPublic(hive.hash, null),
      public_key_acl: emptyAcl(),
      content_type: "hummhive-core-pre-signed-invite-v1",
      bytes: Buffer.from(
        JSON.stringify({ intended_role: "Writer", expiry: null, max_uses: 1 }),
      ),
    });
    assert(invite.hash, "Alice must publish the Public invite");

    // 2. Bob (no hive membership) reads the world-readable invite.
    const seen = await waitFor(() =>
      getEncryptedContent(bob, invite.hash).then((c) => c ?? null),
    );
    assert(seen, "outsider Bob must be able to read the Public invite");

    // 3. Bob publishes an OpenWrite redemption targeting Alice's hive.
    //    Bob holds NO hive membership — OpenWrite only checks author
    //    identity + that the target HiveGenesis resolves.
    const redemption = await createContent(bob, {
      revision_author_signing_public_key: bob.b64,
      acl_spec: aclSpecOpenWrite(hive.hash),
      public_key_acl: emptyAcl(),
      content_type: "hummhive-core-invite-redemption-v1",
      bytes: Buffer.from(JSON.stringify({ invite_action_hash: "…", opaque_token: "…" })),
    });
    assert(redemption.hash, "Bob must publish the OpenWrite redemption");

    // 4. Alice (hive owner) processes the redemption and mints Bob's
    //    HiveMembership — the accept step.
    const minted = await createHiveMembership(alice, hive.hash, bob.key, "Writer", null, null);
    assert(minted.hash, "Alice must mint Bob's membership");

    // 5. Bob now surfaces the hive via his own membership lookup.
    const bobMembership = await waitFor(() =>
      getLatestMembership(bob, bob.key, hive.hash).then((m) => m ?? null),
    );
    assert(bobMembership, "Bob must now hold a hive membership (joined)");

    // 6. And Bob can now author Public content under the hive (Path 2),
    //    proving the membership is live + usable.
    const post = await createContent(bob, {
      revision_author_signing_public_key: bob.b64,
      acl_spec: aclSpecPublic(hive.hash, bobMembership.hash),
      public_key_acl: emptyAcl(),
      content_type: "humm-addon-text-post-v1",
    });
    assert(post.hash, "newly-joined Bob must be able to publish under the hive");
  });
}
