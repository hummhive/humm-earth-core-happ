/**
 * Pure pass-4 wire-shape builders + a minimal assertion/run framework
 * for the manual e2e harness. No tryorama, no conductor coupling — these
 * just shape the `acl_spec` / `public_key_acl` / `recipient_witnesses`
 * payloads exactly as humm-tauri will, and provide `expectOk` /
 * `expectReject` gates for the sequential scenarios.
 */
import { type ActionHash, type AgentPubKey, encodeHashToBase64 } from "@holochain/client";

export type AclBucketName = "Owner" | "Admin" | "Writer" | "Reader";

export type Acl = {
  owner: string;
  admin: string[];
  writer: string[];
  reader: string[];
};

export type AclByGroupGenesis = {
  owner: ActionHash;
  admin: ActionHash[];
  writer: ActionHash[];
  reader: ActionHash[];
};

export type RecipientWitness = {
  pubkey: AgentPubKey;
  bucket: AclBucketName;
  membership_hash: ActionHash;
};

export const emptyAcl = (): Acl => ({ owner: "", admin: [], writer: [], reader: [] });

export const readerAcl = (agents: AgentPubKey[]): Acl => ({
  owner: "",
  admin: [],
  writer: [],
  reader: agents.map(encodeHashToBase64),
});

export const groupAcl = (
  owner: ActionHash,
  admin: ActionHash[] = [],
  writer: ActionHash[] = [],
  reader: ActionHash[] = [],
): AclByGroupGenesis => ({ owner, admin, writer, reader });

export const witness = (
  pubkey: AgentPubKey,
  bucket: AclBucketName,
  membership_hash: ActionHash,
): RecipientWitness => ({ pubkey, bucket, membership_hash });

export const aclSpecHiveGroup = (opts: {
  hiveGenesisHash: ActionHash;
  authorMembershipHash?: ActionHash | null;
  groupAcl: AclByGroupGenesis;
  authorGroupMembershipHash?: ActionHash | null;
  recipientWitnesses?: RecipientWitness[];
}) => ({
  HiveGroup: {
    hive_genesis_hash: opts.hiveGenesisHash,
    author_membership_hash: opts.authorMembershipHash ?? null,
    group_acl: opts.groupAcl,
    author_group_membership_hash: opts.authorGroupMembershipHash ?? null,
    recipient_witnesses: opts.recipientWitnesses ?? [],
  },
});

export const aclSpecDirectMessage = (recipients: AgentPubKey[]) => ({
  DirectMessage: { recipients },
});

export const aclSpecPublic = (
  hiveGenesisHash: ActionHash,
  authorMembershipHash: ActionHash | null = null,
) => ({ Public: { hive_genesis_hash: hiveGenesisHash, author_membership_hash: authorMembershipHash } });

export const aclSpecOpenWrite = (targetHiveGenesisHash: ActionHash | null = null) => ({
  OpenWrite: { target_hive_genesis_hash: targetHiveGenesisHash },
});

/** Past expiry (microseconds, ~1970) for "already expired" membership
 * witness/window tests. */
export const PAST_MICROS = 1_000_000;

// ---------------------------------------------------------------------------
// Minimal sequential test framework
// ---------------------------------------------------------------------------

export type StepResult = { name: string; ok: boolean; error?: string };

let _results: StepResult[] = [];

/** Run a named, state-gated step. Records pass/fail; never throws so the
 * whole sequence runs and reports a full tally. */
export async function step(name: string, fn: () => Promise<void>): Promise<void> {
  try {
    await fn();
    _results.push({ name, ok: true });
    console.log(`  PASS  ${name}`);
  } catch (e) {
    const msg = (e as Error)?.message ?? String(e);
    _results.push({ name, ok: false, error: msg });
    console.log(`  FAIL  ${name}\n        ${msg.split("\n")[0]}`);
  }
}

export function assert(cond: unknown, msg: string): void {
  if (!cond) throw new Error(`assertion failed: ${msg}`);
}

/** Assert a zome call REJECTS (validation failure). Optionally match a
 * substring of the rejection. Fails loudly if the call SUCCEEDS. */
export async function expectReject(
  p: Promise<unknown>,
  match?: string | RegExp,
): Promise<void> {
  let threw = false;
  let message = "";
  try {
    await p;
  } catch (e) {
    threw = true;
    message = (e as Error)?.message ?? String(e);
  }
  if (!threw) throw new Error("expected a validation rejection, but the call SUCCEEDED");
  if (match) {
    const ok = typeof match === "string" ? message.includes(match) : match.test(message);
    if (!ok) throw new Error(`rejection did not match ${match}; got: ${message.split("\n")[0]}`);
  }
}

export function resetResults(): void {
  _results = [];
}

export function results(): StepResult[] {
  return _results;
}

/** Print a tally and return the process exit code (0 = all passed). */
export function report(): number {
  const passed = _results.filter((r) => r.ok).length;
  const failed = _results.length - passed;
  console.log(`\n=== e2e: ${passed}/${_results.length} passed, ${failed} failed ===`);
  if (failed > 0) {
    for (const r of _results.filter((x) => !x.ok)) {
      console.log(`  - ${r.name}: ${r.error?.split("\n")[0]}`);
    }
  }
  return failed === 0 ? 0 : 1;
}
