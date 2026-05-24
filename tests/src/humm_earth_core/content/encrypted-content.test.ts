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
  encodeHashToBase64,
} from "@holochain/client";
import { decode } from "@msgpack/msgpack";

import {
  EncryptedContentResponse,
  cellPubkeyB64,
  createEncryptedContent,
  sampleCreateEncryptedContentInput,
  sampleEncryptedContent,
} from "./common.js";

test("create EncryptedContent", async () => {
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
    const record = await createEncryptedContent(alice.cells[0]);
    assert.ok(record);
  });
});

test("create and read EncryptedContent", async () => {
  await runScenario(async (scenario) => {
    // Construct proper paths for your app.
    // This assumes app bundle created by the `hc app pack` command.
    const testAppPath = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

    // Set up the app to be installed
    const appBundleSource: AppBundleSource = {
      path: testAppPath,
    };

    // Add 2 players with the test app to the Scenario. The returned players
    // can be destructured.
    const [alice, bob] = await scenario.addPlayersWithApps([
      { appBundleSource },
      { appBundleSource },
    ]);

    const alicePubkeyB64 = cellPubkeyB64(alice.cells[0]);
    const sampleContent = sampleEncryptedContent({}, alicePubkeyB64);
    const sampleInput = await sampleCreateEncryptedContentInput(sampleContent);

    // Alice creates a EncryptedContent
    const record = await createEncryptedContent(alice.cells[0], sampleInput);
    assert.ok(record);

    // Wait for the created entry to be propagated to the other node.
    dhtSync([alice, bob], alice.cells[0].cell_id[0]);
    // dhtSync doesnt work?
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Bob gets the created EncryptedContent
    const createReadOutput: EncryptedContentResponse =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "get_encrypted_content",
        payload: record.hash,
      });
    assert.deepEqual(sampleContent, createReadOutput.encrypted_content);
  });
});

test("create and read EncryptedContent by author link", async () => {
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

    const alicePubkeyB64 = cellPubkeyB64(alice.cells[0]);
    const sampleContent = sampleEncryptedContent({}, alicePubkeyB64);
    const sampleInput = await sampleCreateEncryptedContentInput(sampleContent);

    // Alice creates a EncryptedContent
    const record = await createEncryptedContent(alice.cells[0], sampleInput);
    assert.ok(record);

    // Wait for the created entry to be propagated to the other node.
    dhtSync([alice, bob], alice.cells[0].cell_id[0]);
    // dhtSync doesnt work?
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Bob gets the created EncryptedContent
    const createReadOutput: EncryptedContentResponse[] =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "list_by_author",
        payload: {
          author: encodeHashToBase64(alice.agentPubKey),
          content_type: sampleContent.header.content_type,
        },
      });
    assert.deepEqual(sampleContent, createReadOutput[0].encrypted_content);
  });
});

test("create and update EncryptedContent", async () => {
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
    const record = await createEncryptedContent(alice.cells[0]);
    assert.ok(record);

    const originalActionHash = record.hash;

    // Alice updates the EncryptedContent
    const contentUpdate = sampleEncryptedContent({
      bytes: Buffer.from("test-bytes-2"),
    }, cellPubkeyB64(alice.cells[0]));
    let updateInput = {
      previous_encrypted_content_hash: originalActionHash,
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
    const readUpdatedOutput0: EncryptedContentResponse =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "get_encrypted_content",
        payload: originalActionHash,
      });
    assert.deepEqual(contentUpdate, readUpdatedOutput0.encrypted_content);

    // Alice updates the EncryptedContent again
    const contentUpdate2 = sampleEncryptedContent({
      bytes: Buffer.from("test-bytes-3"),
    }, cellPubkeyB64(alice.cells[0]));

    updateInput = {
      previous_encrypted_content_hash: updatedRecord.hash,
      updated_encrypted_content: contentUpdate2,
    };

    updatedRecord = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "update_encrypted_content",
      payload: updateInput,
    });
    assert.ok(updatedRecord);

    // Wait for the updated entry to be propagated to the other node.
    dhtSync([alice, bob], alice.cells[0].cell_id[0]);
    // dhtSync doesnt work?
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Bob gets the updated EncryptedContent
    const readUpdatedOutput1: EncryptedContentResponse =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "get_encrypted_content",
        payload: originalActionHash,
      });
    assert.deepEqual(contentUpdate2, readUpdatedOutput1.encrypted_content);
  });
});

