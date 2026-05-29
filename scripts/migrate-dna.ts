#!/usr/bin/env -S npx tsx
/**
 * DNA migration orchestrator.
 *
 * Migrates `EncryptedContent` data from a source hApp installation
 * (old DNA hash) to a target hApp installation (new DNA hash). Both
 * installations must be present on the same conductor with distinct
 * app-ids.
 *
 * This is the *coordinator-side* migration tool — it shuttles entries
 * through the existing `get_messages_since` (export) and
 * `create_encrypted_content` (import) externs. No new zome surface is
 * required for the migration itself; the surface is two scripts and a
 * remap file.
 *
 *   1. `export <app-id> <out.bundle.json>`
 *      Connect to the OLD app's AppWebsocket; call
 *      `get_messages_since({ since_seq: 0 })` to pull every Record
 *      from the local source chain; decode each Record's entry into an
 *      `EncryptedContent` payload; write a self-contained bundle file.
 *
 *   2. `import <app-id> <in.bundle.json> <out.remap.json>`
 *      Connect to the NEW app's AppWebsocket; iterate the bundle;
 *      replay each entry via `create_encrypted_content` (with the
 *      caller's new `revision_author_signing_public_key`); record the
 *      `old_action_hash -> new_action_hash` map to disk.
 *
 *   3. `mark-migrated <old-app-id> <in.remap.json>`
 *      Connect to the OLD app's AppWebsocket; for each successfully-
 *      imported entry, call `mark_migrated` to write a forward-pointer
 *      `MigrationMarkerV1` onto the old chain. Old clients that later
 *      query the same action hash via `get_migration_marker` see the
 *      marker and can prompt the user to upgrade. Idempotent (re-running
 *      writes a fresh marker; latest-from-trusted-author wins).
 *
 * The remap file is the load-bearing handoff to the host
 * (humm-tauri): every persisted reference (localStorage keys, SS
 * lookups, thread IDs that include action hashes, etc.) MUST be
 * rewritten by walking this map.
 *
 * # SECURITY — receiver-side rules for migration markers
 *
 * The coordinator extern `get_migration_marker` enforces that ONLY
 * updates authored by the original entry's author count as authoritative
 * markers (closes a forge surface where any peer could write a marker
 * on someone else's entry — see the marker security model in
 * docs/DNA_MIGRATION_GUIDE.md). That said, the consuming host
 * (humm-tauri) MUST still:
 *
 *   A. Validate the marker's `from_agent` / original-entry-author
 *      matches the trusted partner identity before treating the marker
 *      as a directive.
 *   B. NEVER auto-follow the marker's `new_dna_hash_base64` /
 *      `new_app_id` without explicit human approval. Switching DNA
 *      crosses a trust boundary and must be a user decision.
 *   C. Cross-verify that `new_action_hash_base64` resolves on the new DNA
 *      before redirecting any UI to it. Also handles the
 *      uninstall/reinstall staleness case.
 *
 * # What is and is NOT migrated
 *
 * - Migrated: for every `header.id` on the local source chain, the
 *   LATEST live `EncryptedContent` payload (latest Create-or-Update
 *   content). Edits are preserved; deleted entries are EXCLUDED so
 *   user deletions are honored, not silently undone on the new DNA.
 *   The bundle's `old_action_hash` is the ORIGINAL Create's action
 *   hash regardless of how many Updates followed — so the host's
 *   persisted references (keyed by the Create hash at first ingest)
 *   remap cleanly.
 * - NOT migrated: signatures, action hashes (regenerated on the new
 *   DNA), intermediate Update versions (only the latest live content
 *   per `id` survives), deleted entries (excluded entirely),
 *   `Dynamic` links (derived from entry state, not in the entry
 *   payload — the host re-stamps via the normal create flow's
 *   `dynamic_links` arg if it preserves the group context).
 *
 * # Limitations (read before running)
 *
 * 1. **Intermediate Update content is lost.** Only the latest live
 *    version per `id` is re-imported. If your application needs the
 *    full edit history, snapshot separately before migration.
 * 2. **Author pubkey changes.** Each cell in a fresh hApp installation
 *    has a fresh agent pubkey; the new `revision_author_signing_public_key`
 *    is the NEW agent's pubkey, not the old one. The integrity zome
 *    enforces this (`check_author_matches_header` in the integrity
 *    zome).
 * 3. **Pair-shared SS coordination.** Cross-agent shared secrets only
 *    work after BOTH parties have migrated. Sequence carefully.
 * 4. **Encrypted bodies pass through opaquely.** Decryption keys
 *    (Tauri keyring) MUST be unchanged across the migration, otherwise
 *    the migrated entries are unreadable.
 * 5. **DHT propagation timing.** After import, the new DNA's DHT needs
 *    time to gossip newly-created links to other agents' arcs. Expect
 *    a settling window before queries against the new DNA return the
 *    full set.
 *
 * # Usage
 *
 *     # Pre-migration: with the OLD hApp installed and running
 *     npx tsx scripts/migrate-dna.ts export old-happ-id /tmp/bundle.json
 *
 *     # User installs the new .happ (different DNA hash → different app-id)
 *     # via humm-tauri's install flow
 *
 *     # Post-migration: with the NEW hApp installed and running
 *     npx tsx scripts/migrate-dna.ts import new-happ-id /tmp/bundle.json /tmp/remap.json
 *
 *     # Optional but recommended: write forward-pointer markers back to
 *     # the OLD hApp so old-DNA clients can detect "this data has moved"
 *     # and prompt their users to upgrade.
 *     npx tsx scripts/migrate-dna.ts mark-migrated old-happ-id /tmp/remap.json
 *
 *     # The remap file is then consumed by humm-tauri's host-side
 *     # rewrite pass (separate, in humm-tauri's repo) to update every
 *     # localStorage key, SS lookup, and thread-id reference that
 *     # carries an old action hash.
 *
 * # Environment
 *
 * Set `ADMIN_PORT` to the conductor's admin websocket port (default
 * 4444). The script issues its own short-lived authentication token
 * via `issueAppAuthenticationToken` — no caller-supplied secret
 * required.
 */

