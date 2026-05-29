/**
 * TR-C1 — proves the C1 anti-spoof guarantee: the `from_agent` field on an
 * emitted `EncryptedContentSignal` is always set from the conductor-attested
 * caller (`call_info().provenance`) on the RECEIVER side, never trusted from
 * the wire payload.
 *
 * Implemented in `lib::recv_remote_signal` (the C7b multi-signal dispatcher):
 * whatever `from_agent` the incoming payload claims is OVERWRITTEN with
 * `call_info()?.provenance` — the lair-attested AgentPubKey of the peer that
 * actually invoked the call — before the signal is re-emitted locally. Sidecar
 * consumers MUST therefore trust `from_agent` as the authoritative sender id.
 *
 * `from_agent` is an `AgentPubKey` = `Uint8Array(39)` on the wire. Direct `===`
 * on the byte arrays does NOT work; compare the multibase holohash form via
 * `encodeHashToBase64(payload.from_agent)` (equivalently
 * `Buffer.from(a).equals(Buffer.from(b))`).
 */
import { assert, expect, test } from "vitest";
import { runScenario, AppSignal, Signal, SignalType } from "@holochain/tryorama";
import { encodeHashToBase64 } from "@holochain/client";
import { Buffer } from "node:buffer";
import { encode } from "@msgpack/msgpack";

import { sampleCreateEncryptedContentInput } from "./common.js";

const TEST_APP_PATH = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

// Real wire shape of the emitted signal. `from_agent` is PRESENT on every
// signal that traversed `recv_remote_signal` (stamped from provenance) and
// ABSENT on a purely-local `emit_signal` (the author's own create/update path,
// where the zome sets `from_agent: None` and serde skips the field).
type EncryptedContentResponse = {
  encrypted_content: any;
  hash: string;
  original_hash: string;
};
type EncryptedContentSignal = {
  action_type: "Create" | "Update" | "Delete";
  data: EncryptedContentResponse;
  from_agent?: Uint8Array;
};

/**
 * Subscribe to app signals on a player's app websocket and resolve the first
 * signal whose decoded payload matches `predicate`. Rejects on timeout so the
 * test fails fast rather than hanging. Mirrors remote-signal.test.ts.
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

test("forwarded create stamps from_agent = the real caller (alice), not the receiver (bob)", async () => {
  // (a) Alice commits an entry that lists Bob in `public_key_acl.reader`. The
  // create path fans out `send_remote_signal(.., [bob])` with `from_agent:
  // None`; Bob's conductor then invokes `recv_remote_signal` with
  // provenance = alice, which stamps `from_agent = alice` before re-emitting.
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { type: "path" as const, value: TEST_APP_PATH } };
    const [alice, bob] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const alicePubkeyB64 = encodeHashToBase64(alice.agentPubKey);
    const bobPubkeyB64 = encodeHashToBase64(bob.agentPubKey);

    // Subscribe BEFORE the create so we never miss the forwarded signal.
    const bobWait = waitForSignal(
      bob,
      (s) => s.action_type === "Create" && s.data.encrypted_content.header.id === "tr-c1-forwarded",
      { timeoutMs: 10_000, label: "bob-forwarded" },
    );

    const input = await sampleCreateEncryptedContentInput(
      {
        header: {
          id: "tr-c1-forwarded",
          public_key_acl: {
            owner: alicePubkeyB64,
            admin: [],
            writer: [],
            reader: [alicePubkeyB64, bobPubkeyB64],
          },
        },
      },
      [],
      alicePubkeyB64,
    );
    const response = (await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "create_encrypted_content",
      payload: input,
    })) as EncryptedContentResponse;
    assert.ok(response.hash);

    const sig = await bobWait;
    // from_agent is Uint8Array(39) on the wire — compare via base64 holohash.
    assert.ok(sig.from_agent, "forwarded signal must carry a stamped from_agent");
    expect(encodeHashToBase64(sig.from_agent!)).toBe(alicePubkeyB64); // real caller = alice
    expect(encodeHashToBase64(sig.from_agent!)).not.toBe(bobPubkeyB64); // NOT the receiver
    // Buffer-equality is the other sanctioned comparison (see file header).
    expect(Buffer.from(sig.from_agent!).equals(Buffer.from(alice.agentPubKey))).toBe(true);
  });
});

test("forged from_agent in the payload is overwritten by the real caller's provenance", async () => {
  // (b) The literal cross-cell form ("Bob calls recv_remote_signal against
  // Alice's cell, Alice's handler sees from_agent=bob not eve") is not
  // expressible through the tryorama JS client: a player can only `callZome`
  // against its OWN cell, and `recv_remote_signal` re-emits LOCALLY, so the
  // caller IS the receiver. There is also no exposed `send_remote_signal`
  // extern to drive the (b') fallback. We therefore exercise the IDENTICAL
  // security property locally: Bob invokes `recv_remote_signal` with a payload
  // whose `from_agent` is forged to a third agent (eve), and asserts the
  // emitted signal carries `from_agent = bob` (the conductor-attested caller),
  // never eve. `recv_remote_signal` is granted Unrestricted, and a local
  // self-call additionally carries the author grant, so this is a legitimate
  // exercise of the cap surface.
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { type: "path" as const, value: TEST_APP_PATH } };
    const [bob, eve] = await scenario.addPlayersWithApps([appSource, appSource]);
    await scenario.shareAllAgents();

    const bobPubkeyB64 = encodeHashToBase64(bob.agentPubKey);
    const evePubkeyB64 = encodeHashToBase64(eve.agentPubKey);

    // Match only the stamped signal (from_agent present) for our forged id.
    const bobWait = waitForSignal(
      bob,
      (s) => s.from_agent !== undefined && s.data.encrypted_content.header.id === "tr-c1-forged",
      { timeoutMs: 10_000, label: "bob-forged" },
    );

    // recv_remote_signal(signal: ExternIO): the JS client msgpack-encodes the
    // `payload` field; the ribosome decodes those bytes into the `ExternIO`
    // parameter, whose CONTENTS are a SECOND msgpack encoding of
    // EncryptedContentSignal. So `payload` must itself be
    // `encode(EncryptedContentSignal)` (a Uint8Array) — byte-for-byte the wire
    // shape `send_remote_signal` puts on the network.
    const forged: EncryptedContentSignal = {
      action_type: "Create",
      data: {
        encrypted_content: {
          header: {
            id: "tr-c1-forged",
            hive_id: "tr-c1-hive",
            content_type: "tr-c1-type",
            acl: { owner: bobPubkeyB64, admin: [], writer: [], reader: [] },
            public_key_acl: { owner: bobPubkeyB64, admin: [], writer: [], reader: [] },
            revision_author_signing_public_key: bobPubkeyB64,
          },
          bytes: new Uint8Array([1, 2, 3]),
        },
        hash: "tr-c1-forged-hash",
        original_hash: "tr-c1-forged-hash",
      },
      from_agent: eve.agentPubKey, // <-- forged: claims the signal came from eve
    };
    const externIo = encode(forged); // Uint8Array -> ExternIO inner bytes

    await bob.cells[0].callZome({
      zome_name: "content",
      fn_name: "recv_remote_signal",
      payload: externIo,
    });

    const sig = await bobWait;
    assert.ok(sig.from_agent, "stamped signal must carry from_agent");
    // Overwritten with the conductor-attested caller (bob), NOT the forged eve.
    expect(encodeHashToBase64(sig.from_agent!)).toBe(bobPubkeyB64);
    expect(encodeHashToBase64(sig.from_agent!)).not.toBe(evePubkeyB64);
  });
});
