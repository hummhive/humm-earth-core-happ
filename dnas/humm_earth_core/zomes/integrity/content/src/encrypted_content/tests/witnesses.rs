use super::super::*;
use super::support::*;
use hdi::prelude::*;

// ---------------------------------------------------------------------
// Pass-4 — G-6.2 recipient-witness verification (pre-fetch branches).
//
// Steps 4 (cardinality) + 5 (bidirectional cross-check) are host-
// reachable; step 6 (per-witness membership fetch) requires a
// live conductor and is covered by Sweettest recipient-witness
// behavior tests.
// ---------------------------------------------------------------------

/// Build an `AclByGroupGenesis` from a literal owner + bucket hashes.
fn group_acl(
    owner: ActionHash,
    admin: Vec<ActionHash>,
    writer: Vec<ActionHash>,
    reader: Vec<ActionHash>,
) -> AclByGroupGenesis {
    AclByGroupGenesis {
        owner,
        admin,
        writer,
        reader,
    }
}

/// PKA with no owner string + arbitrary per-bucket pubkey strings.
fn pka_from_buckets(
    admin: Vec<&AgentPubKey>,
    writer: Vec<&AgentPubKey>,
    reader: Vec<&AgentPubKey>,
) -> Acl {
    Acl {
        owner: "".into(),
        admin: admin.into_iter().map(|p| p.to_string()).collect(),
        writer: writer.into_iter().map(|p| p.to_string()).collect(),
        reader: reader.into_iter().map(|p| p.to_string()).collect(),
    }
}

fn witness(pubkey: AgentPubKey, bucket: AclBucket, membership: ActionHash) -> RecipientWitness {
    RecipientWitness {
        pubkey,
        bucket,
        membership_hash: membership,
    }
}

#[test]
fn witnesses_empty_with_nonempty_pka_rejected() {
    // Forward direction failure: PKA has a reader entry but no
    // witness backs it.
    let bob = agent_pubkey(2);
    let pka = pka_from_buckets(vec![], vec![], vec![&bob]);
    let acl = group_acl(action_hash(10), vec![], vec![], vec![action_hash(11)]);
    let result =
        validate_recipient_witnesses(&[], &pka, &acl, &Timestamp(0)).expect("step 5 is pre-fetch");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(
                msg.contains("not backed by any dominating recipient_witness"),
                "got: {msg}"
            );
            assert!(msg.contains(&bob.to_string()), "got: {msg}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn witnesses_missing_pka_entry_rejected() {
    // PKA has two reader entries; only one is backed by a witness.
    // The unbacked entry rejects.
    let bob = agent_pubkey(2);
    let mallory = agent_pubkey(99);
    let pka = pka_from_buckets(vec![], vec![], vec![&bob, &mallory]);
    let acl = group_acl(action_hash(10), vec![], vec![], vec![action_hash(11)]);
    let witnesses = vec![witness(bob.clone(), AclBucket::Reader, action_hash(20))];
    let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0))
        .expect("step 5 is pre-fetch");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(
                msg.contains("not backed by any dominating recipient_witness"),
                "got: {msg}"
            );
            // Either bob or mallory message — the iteration order
            // is deterministic (owner, admin, writer, reader);
            // mallory comes after bob in reader vec so bob is
            // checked first. bob IS backed, so mallory is the
            // expected failure.
            assert!(msg.contains(&mallory.to_string()), "got: {msg}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn witnesses_over_claim_without_pka_entry_rejected() {
    // Reverse direction failure: witness claims a bucket for a
    // pubkey that is not in the corresponding PKA bucket.
    let bob = agent_pubkey(2);
    let mallory = agent_pubkey(99);
    // PKA has bob in reader; witness over-claims mallory in reader.
    let pka = pka_from_buckets(vec![], vec![], vec![&bob]);
    let acl = group_acl(action_hash(10), vec![], vec![], vec![action_hash(11)]);
    // Two witnesses: one legitimate (bob), one over-claim (mallory).
    // bob covers the forward-direction check; mallory triggers the
    // reverse-direction check.
    let witnesses = vec![
        witness(bob.clone(), AclBucket::Reader, action_hash(20)),
        witness(mallory.clone(), AclBucket::Reader, action_hash(21)),
    ];
    let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0))
        .expect("step 5 is pre-fetch");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("claims bucket Reader"), "got: {msg}");
            assert!(msg.contains(&mallory.to_string()), "got: {msg}");
            assert!(msg.contains("not in public_key_acl.Reader"), "got: {msg}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn witnesses_step5_passes_when_round_trip_consistent_step6_triggers_fetch() {
    // Step 5 (bidirectional cross-check) accepts a single Reader-
    // bucket witness for bob in PKA.reader: the forward and
    // reverse passes both round-trip cleanly. Step 6 then fires
    // the membership fetch, which fails host-side because there
    // is no DHT — we observe Err, not Ok(Invalid). This pins the
    // step-5 → step-6 boundary for the simplest happy case.
    //
    // The dominance-happy-path (Admin-bucket witness backing a
    // Reader-bucket PKA entry) needs a resolvable membership entry,
    // so it belongs in Sweettest/conductor behavior coverage.
    let bob = agent_pubkey(2);
    let pka = pka_from_buckets(vec![], vec![], vec![&bob]);
    let acl = group_acl(action_hash(10), vec![action_hash(11)], vec![], vec![]);
    let witnesses = vec![witness(bob.clone(), AclBucket::Reader, action_hash(20))];
    let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0));
    assert!(
        result.is_err(),
        "expected host fetch error after pre-fetch checks passed; got Ok({:?})",
        result.ok()
    );
}