import {
  AdminWebsocket,
  AppWebsocket,
  encodeHashToBase64,
  decodeHashFromBase64,
  type ActionHash,
  type AgentPubKey,
  type CellId,
  type Record as HolochainRecord,
} from "@holochain/client";
import { decode } from "@msgpack/msgpack";
import * as fs from "node:fs/promises";
import * as path from "node:path";

const ADMIN_PORT = Number(process.env.ADMIN_PORT ?? 4444);
const ZOME_NAME = "content";
const ROLE_NAME = "humm_earth_core";

/** Bundle entry shape. One per `EncryptedContent` action on the source chain. */
type BundleEntry = {
  /** Original action hash from the OLD DNA (multibase holohash string). */
  old_action_hash: string;
  /** Source-chain action sequence number — preserved for diagnostic order. */
  action_seq: number;
  /** ISO timestamp of the original action — preserved for diagnostic order. */
  action_timestamp_iso: string;
  /** Decoded EncryptedContent payload. Replayed as-is on the new DNA except
   * for `revision_author_signing_public_key`, which is restamped. */
  encrypted_content: {
    header: {
      id: string;
      hive_id: string;
      content_type: string;
      revision_author_signing_public_key: string;
      acl: unknown;
      public_key_acl: unknown;
    };
    bytes: Uint8Array;
  };
};

type Bundle = {
  schema_version: 1;
  source_app_id: string;
  source_agent_pubkey_base64: string;
  exported_at_iso: string;
  /** Latest version per `id` after deduping update chains. */
  entries: BundleEntry[];
};

type RemapRecord = {
  id: string;
  old_action_hash: string;
  new_action_hash: string;
  content_type: string;
  hive_id: string;
};

type Remap = {
  schema_version: 1;
  source_app_id: string;
  source_agent_pubkey_base64: string;
  target_app_id: string;
  target_agent_pubkey_base64: string;
  imported_at_iso: string;
  /** Per-entry mapping. `id` is stable across DNAs; the AH pair lets the host
   * remap every persisted reference. */
  entries: RemapRecord[];
  /** Entries that failed to re-import; host should retry or surface to user. */
  failures: { id: string; old_action_hash: string; error: string }[];
};

