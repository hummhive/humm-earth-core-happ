# Pass-1 marker-v2 — humm-tauri BDD sanity test

A drop-in BDD test for the humm-tauri team to **sanity-check the right happ is
wired**: the coordinator-only `pass-1-marker-v2` rebuild that exposes the V2
migration-marker externs (`mark_migrated_v2` / `get_migration_marker_v2`) with
the pass-1 integrity DNA held byte-identical.

Companion to [`PASS_1_MARKER_V2_HANDOFF.md`](./PASS_1_MARKER_V2_HANDOFF.md)
(artifact + wire shapes). This doc is only the sanity test.

## What it proves

Primary "right happ" discriminators:

| id | assertion | why it matters |
|---|---|---|
| PMV2-1 | sha256 of the file `HAPP_PATH_PASS_1` resolves to == `0e6baaea…` | your **pass-1 pointer** resolves to the V2 bundle — trips loudly if it ever regresses to the old `63921f6b` happ (no conductor needed) |
| PMV2-3 | create → `mark_migrated_v2` → `get_migration_marker_v2` returns `{ V2: {…} }` with the written fields | the real **write→read→wire** contract the spec-10 marker step exercises |

Supporting contract checks:

| id | assertion | why it matters |
|---|---|---|
| PMV2-2 | `get_migration_marker_v2(original)` returns `null` before marking | the V2 reader extern is **registered + callable** (the OLD fixture throws unknown-zome-fn here) |
| PMV2-4 | `get_migration_marker` (V1 reader) returns `null` for the V2-only marker | the V1/V2 **cross-version discrimination** the poller's `decodeMigrationMarker` relies on (deterministic: V1 `is_well_formed` rejects `schema_version==2`) |

## Wiring status on rc-cleanup (already done — verified)

- `tests/e2e/paths.ts` `HAPP_PATH_PASS_1` **already resolves** to
  `humm-earth-core-happ_pass-1-marker-v2_dna-uhC0kb0T3Lrh_happ-0e6baaea.happ`.
- Both `.testdata/happs/` pass-1 fixtures are provisioned (the `0e6baaea`
  marker-v2 bundle and the legacy `63921f6b`).

So the test **imports `HAPP_PATH_PASS_1` directly** — no new const, no alias. It
validates your real canonical pass-1 pointer; PMV2-1's sha256 gate is what
guarantees that pointer is the V2 bundle (an alias would only hide a miswire).

If a fresh clone hasn't provisioned yet:

```bash
cp ~/hummhive-official-happ-versions/humm-earth-core-happ_pass-1-marker-v2_dna-uhC0kb0T3Lrh_happ-0e6baaea.happ .testdata/happs/
cp ~/hummhive-official-happ-versions/MANIFEST.tsv .testdata/happs/
```

## The test

Save as `tests/bdd/swarm/pass1-marker-v2-sanity.test.ts`:

```ts
/**
 * Sanity-checks that humm-tauri provisioned the pass-1 coordinator-only rebuild
 * that exposes the V2 migration-marker externs while keeping the pass-1
 * integrity DNA bytes unchanged. The conductor installs test happs with a
 * NETWORK_SEED, which intentionally forks the runtime cell DNA hash from the
 * bundle DNA (`uhC0kb0T3Lrh`), so this test cannot assert that DNA hash from
 * inside the conductor. Instead, the independent fixture gate pins the on-disk
 * `.happ` sha256; the zome calls then prove the V2 marker wire contract through
 * the public coordinator externs.
 */
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';

import { decodeHashFromBase64, type ActionHash } from '@holochain/client';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import { E2EConductor, type Agent } from '../conductor.js';
import { HAPP_PATH_PASS_1 } from '../../e2e/paths.js';
import {
	kitsune2BootstrapSrvAvailable,
	startLocalServices,
	type LocalServices,
} from './localServices.js';

const BEFORE_ALL_TIMEOUT_MS = 120_000;
const TEST_TIMEOUT_MS = 60_000;
const EXPECTED_PASS_1_MARKER_V2_HAPP_SHA256 =
	'0e6baaead4b28bb59483dabc5d321e326c9b88475deb9145eab1e992ca24d624';
const SCHEMA_TAG = 'humm-earth-core-happ/migration-marker';

const markerV2 = {
	schema_tag: SCHEMA_TAG,
	schema_version: 2,
	new_dna_hash_base64: 'uhC0kSANITYnewDNA',
	new_action_hash_base64: 'uhCkkSANITYnewAction',
	new_app_id: 'humm-earth-core-happ@2',
	migrated_at_microseconds: 1_700_000_000_000_000,
	new_hive_genesis_hash_base64: 'uhCkkSANITYgenesis',
	new_hive_genesis_display_id: 'sanity-hive',
} satisfies MigrationMarkerV2;

type Acl = {
	owner: string;
	admin: string[];
	writer: string[];
	reader: string[];
};

type CreateEncryptedContentResponse = {
	hash: string;
	original_hash: string;
};

type MigrationMarkerV2 = {
	schema_tag: string;
	schema_version: 2;
	new_dna_hash_base64: string;
	new_action_hash_base64: string;
	new_app_id: string;
	migrated_at_microseconds: number;
	new_hive_genesis_hash_base64?: string;
	new_hive_genesis_display_id?: string;
};

type MigrationMarker = null | { V1: unknown } | { V2: MigrationMarkerV2 };

type MigrationMarkerV1 = Record<string, unknown>;

const suite = kitsune2BootstrapSrvAvailable() ? describe : describe.skip;

suite('Feature: pass-1-marker-v2 migration marker sanity', () => {
	it('PMV2-1: uses the pinned pass-1-marker-v2 fixture bytes', async () => {
		const happBytes = await readFile(HAPP_PATH_PASS_1);
		const actualSha256 = createHash('sha256').update(happBytes).digest('hex');

		expect(actualSha256).toBe(EXPECTED_PASS_1_MARKER_V2_HAPP_SHA256);
	});

	describe('Scenario: the pass-1 marker owner writes a V2-only migration marker', () => {
		let services: LocalServices;
		let cond: E2EConductor;
		let agent: Agent;
		let originalActionHash: ActionHash;

		beforeAll(async () => {
			services = await startLocalServices();
			cond = new E2EConductor();
			await cond.start({
				bootstrapUrl: services.bootstrapUrl,
				signalUrl: services.signalUrl,
				relayUrl: services.relayUrl,
			});
			agent = await cond.addAgent('marker-owner', HAPP_PATH_PASS_1);

			const acl: Acl = {
				owner: agent.b64,
				admin: [],
				writer: [],
				reader: [],
			};
			const created = await agent.call<CreateEncryptedContentResponse>(
				'create_encrypted_content',
				{
					id: `marker-v2-sanity-${Date.now()}`,
					hive_id: 'sanity-hive',
					content_type: 'sanity',
					revision_author_signing_public_key: agent.b64,
					bytes: new TextEncoder().encode('pass-1 marker-v2 sanity'),
					acl,
					public_key_acl: acl,
					dynamic_links: null,
				},
			);
			originalActionHash = decodeHashFromBase64(created.hash);
		}, BEFORE_ALL_TIMEOUT_MS);

		afterAll(async () => {
			await cond?.stop();
			await services?.stop();
		});

		it(
			'PMV2-2: get_migration_marker_v2 is callable and returns null before marking',
			async () => {
				const got = await agent.call<MigrationMarker>(
					'get_migration_marker_v2',
					originalActionHash,
				);

				expect(got).toBeNull();
			},
			TEST_TIMEOUT_MS,
		);

		it(
			'PMV2-3: mark_migrated_v2 round-trips the V2 tagged marker fields',
			async () => {
				await agent.call('mark_migrated_v2', {
					original_action_hash: originalActionHash,
					marker: markerV2,
				});

				const got = await agent.call<MigrationMarker>(
					'get_migration_marker_v2',
					originalActionHash,
				);
				if (got === null || !('V2' in got)) {
					throw new Error(
						`expected get_migration_marker_v2 to return a V2 marker, got ${JSON.stringify(got)}`,
					);
				}

				expect(got.V2.schema_version).toBe(2);
				expect(got.V2.schema_tag).toBe(SCHEMA_TAG);
				expect(got.V2.new_dna_hash_base64).toBe(markerV2.new_dna_hash_base64);
				expect(got.V2.new_hive_genesis_hash_base64).toBe(
					markerV2.new_hive_genesis_hash_base64,
				);
				expect(got.V2.new_hive_genesis_display_id).toBe(
					markerV2.new_hive_genesis_display_id,
				);
			},
			TEST_TIMEOUT_MS,
		);

		it(
			'PMV2-4: the V1 reader returns null for a V2-only marker',
			async () => {
				const got = await agent.call<MigrationMarkerV1 | null>(
					'get_migration_marker',
					originalActionHash,
				);

				expect(got).toBeNull();
			},
			TEST_TIMEOUT_MS,
		);
	});
});
```

## Running it

It's a bdd-swarm test: it boots a real holochain 0.6.1 conductor, so it needs
`holochain` / `hc` + `kitsune2-bootstrap-srv` on PATH. The
`kitsune2BootstrapSrvAvailable()` gate `describe.skip`s the conductor scenario
otherwise — PMV2-1 (the on-disk sha256 gate) still runs and is the primary
"right file" canary.

```bash
pnpm test -- pass1-marker-v2-sanity          # or your bdd-swarm project runner
```

## Notes / caveats

- **Why no in-conductor DNA-hash assertion:** `addAgent` installs with a
  `NETWORK_SEED`, which forks the runtime cell DNA off the bundle DNA
  (`uhC0kb0T3Lrh`) by design — same behavior your pass-4-rescue swarm test
  relies on. So the seed-independent identity gate is the on-disk sha256
  (PMV2-1), not the cell DNA.
- **Single conductor is sufficient:** the marker is written and read on the
  agent's OWN pass-1 source cell, so `mark_migrated_v2` → `get_migration_marker_v2`
  resolve locally — no second peer, no gossip. This mirrors exactly what the
  spec-10 marker step + poller do against the pass-1 SOURCE cell.
- **Integrity gate:** the create's `revision_author_signing_public_key` MUST be
  `agent.b64`; the marker update inherits the header via struct-update, so the
  same agent's `mark_migrated_v2` validates automatically.

## Verification status (earth-core side)

- Coordinator host unit tests: **22/22 green**, incl. all 11 V2 marker cases
  (decode-priority V2-before-V1, `#[serde(default)]` cross-version decode,
  `{V1|V2}` external tagging, idempotent `_migrated/` prefix).
- Built `content.wasm` exports **exactly** the 2 new externs (0 removed vs the
  original pass-1); `content_integrity.wasm` byte-identical → DNA held.
- Wire shapes in the test match humm-tauri's own `src-tauri/src/migration/wire.rs`
  (`MigrationMarkerV2` / `MarkMigratedV2Input`) and the TS `decodeMigrationMarker`
  tagged-enum contract.
- The test is **unrun on the earth-core side by design** — it targets the
  humm-tauri bdd-swarm harness (hc 0.6.1 conductor) absent from this clone.
  humm-tauri runs it.
