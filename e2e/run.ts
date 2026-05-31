/**
 * Sequential, state-gated e2e suite for the pass-4 humm_earth_core DNA,
 * run against a real holochain 0.6.0 conductor (see conductor.ts for the
 * tryorama-version-gap rationale). Boots ONE conductor with three agents
 * (alice/bob/carol) sharing a single DHT, runs every scenario group in
 * order, prints a tally, and exits non-zero on any failure.
 *
 *   npm run e2e          (from the e2e/ dir)
 *   npx tsx e2e/run.ts   (from the repo root)
 */
import { E2EConductor } from "./conductor.js";
import { report, resetResults } from "./acl.js";
import * as authority from "./scenarios/authority.js";
import * as variants from "./scenarios/variants.js";
import * as witnesses from "./scenarios/witnesses.js";
import * as invite from "./scenarios/invite.js";

const c = new E2EConductor();
let exitCode = 1;
try {
  console.log("[e2e] booting conductor (fresh keystore + data-dir)…");
  await c.start();
  const alice = await c.addAgent("alice");
  const bob = await c.addAgent("bob");
  const carol = await c.addAgent("carol");
  console.log("[e2e] alice + bob + carol installed on one shared DHT.");

  resetResults();
  await authority.run(alice, bob, carol);
  await variants.run(alice, bob, carol);
  await witnesses.run(alice, bob, carol);
  await invite.run(alice, bob, carol);

  exitCode = report();
} catch (e) {
  console.error("[e2e] FATAL:", (e as Error)?.message ?? e);
  exitCode = 1;
} finally {
  await c.stop();
  console.log("[e2e] torn down.");
}
process.exit(exitCode);