async function connectAppWs(
  appId: string,
): Promise<{ appWebsocket: AppWebsocket; cellId: CellId; agentPubKey: AgentPubKey }> {
  const adminWebsocket = await AdminWebsocket.connect({
    url: new URL(`ws://localhost:${ADMIN_PORT}`),
  });
  const appInfo = await adminWebsocket.listApps({});
  const target = appInfo.find((a) => a.installed_app_id === appId);
  if (!target) {
    throw new Error(
      `App "${appId}" not found on conductor (port ${ADMIN_PORT}). ` +
        `Available: ${appInfo.map((a) => a.installed_app_id).join(", ")}`,
    );
  }
  const issued = await adminWebsocket.issueAppAuthenticationToken({
    installed_app_id: appId,
  });
  const appPort = (await adminWebsocket.attachAppInterface({ allowed_origins: "migrate-dna" })).port;
  const appWebsocket = await AppWebsocket.connect({
    token: issued.token,
    url: new URL(`ws://localhost:${appPort}`),
    wsClientOptions: { origin: "migrate-dna" },
  });
  const info = await appWebsocket.appInfo();
  if (!info) throw new Error(`appInfo() returned null for ${appId}`);
  const cell = info.cell_info[ROLE_NAME]?.find(
    (c): c is { type: "provisioned"; value: { cell_id: CellId } } =>
      c.type === "provisioned",
  );
  if (!cell) {
    throw new Error(
      `No provisioned cell for role "${ROLE_NAME}" in app "${appId}". ` +
        `Cell-info keys: ${Object.keys(info.cell_info).join(", ")}`,
    );
  }
  return { appWebsocket, cellId: cell.value.cell_id, agentPubKey: appWebsocket.myPubKey };
}

/**
 * Per-id export state. Walks the source chain in `action_seq` order;
 * the latest Create-or-Update wins; a Delete on any action of the
 * chain marks the id dead and excludes it from the bundle.
 *
 * `original_create_action_hash` is preserved as the bundle's
 * `old_action_hash` so the host's persisted references (keyed by the
 * first Create's AH at ingest time) remap cleanly. `latest_content`
 * is what gets re-published on the new DNA so user edits survive and
 * deleted entries do NOT resurrect.
 */
type IdState = {
  original_create_action_hash: string;
  latest_content: BundleEntry["encrypted_content"];
  latest_action_seq: number;
  latest_action_timestamp_iso: string;
  alive: boolean;
};

