/**
 * Pure-logic sanity tests for the pass-4 recipient-witness bucket
 * dominance arithmetic in `scripts/acl-bucket.ts`.
 *
 * These do NOT spin up a conductor — they pin the math that the
 * migration's `buildHiveGroupAclSpec` (and, by the same contract, any
 * humm-tauri `stampWitnessesFromGroupAcl` helper) relies on to choose
 * the single canonical witness bucket per recipient. A regression here
 * would produce `recipient_witnesses` the integrity validator rejects
 * (bucket-dominance violation, role-mismatch, or owner-slot contention),
 * so this is the cheapest possible guard against shipping broken
 * HiveGroup writes.
 *
 * The integrity-zome side of the same ordering is covered by the
 * `acl_bucket_dominance_matrix` Rust host test; this is the TS mirror.
 */
import { describe, expect, test } from "vitest";

import {
  cappedWitnessRank,
  rankToBucket,
  roleRank,
  type AclBucketName,
} from "../../../scripts/acl-bucket.js";

describe("roleRank", () => {
  test("orders Owner > Admin > Writer > Reader", () => {
    expect(roleRank("Owner")).toBeGreaterThan(roleRank("Admin"));
    expect(roleRank("Admin")).toBeGreaterThan(roleRank("Writer"));
    expect(roleRank("Writer")).toBeGreaterThan(roleRank("Reader"));
  });

  test("matches the integrity zome's 4/3/2/1 ranks", () => {
    expect(roleRank("Owner")).toBe(4);
    expect(roleRank("Admin")).toBe(3);
    expect(roleRank("Writer")).toBe(2);
    expect(roleRank("Reader")).toBe(1);
  });
});

describe("rankToBucket", () => {
  test("is the inverse of roleRank for in-range values", () => {
    for (const role of ["Owner", "Admin", "Writer", "Reader"] as AclBucketName[]) {
      expect(rankToBucket(roleRank(role))).toBe(role);
    }
  });

  test("collapses out-of-range ranks to the nearest bucket", () => {
    expect(rankToBucket(99)).toBe("Owner"); // >= 4
    expect(rankToBucket(0)).toBe("Reader"); // <= 1
    expect(rankToBucket(-5)).toBe("Reader");
  });

  test("clamps a degenerate group bucket rank of 0 to Reader", () => {
    // No production group carries rank 0, but the clamp must hold so a
    // malformed input can never produce a negative/empty bucket.
    expect(rankToBucket(cappedWitnessRank(0, "Owner"))).toBe("Reader");
  });
});

describe("cappedWitnessRank", () => {
  test("common migration case: matching group bucket + member role keeps the bucket", () => {
    // Admin-bucket group, Admin-role member -> Admin witness.
    expect(rankToBucket(cappedWitnessRank(roleRank("Admin"), "Admin"))).toBe("Admin");
    // Writer-bucket group, Writer-role member -> Writer witness.
    expect(rankToBucket(cappedWitnessRank(roleRank("Writer"), "Writer"))).toBe("Writer");
    // Reader-bucket group, Reader-role member -> Reader witness.
    expect(rankToBucket(cappedWitnessRank(roleRank("Reader"), "Reader"))).toBe("Reader");
  });

  test("member role floors the bucket below the group's placement", () => {
    // Reader-role member of an admin-bucket group can only be a Reader
    // witness — the validator's role_satisfies check would reject an
    // Admin claim from a Reader-role membership.
    expect(rankToBucket(cappedWitnessRank(roleRank("Admin"), "Reader"))).toBe("Reader");
    // Writer-role member of an admin-bucket group -> Writer witness.
    expect(rankToBucket(cappedWitnessRank(roleRank("Admin"), "Writer"))).toBe("Writer");
  });

  test("group placement floors the bucket below the member's role", () => {
    // Owner-role member whose group sits in the reader bucket can only
    // be a Reader witness — the group-bucket containment check would
    // reject a higher claim (the group is not in a high-authority
    // group_acl bucket).
    expect(rankToBucket(cappedWitnessRank(roleRank("Reader"), "Owner"))).toBe("Reader");
    expect(rankToBucket(cappedWitnessRank(roleRank("Writer"), "Owner"))).toBe("Writer");
  });

  test("owner is capped to Admin so the single-string public_key_acl.owner slot is never contended", () => {
    // Owner-bucket group + Owner-role member would naively be rank 4
    // (Owner), but the migration caps at Admin (3): an owner-group
    // member is validly representable as an Admin-bucket witness via
    // dominance (Admin accepts owner-union-admin groups; Owner-role
    // satisfies the Admin requirement).
    expect(cappedWitnessRank(roleRank("Owner"), "Owner")).toBe(3);
    expect(rankToBucket(cappedWitnessRank(roleRank("Owner"), "Owner"))).toBe("Admin");
    // Never returns the Owner bucket, for any input combination.
    for (const groupRole of ["Owner", "Admin", "Writer", "Reader"] as AclBucketName[]) {
      for (const memberRole of ["Owner", "Admin", "Writer", "Reader"] as AclBucketName[]) {
        const rank = cappedWitnessRank(roleRank(groupRole), memberRole);
        expect(rank).toBeLessThanOrEqual(3);
        expect(rank).toBeGreaterThanOrEqual(1);
        expect(rankToBucket(rank)).not.toBe("Owner");
      }
    }
  });

  test("result is always min(groupBucketRank, roleRank, 3)", () => {
    // Exhaustive cross-check against the explicit formula — pins the
    // exact semantics so a future refactor cannot silently change the
    // cap or the direction of the min.
    for (const groupRole of ["Owner", "Admin", "Writer", "Reader"] as AclBucketName[]) {
      for (const memberRole of ["Owner", "Admin", "Writer", "Reader"] as AclBucketName[]) {
        const expected = Math.min(roleRank(groupRole), roleRank(memberRole), 3);
        expect(cappedWitnessRank(roleRank(groupRole), memberRole)).toBe(expected);
      }
    }
  });
});
