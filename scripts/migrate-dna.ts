#!/usr/bin/env -S npx tsx
/**
 * DNA migration orchestrator.
 *
 * Migrates `EncryptedContent` data from a source hApp installation
 * (old DNA hash) to a target hApp installation (new DNA hash). Both
 * installations must be present on the same conductor with distinct
 * app-ids.
 *
 * Two coordinated tracks ship in this tool:
 *
 *  - **Per-entry track** (`export` → `import` → `mark-migrated`) — the
 *    pass-1 baseline. Shuttles every live `EncryptedContent` entry from
 *    the old chain onto the new chain via `create_encrypted_content`
 *    and writes forward-pointer markers onto the old chain so old
 *    clients can detect the move.
 *  - **Hive-identity track** (`migrate-hive` → `grant-memberships` →
 *    `mark-hive-migrated`) — pass-2 addition. The pass-2 integrity zome
 *    requires every `EncryptedContent` to carry a `hive_genesis_hash`
 *    (cryptographic hive identity) plus an optional
 *    `author_membership_hash`. Before the per-entry `import` can
 *    succeed, the hive owner MUST publish a `HiveGenesis` on the new
 *    DNA and grant memberships to the cell agents who will re-import
 *    their entries. The hive-bundle file captures
 *    `old_hive_id → new_genesis_hash` so `import` can stamp the new
 *    field onto every entry.
 *
 * # Command pipeline (pass-2)
 *
 *   1. `migrate-hive <new-app-id> <old-hive-id> <old-anchor-ah> <hive-bundle.json>`
 *      Owner-side. Creates a `HiveGenesis` on the new DNA for the
 *      named old hive; appends a hive entry to the hive-bundle. The
 *      old-anchor-ah identifies the OLD entry the marker will be
 *      written onto in step 4 (pass `""` to defer; the bundle's
 *      `old_marker_action_hash` stays null and step 4 skips it with
 *      a warning).
 *
 *   2. `grant-memberships <new-app-id> <hive-bundle.json> <old-hive-id> <role> <member-pubkey-b64> [...]`
 *      Owner-side. Calls `create_hive_membership` on the new DNA for
 *      each listed member pubkey at the given role
 *      (`Owner`|`Admin`|`Writer`|`Reader`). Appends the resulting
 *      membership hashes into the hive-bundle's `granted_memberships`.
 *      Members read these hashes during `import` via
 *      `get_latest_membership` (cached for performance).
 *
 *   3. `export <old-app-id> <out.bundle.json>`
 *      Either side. Identical to the pass-1 export: walks the local
 *      source chain in `action_seq` order, dedupes via `header.id`
 *      (latest live wins; deletes drop the id), emits a self-contained
 *      bundle file.
 *
 *   4. `import <new-app-id> <bundle.json> <hive-bundle.json> <out.remap.json>`
 *      Either side. For every entry in the bundle, looks up
 *      `header.hive_id` in the hive-bundle, resolves the new
 *      `hive_genesis_hash` and the caller's `author_membership_hash`
 *      via a one-time `get_latest_membership` lookup per hive (cached),
 *      and replays the entry via `create_encrypted_content` with the
 *      pass-2 fields stamped. Records `old_action_hash -> new_action_hash`
 *      plus the new genesis hash for downstream rewrite.
 *
 *   5. `mark-hive-migrated <old-app-id> <hive-bundle.json>`
 *      Owner-side. For each hive in the bundle with
 *      `old_marker_action_hash` populated, calls `mark_migrated_v2` on
 *      the OLD app to write a V2 marker pointing at the new
 *      `HiveGenesis`. Members discover the new genesis hash by calling
 *      `get_migration_marker_v2` against the recorded old anchor.
 *
 *   6. `mark-migrated <old-app-id> <in.remap.json> [--v1-only]`
 *      Either side. For each successfully-imported per-entry record,
 *      calls `mark_migrated_v2` (default) or `mark_migrated` (with
 *      `--v1-only`) to write a forward-pointer marker. Old clients
 *      that later query the same action hash via the appropriate
 *      reader see the marker and prompt their user to upgrade.
 *      Idempotent (re-running writes a fresh marker;
 *      latest-from-trusted-author wins).
 *
 * # Marker version selection
 *
 *  - V1 (`MigrationMarkerV1`, `mark_migrated`) — the pass-1 marker.
 *    Recognised by pass-1 and pass-2 readers. Use `--v1-only` against
 *    OLD apps whose coordinator predates pass-2.5 (the `mark_migrated_v2`
 *    extern is unavailable there).
 *  - V2 (`MigrationMarkerV2`, `mark_migrated_v2`) — pass-2.5 addition,
 *    additive superset of V1. V2 carries the
 *    `new_hive_genesis_hash_base64` field used by the hive-identity
 *    track. **V1-only readers see V2 markers as `Ok(None)`** —
 *    pre-pass-2 hosts cannot discover V2 markers and require a host
 *    upgrade before they can follow the migration. See
 *    `docs/DNA_MIGRATION_GUIDE.md` for the receiver-side contract.
 *
 * # Remap file
 *
 * The remap is the load-bearing handoff to the host (humm-tauri):
 * every persisted reference (localStorage keys, SS lookups, thread
 * IDs that include action hashes, etc.) MUST be rewritten by walking
 * this map. Pass-2 remap records also carry
 * `new_hive_genesis_hash_base64` so the host can rebuild its
 * hive-genesis-keyed indices.
 *
 * # SECURITY — receiver-side rules for migration markers
 *
 * The coordinator readers `get_migration_marker` and
 * `get_migration_marker_v2` enforce that ONLY updates authored by the
 * original entry's author count as authoritative markers (closes a
 * forge surface where any peer could write a marker on someone else's
 * entry — see the marker security model in
 * `docs/DNA_MIGRATION_GUIDE.md`). The consuming host (humm-tauri) MUST
 * still:
 *
 *   A. Validate the marker's `from_agent` / original-entry-author
 *      matches the trusted partner identity before treating the marker
 *      as a directive.
 *   B. NEVER auto-follow the marker's `new_dna_hash_base64` /
 *      `new_app_id` / `new_hive_genesis_hash_base64` without explicit
 *      human approval. Switching DNA or joining a new HiveGenesis
 *      crosses a trust boundary and must be a user decision.
 *   C. Cross-verify that `new_action_hash_base64` resolves on the new
 *      DNA before redirecting any UI to it. Also handles the
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
 *   `dynamic_links` arg if it preserves the group context),
 *   `HiveGenesis`/`HiveMembership` entries (re-published by the
 *   hive-identity track).
 *
 * # Limitations (read before running)
 *
 * 1. **Intermediate Update content is lost.** Only the latest live
 *    version per `id` is re-imported. If your application needs the
 *    full edit history, snapshot separately before migration.
 * 2. **Author pubkey changes.** Each cell in a fresh hApp installation
 *    has a fresh agent pubkey; the new
 *    `revision_author_signing_public_key` is the NEW agent's pubkey,
 *    not the old one. The integrity zome enforces this
 *    (`check_author_matches_header`).
 * 3. **Owner-first sequencing.** The hive owner MUST run `migrate-hive`
 *    + `grant-memberships` BEFORE members run `import`. A member whose
 *    pubkey is not yet granted a HiveMembership on the new DNA cannot
 *    re-import their entries (integrity rejects the write).
 * 4. **Pair-shared SS coordination.** Cross-agent shared secrets only
 *    work after BOTH parties have migrated. Sequence carefully.
 * 5. **Encrypted bodies pass through opaquely.** Decryption keys
 *    (Tauri keyring) MUST be unchanged across the migration, otherwise
 *    the migrated entries are unreadable.
 * 6. **DHT propagation timing.** After import, the new DNA's DHT needs
 *    time to gossip newly-created links to other agents' arcs. Expect
 *    a settling window before queries against the new DNA return the
 *    full set.
 *
 * # Environment
 *
 * Set `ADMIN_PORT` to the conductor's admin websocket port (default
 * 4444). The script issues its own short-lived authentication token
 * via `issueAppAuthenticationToken` — no caller-supplied secret
 * required.
 *
 * Set `NEW_DNA_HASH_BASE64` to the multibase holohash of the NEW DNA
 * (run `hc dna hash <new.dna>` to compute it) for use by `mark-migrated`
 * and `mark-hive-migrated`. Optional; if unset, markers are written
 * with an empty `new_dna_hash_base64` field and receivers must resolve
 * the new DNA from `new_app_id` alone.
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
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

import {
  cappedWitnessRank,
  rankToBucket,
  roleRank,
  type AclBucketName,
} from "./acl-bucket.js";

const ADMIN_PORT = Number(process.env.ADMIN_PORT ?? 4444);
const ZOME_NAME = "content";
const ROLE_NAME = "humm_earth_core";
const MIGRATION_MARKER_SCHEMA_TAG = "humm-earth-core-happ/migration-marker";

/** Roles accepted by the pass-2/pass-3 membership integrity zomes.
 *  Wire-form variant names match `Role` in the integrity zome (pass-3
 *  shared between hive + group membership). Alias of the shared
 *  [`AclBucketName`] union so the dominance helpers in `acl-bucket.ts`
 *  accept `HiveRole` values directly. */
type HiveRole = AclBucketName;
const HIVE_ROLES: readonly HiveRole[] = ["Owner", "Admin", "Writer", "Reader"];

