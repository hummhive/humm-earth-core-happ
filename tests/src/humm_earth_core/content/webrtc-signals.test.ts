/**
 * TR-C7 — proves all three WebRTC-signalling externs round-trip end-to-end:
 *   - `send_dm_call_init_request`  (announce a call)
 *   - `send_dm_call_init_accept`   (accept a call)
 *   - `send_dm_call_sdp_data`      (forward an opaque SDP / ICE blob)
 *
 * Each delivers a `DmRemoteSignal::DmCall(DmCallSignal::*)` to a single
 * recipient. The outer envelope is `#[serde(tag = "kind")]` and the inner
 * `DmCallSignal` is `#[serde(tag = "type")]`, so the flattened wire map the
 * recipient's handler sees is:
 *   { kind: "DmCall", type: "InitRequest" | "InitAccept" | "SdpData",
 *     call_id: <string>, [data: <string>,] from_agent: <AgentPubKey bytes> }
 *
 * `from_agent` is stamped by the receiver from `call_info()?.provenance`
 * (C1 anti-spoof), and the zome NEVER parses `data` — it is a pass-through,
 * which test (c) confirms by checking byte-for-byte equality on a 4KB blob.
 */
import { expect, test } from "vitest";
import { runScenario, AppSignal, Signal, SignalType } from "@holochain/tryorama";
import { encodeHashToBase64 } from "@holochain/client";

const TEST_APP_PATH = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

// The DmCall envelope arrives as a `kind`/`type`-tagged map, NOT the legacy
// `action_type`/`data` shape — so the predicate discriminates structurally.
type AnySignal = Record<string, any>;

/**
 * Shape-agnostic sibling of the `waitForSignal` in `remote-signal.test.ts`.
 * That helper hard-guards `action_type`/`data` (the legacy shape) and would
 * silently drop every `DmRemoteSignal`; here the predicate does all the
 * discrimination. Resolves on the first matching app signal; rejects on
 * timeout so the test fails fast.
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

test("init_request round-trips with from_agent", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = encodeHashToBase64(alice.agentPubKey);

    const bobWait = waitForSignal(
      bob,
      (s) => s.kind === "DmCall" && s.type === "InitRequest" && s.call_id === "call-1",
      { label: "bob-init-request" },
    );

    await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "send_dm_call_init_request",
      payload: { call_id: "call-1", recipient: bob.agentPubKey },
    });

    const sig = await bobWait;
    expect(sig.kind).toBe("DmCall");
    expect(sig.type).toBe("InitRequest");
    expect(sig.call_id).toBe("call-1");
    expect(encodeHashToBase64(sig.from_agent)).toBe(aliceB64);
  });
});

test("init_accept round-trips", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const bobB64 = encodeHashToBase64(bob.agentPubKey);

    // Bob announces acceptance; Alice is the recipient this time.
    const aliceWait = waitForSignal(
      alice,
      (s) => s.kind === "DmCall" && s.type === "InitAccept" && s.call_id === "call-1",
      { label: "alice-init-accept" },
    );

    await bob.cells[0].callZome({
      zome_name: "content",
      fn_name: "send_dm_call_init_accept",
      payload: { call_id: "call-1", recipient: alice.agentPubKey },
    });

    const sig = await aliceWait;
    expect(sig.kind).toBe("DmCall");
    expect(sig.type).toBe("InitAccept");
    expect(sig.call_id).toBe("call-1");
    expect(encodeHashToBase64(sig.from_agent)).toBe(bobB64);
  });
});

test("sdp_data preserves a 4KB payload intact", async () => {
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { path: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = encodeHashToBase64(alice.agentPubKey);

    // Deterministic SDP-shaped blob. All ASCII, so byte length == char
    // length: "v=0\r\n" (5) + "a=mid:0\r\n" (9) * 455 = 4100 bytes.
    const sdp = "v=0\r\n" + "a=mid:0\r\n".repeat(455);
    expect(sdp.length).toBeGreaterThanOrEqual(4096);

    const bobWait = waitForSignal(
      bob,
      (s) => s.kind === "DmCall" && s.type === "SdpData" && s.call_id === "call-1",
      { label: "bob-sdp" },
    );

    await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "send_dm_call_sdp_data",
      payload: { call_id: "call-1", data: sdp, recipient: bob.agentPubKey },
    });

    const sig = await bobWait;
    expect(sig.kind).toBe("DmCall");
    expect(sig.type).toBe("SdpData");
    expect(sig.call_id).toBe("call-1");
    // The zome is a pure pass-through for `data`: byte-for-byte identical.
    expect(sig.data).toBe(sdp);
    expect(encodeHashToBase64(sig.from_agent)).toBe(aliceB64);
  });
});
