/**
 * TR-C6 — proves `send_dm_delete_request` delivers an ephemeral
 * `DmRemoteSignal::DmDeleteRequest` cross-host to a single recipient, and
 * that the receiver's `recv_remote_signal` dispatcher stamps `from_agent`
 * from the conductor-attested provenance.
 *
 * The C1 anti-spoof guarantee carried through the C6 envelope: the sender
 * puts `from_agent: None` on the wire (see `send_dm_delete_request`); the
 * receiver overwrites it with `call_info()?.provenance` — the lair-attested
 * caller pubkey — before re-emitting locally. So a peer cannot forge the
 * "from" identity in the signal Bob's UI sees.
 *
 * C6 is fire-and-forget and the zome NEVER validates `target_action_hash`
 * (the receiver's UI decides whether to honor the request), so these tests
 * fabricate a hash via `fakeActionHash()`.
 *
 * Wire shape (msgpack, named) of the signal Bob's handler receives:
 *   { kind: "DmDeleteRequest",
 *     thread_id: <string>,
 *     target_action_hash: <Uint8Array>,
 *     from_agent: <AgentPubKey bytes> }   // stamped on arrival
 */
import { expect, test } from "vitest";
import { runScenario, AppSignal, Signal, SignalType } from "@holochain/tryorama";
import { encodeHashToBase64, fakeActionHash } from "@holochain/client";

const TEST_APP_PATH = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

// The C6/C7 envelope arrives as a `kind`-tagged map, NOT the legacy
// `action_type`/`data` shape — so the predicate discriminates structurally.
type AnySignal = Record<string, any>;

/**
 * Shape-agnostic sibling of the `waitForSignal` in `remote-signal.test.ts`.
 * That helper hard-guards `action_type`/`data` (the legacy shape) and would
 * silently drop every `DmRemoteSignal`; here the predicate does all the
 * discrimination so any payload shape can be matched. Resolves on the first
 * matching app signal; rejects on timeout so the test fails fast.
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

test("bob receives a DmRemoteSignal::DmDeleteRequest with from_agent stamped to alice", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = encodeHashToBase64(alice.agentPubKey);

    // Subscribe BEFORE the send so we never miss the ephemeral signal.
    const bobWait = waitForSignal(
      bob,
      (s) => s.kind === "DmDeleteRequest" && s.thread_id === "thread-1",
      { label: "bob-dm-delete" },
    );

    // The zome does not validate this hash — C6 is ephemeral metadata only.
    const targetActionHash = await fakeActionHash();
    await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "send_dm_delete_request",
      payload: {
        thread_id: "thread-1",
        target_action_hash: targetActionHash,
        recipient: bob.agentPubKey,
      },
    });

    const sig = await bobWait;
    expect(sig.kind).toBe("DmDeleteRequest");
    expect(sig.thread_id).toBe("thread-1");
    // from_agent is the receiver-stamped provenance, NOT a wire-supplied value.
    expect(encodeHashToBase64(sig.from_agent)).toBe(aliceB64);
  });
});

test("multiple delete requests for different threads route independently", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = encodeHashToBase64(alice.agentPubKey);

    // Two thread-specific predicates registered up front; each resolves on
    // its own thread_id and ignores the other's signal (no cross-talk).
    const waitA = waitForSignal(
      bob,
      (s) => s.kind === "DmDeleteRequest" && s.thread_id === "a",
      { label: "thread-a" },
    );
    const waitB = waitForSignal(
      bob,
      (s) => s.kind === "DmDeleteRequest" && s.thread_id === "b",
      { label: "thread-b" },
    );

    const hashA = await fakeActionHash();
    const hashB = await fakeActionHash();

    await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "send_dm_delete_request",
      payload: { thread_id: "a", target_action_hash: hashA, recipient: bob.agentPubKey },
    });
    await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "send_dm_delete_request",
      payload: { thread_id: "b", target_action_hash: hashB, recipient: bob.agentPubKey },
    });

    const [sigA, sigB] = await Promise.all([waitA, waitB]);

    expect(sigA.kind).toBe("DmDeleteRequest");
    expect(sigA.thread_id).toBe("a");
    expect(encodeHashToBase64(sigA.from_agent)).toBe(aliceB64);

    expect(sigB.kind).toBe("DmDeleteRequest");
    expect(sigB.thread_id).toBe("b");
    expect(encodeHashToBase64(sigB.from_agent)).toBe(aliceB64);
  });
});