async function doExport(appId: string, outPath: string): Promise<void> {
  console.log(`[export] connecting to "${appId}" on port ${ADMIN_PORT}...`);
  const { appWebsocket, cellId, agentPubKey } = await connectAppWs(appId);
  console.log(`[export] connected. agent=${encodeHashToBase64(agentPubKey)}`);

  console.log(`[export] querying source chain via get_messages_since(0)...`);
  const records = (await appWebsocket.callZome({
    cell_id: cellId,
    zome_name: ZOME_NAME,
    fn_name: "get_messages_since",
    payload: { since_seq: 0 },
  })) as HolochainRecord[];
  console.log(`[export] retrieved ${records.length} record(s) from source chain.`);

  // Walk the chain in seq order so latest-wins semantics fall out
  // naturally. Tracks state per `header.id` (humm-tauri's stable
  // application-level identifier), not per action hash — Updates
  // change action hashes but preserve `header.id`, and Deletes
  // tombstone the entire id.
  const sortedRecords = [...records].sort(
    (a, b) =>
      a.signed_action.hashed.content.action_seq -
      b.signed_action.hashed.content.action_seq,
  );
  const stateById = new Map<string, IdState>();
  // Lookup table for Delete actions, which reference the prior action
  // hash they delete; we need to map that hash back to its id.
  const idByActionHash = new Map<string, string>();

  let createCount = 0;
  let updateCount = 0;
  let deleteCount = 0;
  let skipCount = 0;

  for (const record of sortedRecords) {
    const action = record.signed_action.hashed.content;
    const actionHash = encodeHashToBase64(
      record.signed_action.hashed.hash as ActionHash,
    );

    if (action.type === "Create" || action.type === "Update") {
      const appEntryType = action.entry_type;
      if (typeof appEntryType !== "object" || !("App" in appEntryType)) {
        skipCount++;
        continue;
      }
      if (record.entry.type !== "Present") {
        skipCount++;
        continue;
      }
      const entry = record.entry.entry;
      if (entry.entry_type !== "App") {
        skipCount++;
        continue;
      }
      let decoded: BundleEntry["encrypted_content"];
      try {
        decoded = decode(entry.entry) as BundleEntry["encrypted_content"];
      } catch (err) {
        console.warn(
          `[export] skipping un-decodable entry at seq ${action.action_seq}: ${err}`,
        );
        skipCount++;
        continue;
      }
      if (!decoded?.header?.id) {
        console.warn(
          `[export] skipping entry at seq ${action.action_seq}: missing header.id`,
        );
        skipCount++;
        continue;
      }
      const id = decoded.header.id;
      idByActionHash.set(actionHash, id);
      const existing = stateById.get(id);
      const timestampIso = new Date(action.timestamp / 1000).toISOString();
      if (!existing) {
        // First action for this id is always the Create (chain order).
        stateById.set(id, {
          original_create_action_hash: actionHash,
          latest_content: decoded,
          latest_action_seq: action.action_seq,
          latest_action_timestamp_iso: timestampIso,
          alive: true,
        });
        createCount++;
      } else {
        // Subsequent action (Update). Overwrite latest content; keep
        // the original Create's AH as the stable key for the remap.
        existing.latest_content = decoded;
        existing.latest_action_seq = action.action_seq;
        existing.latest_action_timestamp_iso = timestampIso;
        // Re-Create with a duplicate id is technically possible on the
        // chain (the integrity zome doesn't enforce id-uniqueness);
        // treat it as an update for export purposes.
        if (action.type === "Update") updateCount++;
        else createCount++;
      }
    } else if (action.type === "Delete") {
      const deletedHash = encodeHashToBase64(action.deletes_address);
      const id = idByActionHash.get(deletedHash);
      // A Delete referencing an action we have not seen (e.g., from
      // before some chain prune) is silently ignored — there is nothing
      // to mark dead. This can only happen if the source chain itself
      // is incomplete, which `get_messages_since(0)` should never
      // produce on an honest local conductor.
      if (!id) continue;
      const state = stateById.get(id);
      if (!state) continue;
      state.alive = false;
      deleteCount++;
    }
    // Other action types (CreateLink, DeleteLink, system actions like
    // AgentValidationPkg) are not entry-bearing for EncryptedContent
    // and do not participate in the migration.
  }

  const aliveStates = [...stateById.values()].filter((s) => s.alive);
  const deadCount = stateById.size - aliveStates.length;
  console.log(
    `[export] walked ${sortedRecords.length} records: ${createCount} creates, ` +
      `${updateCount} updates, ${deleteCount} deletes, ${skipCount} skipped. ` +
      `${stateById.size} unique ids; ${aliveStates.length} alive (${deadCount} ` +
      `excluded as deleted).`,
  );

  const entries: BundleEntry[] = aliveStates.map((s) => ({
    old_action_hash: s.original_create_action_hash,
    action_seq: s.latest_action_seq,
    action_timestamp_iso: s.latest_action_timestamp_iso,
    encrypted_content: s.latest_content,
  }));

  const bundle: Bundle = {
    schema_version: 1,
    source_app_id: appId,
    source_agent_pubkey_base64: encodeHashToBase64(agentPubKey),
    exported_at_iso: new Date().toISOString(),
    entries,
  };

  // Convert Uint8Array bytes to base64 for JSON round-trip stability.
  const serializable = {
    ...bundle,
    entries: bundle.entries.map((e) => ({
      ...e,
      encrypted_content: {
        ...e.encrypted_content,
        bytes: Buffer.from(e.encrypted_content.bytes).toString("base64"),
      },
    })),
  };
  await fs.mkdir(path.dirname(outPath), { recursive: true });
  await fs.writeFile(outPath, JSON.stringify(serializable, null, 2), "utf8");
  console.log(`[export] wrote bundle: ${outPath} (${entries.length} entries)`);
  await appWebsocket.client.close();
}

