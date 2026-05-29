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
  AclRole,
  EncryptedContentResponse,
  cellPubkeyB64,
  createEncryptedContent,
  sampleCreateEncryptedContentInput,
  sampleEncryptedContent,
} from "../common.js";

test("create and read EncryptedContent using acl owner link", async () => {
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
      content_type: sampleContent.header.content_type,
      acl_role: AclRole.Owner,
      entity_id: sampleContent.header.acl.owner,
    };
    const createReadOutput: EncryptedContentResponse[] =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "list_by_acl_link",
        payload: listInput,
      });

    assert.deepEqual(sampleContent, createReadOutput[0].encrypted_content);
  });
});

test("create and read EncryptedContent using acl admin link", async () => {
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
    sampleContent.header.acl.admin.push("test-admin-id");
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
      content_type: sampleContent.header.content_type,
      acl_role: AclRole.Admin,
      entity_id: sampleContent.header.acl.admin[0],
    };
    const createReadOutput: EncryptedContentResponse[] =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "list_by_acl_link",
        payload: listInput,
      });

    assert.deepEqual(sampleContent, createReadOutput[0].encrypted_content);
  });
});

test("create and read EncryptedContent using acl writer link", async () => {
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
    sampleContent.header.acl.writer.push("test-writer-id");
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
      content_type: sampleContent.header.content_type,
      acl_role: AclRole.Writer,
      entity_id: sampleContent.header.acl.writer[0],
    };
    const createReadOutput: EncryptedContentResponse[] =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "list_by_acl_link",
        payload: listInput,
      });

    assert.deepEqual(sampleContent, createReadOutput[0].encrypted_content);
  });
});

test("create and read EncryptedContent using acl reader link", async () => {
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
    sampleContent.header.acl.reader.push("test-reader-id");
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
      content_type: sampleContent.header.content_type,
      acl_role: AclRole.Reader,
      entity_id: sampleContent.header.acl.reader[0],
    };
    const createReadOutput: EncryptedContentResponse[] =
      await bob.cells[0].callZome({
        zome_name: "content",
        fn_name: "list_by_acl_link",
        payload: listInput,
      });

    assert.deepEqual(sampleContent, createReadOutput[0].encrypted_content);
  });
});