/**
 * Pass-3 wire-shape: `AclSpec` discriminated-union variants accepted
 * by the new `EncryptedContentHeader.acl_spec` field. Wire-form follows
 * the serde external-tag convention (`{ "VariantName": { ...fields } }`).
 */
type AclSpecKind = "HiveGroup" | "DirectMessage" | "Public" | "OpenWrite";

/**
 * Content-type → `AclSpec` variant classification table used by the
 * `import` track to re-stamp pass-1/pass-2 entries onto the pass-3
 * wire shape. Every legacy entry on the source chain needs to land in
 * exactly one of the four pass-3 variants; for content types the
 * humm-tauri product roadmap has explicitly placed in DM /
 * OpenWrite / Public scopes we list them here. Everything else falls
 * through to the default mapping below.
 *
 * Coverage of shipped humm-tauri content types is best-effort: the
 * canonical reference is `docs/HUMM_TAURI_ACLSPEC_INTEGRATION.md`
 * (Phase E.1 — not yet written; defer to that doc when it lands).
 * Operators with custom content types should add entries here BEFORE
 * running `import`. A per-bundle classification-overrides file
 * (Phase D.1, deferred) will replace the in-script table when shipped.
 */
const CONTENT_TYPE_ACL_SPEC: Readonly<Record<string, AclSpecKind>> = {
  // DirectMessage — cross-hive pair/small-group messaging.
  "direct_message": "DirectMessage",
  "hummhive-core-peer-identity-claim-v1": "DirectMessage",
  // OpenWrite — outsider-knock + cross-network discovery.
  "hummhive-core-member-request-v1": "OpenWrite",
  "hummhive-core-hive-discovery-v1": "OpenWrite",
  "hummhive-core-agent-directory-v1": "OpenWrite",
  // Public — world-readable hive content.
  "humm-addon-text-post-v1": "Public",
  "hummhive-core-hive-v1": "Public",
};

/** Fallback classification for content types not listed above. We pick
 * `Public` (not `HiveGroup`) intentionally: pass-3+ `HiveGroup` requires
 * the author to hold Writer+ in every group listed in `group_acl.*`
 * AND (pass-4) a `recipient_witnesses` vec covering every pubkey in
 * `public_key_acl`. Pass-1/pass-2 had no `group_acl` field and no
 * groups exist on the new DNA until D.1's migrate-group track has
 * been run, so the migration cannot populate either field without
 * operator input. `Public` keeps the entry readable by every member
 * of the hive (via hive Writer+ on the author), matching the most
 * common humm-tauri "everyone in the hive sees this" pattern.
 * humm-tauri can re-stamp specific entries to `HiveGroup` post-
 * migration once real groups + memberships exist (see Phase D.1
 * follow-up + `classification-overrides.json`). */
const DEFAULT_ACL_SPEC_KIND: AclSpecKind = "Public";

/** Resolve `(hive_genesis_hash, author_membership_hash, agent_pubkey,
 * old_public_key_acl)` into the wire-shape value of `acl_spec` for the
 * given content type. `old_public_key_acl` may be the legacy shape
 * `{owner, admin, writer, reader: string[]}` or `null`; only the DM
 * variant needs it (reader bucket pin). */
function classifyAclSpec(
  contentType: string,
  hiveGenesisHashBytes: Uint8Array,
  authorMembershipHashBytes: Uint8Array | null,
  targetAgentBase64: string,
  oldPublicKeyAcl: unknown,
): { acl_spec: unknown; public_key_acl: unknown; display_hive_id: string | null } {
  const kind = CONTENT_TYPE_ACL_SPEC[contentType] ?? DEFAULT_ACL_SPEC_KIND;
  const oldReaders =
    typeof oldPublicKeyAcl === "object" &&
    oldPublicKeyAcl !== null &&
    Array.isArray((oldPublicKeyAcl as { reader?: unknown }).reader)
      ? ((oldPublicKeyAcl as { reader: string[] }).reader as string[])
      : [];
  switch (kind) {
    case "DirectMessage": {
      // Best-effort: re-use the legacy public_key_acl.reader as the
      // recipient set. The author MUST be in it for the validator to
      // accept; the import flow restamps the author pubkey so we
      // splice it in if absent. Cardinality bounds are checked by the
      // integrity zome at commit time.
      const recipientsB64 = Array.from(
        new Set<string>(oldReaders.concat([targetAgentBase64])),
      );
      // Strip the 'u' multibase prefix and decode to raw 39-byte
      // holohash so the validator's `for_agent == header.author`
      // check passes (action.author is a raw AgentPubKey, not a
      // string).
      const recipients = recipientsB64.map((b64) => decodeHashFromBase64(b64));
      // Reader bucket MUST equal recipients (sorted-equality at the
      // validator). Both sides use the same multibase string form.
      return {
        acl_spec: { DirectMessage: { recipients } },
        public_key_acl: {
          owner: "",
          admin: [],
          writer: [],
          reader: recipientsB64,
        },
        display_hive_id: null,
      };
    }
    case "OpenWrite": {
      // member-request / hive-discovery: keep the hive context as the
      // target (so list_by_hive on the new DNA still surfaces them
      // under the target hive's discovery index). hive-discovery
      // entries published with empty hive_id translate to
      // OpenWrite { target: None }; everything else stays bound.
      // The import flow passes hive_genesis_hash even for these,
      // because the bundle's `header.hive_id` was non-empty on the
      // source. We let the operator drop the binding by tweaking
      // their bundle pre-import (or by adjusting this table).
      return {
        acl_spec: { OpenWrite: { target_hive_genesis_hash: hiveGenesisHashBytes } },
        public_key_acl: oldPublicKeyAcl ?? {
          owner: "",
          admin: [],
          writer: [],
          reader: [],
        },
        display_hive_id: null,
      };
    }
    case "Public": {
      return {
        acl_spec: {
          Public: {
            hive_genesis_hash: hiveGenesisHashBytes,
            author_membership_hash: authorMembershipHashBytes,
          },
        },
        public_key_acl: oldPublicKeyAcl ?? {
          owner: "",
          admin: [],
          writer: [],
          reader: [],
        },
        display_hive_id: null,
      };
    }
    case "HiveGroup": {
      // Reaching this branch means a content_type was mapped to
      // HiveGroup in CONTENT_TYPE_ACL_SPEC *without* a per-entry
      // classification-override. The HiveGroup wire shape needs a
      // concrete group_acl (ActionHash-keyed) + recipient_witnesses,
      // neither of which is derivable from content_type alone — both
      // require the operator to name the migrated groups per entry.
      //
      // D.1 supplies that via `classification-overrides.json`: an
      // override forces an entry into HiveGroup and `buildHiveGroupAclSpec`
      // (in doImport) resolves the groups + stamps the witnesses,
      // bypassing this classifier branch entirely. So this throw now
      // only fires for the genuine misconfiguration "content_type
      // blanket-mapped to HiveGroup with no per-entry override".
      throw new Error(
        `HiveGroup classification for content_type "${contentType}" is not ` +
          `derivable from content_type alone. Use a per-entry ` +
          `classification-overrides.json (D.1) to name the migrated ` +
          `group_acl groups for each entry — a blanket ` +
          `CONTENT_TYPE_ACL_SPEC → HiveGroup mapping cannot populate ` +
          `group_acl or recipient_witnesses. Remove the mapping or supply ` +
          `overrides.`,
      );
    }
  }
}