async function doImport(
  appId: string,
  bundlePath: string,
  remapPath: string,
): Promise<void> {
  console.log(`[import] reading bundle from ${bundlePath}...`);
  const raw = JSON.parse(await fs.readFile(bundlePath, "utf8")) as {
    schema_version: number;
    source_app_id: string;
    source_agent_pubkey_base64: string;
    exported_at_iso: string;
    entries: (Omit<BundleEntry, "encrypted_content"> & {
      encrypted_content: Omit<BundleEntry["encrypted_content"], "bytes"> & {
        bytes: string; // base64
      };
    })[];
  };
  if (raw.schema_version !== 1) {
    throw new Error(
      `Unsupported bundle schema_version: ${raw.schema_version} (expected 1)`,
    );
  }
  console.log(
    `[import] bundle from ${raw.source_app_id} (${raw.source_agent_pubkey_base64}) ` +
      `with ${raw.entries.length} entries.`,
  );

  console.log(`[import] connecting to target "${appId}" on port ${ADMIN_PORT}...`);
  const { appWebsocket, cellId, agentPubKey } = await connectAppWs(appId);
  const targetAgentBase64 = encodeHashToBase64(agentPubKey);
  console.log(`[import] connected. agent=${targetAgentBase64}`);

  if (targetAgentBase64 === raw.source_agent_pubkey_base64) {
    console.warn(
      `[import] WARNING: target agent pubkey matches source. This is only ` +
        `expected if you re-installed onto the same lair key — confirm before ` +
        `proceeding.`,
    );
  }

  const remap: Remap = {
    schema_version: 1,
    source_app_id: raw.source_app_id,
    source_agent_pubkey_base64: raw.source_agent_pubkey_base64,
    target_app_id: appId,
    target_agent_pubkey_base64: targetAgentBase64,
    imported_at_iso: new Date().toISOString(),
    entries: [],
    failures: [],
  };

  for (const entry of raw.entries) {
    const { header, bytes: bytesBase64 } = entry.encrypted_content;
    const bytes = new Uint8Array(Buffer.from(bytesBase64, "base64"));
    // Restamp the signing pubkey to match the new agent. The integrity
    // zome enforces action.author == header.revision_author_signing_public_key
    // (`check_author_matches_header`) — failing this would invalidate every
    // committed entry.
    const input = {
      id: header.id,
      hive_id: header.hive_id,
      content_type: header.content_type,
      revision_author_signing_public_key: targetAgentBase64,
      bytes,
      acl: header.acl,
      public_key_acl: header.public_key_acl,
      dynamic_links: null, // host re-stamps from app state if needed
    };
    try {
      const response = (await appWebsocket.callZome({
        cell_id: cellId,
        zome_name: ZOME_NAME,
        fn_name: "create_encrypted_content",
        payload: input,
      })) as { hash: string; original_hash: string; encrypted_content: unknown };
      // `hash` field on `EncryptedContentResponse` is the action hash
      // as a multibase holohash string.
      remap.entries.push({
        id: header.id,
        old_action_hash: entry.old_action_hash,
        new_action_hash: response.hash,
        content_type: header.content_type,
        hive_id: header.hive_id,
      });
      process.stdout.write(".");
    } catch (err) {
      remap.failures.push({
        id: header.id,
        old_action_hash: entry.old_action_hash,
        error: String(err),
      });
      process.stdout.write("F");
    }
  }
  process.stdout.write("\n");

  await fs.mkdir(path.dirname(remapPath), { recursive: true });
  await fs.writeFile(remapPath, JSON.stringify(remap, null, 2), "utf8");
  console.log(
    `[import] wrote remap: ${remapPath} ` +
      `(${remap.entries.length} succeeded, ${remap.failures.length} failed)`,
  );
  if (remap.failures.length > 0) {
    console.log(
      `[import] failures present — review ${remapPath} 'failures' array, ` +
        `address root cause (e.g. integrity validator changes, conductor ` +
        `state) and re-run with the same bundle. Re-imports are NOT ` +
        `idempotent at the action-hash level (a re-run creates fresh ` +
        `actions); dedupe by 'id' on the host side.`,
    );
    process.exit(1);
  }
  await appWebsocket.client.close();
}

