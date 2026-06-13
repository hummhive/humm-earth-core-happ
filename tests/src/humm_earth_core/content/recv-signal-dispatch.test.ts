/**
 * TR-C7b — proves the single `recv_remote_signal` extern routes each signal
 * family to the right handler via ordered try-decode, AND that the legacy
 * `EncryptedContentSignal` wire shape still decodes unchanged (the
 * regression guard for the C6/C7 envelope addition).
 *
 * Holochain permits exactly ONE `recv_remote_signal` per zome. The
 * dispatcher (`lib::recv_remote_signal`) tries `EncryptedContentSignal`
 * FIRST (the shipped wire shape: `action_type` + `data`), then the new
 * `DmRemoteSignal` envelope (kind-tagged: `DmDeleteRequest` / `DmCall`).
 * The two shapes are structurally disjoint — neither decodes as the other
 * under msgpack — so each payload lands in exactly one handler.
 *
 * These tests assert that disjointness end-to-end at the signal boundary:
 *   - legacy payloads carry `action_type` and have NO `kind`;
 *   - envelope payloads carry `kind` and have NO `action_type`.
 * A regression that made the shapes ambiguous (or reordered the arms) would
 * either mis-route a signal or surface the wrong field set here.
 */
import { assert, expect, test } from "vitest";
import { runScenario, AppSignal, Signal, SignalType } from "@holochain/tryorama";
import { encodeHashToBase64, fakeActionHash } from "@holochain/client";

import {
  sampleCreateEncryptedContentInput,
  EncryptedContentResponse,
} from "./common.js";

const TEST_APP_PATH = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

// Both families ride the same app-signal channel; the predicate must work
// across the legacy (`action_type`/`data`) and envelope (`kind`) shapes.
type AnySignal = Record<string, any>;

/**
 * Shape-agnostic sibling of the `waitForSignal` in `remote-signal.test.ts`.
 * That helper hard-guards `action_type`/`data` (legacy only); here the
 * predicate does all discrimination so legacy AND envelope payloads are
 * matchable. Resolves on the first matching app signal; rejects on timeout.
 */
function waitForSignal(
  player: { appWs: { on: (event: "signal", handler: (s: Signal) => void) => () => void } },
  predicate: (signal: AnySignal) => boolean,
  { timeoutMs = 10_000, label = "signal" }: { timeoutMs?: number; label?: string } = {},
): Promise<AnySignal> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      unlisten();
      reject(new Error(`waitForSignal(${label}): no matching signal within ${timeoutMs}ms`));
    }, timeoutMs);
    const unlisten = player.appWs.on("signal", (signal: Signal) => {
      if (signal.type !== SignalType.App) return;
      const payload = (signal.value as AppSignal).payload as AnySignal;
      if (!payload || typeof payload !== "object") return;
      if (!predicate(payload)) return;
      clearTimeout(timer);
      unlisten();
      resolve(payload);
    });
  });
}

test("EncryptedContentSignal (legacy) routes to handler", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = encodeHashToBase64(alice.agentPubKey);
    const bobB64 = encodeHashToBase64(bob.agentPubKey);

    const bobWait = waitForSignal(
      bob,
      (s) => s.action_type === "Create" && s.data?.encrypted_content?.header?.id === "dispatch-legacy-1",
      { label: "legacy-create" },
    );

    // Bob in the reader ACL → create_encrypted_content remote-signals him
    // with the shipped EncryptedContentSignal shape.
    const input = await sampleCreateEncryptedContentInput({
      header: {
        id: "dispatch-legacy-1",
        public_key_acl: { owner: aliceB64, admin: [], writer: [], reader: [aliceB64, bobB64] },
      },
    }, [], aliceB64);
    const response: EncryptedContentResponse = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "create_encrypted_content",
      payload: input,
    });
    assert.ok(response.hash);

    const sig = await bobWait;
    expect(sig.action_type).toBe("Create");
    expect(sig.data.encrypted_content.header.id).toBe("dispatch-legacy-1");
    // Decoded via the FIRST try-decode arm — this is the legacy shape, NOT
    // the new envelope, so it must carry no `kind` discriminator.
    expect("kind" in sig).toBe(false);
  });
});

