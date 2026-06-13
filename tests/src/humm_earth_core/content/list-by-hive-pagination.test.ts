/**
 * TR-C2 — proves the `list_by_hive_link` pagination contract:
 *   - oldest-first ordering (ascending link timestamp),
 *   - `since_ts` is an EXCLUSIVE lower bound (strictly-after watermark),
 *   - `limit` truncates AFTER the oldest-first sort.
 *
 * The load-bearing assertion is (c): with `limit: 3` the query returns the
 * THREE OLDEST entries, not the newest. The pre-fix code sorted newest-first
 * then truncated, which silently dropped the older entries past `limit` during
 * a watermark sweep — the host advanced its watermark past them and they were
 * never re-fetched (data loss). Oldest-first + truncate makes the
 * `(since_ts, limit)` sweep gap-free: the host sets the next `since_ts` to the
 * max returned timestamp and re-sweeps, so nothing is skipped.
 */
import { expect, test } from "vitest";
import { runScenario, dhtSync, CallableCell } from "@holochain/tryorama";

import { sampleCreateEncryptedContentInput, cellPubkeyB64 } from "./common.js";

const TEST_APP_PATH = process.cwd() + "/../workdir/humm-earth-core-happ.happ";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

// Seed one EncryptedContent onto the [hive_id, content_type] Hive path. The
// author pubkey MUST match `revision_author_signing_public_key` or the
// integrity validator rejects the commit, so we thread it through. The default
// `public_key_acl.reader` is empty, so no remote signal fan-out fires.
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

const idsOf = (rows: any[]): string[] => rows.map((r) => r.encrypted_content.header.id);

const CONTENT_TYPE = "tr-c2-type";

test("no fields returns the full set in oldest-first chain order", async () => {
  // (a)
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { type: "path" as const, value: TEST_APP_PATH } };
    const [alice] = await scenario.addPlayersWithApps([appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = cellPubkeyB64(alice.cells[0]);
    const hiveId = "tr-c2-hive-all";

    // 20ms gap between creates so link timestamps are distinct + ordered.
    for (const n of [1, 2, 3, 4, 5]) {
      await seedEntry(alice.cells[0], aliceB64, hiveId, CONTENT_TYPE, `entry-${n}`);
      await delay(20);
    }
    // Wait for every CreateLink op to integrate before reading them back.
    await dhtSync([alice], alice.cells[0].cell_id[0]);

    const rows = (await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "list_by_hive_link",
      payload: { hive_id: hiveId, content_type: CONTENT_TYPE },
    })) as any[];

    expect(idsOf(rows)).toEqual(["entry-1", "entry-2", "entry-3", "entry-4", "entry-5"]);
  });
});

test("since_ts filters strictly after the watermark", async () => {
  // (b)
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { type: "path" as const, value: TEST_APP_PATH } };
    const [alice] = await scenario.addPlayersWithApps([appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = cellPubkeyB64(alice.cells[0]);
    const hiveId = "tr-c2-hive-watermark";

    for (const n of [1, 2, 3]) {
      await seedEntry(alice.cells[0], aliceB64, hiveId, CONTENT_TYPE, `entry-${n}`);
      await delay(20);
    }
    // Watermark sits strictly between entry-3 and entry-4 link timestamps.
    // Holochain Timestamp is in microseconds; Date.now() is in milliseconds.
    const watermark = Date.now() * 1000;
    await delay(20);
    for (const n of [4, 5]) {
      await seedEntry(alice.cells[0], aliceB64, hiveId, CONTENT_TYPE, `entry-${n}`);
      await delay(20);
    }
    await dhtSync([alice], alice.cells[0].cell_id[0]);

    const rows = (await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "list_by_hive_link",
      payload: { hive_id: hiveId, content_type: CONTENT_TYPE, since_ts: watermark },
    })) as any[];

    // since_ts is EXCLUSIVE: only entries committed strictly after the
    // watermark — entries 4 and 5, never 1-3.
    expect(idsOf(rows)).toEqual(["entry-4", "entry-5"]);
  });
});

test("limit returns the OLDEST entries first (gap-free watermark sweep)", async () => {
  // (c) — the load-bearing C2 assertion.
  await runScenario(async (scenario) => {
    const appSource = { appBundleSource: { type: "path" as const, value: TEST_APP_PATH } };
    const [alice] = await scenario.addPlayersWithApps([appSource]);
    await scenario.shareAllAgents();

    const aliceB64 = cellPubkeyB64(alice.cells[0]);
    const hiveId = "tr-c2-hive-limit";

    for (const n of [1, 2, 3, 4, 5]) {
      await seedEntry(alice.cells[0], aliceB64, hiveId, CONTENT_TYPE, `entry-${n}`);
      await delay(20);
    }
    await dhtSync([alice], alice.cells[0].cell_id[0]);

    const rows = (await alice.cells[0].callZome({
      zome_name: "content",
      fn_name: "list_by_hive_link",
      payload: { hive_id: hiveId, content_type: CONTENT_TYPE, limit: 3 },
    })) as any[];

    // OLDEST three, not the newest three. The broken pre-fix behaviour sorted
    // newest-first then truncated, returning [entry-3, entry-4, entry-5] and
    // permanently dropping entry-1/entry-2 from the watermark sweep.
    expect(idsOf(rows)).toEqual(["entry-1", "entry-2", "entry-3"]);
  });
});
