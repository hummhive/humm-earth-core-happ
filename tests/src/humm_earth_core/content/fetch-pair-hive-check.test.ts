/**
 * TR-C4 — `fetch_pair_ss_with_hive_check` intersection guard.
 *
 * **What this proves (and what it does NOT prove).** The intersection
 * narrows results to entries that are both authored-by-target AND placed
 * under the caller's chosen `(active_hive_id, content_type, group_id)`
 * dynamic path. Against an UNMODIFIED-CLIENT attacker (one that can only
 * invoke the stock `create_encrypted_content` extern), this excludes
 * Mallory's pair-SS seeded under her own hive — which is what these
 * tests exercise. C4 is therefore a meaningful defense-in-depth narrowing
 * for the realistic attacker.
 *
 * **Defense-in-depth posture.** Pass-2's I-H validators (`Hive` and
 * `Dynamic` link validators recompute the expected base from each
 * link's target-entry header fields and reject mismatches) close the
 * cryptographic H-1 gap that the original I-D row in the integration
 * guide flagged. C4's intersection is now the FIRST-stage filter on
 * top of those integrity-enforced link constraints rather than a
 * standalone control. These tests still exercise the structural
 * intersection only; the per-link integrity validators have their own
 * coverage in `cargo test -p content_integrity --lib`.
 *
 * Fixture — three players:
 *   - alice   : victim / caller. Her active hive is SHARED_HIVE; she
 *               writes no pair-SS of her own.
 *   - bob     : legitimate author writing in SHARED_HIVE.
 *   - mallory : unmodified-client attacker who seeds the SAME
 *               `content_type` + `group_id` but in her OWN hive
 *               (MALLORY_HIVE).
 *
 * Mallory's entry lands on the author path (she authored it) but NOT on
 * SHARED_HIVE's dynamic path — her `Dynamic` link is keyed by
 * MALLORY_HIVE — so the intersection excludes it.
 */
import { expect, test } from "vitest";

import { runScenario, dhtSync, CallableCell } from "@holochain/tryorama";
import { encodeHashToBase64 } from "@holochain/client";

import {
  cellPubkeyB64,
  sampleCreateEncryptedContentInput,
  type EncryptedContentResponse,
} from "./common.js";

const TEST_APP_PATH = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

const SHARED_HIVE = "shared-hive-tr-c4";
const MALLORY_HIVE = "mallory-only-hive";
const CONTENT_TYPE = "pair-ss-tr-c4";
const GROUP_ID = "group-42";

// Seed one `create_encrypted_content` from `cell`. This creates the
// author-path `Hive` link `[caller_pubkey, CONTENT_TYPE]` AND the
// dynamic-path `Dynamic` link `[hiveId, CONTENT_TYPE, GROUP_ID]`. The
// caller's own pubkey is threaded into
// `revision_author_signing_public_key` so the integrity zome's
// author-match guard accepts the commit.
async function seedAuthorEntry(
  cell: CallableCell,
  id: string,
  hiveId: string,
): Promise<void> {
  const input = await sampleCreateEncryptedContentInput(
    { header: { id, hive_id: hiveId, content_type: CONTENT_TYPE } },
    [GROUP_ID],
    cellPubkeyB64(cell),
  );
  await cell.callZome({
    zome_name: "content",
    fn_name: "create_encrypted_content",
    payload: input,
  });
}

// Alice's active hive is always SHARED_HIVE; only the queried `author`
// varies across the three cases.
function pairCheckPayload(author: string) {
  return {
    author,
    active_hive_id: SHARED_HIVE,
    content_type: CONTENT_TYPE,
    group_id: GROUP_ID,
  };
}

const appSource = {
  appBundleSource: { type: "path" as const, value: TEST_APP_PATH },
};

test("intersection returns Bob's canonical SS for the shared hive", async () => {
  await runScenario(async (scenario) => {
    const [alice, bob, mallory] = await scenario.addPlayersWithApps([
      appSource,
      appSource,
      appSource,
    ]);
    await scenario.shareAllAgents();

    await seedAuthorEntry(bob.cells[0], "bob-canonical", SHARED_HIVE);
    await seedAuthorEntry(mallory.cells[0], "mallory-poisoned", MALLORY_HIVE);

    await dhtSync([alice, bob, mallory], alice.cells[0].cell_id[0]);

    // Bob is on BOTH the author path `[bob, CONTENT_TYPE]` and the
    // shared hive's dynamic path -> intersection yields exactly his entry.
    const result: EncryptedContentResponse[] = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "fetch_pair_ss_with_hive_check",
      payload: pairCheckPayload(encodeHashToBase64(bob.agentPubKey)),
    });

    expect(result.length).toBe(1);
    expect(result[0].encrypted_content.header.id).toBe("bob-canonical");
  });
});

test("intersection excludes Mallory's poisoned SS", async () => {
  await runScenario(async (scenario) => {
    const [alice, bob, mallory] = await scenario.addPlayersWithApps([
      appSource,
      appSource,
      appSource,
    ]);
    await scenario.shareAllAgents();

    await seedAuthorEntry(bob.cells[0], "bob-canonical", SHARED_HIVE);
    await seedAuthorEntry(mallory.cells[0], "mallory-poisoned", MALLORY_HIVE);

    await dhtSync([alice, bob, mallory], alice.cells[0].cell_id[0]);

    // Mallory IS on the author path `[mallory, CONTENT_TYPE]`, but her
    // `Dynamic` link is keyed by MALLORY_HIVE, so she is absent from
    // SHARED_HIVE's dynamic path (which holds Bob's entry). The
    // intersection is therefore empty — a true exclusion, not a vacuous
    // empty-hive-path result.
    const result: EncryptedContentResponse[] = await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "fetch_pair_ss_with_hive_check",
      payload: pairCheckPayload(encodeHashToBase64(mallory.agentPubKey)),
    });

    expect(result).toEqual([]);
  });
});

test("no entries for unknown author returns []", async () => {
  await runScenario(async (scenario) => {
    const [alice, bob, mallory] = await scenario.addPlayersWithApps([
      appSource,
      appSource,
      appSource,
    ]);
    await scenario.shareAllAgents();

    await seedAuthorEntry(bob.cells[0], "bob-canonical", SHARED_HIVE);
    await seedAuthorEntry(mallory.cells[0], "mallory-poisoned", MALLORY_HIVE);

    await dhtSync([alice, bob, mallory], alice.cells[0].cell_id[0]);

    // Alice authored no pair-SS, so the author path `[alice, CONTENT_TYPE]`
    // is empty and the extern short-circuits. It MUST resolve with `[]`,
    // not reject — even though SHARED_HIVE's dynamic path is populated.
    await expect(
      alice.cells[0].callZome({
        zome_name: "content",
        fn_name: "fetch_pair_ss_with_hive_check",
        payload: pairCheckPayload(encodeHashToBase64(alice.agentPubKey)),
      }),
    ).resolves.toEqual([]);
  });
});
