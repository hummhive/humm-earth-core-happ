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

test("create and read EncryptedContent using content id link", async () => {
  await runScenario(async (scenario) => {
    // Construct proper paths for your app.
    // This assumes app bundle created by the `hc app pack` command.
    const testAppPath = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

    // Set up the app to be installed
    const appSource = { appBundleSource: { path: testAppPath } };

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
    const sampleInput = await sampleCreateEncryptedContentInput(sampleContent);
    const record = await createEncryptedContent(alice.cells[0], sampleInput);
    assert.ok(record);

    // Wait for the created entry to be propagated to the other node.
    dhtSync([alice, bob], alice.cells[0].cell_id[0]);
    // dhtSync doesnt work?
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Bob gets the created EncryptedContent
    const listInput = {
      hive_id: sampleContent.header.hive_id,
      content_id: sampleContent.header.id,
    };
    const createReadOutput: EncryptedContentResponse =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "get_by_content_id_link",
        payload: listInput,
      });

    assert.deepEqual(sampleContent, createReadOutput.encrypted_content);
  });
});

test("create, update, and read EncryptedContent using hive link", async () => {
  await runScenario(async (scenario) => {
    // Construct proper paths for your app.
    // This assumes app bundle created by the `hc app pack` command.
    const testAppPath = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

    // Set up the app to be installed
    const appSource = { appBundleSource: { path: testAppPath } };

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
    const sampleInput = await sampleCreateEncryptedContentInput(sampleContent);

    const record = await createEncryptedContent(alice.cells[0], sampleInput);
    assert.ok(record);

    // Wait for the created entry to be propagated to the other node.
    dhtSync([alice, bob], alice.cells[0].cell_id[0]);
    // dhtSync doesnt work?
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Bob gets the created EncryptedContent
    const listInput = {
      hive_id: sampleContent.header.hive_id,
      content_id: sampleContent.header.id,
    };
    const createReadOutput: EncryptedContentResponse =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "get_by_content_id_link",
        payload: listInput,
      });

    assert.deepEqual(sampleContent, createReadOutput.encrypted_content);

    const contentUpdate = sampleEncryptedContent({
      bytes: Buffer.from("test-bytes-2"),
    }, cellPubkeyB64(alice.cells[0]));
    let updateInput = {
      previous_encrypted_content_hash: createReadOutput.hash,
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
    dhtSync([alice, bob], alice.cells[0].cell_id[0]);
    // dhtSync doesnt work?
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Bob gets the updated EncryptedContent
    const readOutput2: EncryptedContentResponse = await bob.cells[0].callZome({
      zome_name: "content",
      fn_name: "get_by_content_id_link",
      payload: listInput,
    });
    assert.deepEqual(contentUpdate, readOutput2.encrypted_content);
  });
});