/**
 * Phase 3 — write `MigrationMarkerV1` forward pointers onto the OLD
 * chain's entries by calling the `mark_migrated` coordinator extern for
 * each successfully-imported entry in the remap.
 *
 * Requires the OLD hApp's coordinator zome to include `mark_migrated`
 * (added in pass-1 follow-up, ships in the same `.happ` rebuild after
 * the coordinator hot-swap that lands the pass-1 changes). If the OLD
 * hApp predates this addition, the call will fail with "no such
 * function" — bump COORDINATOR_WASM_VERSION on the OLD hApp and
 * hot-swap before invoking this phase.
 *
 * Each marker write is itself an update to the original entry on the
 * OLD chain. Per the coordinator's SECURITY model, only the original
 * author can write a valid marker — and `mark_migrated` is NOT in the
 * cap grant, so only the local UI / this script (running as the
 * original author via lair) can invoke it.
 */
async function doMarkMigrated(
  oldAppId: string,
  remapPath: string,
): Promise<void> {
  console.log(`[mark-migrated] reading remap from ${remapPath}...`);
  const remap = JSON.parse(await fs.readFile(remapPath, "utf8")) as {
    schema_version: number;
    source_app_id: string;
    target_app_id: string;
    target_agent_pubkey_base64: string;
    imported_at_iso: string;
    entries: RemapRecord[];
    failures: { id: string; old_action_hash: string; error: string }[];
  };
  if (remap.schema_version !== 1) {
    throw new Error(
      `Unsupported remap schema_version: ${remap.schema_version} (expected 1)`,
    );
  }
  if (remap.source_app_id !== oldAppId) {
    console.warn(
      `[mark-migrated] WARNING: remap.source_app_id=${remap.source_app_id} ` +
        `differs from CLI arg oldAppId=${oldAppId}. Proceeding — confirm ` +
        `this is intentional (e.g. you re-installed the old hApp under a ` +
        `different installed_app_id).`,
    );
  }
  console.log(
    `[mark-migrated] ${remap.entries.length} successful imports + ` +
      `${remap.failures.length} failures from ${remap.imported_at_iso}. ` +
      `Failed entries will be SKIPPED (no marker written — host treats ` +
      `them as 'not migrated yet' on the old DNA).`,
  );

  console.log(`[mark-migrated] connecting to old app "${oldAppId}"...`);
  const { appWebsocket, cellId } = await connectAppWs(oldAppId);

  // Use the timestamp from the import phase so re-running mark-migrated
  // multiple times against the same remap yields identical marker
  // payloads (modulo schema_tag/version which are constants). This makes
  // the operation idempotent in spirit even though each call still
  // produces a fresh Update action on the chain.
  //
  // Defensive: Date.parse returns NaN for malformed isos, which would
  // serialize as a non-integer and trip Rust's i64 decode with a cryptic
  // error — guard explicitly and fail fast with a useful message.
  const importedAtMilliseconds = Date.parse(remap.imported_at_iso);
  if (!Number.isFinite(importedAtMilliseconds)) {
    throw new Error(
      `Invalid remap.imported_at_iso (${JSON.stringify(remap.imported_at_iso)}). ` +
        `Must be a Date.parse-able ISO 8601 string. The remap was likely ` +
        `produced by an older script version or manually edited; re-run ` +
        `'migrate-dna.ts import' to regenerate.`,
    );
  }
  const migratedAtMicroseconds = importedAtMilliseconds * 1000;

  // Marker DNA hash and app-id come from the remap's target side.
  const newAppId = remap.target_app_id;
  // `target_dna_hash_base64` is NOT in the remap today (the script writes
  // remap without explicitly asking the conductor for the new DNA hash —
  // it can be derived from the new app's appInfo if needed). For the
  // marker we fetch it now from the OLD app's view of the world is wrong
  // — we need the NEW DNA's hash. Defer to the user: pass via env, or
  // omit and accept the marker pointing at app_id alone.
  const newDnaHashBase64 = process.env.NEW_DNA_HASH_BASE64 ?? "";
  if (!newDnaHashBase64) {
    console.warn(
      `[mark-migrated] WARNING: NEW_DNA_HASH_BASE64 not set in env. Markers ` +
        `will be written with new_dna_hash_base64="" — receivers will need ` +
        `to resolve the new DNA from new_app_id alone. To populate, run: ` +
        `\`hc dna hash <new.dna>\` and pass it via NEW_DNA_HASH_BASE64=… .`,
    );
  }

  let succeeded = 0;
  const newFailures: { id: string; old_action_hash: string; error: string }[] = [];
  for (const entry of remap.entries) {
    const marker = {
      schema_tag: "humm-earth-core-happ/migration-marker",
      schema_version: 1,
      new_dna_hash_base64: newDnaHashBase64,
      new_action_hash_base64: entry.new_action_hash,
      new_app_id: newAppId,
      migrated_at_microseconds: migratedAtMicroseconds,
    };
    const input = {
      original_action_hash: decodeHashFromBase64(entry.old_action_hash),
      marker,
    };
    try {
      await appWebsocket.callZome({
        cell_id: cellId,
        zome_name: ZOME_NAME,
        fn_name: "mark_migrated",
        payload: input,
      });
      succeeded++;
      process.stdout.write(".");
    } catch (err) {
      newFailures.push({
        id: entry.id,
        old_action_hash: entry.old_action_hash,
        error: String(err),
      });
      process.stdout.write("F");
    }
  }
  process.stdout.write("\n");
  console.log(
    `[mark-migrated] wrote ${succeeded} markers; ${newFailures.length} failed.`,
  );
  if (newFailures.length > 0) {
    // Append failures back into the remap file so the operator has one
    // place to look. Re-running mark-migrated against the same file is
    // safe (the OLD coordinator's update validator passes for the
    // original author; latest marker wins on the reader side).
    const augmented = {
      ...remap,
      mark_migrated_at_iso: new Date().toISOString(),
      mark_migrated_failures: newFailures,
    };
    await fs.writeFile(remapPath, JSON.stringify(augmented, null, 2), "utf8");
    console.log(
      `[mark-migrated] failure list appended to ${remapPath} as ` +
        `mark_migrated_failures. Address root cause and re-run.`,
    );
    process.exit(1);
  }
  await appWebsocket.client.close();
}