/** Bundle entry shape. One per `EncryptedContent` action on the source chain. */
type BundleEntry = {
  /** Original action hash from the OLD DNA (multibase holohash string). */
  old_action_hash: string;
  /** Source-chain action sequence number — preserved for diagnostic order. */
  action_seq: number;
  /** ISO timestamp of the original action — preserved for diagnostic order. */
  action_timestamp_iso: string;
  /** Decoded EncryptedContent payload. Replayed as-is on the new DNA except
   * for `revision_author_signing_public_key`, which is restamped, AND the
   * pass-2 `hive_genesis_hash` / `author_membership_hash` fields which are
   * re-resolved against the hive-bundle + the new DNA. */
  encrypted_content: {
    header: {
      id: string;
      /** Legacy field name (was `hive_id` in pass-1/2 bundles). The
       *  pass-3 wire field is `display_hive_id`; the bundle preserves
       *  the original property name so older exports round-trip. */
      hive_id: string;
      /** Pass-2 schema: present on bundles sourced from pass-2 DNAs;
       * absent on pass-1 bundles. The raw decoded value is a Uint8Array
       * (msgpack ActionHash); `import` ignores this field and re-resolves
       * the new-DNA hash from the hive-bundle keyed by `hive_id`. */
      hive_genesis_hash?: Uint8Array;
      /** Pass-2 schema. Carried for completeness; `import` re-resolves. */
      author_membership_hash?: Uint8Array | null;
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
  /** Pass-2 addition: present iff the entry was imported with a
   * `hive_genesis_hash` (always true for pass-2 imports, absent for
   * legacy pass-1 remaps). Multibase holohash string. */
  new_hive_genesis_hash_base64?: string;
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

/** Granted-membership record stored inside a `HiveBundleHive` (hive
 * layer) or a `GroupBundleGroup` (group layer). Shared shape. */
type GrantedMembership = {
  for_agent_base64: string;
  role: HiveRole;
  membership_hash_base64: string;
};

/** Per-group record stored inside a `HiveBundleHive.groups[]` (D.1
 * group-migration track). Mirror of `HiveBundleHive` one level down:
 * a migrated `GroupGenesis` on the new DNA + the memberships granted
 * into it. The `classification-overrides.json` mechanism resolves an
 * operator-supplied `owner_old_group_id` / `*_old_group_ids` set to
 * these records' `new_group_genesis_hash_base64` at import time. */
type GroupBundleGroup = {
  /** The squuid group_id on the OLD DNA — the key
   * `classification-overrides.json` references and the key
   * `grant-group-memberships` / `mark-group-migrated` match against.
   * Squuids are globally unique, so a group is locatable across all
   * hives in the bundle without its parent hive id. */
  old_group_id: string;
  /** Multibase holohash of the `GroupGenesis` action on the NEW DNA. */
  new_group_genesis_hash_base64: string;
  /** Display alias stamped on the new `GroupGenesis` (defaults to
   * `old_group_id` for continuity). */
  new_display_id: string;
  /** Pubkey of the agent that created the new GroupGenesis (implicit
   * group Owner via the integrity zome's group-author rule). */
  creator_pubkey_base64: string;
  /** `Some(role)` marks a hive-wide system role group (required hive
   * Owner to create); `null` marks an ordinary custom group. */
  hive_wide_role: HiveRole | null;
  /** Multibase holohash of the OLD-DNA `Group` content entry that
   * `mark-group-migrated` writes the V2 marker onto. `null` to defer
   * — the group is SKIPPED by `mark-group-migrated` with a warning. */
  old_marker_action_hash_base64: string | null;
  /** Memberships granted into this group via
   * `grant-group-memberships`. Sourced by the classifier's HiveGroup
   * override path to build both `public_key_acl` and
   * `recipient_witnesses`. */
  granted_memberships: GrantedMembership[];
};

/** Per-hive record inside a `HiveBundle`. */
type HiveBundleHive = {
  /** The squuid hive_id on the OLD DNA — the key the per-entry bundle's
   * `header.hive_id` is matched against during `import`. */
  old_hive_id: string;
  /** Multibase holohash of the `HiveGenesis` action on the NEW DNA. */
  new_genesis_hash_base64: string;
  /** Display alias stamped on the new `HiveGenesis` (defaults to
   * `old_hive_id` for continuity). */
  new_display_id: string;
  /** Pubkey of the agent that created the new HiveGenesis. Implicit
   * Owner of the new hive (no membership entry required). */
  owner_pubkey_base64: string;
  /** Always `null`: the owner is implicit Owner via the integrity
   * zome's "genesis author == implicit Owner" rule. Preserved as a
   * field for forward compatibility — a future migration that requires
   * an explicit owner membership could populate it. */
  owner_membership_hash_base64: string | null;
  /** Multibase holohash of the OLD-DNA entry that `mark-hive-migrated`
   * will write the V2 marker onto. `null` to defer — the hive will be
   * SKIPPED by `mark-hive-migrated` with a warning. */
  old_marker_action_hash_base64: string | null;
  /** Memberships granted to other agents via `grant-memberships`. */
  granted_memberships: GrantedMembership[];
  /** D.1 group-migration track: groups migrated under this hive via
   * `migrate-group`. Optional for back-compat — pre-D.1 bundles have
   * no `groups` key, and the hive-identity track never reads it. */
  groups?: GroupBundleGroup[];
};

type HiveBundle = {
  schema_version: 1;
  generated_at_iso: string;
  hives: HiveBundleHive[];
};

/** Per-entry override in `classification-overrides.json`. The operator
 * authors this file BEFORE running `import` to force specific old-DNA
 * entries into the `HiveGroup` variant (the classifier otherwise
 * defaults unknown content types to `Public`). `group_acl` is keyed by
 * OLD group squuids; the classifier resolves them to new
 * `GroupGenesis` hashes via the hive-bundle's `groups[]`. */
type ClassificationOverrideEntry = {
  kind: "HiveGroup";
  group_acl: {
    owner_old_group_id: string;
    admin_old_group_ids: string[];
    writer_old_group_ids: string[];
    reader_old_group_ids: string[];
  };
};

/** Parsed `classification-overrides.json`. `entries` is keyed by the
 * OLD-DNA action-hash (multibase) of the entry being overridden — the
 * same `old_action_hash` that appears in the per-entry bundle. */
type ClassificationOverrides = {
  schema_version: 1;
  entries: Record<string, ClassificationOverrideEntry>;
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

// ---------------------------------------------------------------------------
// Hive-bundle I/O helpers
// ---------------------------------------------------------------------------

/** Load + parse a hive-bundle file, or return an empty bundle if the file
 * does not exist yet (`migrate-hive` builds the bundle incrementally). */
async function loadHiveBundle(hiveBundlePath: string): Promise<HiveBundle> {
  let raw: string;
  try {
    raw = await readFile(hiveBundlePath, "utf8");
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === "ENOENT") {
      return {
        schema_version: 1,
        generated_at_iso: new Date().toISOString(),
        hives: [],
      };
    }
    throw err;
  }
  const parsed = JSON.parse(raw) as HiveBundle;
  if (parsed.schema_version !== 1) {
    throw new Error(
      `Unsupported hive-bundle schema_version: ${parsed.schema_version} (expected 1)`,
    );
  }
  return parsed;
}

async function saveHiveBundle(hiveBundlePath: string, bundle: HiveBundle): Promise<void> {
  await mkdir(dirname(hiveBundlePath), { recursive: true });
  await writeFile(hiveBundlePath, JSON.stringify(bundle, null, 2), "utf8");
}

function findHiveOrThrow(bundle: HiveBundle, oldHiveId: string): HiveBundleHive {
  const hive = bundle.hives.find((h) => h.old_hive_id === oldHiveId);
  if (!hive) {
    throw new Error(
      `Hive "${oldHiveId}" not found in hive-bundle. Run \`migrate-hive\` first.`,
    );
  }
  return hive;
}

/** Locate a migrated group by its OLD squuid across every hive's
 * `groups[]` in the bundle. Squuids are globally unique, so the parent
 * hive id is not needed for lookup. Returns the parent hive + the
 * group record, or `null` if the group has not been migrated yet. */
function findGroupAcrossHives(
  bundle: HiveBundle,
  oldGroupId: string,
): { hive: HiveBundleHive; group: GroupBundleGroup } | null {
  for (const hive of bundle.hives) {
    const group = hive.groups?.find((g) => g.old_group_id === oldGroupId);
    if (group) return { hive, group };
  }
  return null;
}

/** Same as [`findGroupAcrossHives`] but throws a clear, actionable
 * error when the group is missing. Used by the classifier override
 * path + `grant-group-memberships` + `mark-group-migrated`. */
function findGroupOrThrow(
  bundle: HiveBundle,
  oldGroupId: string,
): { hive: HiveBundleHive; group: GroupBundleGroup } {
  const found = findGroupAcrossHives(bundle, oldGroupId);
  if (!found) {
    throw new Error(
      `Group "${oldGroupId}" not found in any hive's groups[] in the ` +
        `hive-bundle. Run \`migrate-group\` for it first.`,
    );
  }
  return found;
}

/** Load + parse `classification-overrides.json`, or return `null` when
 * no path is supplied (the common case — most imports use the
 * content-type classifier + Public default). Validates schema_version
 * and the per-entry `kind`. */
async function loadClassificationOverrides(
  overridesPath: string | undefined,
): Promise<ClassificationOverrides | null> {
  if (!overridesPath) return null;
  const parsed = JSON.parse(
    await readFile(overridesPath, "utf8"),
  ) as ClassificationOverrides;
  if (parsed.schema_version !== 1) {
    throw new Error(
      `Unsupported classification-overrides schema_version: ` +
        `${parsed.schema_version} (expected 1)`,
    );
  }
  for (const [oldActionHash, entry] of Object.entries(parsed.entries ?? {})) {
    if (entry.kind !== "HiveGroup") {
      throw new Error(
        `classification-override for ${oldActionHash} has unsupported ` +
          `kind "${entry.kind}" (only "HiveGroup" is supported)`,
      );
    }
    if (!entry.group_acl || typeof entry.group_acl.owner_old_group_id !== "string") {
      throw new Error(
        `classification-override for ${oldActionHash} is missing a valid ` +
          `group_acl.owner_old_group_id`,
      );
    }
    for (const field of [
      "admin_old_group_ids",
      "writer_old_group_ids",
      "reader_old_group_ids",
    ] as const) {
      if (!Array.isArray(entry.group_acl[field])) {
        throw new Error(
          `classification-override for ${oldActionHash} has a non-array ` +
            `group_acl.${field} (expected an array of old group squuids; ` +
            `use [] for an empty bucket)`,
        );
      }
    }
  }
  return parsed;
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
  // Header pass-2 fields (hive_genesis_hash, author_membership_hash)
  // are also Uint8Array on the wire; preserve as base64 strings under
  // a `_base64` suffix so the import side can decode them back without
  // ambiguity. (Import re-resolves these from the hive-bundle, but the
  // bytes carry diagnostic value for debugging mismatches.)
  const serializable = {
    ...bundle,
    entries: bundle.entries.map((e) => ({
      ...e,
      encrypted_content: {
        ...e.encrypted_content,
        header: {
          ...e.encrypted_content.header,
          hive_genesis_hash: e.encrypted_content.header.hive_genesis_hash
            ? encodeHashToBase64(e.encrypted_content.header.hive_genesis_hash as ActionHash)
            : undefined,
          author_membership_hash:
            e.encrypted_content.header.author_membership_hash != null
              ? encodeHashToBase64(
                  e.encrypted_content.header.author_membership_hash as ActionHash,
                )
              : e.encrypted_content.header.author_membership_hash,
        },
        bytes: Buffer.from(e.encrypted_content.bytes).toString("base64"),
      },
    })),
  };
  await mkdir(dirname(outPath), { recursive: true });
  await writeFile(outPath, JSON.stringify(serializable, null, 2), "utf8");
  console.log(`[export] wrote bundle: ${outPath} (${entries.length} entries)`);
  await appWebsocket.client.close();
}

// ---------------------------------------------------------------------------
// Hive-identity track
// ---------------------------------------------------------------------------

async function doMigrateHive(
  newAppId: string,
  oldHiveId: string,
  oldAnchorActionHashB64: string,
  hiveBundlePath: string,
): Promise<void> {
  if (!oldHiveId) throw new Error("old-hive-id must be a non-empty string");
  console.log(
    `[migrate-hive] loading hive-bundle from ${hiveBundlePath} ` +
      `(will create if missing)...`,
  );
  const bundle = await loadHiveBundle(hiveBundlePath);
  if (bundle.hives.some((h) => h.old_hive_id === oldHiveId)) {
    throw new Error(
      `Hive "${oldHiveId}" already present in ${hiveBundlePath}. ` +
        `Refusing to overwrite — delete the entry or use a different bundle.`,
    );
  }

  console.log(
    `[migrate-hive] connecting to new app "${newAppId}" on port ${ADMIN_PORT}...`,
  );
  const { appWebsocket, cellId, agentPubKey } = await connectAppWs(newAppId);
  const ownerPubkeyB64 = encodeHashToBase64(agentPubKey);
  console.log(`[migrate-hive] connected. owner=${ownerPubkeyB64}`);

  console.log(
    `[migrate-hive] creating HiveGenesis with display_id=${JSON.stringify(oldHiveId)}...`,
  );
  const response = (await appWebsocket.callZome({
    cell_id: cellId,
    zome_name: ZOME_NAME,
    fn_name: "create_hive_genesis",
    payload: { display_id: oldHiveId },
  })) as { genesis: { display_id: string }; hash: ActionHash };
  const newGenesisHashB64 = encodeHashToBase64(response.hash);
  console.log(`[migrate-hive] created. new_genesis_hash=${newGenesisHashB64}`);

  const oldMarkerHash = oldAnchorActionHashB64.trim();
  bundle.hives.push({
    old_hive_id: oldHiveId,
    new_genesis_hash_base64: newGenesisHashB64,
    new_display_id: response.genesis.display_id,
    owner_pubkey_base64: ownerPubkeyB64,
    owner_membership_hash_base64: null,
    old_marker_action_hash_base64: oldMarkerHash === "" ? null : oldMarkerHash,
    granted_memberships: [],
  });
  bundle.generated_at_iso = new Date().toISOString();
  await saveHiveBundle(hiveBundlePath, bundle);
  console.log(
    `[migrate-hive] hive-bundle updated: ${hiveBundlePath} ` +
      `(${bundle.hives.length} hives total)`,
  );
  if (oldMarkerHash === "") {
    console.warn(
      `[migrate-hive] NOTE: old_marker_action_hash is null for "${oldHiveId}". ` +
        `mark-hive-migrated will SKIP this hive. Edit the bundle JSON or ` +
        `re-run migrate-hive on a fresh bundle to set it.`,
    );
  }
  await appWebsocket.client.close();
}

async function doGrantMemberships(
  newAppId: string,
  hiveBundlePath: string,
  oldHiveId: string,
  role: HiveRole,
  memberPubkeysB64: string[],
): Promise<void> {
  if (!HIVE_ROLES.includes(role)) {
    throw new Error(
      `Unknown role "${role}". Expected one of: ${HIVE_ROLES.join(", ")}`,
    );
  }
  if (memberPubkeysB64.length === 0) {
    throw new Error("grant-memberships requires at least one member pubkey");
  }
  const bundle = await loadHiveBundle(hiveBundlePath);
  const hive = findHiveOrThrow(bundle, oldHiveId);
  const genesisHash = decodeHashFromBase64(hive.new_genesis_hash_base64);

  console.log(
    `[grant-memberships] connecting to new app "${newAppId}" on port ${ADMIN_PORT}...`,
  );
  const { appWebsocket, cellId, agentPubKey } = await connectAppWs(newAppId);
  const callerPubkeyB64 = encodeHashToBase64(agentPubKey);
  if (callerPubkeyB64 !== hive.owner_pubkey_base64) {
    console.warn(
      `[grant-memberships] WARNING: caller pubkey ${callerPubkeyB64} differs ` +
        `from hive owner ${hive.owner_pubkey_base64}. The integrity zome ` +
        `accepts grants from non-owners only if the caller holds Admin+ in ` +
        `this hive — proceed with caution.`,
    );
  }

  let succeeded = 0;
  const failures: { for_agent_base64: string; error: string }[] = [];
  for (const memberB64 of memberPubkeysB64) {
    try {
      const agent = decodeHashFromBase64(memberB64);
      const response = (await appWebsocket.callZome({
        cell_id: cellId,
        zome_name: ZOME_NAME,
        fn_name: "create_hive_membership",
        payload: {
          hive_genesis_hash: genesisHash,
          for_agent: agent,
          role,
          grantor_membership_hash: null,
          expiry: null,
        },
      })) as { hash: ActionHash };
      hive.granted_memberships.push({
        for_agent_base64: memberB64,
        role,
        membership_hash_base64: encodeHashToBase64(response.hash),
      });
      succeeded++;
      process.stdout.write(".");
    } catch (err) {
      failures.push({ for_agent_base64: memberB64, error: String(err) });
      process.stdout.write("F");
    }
  }
  process.stdout.write("\n");

  bundle.generated_at_iso = new Date().toISOString();
  await saveHiveBundle(hiveBundlePath, bundle);
  console.log(
    `[grant-memberships] granted ${succeeded} ${role} membership(s) ` +
      `for hive "${oldHiveId}"; ${failures.length} failed.`,
  );
  if (failures.length > 0) {
    for (const f of failures) {
      console.error(`  FAILED for ${f.for_agent_base64}: ${f.error}`);
    }
    process.exit(1);
  }
  await appWebsocket.client.close();
}

async function doMarkHiveMigrated(
  oldAppId: string,
  hiveBundlePath: string,
): Promise<void> {
  const bundle = await loadHiveBundle(hiveBundlePath);
  const targets = bundle.hives.filter(
    (h) => h.old_marker_action_hash_base64 != null,
  );
  const skipped = bundle.hives.length - targets.length;
  if (skipped > 0) {
    console.warn(
      `[mark-hive-migrated] skipping ${skipped} hive(s) without ` +
        `old_marker_action_hash_base64 set (run migrate-hive with the ` +
        `old anchor hash, or edit the bundle JSON to populate).`,
    );
  }
  if (targets.length === 0) {
    console.log(`[mark-hive-migrated] nothing to do.`);
    return;
  }

  const newDnaHashBase64 = process.env.NEW_DNA_HASH_BASE64 ?? "";
  if (!newDnaHashBase64) {
    console.warn(
      `[mark-hive-migrated] WARNING: NEW_DNA_HASH_BASE64 not set; markers ` +
        `will carry new_dna_hash_base64="". Set via ` +
        `\`NEW_DNA_HASH_BASE64=$(hc dna hash <new.dna>) ...\` to populate.`,
    );
  }

  console.log(
    `[mark-hive-migrated] connecting to old app "${oldAppId}" on port ${ADMIN_PORT}...`,
  );
  const { appWebsocket, cellId } = await connectAppWs(oldAppId);

  const migratedAtMicroseconds = Date.now() * 1000;
  // `new_app_id` for hive-identity markers points at the NEW app
  // (the one carrying the new HiveGenesis). The hive-bundle does not
  // record it (multiple new apps could share one hive-bundle in
  // theory), so we fall back to the NEW_APP_ID env var. Empty string
  // is acceptable: receivers can still resolve the new DNA via
  // `new_dna_hash_base64` + the genesis hash — but if BOTH env vars
  // are unset, receivers have no resolution path from the marker
  // payload alone and depend on out-of-band info, so we warn parallel
  // to NEW_DNA_HASH_BASE64.
  const newAppId = process.env.NEW_APP_ID ?? "";
  if (!newAppId) {
    console.warn(
      `[mark-hive-migrated] WARNING: NEW_APP_ID not set; markers will ` +
        `carry new_app_id="". Set via \`NEW_APP_ID=<installed_app_id> ...\` ` +
        `to populate.`,
    );
  }
  let succeeded = 0;
  const failures: { old_hive_id: string; error: string }[] = [];
  for (const hive of targets) {
    // Filter above guarantees this is non-null.
    const oldMarkerHashB64 = hive.old_marker_action_hash_base64!;
    const marker = {
      schema_tag: MIGRATION_MARKER_SCHEMA_TAG,
      schema_version: 2,
      new_dna_hash_base64: newDnaHashBase64,
      new_action_hash_base64: hive.new_genesis_hash_base64,
      new_app_id: newAppId,
      migrated_at_microseconds: migratedAtMicroseconds,
      new_hive_genesis_hash_base64: hive.new_genesis_hash_base64,
      new_hive_genesis_display_id: hive.new_display_id,
    };
    try {
      await appWebsocket.callZome({
        cell_id: cellId,
        zome_name: ZOME_NAME,
        fn_name: "mark_migrated_v2",
        payload: {
          original_action_hash: decodeHashFromBase64(oldMarkerHashB64),
          marker,
        },
      });
      succeeded++;
      process.stdout.write(".");
    } catch (err) {
      failures.push({ old_hive_id: hive.old_hive_id, error: String(err) });
      process.stdout.write("F");
    }
  }
  process.stdout.write("\n");
  console.log(
    `[mark-hive-migrated] wrote ${succeeded} V2 marker(s); ${failures.length} failed.`,
  );
  if (failures.length > 0) {
    for (const f of failures) {
      console.error(`  FAILED for hive "${f.old_hive_id}": ${f.error}`);
    }
    process.exit(1);
  }
  await appWebsocket.client.close();
}

// ---------------------------------------------------------------------------
// Group-identity track (D.1)
// ---------------------------------------------------------------------------

// Witness-bucket dominance helpers (`roleRank`, `rankToBucket`,
// `cappedWitnessRank`) live in `./acl-bucket.ts` so they are unit-
// testable without importing this CLI module.

async function doMigrateGroup(
  newAppId: string,
  hiveBundlePath: string,
  oldHiveId: string,
  oldGroupId: string,
  hiveWideRole: HiveRole | null,
  creatorMembershipHashB64: string | null,
  oldMarkerActionHashB64: string | null,
): Promise<void> {
  if (!oldGroupId) throw new Error("old-group-id must be a non-empty string");
  const bundle = await loadHiveBundle(hiveBundlePath);
  const hive = findHiveOrThrow(bundle, oldHiveId);
  if (findGroupAcrossHives(bundle, oldGroupId)) {
    throw new Error(
      `Group "${oldGroupId}" already present in the hive-bundle. Refusing ` +
        `to overwrite — delete the entry or use a different bundle.`,
    );
  }

  console.log(
    `[migrate-group] connecting to new app "${newAppId}" on port ${ADMIN_PORT}...`,
  );
  const { appWebsocket, cellId, agentPubKey } = await connectAppWs(newAppId);
  const creatorPubkeyB64 = encodeHashToBase64(agentPubKey);
  console.log(`[migrate-group] connected. creator=${creatorPubkeyB64}`);

  console.log(
    `[migrate-group] creating GroupGenesis display_id=${JSON.stringify(oldGroupId)} ` +
      `in hive "${oldHiveId}" (hive_wide_role=${hiveWideRole ?? "null"})...`,
  );
  const response = (await appWebsocket.callZome({
    cell_id: cellId,
    zome_name: ZOME_NAME,
    fn_name: "create_group_genesis",
    payload: {
      hive_genesis_hash: decodeHashFromBase64(hive.new_genesis_hash_base64),
      display_id: oldGroupId,
      hive_wide_role: hiveWideRole,
      creator_hive_membership_hash:
        creatorMembershipHashB64 && creatorMembershipHashB64 !== ""
          ? decodeHashFromBase64(creatorMembershipHashB64)
          : null,
    },
  })) as { genesis: { display_id: string }; hash: ActionHash };
  const newGroupGenesisHashB64 = encodeHashToBase64(response.hash);
  console.log(
    `[migrate-group] created. new_group_genesis_hash=${newGroupGenesisHashB64}`,
  );

  const oldMarkerHash = (oldMarkerActionHashB64 ?? "").trim();
  (hive.groups ??= []).push({
    old_group_id: oldGroupId,
    new_group_genesis_hash_base64: newGroupGenesisHashB64,
    new_display_id: response.genesis.display_id,
    creator_pubkey_base64: creatorPubkeyB64,
    hive_wide_role: hiveWideRole,
    old_marker_action_hash_base64: oldMarkerHash === "" ? null : oldMarkerHash,
    granted_memberships: [],
  });
  bundle.generated_at_iso = new Date().toISOString();
  await saveHiveBundle(hiveBundlePath, bundle);
  console.log(
    `[migrate-group] hive-bundle updated: ${hiveBundlePath} ` +
      `(hive "${oldHiveId}" now has ${hive.groups.length} group(s))`,
  );
  if (oldMarkerHash === "") {
    console.warn(
      `[migrate-group] NOTE: old_marker_action_hash is null for "${oldGroupId}". ` +
        `mark-group-migrated will SKIP this group. Pass --old-marker <hash-b64> ` +
        `or edit the bundle JSON to set it.`,
    );
  }
  await appWebsocket.client.close();
}

async function doGrantGroupMemberships(
  newAppId: string,
  hiveBundlePath: string,
  oldGroupId: string,
  role: HiveRole,
  memberPubkeysB64: string[],
): Promise<void> {
  if (!HIVE_ROLES.includes(role)) {
    throw new Error(
      `Unknown role "${role}". Expected one of: ${HIVE_ROLES.join(", ")}`,
    );
  }
  if (memberPubkeysB64.length === 0) {
    throw new Error("grant-group-memberships requires at least one member pubkey");
  }
  const bundle = await loadHiveBundle(hiveBundlePath);
  const { group } = findGroupOrThrow(bundle, oldGroupId);
  const groupGenesisHash = decodeHashFromBase64(group.new_group_genesis_hash_base64);

  console.log(
    `[grant-group-memberships] connecting to new app "${newAppId}" on port ${ADMIN_PORT}...`,
  );
  const { appWebsocket, cellId, agentPubKey } = await connectAppWs(newAppId);
  const callerPubkeyB64 = encodeHashToBase64(agentPubKey);
  if (callerPubkeyB64 !== group.creator_pubkey_base64) {
    console.warn(
      `[grant-group-memberships] WARNING: caller pubkey ${callerPubkeyB64} ` +
        `differs from group creator ${group.creator_pubkey_base64}. The ` +
        `integrity zome accepts grants only from agents holding group Admin+ ` +
        `(Path A/B/C) — proceed with caution.`,
    );
  }
  // The group creator is the implicit group Owner (Path A); they grant
  // with grantor_membership_hash = null. A non-creator caller must
  // supply their own Path-C group-membership witness by editing the
  // bundle / extending this command — the common migration path is the
  // creator granting, so null is the default.
  let succeeded = 0;
  const failures: { for_agent_base64: string; error: string }[] = [];
  for (const memberB64 of memberPubkeysB64) {
    try {
      const response = (await appWebsocket.callZome({
        cell_id: cellId,
        zome_name: ZOME_NAME,
        fn_name: "create_group_membership",
        payload: {
          group_genesis_hash: groupGenesisHash,
          for_agent: decodeHashFromBase64(memberB64),
          role,
          grantor_membership_hash: null,
          grantor_hive_membership_hash: null,
          expiry: null,
        },
      })) as { hash: ActionHash };
      group.granted_memberships.push({
        for_agent_base64: memberB64,
        role,
        membership_hash_base64: encodeHashToBase64(response.hash),
      });
      succeeded++;
      process.stdout.write(".");
    } catch (err) {
      failures.push({ for_agent_base64: memberB64, error: String(err) });
      process.stdout.write("F");
    }
  }
  process.stdout.write("\n");

  bundle.generated_at_iso = new Date().toISOString();
  await saveHiveBundle(hiveBundlePath, bundle);
  console.log(
    `[grant-group-memberships] granted ${succeeded} ${role} membership(s) ` +
      `for group "${oldGroupId}"; ${failures.length} failed.`,
  );
  if (failures.length > 0) {
    for (const f of failures) {
      console.error(`  FAILED for ${f.for_agent_base64}: ${f.error}`);
    }
    process.exit(1);
  }
  await appWebsocket.client.close();
}

async function doMarkGroupMigrated(
  oldAppId: string,
  hiveBundlePath: string,
): Promise<void> {
  const bundle = await loadHiveBundle(hiveBundlePath);
  const allGroups = bundle.hives.flatMap((h) => h.groups ?? []);
  const targets = allGroups.filter((g) => g.old_marker_action_hash_base64 != null);
  const skipped = allGroups.length - targets.length;
  if (skipped > 0) {
    console.warn(
      `[mark-group-migrated] skipping ${skipped} group(s) without ` +
        `old_marker_action_hash_base64 set (run migrate-group with ` +
        `--old-marker, or edit the bundle JSON to populate).`,
    );
  }
  if (targets.length === 0) {
    console.log(`[mark-group-migrated] nothing to do.`);
    return;
  }

  const newDnaHashBase64 = process.env.NEW_DNA_HASH_BASE64 ?? "";
  if (!newDnaHashBase64) {
    console.warn(
      `[mark-group-migrated] WARNING: NEW_DNA_HASH_BASE64 not set; markers ` +
        `will carry new_dna_hash_base64="". Set via ` +
        `\`NEW_DNA_HASH_BASE64=$(hc dna hash <new.dna>) ...\` to populate.`,
    );
  }
  const newAppId = process.env.NEW_APP_ID ?? "";
  if (!newAppId) {
    console.warn(
      `[mark-group-migrated] WARNING: NEW_APP_ID not set; markers will ` +
        `carry new_app_id="".`,
    );
  }

  console.log(
    `[mark-group-migrated] connecting to old app "${oldAppId}" on port ${ADMIN_PORT}...`,
  );
  const { appWebsocket, cellId } = await connectAppWs(oldAppId);

  const migratedAtMicroseconds = Date.now() * 1000;
  let succeeded = 0;
  const failures: { old_group_id: string; error: string }[] = [];
  for (const group of targets) {
    const oldMarkerHashB64 = group.old_marker_action_hash_base64!;
    // The group marker points members at the NEW GroupGenesis action
    // hash via `new_hive_genesis_hash_base64` (reused field — the V2
    // marker schema has no group-specific slot, so the group genesis
    // travels in the same field the hive-identity track uses for the
    // hive genesis; receivers disambiguate by the marked entry's
    // content_type).
    const marker = {
      schema_tag: MIGRATION_MARKER_SCHEMA_TAG,
      schema_version: 2,
      new_dna_hash_base64: newDnaHashBase64,
      new_action_hash_base64: group.new_group_genesis_hash_base64,
      new_app_id: newAppId,
      migrated_at_microseconds: migratedAtMicroseconds,
      new_hive_genesis_hash_base64: group.new_group_genesis_hash_base64,
      new_hive_genesis_display_id: group.new_display_id,
    };
    try {
      await appWebsocket.callZome({
        cell_id: cellId,
        zome_name: ZOME_NAME,
        fn_name: "mark_migrated_v2",
        payload: {
          original_action_hash: decodeHashFromBase64(oldMarkerHashB64),
          marker,
        },
      });
      succeeded++;
      process.stdout.write(".");
    } catch (err) {
      failures.push({ old_group_id: group.old_group_id, error: String(err) });
      process.stdout.write("F");
    }
  }
  process.stdout.write("\n");
  console.log(
    `[mark-group-migrated] wrote ${succeeded} V2 marker(s); ${failures.length} failed.`,
  );
  if (failures.length > 0) {
    for (const f of failures) {
      console.error(`  FAILED for group "${f.old_group_id}": ${f.error}`);
    }
    process.exit(1);
  }
  await appWebsocket.client.close();
}

// ---------------------------------------------------------------------------
// Per-entry track
// ---------------------------------------------------------------------------

/** Decode the on-disk bundle's per-entry shape (where `bytes` and the
 * optional pass-2 header hashes are base64 strings) back into the wire
 * shape (Uint8Array everywhere). Mirror of the encode block in
 * `doExport`. */
type SerializedBundleEntry = Omit<BundleEntry, "encrypted_content"> & {
  encrypted_content: {
    header: Omit<
      BundleEntry["encrypted_content"]["header"],
      "hive_genesis_hash" | "author_membership_hash"
    > & {
      hive_genesis_hash?: string;
      author_membership_hash?: string | null;
    };
    bytes: string;
  };
};

/** Build a pass-4 `AclSpec::HiveGroup` wire value + matching
 * `public_key_acl` for an overridden entry. Resolves the override's
 * OLD group squuids to NEW `GroupGenesis` hashes via the hive-bundle,
 * then derives both the recipient set AND the `recipient_witnesses`
 * from the live group rosters (confirmed via
 * `get_latest_group_membership`).
 *
 * ## Witness/PKA derivation
 *
 * The recipient set is sourced from the migrated groups' members
 * (recorded by `grant-group-memberships`), NOT from the legacy
 * `public_key_acl` — legacy PKA carries OLD-DNA pubkeys, whereas the
 * grant step used NEW-DNA pubkeys. For each member, the witness bucket
 * is `min(group_bucket, member_role)` by rank (so a Reader-role member
 * of an admin-bucket group is represented as a Reader witness), capped
 * at Admin so the single-string `public_key_acl.owner` slot is never
 * contended (an owner-group member is validly representable as an
 * Admin-bucket witness via the validator's bucket-dominance rule:
 * Admin accepts owner∪admin groups and Owner-role satisfies Admin).
 * Each member is stamped exactly once at the highest bucket they
 * qualify for across all groups they appear in (dedup), satisfying
 * the validator's one-witness-per-pubkey + bidirectional cross-check.
 *
 * A member whose `get_latest_group_membership` lookup returns null
 * (revoked / expired since the grant) is dropped with a warning — it
 * is correctly absent from both the PKA and the witnesses, so the
 * entry stays valid. */
async function buildHiveGroupAclSpec(
  override: ClassificationOverrideEntry,
  parentHive: HiveBundleHive,
  hiveBundle: HiveBundle,
  authorMembershipHashBytes: Uint8Array | null,
  callerPubkey: AgentPubKey,
  appWebsocket: AppWebsocket,
  cellId: CellId,
): Promise<{ acl_spec: unknown; public_key_acl: unknown; display_hive_id: string | null }> {
  const resolve = (oldGroupId: string): GroupBundleGroup =>
    findGroupOrThrow(hiveBundle, oldGroupId).group;
  const ownerGroup = resolve(override.group_acl.owner_old_group_id);
  const adminGroups = override.group_acl.admin_old_group_ids.map(resolve);
  const writerGroups = override.group_acl.writer_old_group_ids.map(resolve);
  const readerGroups = override.group_acl.reader_old_group_ids.map(resolve);
  const allGroups = [ownerGroup, ...adminGroups, ...writerGroups, ...readerGroups];

  const group_acl = {
    owner: decodeHashFromBase64(ownerGroup.new_group_genesis_hash_base64),
    admin: adminGroups.map((g) => decodeHashFromBase64(g.new_group_genesis_hash_base64)),
    writer: writerGroups.map((g) => decodeHashFromBase64(g.new_group_genesis_hash_base64)),
    reader: readerGroups.map((g) => decodeHashFromBase64(g.new_group_genesis_hash_base64)),
  };

  // Resolve the IMPORTER's own authorising group membership so the
  // integrity validator's per-group `check_group_authority` can pass
  // via Path C (explicit group membership) for an importer who is a
  // group member but neither the group creator (Path A) nor a hive
  // Admin+ (Path B). Without this the validator would reject every
  // overridden HiveGroup entry for a hive-Writer importer (the
  // author_group_membership_hash would be null → Path C unavailable).
  // We stamp the caller's membership in the first group_acl group they
  // hold one in (owner bucket first — the same witness is reused across
  // all groups by the validator, matching its documented single-witness
  // simplification). `null` is correct + sufficient when the importer
  // IS the group creator or a hive Admin+ (Path A / Path B).
  let authorGroupMembershipHash: Uint8Array | null = null;
  for (const g of allGroups) {
    const mine = (await appWebsocket.callZome({
      cell_id: cellId,
      zome_name: ZOME_NAME,
      fn_name: "get_latest_group_membership",
      payload: {
        agent: callerPubkey,
        group_genesis_hash: decodeHashFromBase64(g.new_group_genesis_hash_base64),
      },
    })) as { hash: ActionHash } | null;
    if (mine) {
      authorGroupMembershipHash = mine.hash;
      break;
    }
  }

  // Per-pubkey: track the highest achievable witness bucket + the
  // group whose roster proves it. `groupBucketRank` is the rank of the
  // group_acl bucket the group sits in; the achievable witness rank is
  // min(groupBucketRank, memberRoleRank) capped at Admin (3).
  type Candidate = { rank: number; group: GroupBundleGroup };
  const best = new Map<string, Candidate>();
  const consider = (groups: GroupBundleGroup[], groupBucketRank: number) => {
    for (const g of groups) {
      for (const m of g.granted_memberships) {
        const achievable = cappedWitnessRank(groupBucketRank, m.role);
        const existing = best.get(m.for_agent_base64);
        if (!existing || achievable > existing.rank) {
          best.set(m.for_agent_base64, { rank: achievable, group: g });
        }
      }
    }
  };
  consider([ownerGroup], roleRank("Owner"));
  consider(adminGroups, roleRank("Admin"));
  consider(writerGroups, roleRank("Writer"));
  consider(readerGroups, roleRank("Reader"));

  const witnesses: { pubkey: Uint8Array; bucket: HiveRole; membership_hash: Uint8Array }[] = [];
  const public_key_acl = { owner: "", admin: [] as string[], writer: [] as string[], reader: [] as string[] };
  // Array.from(...) keeps this a real-array for-of (the bare
  // `for (… of best)` Map iteration would trip TS2802 under this
  // repo's pre-es2015 downlevel target, adding a new tsc error).
  for (const [pubkeyB64, candidate] of Array.from(best.entries())) {
    const agent = decodeHashFromBase64(pubkeyB64);
    const genesisHash = decodeHashFromBase64(candidate.group.new_group_genesis_hash_base64);
    // Re-verify the membership is live (re-grants supersede the bundle
    // hash; revocations drop the member). Use the authoritative latest
    // hash from the lookup, not the bundle's recorded one.
    const live = (await appWebsocket.callZome({
      cell_id: cellId,
      zome_name: ZOME_NAME,
      fn_name: "get_latest_group_membership",
      payload: { agent, group_genesis_hash: genesisHash },
    })) as { hash: ActionHash } | null;
    if (!live) {
      console.warn(
        `[import] dropping ${pubkeyB64} from HiveGroup recipients: no live ` +
          `GroupMembership in group "${candidate.group.old_group_id}" ` +
          `(revoked or expired since grant).`,
      );
      continue;
    }
    const bucket = rankToBucket(candidate.rank);
    witnesses.push({ pubkey: agent, bucket, membership_hash: live.hash });
    if (bucket === "Admin") public_key_acl.admin.push(pubkeyB64);
    else if (bucket === "Writer") public_key_acl.writer.push(pubkeyB64);
    else public_key_acl.reader.push(pubkeyB64);
  }

  return {
    acl_spec: {
      HiveGroup: {
        hive_genesis_hash: decodeHashFromBase64(parentHive.new_genesis_hash_base64),
        author_membership_hash: authorMembershipHashBytes,
        group_acl,
        author_group_membership_hash: authorGroupMembershipHash,
        recipient_witnesses: witnesses,
      },
    },
    public_key_acl,
    display_hive_id: null,
  };
}

async function doImport(
  appId: string,
  bundlePath: string,
  hiveBundlePath: string,
  remapPath: string,
  overridesPath?: string,
): Promise<void> {
  console.log(`[import] reading bundle from ${bundlePath}...`);
  const raw = JSON.parse(await readFile(bundlePath, "utf8")) as {
    schema_version: number;
    source_app_id: string;
    source_agent_pubkey_base64: string;
    exported_at_iso: string;
    entries: SerializedBundleEntry[];
  };
  // Bundle schema_version contract:
  //  - 1 = pre-pass-3 (every legacy export tagged 1; pass-1/2 wire shape).
  //  - 2 = pass-3 (new wire shape in the exported header — currently
  //        produced only by hypothetical pass-3-aware exports; absent in
  //        practice today).
  // The classifier below restamps schema_version 1 bundles into the
  // pass-3 wire shape on import; the operator does not need to
  // pre-translate. Anything other than {1, 2} is unknown.
  if (raw.schema_version !== 1 && raw.schema_version !== 2) {
    throw new Error(
      `Unsupported bundle schema_version: ${raw.schema_version} (expected 1 or 2)`,
    );
  }
  if (raw.schema_version === 2) {
    console.log(
      `[import] bundle schema_version=2 (pass-3 wire shape); restamp ` +
        `via classifier still applies for cross-DNA migration`,
    );
  }
  console.log(
    `[import] bundle from ${raw.source_app_id} (${raw.source_agent_pubkey_base64}) ` +
      `with ${raw.entries.length} entries.`,
  );

  console.log(`[import] reading hive-bundle from ${hiveBundlePath}...`);
  const hiveBundle = await loadHiveBundle(hiveBundlePath);
  if (hiveBundle.hives.length === 0) {
    throw new Error(
      `Hive-bundle ${hiveBundlePath} is empty. Run \`migrate-hive\` for ` +
        `each hive present in the bundle before importing.`,
    );
  }
  const hivesByOldId = new Map(hiveBundle.hives.map((h) => [h.old_hive_id, h]));
  console.log(`[import] ${hiveBundle.hives.length} hive mapping(s) loaded.`);

  const overrides = await loadClassificationOverrides(overridesPath);
  if (overrides) {
    const count = Object.keys(overrides.entries ?? {}).length;
    console.log(
      `[import] loaded ${count} classification-override(s) from ${overridesPath} ` +
        `(matching entries import as AclSpec::HiveGroup).`,
    );
  }

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

  // Resolve author_membership_hash for each hive ONCE up-front. Two
  // cases:
  // - Caller IS the new hive's owner (genesis author) → null (implicit
  //   Owner; no membership entry required by the integrity zome).
  // - Otherwise → call `get_latest_membership` and use the returned
  //   hash. If `None`, this caller cannot import entries for this
  //   hive — pre-fail every affected entry with a clear error.
  const membershipByGenesisB64 = new Map<string, ActionHash | null>();
  const blockedHiveIds = new Set<string>();
  for (const hive of hiveBundle.hives) {
    if (hive.owner_pubkey_base64 === targetAgentBase64) {
      membershipByGenesisB64.set(hive.new_genesis_hash_base64, null);
      continue;
    }
    const response = (await appWebsocket.callZome({
      cell_id: cellId,
      zome_name: ZOME_NAME,
      fn_name: "get_latest_membership",
      payload: {
        agent: agentPubKey,
        hive_genesis_hash: decodeHashFromBase64(hive.new_genesis_hash_base64),
      },
    })) as { hash: ActionHash } | null;
    if (!response) {
      blockedHiveIds.add(hive.old_hive_id);
      console.warn(
        `[import] no membership for ${targetAgentBase64} in hive ` +
          `"${hive.old_hive_id}" — entries in this hive will fail. ` +
          `Ask the hive owner to run grant-memberships for your pubkey.`,
      );
      continue;
    }
    membershipByGenesisB64.set(hive.new_genesis_hash_base64, response.hash);
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
    const hive = hivesByOldId.get(header.hive_id);
    if (!hive) {
      remap.failures.push({
        id: header.id,
        old_action_hash: entry.old_action_hash,
        error: `hive_not_in_hive_bundle: ${header.hive_id}`,
      });
      process.stdout.write("F");
      continue;
    }
    if (blockedHiveIds.has(hive.old_hive_id)) {
      remap.failures.push({
        id: header.id,
        old_action_hash: entry.old_action_hash,
        error: `no_membership_in_new_hive: ${hive.old_hive_id}`,
      });
      process.stdout.write("F");
      continue;
    }
    const authorMembershipHash =
      membershipByGenesisB64.get(hive.new_genesis_hash_base64) ?? null;
    // Classify the entry into one of the four AclSpec variants. Order:
    //  1. A classification-override (D.1) forces AclSpec::HiveGroup,
    //     resolving the override's old group squuids to new
    //     GroupGenesis hashes + stamping recipient_witnesses from the
    //     live group rosters.
    //  2. Otherwise the content_type → AclSpec classifier
    //     (CONTENT_TYPE_ACL_SPEC) + Public default applies; it handles
    //     the DirectMessage author-pubkey restamp and inlines the
    //     hive_genesis_hash + author_membership_hash into
    //     HiveGroup/Public variants. Legacy `header.acl` is NOT carried
    //     over — pass-3+ HiveGroup uses an ActionHash-keyed group_acl.
    const hiveGenesisHashBytes = decodeHashFromBase64(hive.new_genesis_hash_base64);
    const override = overrides?.entries[entry.old_action_hash];
    let classified;
    try {
      classified = override
        ? await buildHiveGroupAclSpec(
            override,
            hive,
            hiveBundle,
            authorMembershipHash,
            agentPubKey,
            appWebsocket,
            cellId,
          )
        : classifyAclSpec(
            header.content_type,
            hiveGenesisHashBytes,
            authorMembershipHash,
            targetAgentBase64,
            header.public_key_acl,
          );
    } catch (err) {
      remap.failures.push({
        id: header.id,
        old_action_hash: entry.old_action_hash,
        error: `classify_acl_spec_failed: ${String(err)}`,
      });
      process.stdout.write("F");
      continue;
    }
    // Restamp the signing pubkey to match the new agent. The integrity
    // zome enforces action.author == header.revision_author_signing_public_key
    // (`check_author_matches_header`) — failing this would invalidate every
    // committed entry.
    const input = {
      id: header.id,
      display_hive_id: classified.display_hive_id ?? header.hive_id,
      content_type: header.content_type,
      revision_author_signing_public_key: targetAgentBase64,
      bytes,
      acl_spec: classified.acl_spec,
      public_key_acl: classified.public_key_acl,
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
        new_hive_genesis_hash_base64: hive.new_genesis_hash_base64,
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

  await mkdir(dirname(remapPath), { recursive: true });
  await writeFile(remapPath, JSON.stringify(remap, null, 2), "utf8");
  console.log(
    `[import] wrote remap: ${remapPath} ` +
      `(${remap.entries.length} succeeded, ${remap.failures.length} failed)`,
  );
  if (remap.failures.length > 0) {
    console.log(
      `[import] failures present — review ${remapPath} 'failures' array, ` +
        `address root cause (e.g. missing hive in hive-bundle, missing ` +
        `membership) and re-run with the same bundle. Re-imports are NOT ` +
        `idempotent at the action-hash level (a re-run creates fresh ` +
        `actions); dedupe by 'id' on the host side.`,
    );
    process.exit(1);
  }
  await appWebsocket.client.close();
}

/**
 * Per-entry forward-pointer markers onto the OLD chain's entries by
 * calling `mark_migrated_v2` (default) or `mark_migrated` (with
 * `--v1-only`) for each successfully-imported entry in the remap.
 *
 * V2 markers carry the same per-entry redirect fields as V1 plus
 * `new_hive_genesis_hash_base64: null` and
 * `new_hive_genesis_display_id: null` (this is per-entry, not
 * hive-identity, so the genesis fields stay None). Pass-2 readers
 * (`get_migration_marker_v2`) return them via the `MigrationMarker`
 * enum's `V2` variant.
 *
 * `--v1-only` is required when the OLD app's coordinator predates the
 * pass-2.5 hot-swap (the `mark_migrated_v2` extern is unavailable
 * there). The OLD chain still receives the redirect, just under the V1
 * shape.
 *
 * Each marker write is itself an update to the original entry on the
 * OLD chain. Per the coordinator's SECURITY model, only the original
 * author can write a valid marker — and neither `mark_migrated` nor
 * `mark_migrated_v2` is in the cap grant, so only the local UI / this
 * script (running as the original author via lair) can invoke them.
 */
async function doMarkMigrated(
  oldAppId: string,
  remapPath: string,
  useV1Only: boolean,
): Promise<void> {
  console.log(`[mark-migrated] reading remap from ${remapPath}...`);
  const remap = JSON.parse(await readFile(remapPath, "utf8")) as {
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
      `them as 'not migrated yet' on the old DNA). ` +
      `Marker version: ${useV1Only ? "V1 (legacy)" : "V2 (default)"}.`,
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
    const baseMarker = {
      schema_tag: MIGRATION_MARKER_SCHEMA_TAG,
      new_dna_hash_base64: newDnaHashBase64,
      new_action_hash_base64: entry.new_action_hash,
      new_app_id: newAppId,
      migrated_at_microseconds: migratedAtMicroseconds,
    };
    const marker = useV1Only
      ? { ...baseMarker, schema_version: 1 }
      : {
          ...baseMarker,
          schema_version: 2,
          new_hive_genesis_hash_base64: null,
          new_hive_genesis_display_id: null,
        };
    const fnName = useV1Only ? "mark_migrated" : "mark_migrated_v2";
    const input = {
      original_action_hash: decodeHashFromBase64(entry.old_action_hash),
      marker,
    };
    try {
      await appWebsocket.callZome({
        cell_id: cellId,
        zome_name: ZOME_NAME,
        fn_name: fnName,
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
    await writeFile(remapPath, JSON.stringify(augmented, null, 2), "utf8");
    console.log(
      `[mark-migrated] failure list appended to ${remapPath} as ` +
        `mark_migrated_failures. Address root cause and re-run.`,
    );
    process.exit(1);
  }
  await appWebsocket.client.close();
}

// ---------------------------------------------------------------------------
// CLI entry point
// ---------------------------------------------------------------------------

function usage(): string {
  return (
    "Usage:\n" +
    "  migrate-dna.ts export <app-id> <out.bundle.json>\n" +
    "  migrate-dna.ts migrate-hive <new-app-id> <old-hive-id> <old-anchor-ah-b64-or-empty> <hive-bundle.json>\n" +
    "  migrate-dna.ts grant-memberships <new-app-id> <hive-bundle.json> <old-hive-id> <role> <member-pubkey-b64> [...]\n" +
    "  migrate-dna.ts migrate-group <new-app-id> <hive-bundle.json> <old-hive-id> <old-group-id> [--hive-wide-role <Role>] [--creator-membership <hash-b64>] [--old-marker <hash-b64>]\n" +
    "  migrate-dna.ts grant-group-memberships <new-app-id> <hive-bundle.json> <old-group-id> <role> <member-pubkey-b64> [...]\n" +
    "  migrate-dna.ts import <new-app-id> <in.bundle.json> <hive-bundle.json> <out.remap.json> [classification-overrides.json]\n" +
    "  migrate-dna.ts mark-migrated <old-app-id> <in.remap.json> [--v1-only]\n" +
    "  migrate-dna.ts mark-hive-migrated <old-app-id> <hive-bundle.json>\n" +
    "  migrate-dna.ts mark-group-migrated <old-app-id> <hive-bundle.json>\n" +
    "\n" +
    "Roles: Owner | Admin | Writer | Reader\n" +
    "\n" +
    "Env:\n" +
    "  ADMIN_PORT             conductor admin websocket port (default 4444)\n" +
    "  NEW_DNA_HASH_BASE64    new DNA's multibase holohash (for mark-migrated,\n" +
    "                         mark-hive-migrated, and mark-group-migrated;\n" +
    "                         get via `hc dna hash`)\n" +
    "  NEW_APP_ID             new app's installed_app_id (mark-hive-migrated\n" +
    "                         and mark-group-migrated marker payload; optional)\n" +
    "\n" +
    "Marker versions:\n" +
    "  mark-migrated defaults to V2 (mark_migrated_v2 extern). Pass --v1-only\n" +
    "  when the OLD app's coordinator predates the pass-2.5 hot-swap.\n"
  );
}

async function main(): Promise<void> {
  const [mode, ...args] = process.argv.slice(2);
  switch (mode) {
    case "export": {
      const [appId, outPath] = args;
      if (!appId || !outPath) {
        console.error(usage());
        process.exit(2);
      }
      await doExport(appId, outPath);
      break;
    }
    case "migrate-hive": {
      const [newAppId, oldHiveId, oldAnchorB64, hiveBundlePath] = args;
      if (!newAppId || !oldHiveId || oldAnchorB64 === undefined || !hiveBundlePath) {
        console.error(usage());
        process.exit(2);
      }
      await doMigrateHive(newAppId, oldHiveId, oldAnchorB64, hiveBundlePath);
      break;
    }
    case "grant-memberships": {
      const [newAppId, hiveBundlePath, oldHiveId, roleArg, ...memberArgs] = args;
      if (!newAppId || !hiveBundlePath || !oldHiveId || !roleArg || memberArgs.length === 0) {
        console.error(usage());
        process.exit(2);
      }
      if (!HIVE_ROLES.includes(roleArg as HiveRole)) {
        console.error(`Unknown role "${roleArg}". Expected: ${HIVE_ROLES.join(", ")}`);
        process.exit(2);
      }
      await doGrantMemberships(
        newAppId,
        hiveBundlePath,
        oldHiveId,
        roleArg as HiveRole,
        memberArgs,
      );
      break;
    }
    case "migrate-group": {
      // Flag-aware parse: strip --hive-wide-role / --creator-membership
      // / --old-marker (each consumes the following token) before
      // matching the positionals.
      let hiveWideRole: HiveRole | null = null;
      let creatorMembership: string | null = null;
      let oldMarker: string | null = null;
      const positional: string[] = [];
      for (let i = 0; i < args.length; i++) {
        const a = args[i];
        if (a === "--hive-wide-role") {
          hiveWideRole = (args[++i] ?? "") as HiveRole;
        } else if (a === "--creator-membership") {
          creatorMembership = args[++i] ?? null;
        } else if (a === "--old-marker") {
          oldMarker = args[++i] ?? null;
        } else {
          positional.push(a);
        }
      }
      const [newAppId, hiveBundlePath, oldHiveId, oldGroupId] = positional;
      if (!newAppId || !hiveBundlePath || !oldHiveId || !oldGroupId) {
        console.error(usage());
        process.exit(2);
      }
      if (hiveWideRole !== null && !HIVE_ROLES.includes(hiveWideRole)) {
        console.error(
          `Unknown --hive-wide-role "${hiveWideRole}". Expected: ${HIVE_ROLES.join(", ")}`,
        );
        process.exit(2);
      }
      await doMigrateGroup(
        newAppId,
        hiveBundlePath,
        oldHiveId,
        oldGroupId,
        hiveWideRole,
        creatorMembership,
        oldMarker,
      );
      break;
    }
    case "grant-group-memberships": {
      const [newAppId, hiveBundlePath, oldGroupId, roleArg, ...memberArgs] = args;
      if (!newAppId || !hiveBundlePath || !oldGroupId || !roleArg || memberArgs.length === 0) {
        console.error(usage());
        process.exit(2);
      }
      if (!HIVE_ROLES.includes(roleArg as HiveRole)) {
        console.error(`Unknown role "${roleArg}". Expected: ${HIVE_ROLES.join(", ")}`);
        process.exit(2);
      }
      await doGrantGroupMemberships(
        newAppId,
        hiveBundlePath,
        oldGroupId,
        roleArg as HiveRole,
        memberArgs,
      );
      break;
    }
    case "import": {
      const [appId, bundlePath, hiveBundlePath, remapPath, overridesPath] = args;
      if (!appId || !bundlePath || !hiveBundlePath || !remapPath) {
        console.error(usage());
        process.exit(2);
      }
      await doImport(appId, bundlePath, hiveBundlePath, remapPath, overridesPath);
      break;
    }
    case "mark-migrated": {
      // Strip the optional flag before positional matching to keep the
      // positional contract stable regardless of flag placement.
      const useV1Only = args.includes("--v1-only");
      const positional = args.filter((a) => !a.startsWith("--"));
      const [appId, remapPath] = positional;
      if (!appId || !remapPath) {
        console.error(usage());
        process.exit(2);
      }
      await doMarkMigrated(appId, remapPath, useV1Only);
      break;
    }
    case "mark-hive-migrated": {
      const [appId, hiveBundlePath] = args;
      if (!appId || !hiveBundlePath) {
        console.error(usage());
        process.exit(2);
      }
      await doMarkHiveMigrated(appId, hiveBundlePath);
      break;
    }
    case "mark-group-migrated": {
      const [appId, hiveBundlePath] = args;
      if (!appId || !hiveBundlePath) {
        console.error(usage());
        process.exit(2);
      }
      await doMarkGroupMigrated(appId, hiveBundlePath);
      break;
    }
    default:
      console.error(usage());
      process.exit(2);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