test("DmRemoteSignal::DmDeleteRequest routes to its handler", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = encodeHashToBase64(alice.agentPubKey);

    const bobWait = waitForSignal(bob, (s) => s.kind === "DmDeleteRequest", { label: "dm-delete" });

    await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "send_dm_delete_request",
      payload: {
        thread_id: "dispatch-del",
        target_action_hash: await fakeActionHash(),
        recipient: bob.agentPubKey,
      },
    });

    const sig = await bobWait;
    expect(sig.kind).toBe("DmDeleteRequest");
    expect(sig.thread_id).toBe("dispatch-del");
    expect(encodeHashToBase64(sig.from_agent)).toBe(aliceB64);
    // Decoded via the SECOND try-decode arm — disjoint from legacy.
    expect("action_type" in sig).toBe(false);
  });
});

test("DmRemoteSignal::DmCall routes to its handler", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = encodeHashToBase64(alice.agentPubKey);

    const bobWait = waitForSignal(bob, (s) => s.kind === "DmCall", { label: "dm-call" });

    await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "send_dm_call_init_request",
      payload: { call_id: "dispatch-call", recipient: bob.agentPubKey },
    });

    const sig = await bobWait;
    expect(sig.kind).toBe("DmCall");
    expect(sig.type).toBe("InitRequest");
    expect(sig.call_id).toBe("dispatch-call");
    expect(encodeHashToBase64(sig.from_agent)).toBe(aliceB64);
    // Decoded via the SECOND try-decode arm — disjoint from legacy.
    expect("action_type" in sig).toBe(false);
  });
});

test("all three subscriber predicates can coexist on the same player", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = encodeHashToBase64(alice.agentPubKey);
    const bobB64 = encodeHashToBase64(bob.agentPubKey);

    // Three independent predicates registered on Bob BEFORE any send. Each
    // matches exactly one signal family; none can match another's payload.
    const legacyWait = waitForSignal(
      bob,
      (s) => s.action_type === "Create" && s.data?.encrypted_content?.header?.id === "coexist-1",
      { label: "coexist-legacy" },
    );
    const deleteWait = waitForSignal(bob, (s) => s.kind === "DmDeleteRequest", { label: "coexist-delete" });
    const callWait = waitForSignal(
      bob,
      (s) => s.kind === "DmCall" && s.type === "InitRequest",
      { label: "coexist-call" },
    );

    // Fire all three signal kinds from Alice.
    const input = await sampleCreateEncryptedContentInput({
      header: {
        id: "coexist-1",
        public_key_acl: { owner: aliceB64, admin: [], writer: [], reader: [aliceB64, bobB64] },
      },
    }, [], aliceB64);
    const created: EncryptedContentResponse = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "create_encrypted_content",
      payload: input,
    });
    assert.ok(created.hash);

    await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "send_dm_delete_request",
      payload: {
        thread_id: "coexist-del",
        target_action_hash: await fakeActionHash(),
        recipient: bob.agentPubKey,
      },
    });
    await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "send_dm_call_init_request",
      payload: { call_id: "coexist-call", recipient: bob.agentPubKey },
    });

    const [legacySig, deleteSig, callSig] = await Promise.all([legacyWait, deleteWait, callWait]);

    // Legacy arm — action_type/data, no envelope tag.
    expect(legacySig.action_type).toBe("Create");
    expect(legacySig.data.encrypted_content.header.id).toBe("coexist-1");
    expect("kind" in legacySig).toBe(false);

    // Delete arm — kind only, disjoint from legacy.
    expect(deleteSig.kind).toBe("DmDeleteRequest");
    expect(deleteSig.thread_id).toBe("coexist-del");
    expect("action_type" in deleteSig).toBe(false);

    // Call arm — kind + inner type, disjoint from legacy.
    expect(callSig.kind).toBe("DmCall");
    expect(callSig.type).toBe("InitRequest");
    expect(callSig.call_id).toBe("coexist-call");
    expect("action_type" in callSig).toBe(false);

    // from_agent stamped on every envelope variant (C1 anti-spoof).
    expect(encodeHashToBase64(deleteSig.from_agent)).toBe(aliceB64);
    expect(encodeHashToBase64(callSig.from_agent)).toBe(aliceB64);
  });
});
