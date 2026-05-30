/**
 * Pure ACL-bucket dominance arithmetic shared by the migration script
 * (`migrate-dna.ts`) and its unit tests.
 *
 * The pass-4 integrity zome enforces a bucket-dominance ordering on
 * `AclSpec::HiveGroup` recipient witnesses:
 *
 *   Owner > Admin > Writer > Reader
 *
 * A witness in bucket X validly backs a `public_key_acl` entry in any
 * bucket Y where X dominates Y, AND its cited `GroupMembership` must
 * (a) grant a role satisfying `bucket_required_role(X)` and (b) live in
 * a `group_acl` bucket of equal-or-higher authority than X. The
 * migration's `buildHiveGroupAclSpec` uses [`cappedWitnessRank`] to pick
 * the single canonical witness bucket per recipient so the stamped
 * witnesses always satisfy the validator's bidirectional cross-check.
 *
 * Kept dependency-free so it imports cleanly under both `tsx` (CLI) and
 * `vitest` (unit tests) with no Holochain/Node coupling.
 */

/** The four ACL buckets, matching the integrity zome's `AclBucket`
 * enum + the `Role` membership-role vocabulary. */
export type AclBucketName = "Owner" | "Admin" | "Writer" | "Reader";

/** Authority rank. Higher dominates lower: Owner(4) > Admin(3) >
 * Writer(2) > Reader(1). Single source of truth for the ordering on
 * the TS side; mirrors `bucket_required_role` + `role_satisfies` in the
 * integrity zome. */
export function roleRank(role: AclBucketName): number {
  switch (role) {
    case "Owner":
      return 4;
    case "Admin":
      return 3;
    case "Writer":
      return 2;
    case "Reader":
      return 1;
  }
}

/** Inverse of [`roleRank`]. Ranks at or above 4 collapse to Owner; 3 →
 * Admin; 2 → Writer; everything else → Reader. */
export function rankToBucket(rank: number): AclBucketName {
  if (rank >= 4) return "Owner";
  if (rank === 3) return "Admin";
  if (rank === 2) return "Writer";
  return "Reader";
}

/**
 * The highest witness bucket rank a recipient can claim, given the
 * `group_acl` bucket their backing group sits in (`groupBucketRank`)
 * and the role their `GroupMembership` grants (`memberRole`).
 *
 * - Capped by `groupBucketRank`: a witness cannot claim more authority
 *   than the group's placement confers (the validator's group-bucket
 *   containment check would reject it).
 * - Capped by the member's role: a Reader-role member of an
 *   admin-bucket group can only be a Reader witness (the validator's
 *   `role_satisfies` check would reject an Admin claim).
 * - Capped at Admin (3): the migration never uses the Owner bucket
 *   because `public_key_acl.owner` is a single string (one slot). An
 *   owner-group member is validly representable as an Admin-bucket
 *   witness via dominance — Admin accepts owner∪admin groups and
 *   Owner-role satisfies the Admin requirement.
 *
 * Returns a rank in 1..=3; feed through [`rankToBucket`] for the name.
 */
export function cappedWitnessRank(
  groupBucketRank: number,
  memberRole: AclBucketName,
): number {
  return Math.min(Math.min(groupBucketRank, roleRank(memberRole)), 3);
}
