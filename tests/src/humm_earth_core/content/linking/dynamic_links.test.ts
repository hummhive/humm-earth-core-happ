import { assert, expect, test } from "vitest";

import { runScenario, dhtSync, CallableCell } from "@holochain/tryorama";
import {
  NewEntryAction,
  ActionHash,
  Record,
  AppBundleSource,
  fakeDnaHash,
  fakeActionHash,
  fakeAgentPubKey,
  fakeEntryHash,
} from "@holochain/client";
import { decode, encode } from "@msgpack/msgpack";

import {
  EncryptedContentResponse,
  cellPubkeyB64,
  createEncryptedContent,
  sampleCreateEncryptedContentInput,
  sampleEncryptedContent,
} from "../common.js";

test("create and read EncryptedContent using dynamic link", async () => {
  await runScenario(async (scenario) => {
    // Construct proper paths for your app.
    // This assumes app bundle created by the `hc app pack` command.
    const testAppPath = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

    // Set up the app to be installed
    const appSource = { appBundleSource: { type: "path" as const, value: testAppPath } };

    // Add 2 players with the test app to the Scenario. The returned players
    // can be destructured.
    const [alice, bob] = await scenario.addPlayersWithApps([
      appSource,
      appSource,
    ]);

    // Shortcut peer discovery through gossip and register all agents in every
    // conductor of the scenario.
    await scenario.shareAllAgents();

    // Alice creates a EncryptedContent
    const sampleContent = sampleEncryptedContent({}, cellPubkeyB64(alice.cells[0]));
    const sampleInput = await sampleCreateEncryptedContentInput(sampleContent, [
      "test-dynamic-link",
    ]);

    const record = await createEncryptedContent(alice.cells[0], sampleInput);
    assert.ok(record);

    // Wait for the created entry to be propagated to the other node.
    await dhtSync([alice, bob], alice.cells[0].cell_id[0]);

    // Bob gets the created EncryptedContent
    const listInput = {
      hive_id: sampleContent.header.hive_id,
      content_type: sampleContent.header.content_type,
      dynamic_link: "test-dynamic-link",
    };
    const createReadOutput: EncryptedContentResponse[] =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "list_by_dynamic_link",
        payload: listInput,
      });

    assert.deepEqual(sampleContent, createReadOutput[0].encrypted_content);
  });
});

test("create, update, and read EncryptedContent using dynamic link", async () => {
  await runScenario(async (scenario) => {
    // Construct proper paths for your app.
    // This assumes app bundle created by the `hc app pack` command.
    const testAppPath = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

    // Set up the app to be installed
    const appSource = { appBundleSource: { type: "path" as const, value: testAppPath } };

    // Add 2 players with the test app to the Scenario. The returned players
    // can be destructured.
    const [alice, bob] = await scenario.addPlayersWithApps([
      appSource,
      appSource,
    ]);

    // Shortcut peer discovery through gossip and register all agents in every
    // conductor of the scenario.
    await scenario.shareAllAgents();

    // Alice creates a EncryptedContent
    const sampleContent = sampleEncryptedContent({}, cellPubkeyB64(alice.cells[0]));
    const sampleInput = await sampleCreateEncryptedContentInput(sampleContent, [
      "test-dynamic-link",
    ]);

    const record = await createEncryptedContent(alice.cells[0], sampleInput);
    assert.ok(record);

    // Wait for the created entry to be propagated to the other node.
    await dhtSync([alice, bob], alice.cells[0].cell_id[0]);

    // Bob gets the created EncryptedContent
    const listInput = {
      hive_id: sampleContent.header.hive_id,
      content_type: sampleContent.header.content_type,
      dynamic_link: "test-dynamic-link",
    };
    const createReadOutput: EncryptedContentResponse[] =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "list_by_dynamic_link",
        payload: listInput,
      });

    assert.deepEqual(sampleContent, createReadOutput[0].encrypted_content);

    const contentUpdate = sampleEncryptedContent({
      bytes: Buffer.from("test-bytes-2"),
    }, cellPubkeyB64(alice.cells[0]));
    let updateInput = {
      previous_encrypted_content_hash: createReadOutput[0].hash,
      updated_encrypted_content: contentUpdate,
    };

    let updatedRecord: EncryptedContentResponse = await alice.cells[0].callZome(
      {
        zome_name: "content",
        fn_name: "update_encrypted_content",
        payload: updateInput,
      }
    );
    assert.ok(updatedRecord);

    // Wait for the updated entry to be propagated to the other node.
    await dhtSync([alice, bob], alice.cells[0].cell_id[0]);

    // Bob gets the updated EncryptedContent
    const readUpdatedOutput0: EncryptedContentResponse[] =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "list_by_dynamic_link",
        payload: listInput,
      });
    assert.deepEqual(contentUpdate, readUpdatedOutput0[0].encrypted_content);
  });
});
