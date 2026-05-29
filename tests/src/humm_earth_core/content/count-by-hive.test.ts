/**
 * TR-C3 — proves the `count_links_by_hive` semantics:
 *   - counts exactly the Hive-path links for [hive_id, content_type],
 *   - honours `since_ts` (strictly-after) by falling back to a link fetch,
 *   - returns 0 (RESOLVES, never rejects) for a hive path that has no links.
 *
 * `count_links_by_hive` exists so unread badges / item counts / sync indicators
 * can be cheap: with no `since_ts` it takes the `count_links` fast path (no link
 * fan-out); with `since_ts` it falls back to `get_links(..).len()` because the
 * host's `count_links` has no time filter.
 */
import { expect, test } from "vitest";
import { runScenario, dhtSync, CallableCell } from "@holochain/tryorama";

import { sampleCreateEncryptedContentInput, cellPubkeyB64 } from "./common.js";

const TEST_APP_PATH = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

// Seed one EncryptedContent onto the [hive_id, content_type] Hive path. The
// author pubkey MUST match `revision_author_signing_public_key` or the
// integrity validator rejects the commit, so we thread it through.
async function seedEntry(
  cell: CallableCell,
  authorB64: string,
  hiveId: string,
  contentType: string,
  id: string,
): Promise<void> {
  const input = await sampleCreateEncryptedContentInput(
    { header: { id, hive_id: hiveId, content_type: contentType } },
    [],
    authorB64,
  );
  await cell.callZome({
    zome_name: "content",
    fn_name: "create_encrypted_content",
    payload: input,
  });
}

const CONTENT_TYPE = "tr-c3-type";

test("count equals the number of seeded entries", async () => {
  // (a)
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { type: "path" as const, value: TEST_APP_PATH } };
    const [alice] = await scenario.addPlayersWithApps([appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = cellPubkeyB64(alice.cells[0]);
    const hiveId = "tr-c3-hive-count";

    for (const n of [1, 2, 3, 4]) {
      await seedEntry(alice.cells[0], aliceB64, hiveId, CONTENT_TYPE, `entry-${n}`);
      await delay(20);
    }
    // Wait for every CreateLink op to integrate before counting them.
    await dhtSync([alice], alice.cells[0].cell_id[0]);

    const count = (await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "count_links_by_hive",
      payload: { hive_id: hiveId, content_type: CONTENT_TYPE },
    })) as number;

    expect(count).toBe(4);
  });
});

test("count with since_ts counts only links strictly after the watermark", async () => {
  // (b)
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { type: "path" as const, value: TEST_APP_PATH } };
    const [alice] = await scenario.addPlayersWithApps([appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = cellPubkeyB64(alice.cells[0]);
    const hiveId = "tr-c3-hive-watermark";

    for (const n of [1, 2]) {
      await seedEntry(alice.cells[0], aliceB64, hiveId, CONTENT_TYPE, `entry-${n}`);
      await delay(20);
    }
    // Watermark sits strictly between entry-2 and entry-3 link timestamps.
    // Holochain Timestamp is in microseconds; Date.now() is in milliseconds.
    const watermark = Date.now() * 1000;
    await delay(20);
    for (const n of [3, 4]) {
      await seedEntry(alice.cells[0], aliceB64, hiveId, CONTENT_TYPE, `entry-${n}`);
      await delay(20);
    }
    await dhtSync([alice], alice.cells[0].cell_id[0]);

    const count = (await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "count_links_by_hive",
      payload: { hive_id: hiveId, content_type: CONTENT_TYPE, since_ts: watermark },
    })) as number;

    // Only the two entries committed strictly after the watermark.
    expect(count).toBe(2);
  });
});

test("empty hive path returns 0 (resolves, does not reject)", async () => {
  // (c)
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { type: "path" as const, value: TEST_APP_PATH } };
    const [alice] = await scenario.addPlayersWithApps([appSource]);
    await scenario.shareAllAgents();

    // A hive path that was never written. The count MUST resolve with 0, not
    // reject — `count_links` over an empty path is 0 links, not an error.
    const countPromise = alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "count_links_by_hive",
      payload: { hive_id: "never-used-hive", content_type: "never-used-type" },
    });

    await expect(countPromise).resolves.toBe(0);
  });
});