async function main(): Promise<void> {
  const [mode, ...args] = process.argv.slice(2);
  switch (mode) {
    case "export": {
      const [appId, outPath] = args;
      if (!appId || !outPath) {
        console.error("Usage: migrate-dna.ts export <app-id> <out.bundle.json>");
        process.exit(2);
      }
      await doExport(appId, outPath);
      break;
    }
    case "import": {
      const [appId, bundlePath, remapPath] = args;
      if (!appId || !bundlePath || !remapPath) {
        console.error(
          "Usage: migrate-dna.ts import <app-id> <in.bundle.json> <out.remap.json>",
        );
        process.exit(2);
      }
      await doImport(appId, bundlePath, remapPath);
      break;
    }
    case "mark-migrated": {
      const [appId, remapPath] = args;
      if (!appId || !remapPath) {
        console.error(
          "Usage: migrate-dna.ts mark-migrated <old-app-id> <in.remap.json>",
        );
        process.exit(2);
      }
      await doMarkMigrated(appId, remapPath);
      break;
    }
    default:
      console.error(
        "Usage:\n" +
          "  migrate-dna.ts export <app-id> <out.bundle.json>\n" +
          "  migrate-dna.ts import <app-id> <in.bundle.json> <out.remap.json>\n" +
          "  migrate-dna.ts mark-migrated <old-app-id> <in.remap.json>\n" +
          "\n" +
          "Env:\n" +
          "  ADMIN_PORT          conductor admin websocket port (default 4444)\n" +
          "  NEW_DNA_HASH_BASE64    new DNA's multibase holohash (for mark-migrated;\n" +
          "                      get via `hc dna hash`. Optional; falls back to\n" +
          "                      app-id-only resolution on the receiver.)",
      );
      process.exit(2);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});