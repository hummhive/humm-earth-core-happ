/**
 * Integrity-zome cross-check: `revision_author_signing_public_key` MUST
 * equal `action.author` for every `EncryptedContent` create and update.
 *
 * Pre-fix history (no-op `validate_create_encrypted_content`): the field
 * was sender-controlled, so any peer could write an entry claiming
 * another agent's signing public key. DM identification, member entries,
 * and audit trails downstream all trust this field, so a forged entry
 * impersonated whoever the recipient trusted.
 *
 * Fix: the integrity zome rejects creates/updates whose entry header
 * does not match the cryptographically-attested action author. The
 * coordinator stays a passthrough; the integrity zome is the load-bearing
 * guard because a malicious node running a custom DNA bypasses the
 * coordinator entirely.
 */
import { assert, expect, test } from "vitest";

import { runScenario } from "@holochain/tryorama";
import { encodeHashToBase64 } from "@holochain/client";

import {
  cellPubkeyB64,
  sampleCreateEncryptedContentInput,
  sampleEncryptedContent,
  type EncryptedContentResponse,
} from "./common.js";

const TEST_APP_PATH = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

test("create_encrypted_content REJECTS a forged revision_author_signing_public_key", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const bobPubkeyB64 = encodeHashToBase64(bob.agentPubKey);

    // Alice attempts to write an entry whose header claims bob's signing
    // public key. The action is still signed by alice's lair, so
    // `action.author == alice_pk` but the header says `bob_pk`. The
    // integrity zome MUST reject.
    const forgedInput = await sampleCreateEncryptedContentInput(
      { header: { id: "forged-create" } },
      [],
      bobPubkeyB64,
    );

    await expect(
      alice.cells[0].callZome({
        zome_name: "content",
        fn_name: "create_encrypted_content",
        payload: forgedInput,
      }),
    ).rejects.toThrow(/revision_author_signing_public_key/);
  });
});

test("create_encrypted_content ACCEPTS a header pubkey matching action.author", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice] = await scenario.addPlayersWithApps([appSource]);
    await scenario.shareAllAgents();

    const alicePubkeyB64 = cellPubkeyB64(alice.cells[0]);

    const legitInput = await sampleCreateEncryptedContentInput(
      { header: { id: "legit-create" } },
      [],
      alicePubkeyB64,
    );

    const response: EncryptedContentResponse = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "create_encrypted_content",
      payload: legitInput,
    });
    assert.ok(response.hash);
    expect(
      response.encrypted_content.header.revision_author_signing_public_key,
    ).toBe(alicePubkeyB64);
  });
});

test("update_encrypted_content REJECTS a forged revision_author_signing_public_key", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const alicePubkeyB64 = cellPubkeyB64(alice.cells[0]);
    const bobPubkeyB64 = encodeHashToBase64(bob.agentPubKey);

    const legitInput = await sampleCreateEncryptedContentInput(
      { header: { id: "legit-create-then-forged-update" } },
      [],
      alicePubkeyB64,
    );
    const original: EncryptedContentResponse = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "create_encrypted_content",
      payload: legitInput,
    });
    assert.ok(original.hash);

    // Alice updates honestly but the updated entry claims bob's pubkey.
    const forgedUpdate = sampleEncryptedContent(
      { bytes: Buffer.from("test-bytes-2") },
      bobPubkeyB64,
    );
    await expect(
      alice.cells[0].callZome({
        zome_name: "content",
        fn_name: "update_encrypted_content",
        payload: {
          previous_encrypted_content_hash: original.hash,
          updated_encrypted_content: forgedUpdate,
        },
      }),
    ).rejects.toThrow(/revision_author_signing_public_key/);
  });
});

test("update_encrypted_content ACCEPTS an honest update", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice] = await scenario.addPlayersWithApps([appSource]);
    await scenario.shareAllAgents();

    const alicePubkeyB64 = cellPubkeyB64(alice.cells[0]);

    const legitInput = await sampleCreateEncryptedContentInput(
      { header: { id: "legit-create-then-legit-update" } },
      [],
      alicePubkeyB64,
    );
    const original: EncryptedContentResponse = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "create_encrypted_content",
      payload: legitInput,
    });
    assert.ok(original.hash);

    const legitUpdate = sampleEncryptedContent(
      { bytes: Buffer.from("test-bytes-2") },
      alicePubkeyB64,
    );
    const updated: EncryptedContentResponse = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "update_encrypted_content",
      payload: {
        previous_encrypted_content_hash: original.hash,
        updated_encrypted_content: legitUpdate,
      },
    });
    assert.ok(updated.hash);
  });
});