#[test]
fn witnesses_reader_cannot_back_admin_pka_step5() {
    // Bucket-dominance violation: bob is in PKA.admin; only a
    // Reader-bucket witness is provided. Reader does NOT dominate
    // Admin — step 5 must reject before any fetch.
    let bob = agent_pubkey(2);
    let pka = pka_from_buckets(vec![&bob], vec![], vec![]);
    let acl = group_acl(action_hash(10), vec![action_hash(11)], vec![], vec![]);
    let witnesses = vec![witness(bob.clone(), AclBucket::Reader, action_hash(20))];
    let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0))
        .expect("step 5 dominance check is pre-fetch");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            // Forward direction message — the Admin-bucket PKA
            // entry has no dominating witness.
            assert!(msg.contains("public_key_acl.Admin"), "got: {msg}");
            assert!(
                msg.contains("not backed by any dominating recipient_witness"),
                "got: {msg}"
            );
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn witnesses_exceed_max_count_rejected() {
    // Cardinality bound (step 4) fires before the cross-check
    // (step 5). Build HIVEGROUP_MAX_WITNESSES + 1 witnesses and
    // confirm the cardinality message is the rejection cause.
    let acl = group_acl(action_hash(10), vec![], vec![], vec![]);
    let pka = pka_from_buckets(vec![], vec![], vec![]); // contents irrelevant — step 4 fires first
    let witnesses: Vec<RecipientWitness> = (0..HIVEGROUP_MAX_WITNESSES + 1)
        .map(|i| {
            // Spread `i` across the first 4 bytes of the 36-byte
            // pubkey so no two witnesses collide even if
            // HIVEGROUP_MAX_WITNESSES is raised above 2^32. A
            // collision would surface as a dedup hit (step 5)
            // before the cardinality bound (step 4) fires —
            // changing the rejection reason from
            // "exceeds HIVEGROUP_MAX_WITNESSES" to "duplicate
            // pubkey", which is the wrong invariant to pin in
            // this test.
            let mut bytes = vec![0u8; 36];
            let i_bytes = (i as u32).to_le_bytes();
            bytes[..4].copy_from_slice(&i_bytes);
            let pk = AgentPubKey::from_raw_36(bytes);
            witness(pk, AclBucket::Reader, action_hash(20))
        })
        .collect();
    let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0))
        .expect("step 4 cardinality is pre-fetch");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(
                msg.contains("HIVEGROUP_MAX_WITNESSES"),
                "expected cardinality message; got: {msg}"
            );
            assert!(
                msg.contains(&format!("= {}", HIVEGROUP_MAX_WITNESSES + 1)),
                "expected actual count in message; got: {msg}"
            );
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn witnesses_duplicate_pubkey_rejected() {
    // Defense-in-depth: a duplicate-witness-for-same-pubkey forge
    // (Mallory stamps her pubkey twice across two buckets to mask
    // an over-claim) is rejected at the dedup check inside step 5.
    let bob = agent_pubkey(2);
    let pka = pka_from_buckets(vec![], vec![], vec![&bob]);
    let acl = group_acl(action_hash(10), vec![], vec![], vec![action_hash(11)]);
    let witnesses = vec![
        witness(bob.clone(), AclBucket::Reader, action_hash(20)),
        witness(bob.clone(), AclBucket::Reader, action_hash(21)),
    ];
    let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0))
        .expect("dedup is pre-fetch");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("duplicate"), "got: {msg}");
            assert!(msg.contains(&bob.to_string()), "got: {msg}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn witnesses_reader_cannot_back_owner_pka_entry() {
    // Owner-bucket PKA entry must be backed by an Owner-bucket
    // (or higher — Owner is the highest) witness. Reader-bucket
    // witness does NOT satisfy the Owner PKA entry. Confirms
    // dominance applies uniformly across all four PKA buckets.
    let alice = agent_pubkey(1);
    let pka = Acl {
        owner: alice.to_string(),
        admin: vec![],
        writer: vec![],
        reader: vec![],
    };
    let acl = group_acl(action_hash(10), vec![], vec![], vec![]);
    // Wrong bucket witness — Reader cannot back Owner.
    let witnesses = vec![witness(alice.clone(), AclBucket::Reader, action_hash(20))];
    let result = validate_recipient_witnesses(&witnesses, &pka, &acl, &Timestamp(0))
        .expect("step 5 is pre-fetch");
    match result {
        ValidateCallbackResult::Invalid(msg) => {
            assert!(msg.contains("public_key_acl.Owner"), "got: {msg}");
            assert!(
                msg.contains("not backed by any dominating recipient_witness"),
                "got: {msg}"
            );
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn acl_bucket_dominance_matrix() {
    // Pin the dominance ordering — Owner > Admin > Writer > Reader.
    // Any change to the AclBucket variant order or
    // bucket_required_role must keep this matrix intact.
    use AclBucket::*;
    for higher in [Owner, Admin, Writer, Reader] {
        assert!(higher.dominates(higher), "{higher:?} dominates self");
    }
    // Owner dominates everything.
    assert!(Owner.dominates(Admin));
    assert!(Owner.dominates(Writer));
    assert!(Owner.dominates(Reader));
    // Admin dominates Writer + Reader, not Owner.
    assert!(!Admin.dominates(Owner));
    assert!(Admin.dominates(Writer));
    assert!(Admin.dominates(Reader));
    // Writer dominates Reader, not above.
    assert!(!Writer.dominates(Owner));
    assert!(!Writer.dominates(Admin));
    assert!(Writer.dominates(Reader));
    // Reader dominates only itself.
    assert!(!Reader.dominates(Owner));
    assert!(!Reader.dominates(Admin));
    assert!(!Reader.dominates(Writer));
}
