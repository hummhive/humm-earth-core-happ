/**
 * Verifies that `create_encrypted_content` / `update_encrypted_content` /
 * `delete_encrypted_content` now ALSO send a `send_remote_signal` to
 * every agent listed in the entry's `public_key_acl.reader`. The
 * existing local `emit_signal` behavior is preserved (covered by
 * `encrypted-content.test.ts`); this file pins the new additive
 * behavior plus its backwards-compatibility guarantees.
 *
 * Wire format note: the zome's `Acl::reader` is `Vec<String>` of
 * standard-base64-encoded 39-byte AgentPubKey blobs (3-byte holochain
 * us `agentPubKey: Uint8Array`, which `Buffer.from(...).toString("base64")`
 * encodes the same way the production TS client does.
 */
import { assert, expect, test } from "vitest";
import { runScenario, AppSignal, Signal, SignalType } from "@holochain/tryorama";
import { Buffer } from "node:buffer";

import {
  sampleCreateEncryptedContentInput,
  EncryptedContentResponse,
} from "./common.js";

const TEST_APP_PATH = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

type EncryptedContentSignal = {
  action_type: "Create" | "Update" | "Delete";
  data: EncryptedContentResponse;
};

/**
 * Subscribe to app signals on a player's app websocket and resolve
 * the first signal whose decoded payload matches `predicate`. Rejects
 * on timeout so the test fails fast rather than hanging.
 */
function waitForSignal(
  player: { appWs: { on: (event: "signal", handler: (s: Signal) => void) => () => void } },
  predicate: (signal: EncryptedContentSignal) => boolean,
  { timeoutMs = 10_000, label = "signal" }: { timeoutMs?: number; label?: string } = {},
): Promise<EncryptedContentSignal> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      unlisten();
      reject(new Error(`waitForSignal(${label}): no matching signal within ${timeoutMs}ms`));
    }, timeoutMs);
    const unlisten = player.appWs.on("signal", (signal: Signal) => {
      if (signal.type !== SignalType.App) return;
      const payload = (signal.value as AppSignal).payload as EncryptedContentSignal;
      if (!payload || typeof payload !== "object") return;
      if (!("action_type" in payload) || !("data" in payload)) return;
      if (!predicate(payload)) return;
      clearTimeout(timer);
      unlisten();
      resolve(payload);
    });
  });
}

function encodePubkeyB64(pubkey: Uint8Array): string {
  return Buffer.from(pubkey).toString("base64");
}

test("create_encrypted_content fires remote signal to every public_key_acl.reader", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const bobPubkeyB64 = encodePubkeyB64(bob.agentPubKey);
    const alicePubkeyB64 = encodePubkeyB64(alice.agentPubKey);

    // Subscribe BEFORE the create call so we don't miss the signal.
    const bobSignalWait = waitForSignal(
      bob,
      (s) => s.action_type === "Create" && s.data.encrypted_content.header.id === "remote-signal-test-1",
      { label: "bob-create" },
    );

    const input = await sampleCreateEncryptedContentInput({
      header: {
        id: "remote-signal-test-1",
        public_key_acl: {
          owner: alicePubkeyB64,
          admin: [],
          writer: [],
          reader: [alicePubkeyB64, bobPubkeyB64],
        },
      },
    });
    const response: EncryptedContentResponse = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "create_encrypted_content",
      payload: input,
    });
    assert.ok(response.hash);

    const bobSignal = await bobSignalWait;
    expect(bobSignal.action_type).toBe("Create");
    expect(bobSignal.data.encrypted_content.header.id).toBe("remote-signal-test-1");
  });
});

test("create_encrypted_content with empty public_key_acl.reader does NOT remote-signal anyone (backwards compat)", async () => {
  // Old clients that wrote entries without populating public_key_acl.reader
  // should see exactly pre-change behaviour: local emit_signal fires on
  // the author, nothing fires elsewhere.
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    let bobSignalCount = 0;
    bob.appWs.on("signal", (signal: Signal) => {
      if (signal.type === SignalType.App) bobSignalCount++;
    });

    const input = await sampleCreateEncryptedContentInput({
      header: {
        id: "empty-acl-test-2",
        public_key_acl: {
          owner: encodePubkeyB64(alice.agentPubKey),
          admin: [],
          writer: [],
          reader: [],  // explicitly empty — backwards-compat case
        },
      },
    });
    await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "create_encrypted_content",
      payload: input,
    });

    // Wait a generous window for any unexpected remote signal to land.
    await new Promise((res) => setTimeout(res, 3_000));
    expect(bobSignalCount).toBe(0);
  });
});

test("create_encrypted_content with malformed reader entries skips silently (does not fail commit)", async () => {
  // If `public_key_acl.reader` contains garbage strings (non-base64 or
  // wrong length), the parse step in `remote_signal_acl_readers` filters
  // them out one-by-one. The commit itself MUST still succeed and the
  // local emit_signal MUST still fire on the author.
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const bobPubkeyB64 = encodePubkeyB64(bob.agentPubKey);
    const bobSignalWait = waitForSignal(
      bob,
      (s) => s.data.encrypted_content.header.id === "malformed-mix-test-3",
      { label: "bob-mixed" },
    );

    const input = await sampleCreateEncryptedContentInput({
      header: {
        id: "malformed-mix-test-3",
        public_key_acl: {
          owner: encodePubkeyB64(alice.agentPubKey),
          admin: [],
          writer: [],
          // Mix of valid and garbage. Valid (bob) should still receive.
          // Garbage entries should be silently filtered.
          reader: [
            bobPubkeyB64,
            "not-base64!@#$",       // invalid base64
            "dGVzdA==",              // valid base64 but wrong length (4 bytes, not 36)
            "AAAA",                  // valid base64, wrong length
          ],
        },
      },
    });
    const response = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "create_encrypted_content",
      payload: input,
    });
    assert.ok(response.hash);  // commit succeeded despite garbage

    // Bob still gets his signal — the valid entry in the reader list
    // is processed independently of the malformed ones.
    const sig = await bobSignalWait;
    expect(sig.data.encrypted_content.header.id).toBe("malformed-mix-test-3");
  });
});

test("author is filtered from recipients (no self-loop)", async () => {
  // The author's own pubkey appearing in `public_key_acl.reader` is
  // legitimate (the author is always a reader of their own entry).
  // But `send_remote_signal` from author → author would either bounce
  // or duplicate the local `emit_signal`. Helper filters self out.
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice] = await scenario.addPlayersWithApps([appSource]);
    await scenario.shareAllAgents();

    const alicePubkeyB64 = encodePubkeyB64(alice.agentPubKey);
    let aliceSignalCount = 0;
    alice.appWs.on("signal", (signal: Signal) => {
      if (signal.type === SignalType.App) aliceSignalCount++;
    });

    const input = await sampleCreateEncryptedContentInput({
      header: {
        id: "self-only-test-4",
        public_key_acl: {
          owner: alicePubkeyB64,
          admin: [],
          writer: [],
          reader: [alicePubkeyB64],  // only self
        },
      },
    });
    await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "create_encrypted_content",
      payload: input,
    });

    // Wait long enough for any remote signal to make a round trip.
    await new Promise((res) => setTimeout(res, 3_000));
    // Exactly ONE signal: the local emit_signal. NOT a second one from
    // a self-targeted remote_signal.
    expect(aliceSignalCount).toBe(1);
  });
});