test("create and delete EncryptedContent", async () => {
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
    const record = await createEncryptedContent(alice.cells[0]);
    assert.ok(record);

    // Alice deletes the EncryptedContent
    const deleteActionHash = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "delete_encrypted_content",
      payload: record.hash,
    });
    assert.ok(deleteActionHash);

    // Wait for the entry deletion to be propagated to the other node.
    dhtSync([alice, bob], alice.cells[0].cell_id[0]);
    // dhtSync doesnt work?
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Bob tries to get the deleted EncryptedContent
    await expect(
      async () =>
        await bob.cells[0].callZome({
          zome_name: "content",
          fn_name: "get_encrypted_content",
          payload: record.hash,
        })
    ).rejects.toThrow();
  });
});

test("create, update, and delete EncryptedContent using original hash", async () => {
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
    const record = await createEncryptedContent(alice.cells[0]);
    assert.ok(record);

    // Alice updates the EncryptedContent
    const contentUpdate = sampleEncryptedContent({
      bytes: Buffer.from("test-bytes-2"),
    }, cellPubkeyB64(alice.cells[0]));
    let updateInput = {
      previous_encrypted_content_hash: record.hash,
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
    const readUpdatedOutput0: EncryptedContentResponse =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "get_encrypted_content",
        payload: record.hash,
      });
    assert.deepEqual(contentUpdate, readUpdatedOutput0.encrypted_content);

    // Alice deletes the EncryptedContent
    const deleteActionHash = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "delete_encrypted_content",
      payload: record.hash,
    });
    assert.ok(deleteActionHash);

    // Wait for the entry deletion to be propagated to the other node.
    dhtSync([alice, bob], alice.cells[0].cell_id[0]);
    // dhtSync doesnt work?
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Bob tries to get the deleted EncryptedContent using the original hash
    await expect(
      async () =>
        await bob.cells[0].callZome({
          zome_name: "content",
          fn_name: "get_encrypted_content",
          payload: record.hash,
        })
    ).rejects.toThrow();

    // Bob tries to get the deleted EncryptedContent using the updated hash
    await expect(
      async () =>
        await bob.cells[0].callZome({
          zome_name: "content",
          fn_name: "get_encrypted_content",
          payload: readUpdatedOutput0.hash,
        })
    ).rejects.toThrow();
  });
});

test("create, update, and delete EncryptedContent using updated hash", async () => {
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
    const record = await createEncryptedContent(alice.cells[0]);
    assert.ok(record);

    // Alice updates the EncryptedContent
    const contentUpdate = sampleEncryptedContent({
      bytes: Buffer.from("test-bytes-2"),
    }, cellPubkeyB64(alice.cells[0]));
    let updateInput = {
      previous_encrypted_content_hash: record.hash,
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
    const readUpdatedOutput0: EncryptedContentResponse =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "get_encrypted_content",
        payload: record.hash,
      });
    assert.deepEqual(contentUpdate, readUpdatedOutput0.encrypted_content);

    // Alice deletes the EncryptedContent
    const deleteActionHash = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "delete_encrypted_content",
      payload: readUpdatedOutput0.hash,
    });
    assert.ok(deleteActionHash);

    // Wait for the entry deletion to be propagated to the other node.
    dhtSync([alice, bob], alice.cells[0].cell_id[0]);
    // dhtSync doesnt work?
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Bob tries to get the deleted EncryptedContent using the original hash
    await expect(
      async () =>
        await bob.cells[0].callZome({
          zome_name: "content",
          fn_name: "get_encrypted_content",
          payload: record.hash,
        })
    ).rejects.toThrow();

    // Bob tries to get the deleted EncryptedContent using the updated hash
    await expect(
      async () =>
        await bob.cells[0].callZome({
          zome_name: "content",
          fn_name: "get_encrypted_content",
          payload: readUpdatedOutput0.hash,
        })
    ).rejects.toThrow();
  });
});
