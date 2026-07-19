use hdi::prelude::*;

use super::types::*;
use crate::group::{check_group_authority, fetch_group_membership};
use crate::hive::{check_hive_authority, role_satisfies, Role};

// Cross-check the sender-controlled `revision_author_signing_public_key`
// field against the cryptographically-attested `action.author`. Without
// this guard the field is forgeable: any peer with a custom DNA can
// commit an entry claiming another agent's signing pubkey, and every
// downstream consumer (DMs, member entries, audit trails) trusts the
// header value as the sender identity. Comparing the string form is
// sound because `AgentPubKey::to_string()` emits the same multibase
// holohash form (`'u' + URL_SAFE_NO_PAD(39 bytes)`) that
// `@holochain/client::encodeHashToBase64` writes into the header.
pub(super) fn check_author_matches_header(
    action_author: &AgentPubKey,
    header_pubkey: &str,
) -> ValidateCallbackResult {
    let attested = action_author.to_string();
    if attested != header_pubkey {
        return ValidateCallbackResult::Invalid(format!(
            "revision_author_signing_public_key ({}) does not match action.author ({})",
            header_pubkey, attested,
        ));
    }
    ValidateCallbackResult::Valid
}

/// Structural DoS floor on every header: string-length caps plus
/// per-bucket `public_key_acl` cardinality, key-length, and duplicate
/// rejection. Runs on create AND update, all `AclSpec` variants.
pub(super) fn validate_header_bounds(header: &EncryptedContentHeader) -> ValidateCallbackResult {
    let id_chars = header.id.chars().count();
    if id_chars == 0 || id_chars > HEADER_ID_MAX_CHARS {
        return ValidateCallbackResult::Invalid(format!(
            "header id must be 1-{HEADER_ID_MAX_CHARS} chars"
        ));
    }
    let content_type_chars = header.content_type.chars().count();
    if content_type_chars == 0 || content_type_chars > HEADER_CONTENT_TYPE_MAX_CHARS {
        return ValidateCallbackResult::Invalid(format!(
            "header content_type must be 1-{HEADER_CONTENT_TYPE_MAX_CHARS} chars"
        ));
    }
    if header.display_hive_id.chars().count() > HEADER_DISPLAY_HIVE_ID_MAX_CHARS {
        return ValidateCallbackResult::Invalid(format!(
            "header display_hive_id must be at most {HEADER_DISPLAY_HIVE_ID_MAX_CHARS} chars"
        ));
    }
    let acl = &header.public_key_acl;
    if acl.owner.chars().count() > PUBLIC_KEY_ACL_MAX_KEY_CHARS {
        return ValidateCallbackResult::Invalid(format!(
            "public_key_acl owner must be at most {PUBLIC_KEY_ACL_MAX_KEY_CHARS} chars"
        ));
    }
    for bucket in [&acl.admin, &acl.writer, &acl.reader] {
        if bucket.len() > PUBLIC_KEY_ACL_MAX_ENTRIES {
            return ValidateCallbackResult::Invalid(format!(
                "public_key_acl buckets accept at most {PUBLIC_KEY_ACL_MAX_ENTRIES} entries"
            ));
        }
        let mut seen = std::collections::HashSet::with_capacity(bucket.len());
        for key in bucket {
            let key_chars = key.chars().count();
            if key_chars == 0 || key_chars > PUBLIC_KEY_ACL_MAX_KEY_CHARS {
                return ValidateCallbackResult::Invalid(format!(
                    "public_key_acl keys must be 1-{PUBLIC_KEY_ACL_MAX_KEY_CHARS} chars"
                ));
            }
            if !seen.insert(key.as_str()) {
                return ValidateCallbackResult::Invalid(
                    "public_key_acl buckets must not contain duplicate keys".to_string(),
                );
            }
        }
    }
    ValidateCallbackResult::Valid
}

/// Identity fields an update may never change: `id`, hive context, and
/// the `AclSpec` variant; `content_type` may only take the one-way
/// `_migrated/` stamp. ACL and display fields stay deliberately mutable
/// (header-convergence upserts depend on that).
pub(super) fn validate_update_continuity(
    old: &EncryptedContentHeader,
    new: &EncryptedContentHeader,
) -> ValidateCallbackResult {
    if new.id != old.id {
        return ValidateCallbackResult::Invalid(
            "EncryptedContent updates must not change the id".to_string(),
        );
    }
    if new.hive_context() != old.hive_context() {
        return ValidateCallbackResult::Invalid(
            "EncryptedContent updates must not change the hive context".to_string(),
        );
    }
    if std::mem::discriminant(&new.acl_spec) != std::mem::discriminant(&old.acl_spec) {
        return ValidateCallbackResult::Invalid(
            "EncryptedContent updates must not change the acl_spec variant".to_string(),
        );
    }
    let single_migration_stamp = !old
        .content_type
        .starts_with(MIGRATION_MARKER_CONTENT_TYPE_PREFIX)
        && new.content_type
            == format!("{MIGRATION_MARKER_CONTENT_TYPE_PREFIX}{}", old.content_type);
    if new.content_type != old.content_type && !single_migration_stamp {
        return ValidateCallbackResult::Invalid(
            "EncryptedContent updates may only stamp content_type with the _migrated/ prefix"
                .to_string(),
        );
    }
    ValidateCallbackResult::Valid
}

fn validate_open_write_payload_size(content: &EncryptedContent) -> ValidateCallbackResult {
    if content.bytes.bytes().len() > OPEN_WRITE_MAX_PAYLOAD_BYTES {
        return ValidateCallbackResult::Invalid(format!(
            "Public and OpenWrite payloads accept at most {OPEN_WRITE_MAX_PAYLOAD_BYTES} bytes"
        ));
    }
    ValidateCallbackResult::Valid
}
/// Variant-dispatch entrypoint for create + update validation. Runs the
/// pass-1 author-vs-header guard once, then delegates to the
/// per-variant validator that carries the right authority contract.
pub(super) fn run_content_validators(
    author: &AgentPubKey,
    timestamp: &Timestamp,
    content: &EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    let header_check =
        check_author_matches_header(author, &content.header.revision_author_signing_public_key);
    if !matches!(header_check, ValidateCallbackResult::Valid) {
        return Ok(header_check);
    }
    let bounds_check = validate_header_bounds(&content.header);
    if !matches!(bounds_check, ValidateCallbackResult::Valid) {
        return Ok(bounds_check);
    }
    if matches!(
        content.header.acl_spec,
        AclSpec::Public { .. } | AclSpec::OpenWrite { .. }
    ) {
        let size_check = validate_open_write_payload_size(content);
        if !matches!(size_check, ValidateCallbackResult::Valid) {
            return Ok(size_check);
        }
    }
    match &content.header.acl_spec {
        AclSpec::HiveGroup {
            hive_genesis_hash,
            author_membership_hash,
            group_acl,
            author_group_membership_hash,
            recipient_witnesses,
        } => validate_hivegroup_acl(
            author,
            timestamp,
            hive_genesis_hash,
            author_membership_hash.as_ref(),
            group_acl,
            author_group_membership_hash.as_ref(),
            recipient_witnesses,
            &content.header.public_key_acl,
        ),
        AclSpec::DirectMessage { recipients } => {
            validate_directmessage_acl(author, recipients, &content.header.public_key_acl)
        }
        AclSpec::Public {
            hive_genesis_hash,
            author_membership_hash,
        } => check_hive_authority(
            author,
            hive_genesis_hash,
            author_membership_hash.as_ref(),
            Role::Writer,
            timestamp,
        ),
        AclSpec::OpenWrite {
            target_hive_genesis_hash,
        } => validate_openwrite_acl(target_hive_genesis_hash.as_ref()),
    }
}

/// `AclSpec::HiveGroup` validator. The author must hold Writer+ in
/// the hive AND Writer+ in every group listed in `group_acl.*`. Every
/// group hash in `group_acl` MUST resolve to a `GroupGenesis` in the
/// same hive (closes cross-hive group claim, attack #9).
///
/// **G-6.2 recipient-set integrity (pass-4).** The `recipient_witnesses`
/// list MUST cover every pubkey in `public_key_acl` exactly once
/// (bidirectional cross-check) and each witness MUST resolve to a
/// real, unexpired, role-sufficient [`crate::group::GroupMembership`]
/// for the named pubkey in a group present in the corresponding (or
/// higher) bucket of `group_acl`. Closes attack #5: recipient-list
/// forgery for routing fan-out.
///
/// ## Step order (fail-fast)
///
/// 1. Cardinality bound on `group_acl` (cheap, pre-fetch).
/// 2. Hive Writer+ authority (1 fetch).
/// 3. Per-group cross-hive consistency + per-group Writer+ authority
///    (≤ 3 fetches per group, bounded by [`GROUP_ACL_MAX_GROUPS`]).
/// 4. Cardinality bound on `recipient_witnesses` (cheap, pre-fetch).
/// 5. Bidirectional set-equality between `public_key_acl` buckets and
///    witnesses with bucket-dominance check (cheap, pre-fetch — pure
///    string compares + role-table lookups).
/// 6. Per-witness [`crate::group::GroupMembership`] verification
///    (1 fetch per witness, bounded by [`HIVEGROUP_MAX_WITNESSES`]).
///
/// Steps 1-3 stay pre-witness so hive/group authority failure short-
/// circuits the entire validator before any witness-fetch cost.
/// Steps 4-5 are pre-fetch within the witness suite so the cheap
/// cross-check rejections fire before the per-witness fetches.
fn validate_hivegroup_acl(
    author: &AgentPubKey,
    timestamp: &Timestamp,
    hive_genesis_hash: &ActionHash,
    author_membership_hash: Option<&ActionHash>,
    group_acl: &AclByGroupGenesis,
    author_group_membership_hash: Option<&ActionHash>,
    recipient_witnesses: &[RecipientWitness],
    public_key_acl: &Acl,
) -> ExternResult<ValidateCallbackResult> {
    // Step 1 — group_acl cardinality bound. Same amplification
    // mitigation as DM_MAX_RECIPIENTS. Each group hash forces network
    // calls; unbounded fan-out is a validator-DoS surface.
    let total_groups = 1usize
        .saturating_add(group_acl.admin.len())
        .saturating_add(group_acl.writer.len())
        .saturating_add(group_acl.reader.len());
    if total_groups > GROUP_ACL_MAX_GROUPS {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveGroup group_acl references {total_groups} groups; \
             maximum is GROUP_ACL_MAX_GROUPS = {GROUP_ACL_MAX_GROUPS}",
        )));
    }
    // Step 2 — hive authority: Writer+ in the named hive (mirrors pass-2).
    let hive_check = check_hive_authority(
        author,
        hive_genesis_hash,
        author_membership_hash,
        Role::Writer,
        timestamp,
    )?;
    if !matches!(hive_check, ValidateCallbackResult::Valid) {
        return Ok(hive_check);
    }
    // Step 3 — for each group listed in group_acl, the author must
    // hold Writer+. The same author_group_membership_hash witness is
    // reused across all groups; this is a deliberate simplification —
    // the common humm-tauri pattern publishes content under groups the
    // author is a member of via one specific membership. Cross-group
    // multi-membership writes are still possible by issuing separate
    // entries.
    for group_hash in std::iter::once(&group_acl.owner)
        .chain(group_acl.admin.iter())
        .chain(group_acl.writer.iter())
        .chain(group_acl.reader.iter())
    {
        // Per-group cross-hive consistency check. fetch_group_genesis
        // returns the parent hive; reject if it does not match the
        // entry's hive (attack #9).
        let (_group_author, group) = crate::group::fetch_group_genesis(group_hash)?;
        if &group.hive_genesis_hash != hive_genesis_hash {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "HiveGroup acl references group {group_hash} in hive {} \
                 but entry claims hive {hive_genesis_hash}",
                group.hive_genesis_hash,
            )));
        }
        let group_check = check_group_authority(
            author,
            group_hash,
            author_group_membership_hash,
            author_membership_hash,
            Role::Writer,
            timestamp,
        )?;
        if !matches!(group_check, ValidateCallbackResult::Valid) {
            return Ok(group_check);
        }
    }
    // Steps 4-6 — G-6.2 recipient-witness verification
    // (cardinality → bidirectional PKA cross-check → per-witness fetch).
    validate_recipient_witnesses(recipient_witnesses, public_key_acl, group_acl, timestamp)
}

/// G-6.2 — recipient-witness verification. Three sub-checks:
///
/// - **Cardinality** (step 4): bounded by [`HIVEGROUP_MAX_WITNESSES`]
///   so a modified coordinator cannot DoS the validator with thousands
///   of witnesses.
/// - **Bidirectional cross-check** (step 5, pre-fetch): every pubkey
///   in `public_key_acl.{owner,admin,writer,reader}` must be backed by
///   exactly one witness whose `bucket` dominates the PKA bucket the
///   pubkey lives in, AND every witness's `pubkey` must appear in the
///   PKA bucket it claims. No over-claiming, no under-claiming, no
///   duplicates.
/// - **Per-witness verification** (step 6, fetch): each witness's
///   `membership_hash` resolves to a real [`crate::group::GroupMembership`]
///   for the named pubkey, in a group present in the bucket
///   (or any dominating bucket) of `group_acl`, granting a role that
///   satisfies the witness bucket, unexpired at `timestamp`.
///
/// Extracted as a free function so the host-side test module can
/// exercise the pre-fetch branches (steps 4 + 5) without a DHT.
pub(super) fn validate_recipient_witnesses(
    witnesses: &[RecipientWitness],
    public_key_acl: &Acl,
    group_acl: &AclByGroupGenesis,
    timestamp: &Timestamp,
) -> ExternResult<ValidateCallbackResult> {
    // Step 4 — cardinality.
    if witnesses.len() > HIVEGROUP_MAX_WITNESSES {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveGroup recipient_witnesses.len() = {} exceeds \
             HIVEGROUP_MAX_WITNESSES = {}",
            witnesses.len(),
            HIVEGROUP_MAX_WITNESSES,
        )));
    }
    // Step 5 — bidirectional cross-check with bucket dominance.
    if let Some(invalid) = check_witness_pka_bidirectional(witnesses, public_key_acl) {
        return Ok(invalid);
    }
    // Step 6 — per-witness fetch + verification.
    for witness in witnesses {
        let result = verify_recipient_witness(witness, group_acl, timestamp)?;
        if !matches!(result, ValidateCallbackResult::Valid) {
            return Ok(result);
        }
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Pre-fetch bidirectional cross-check between PKA buckets and the
/// witness list. Returns `Some(Invalid)` on any forgery (PKA pubkey
/// without dominating witness, or witness pubkey not in claimed PKA
/// bucket, or duplicate witnesses for the same pubkey). Pure string
/// compares + role-table lookups — host-reachable, no DHT.
///
/// The dominance rule (witness bucket ≥ PKA bucket) lets humm-tauri
/// stamp ONE witness per pubkey at the highest role the pubkey holds:
/// an Admin-bucket witness automatically satisfies Writer + Reader
/// PKA entries for the same pubkey, matching the pass-3 link
/// validator's admin ⊆ writer ⊆ reader semantics.
pub(super) fn check_witness_pka_bidirectional(
    witnesses: &[RecipientWitness],
    public_key_acl: &Acl,
) -> Option<ValidateCallbackResult> {
    // Combined worst-case cost: O(N_witnesses * N_pka) string
    // comparisons across the forward + reverse passes; with
    // HIVEGROUP_MAX_WITNESSES = 256 and a similarly-bounded PKA, this
    // is ≤ 2 * 256 * 256 = 131_072 ~50-char compares per validate call.
    // Bounded; acceptable as a one-shot commit-time check.
    // Pre-compute every witness's pubkey-string + bucket exactly once.
    // The forward and reverse passes both iterate witnesses N_pka and
    // N_witnesses times respectively; without this pre-pass each
    // iteration would re-`to_string()` the pubkey, producing
    // O(N_pka * N_witnesses) String allocations. With it the allocator
    // pressure is O(N_witnesses).
    let witness_strings: Vec<(String, AclBucket)> = witnesses
        .iter()
        .map(|w| (w.pubkey.to_string(), w.bucket))
        .collect();
    // Reject duplicate witnesses for the same pubkey — would let a
    // coordinator over-stamp a pubkey across multiple buckets to mask
    // an over-claim. One canonical witness per pubkey, period.
    {
        let mut seen: std::collections::HashSet<&str> =
            std::collections::HashSet::with_capacity(witness_strings.len());
        for (pubkey_str, _bucket) in &witness_strings {
            if !seen.insert(pubkey_str.as_str()) {
                return Some(ValidateCallbackResult::Invalid(format!(
                    "HiveGroup recipient_witnesses contains duplicate \
                     pubkey {pubkey_str} (one canonical witness per pubkey)",
                )));
            }
        }
    }
    // Forward direction — every PKA pubkey backed by a dominating
    // witness. Iterates the chained PKA buckets lazily (no Vec).
    // Owner bucket is a single string (may be empty for non-Owner-
    // bearing headers); admin/writer/reader are vecs.
    let pka_iter = std::iter::once((AclBucket::Owner, public_key_acl.owner.as_str()))
        .filter(|(_, s)| !s.is_empty())
        .chain(
            public_key_acl
                .admin
                .iter()
                .map(|s| (AclBucket::Admin, s.as_str())),
        )
        .chain(
            public_key_acl
                .writer
                .iter()
                .map(|s| (AclBucket::Writer, s.as_str())),
        )
        .chain(
            public_key_acl
                .reader
                .iter()
                .map(|s| (AclBucket::Reader, s.as_str())),
        );
    for (bucket, pubkey_str) in pka_iter {
        let backed = witness_strings
            .iter()
            .any(|(wp, wb)| wp == pubkey_str && wb.dominates(bucket));
        if !backed {
            return Some(ValidateCallbackResult::Invalid(format!(
                "HiveGroup public_key_acl.{:?} entry {} is not backed by \
                 any dominating recipient_witness",
                bucket, pubkey_str,
            )));
        }
    }
    // Reverse direction — every witness's pubkey appears in the PKA
    // bucket it claims (no over-claim).
    for (pubkey_str, bucket) in &witness_strings {
        let pka_bucket = match bucket {
            AclBucket::Owner => std::slice::from_ref(&public_key_acl.owner),
            AclBucket::Admin => public_key_acl.admin.as_slice(),
            AclBucket::Writer => public_key_acl.writer.as_slice(),
            AclBucket::Reader => public_key_acl.reader.as_slice(),
        };
        if !pka_bucket.iter().any(|s| s == pubkey_str) {
            return Some(ValidateCallbackResult::Invalid(format!(
                "HiveGroup recipient_witness for {pubkey_str} claims bucket {:?} \
                 but pubkey is not in public_key_acl.{:?}",
                bucket, bucket,
            )));
        }
    }
    None
}

/// Per-witness commit-time verification: fetch the cited
/// [`crate::group::GroupMembership`], confirm grantee identity, group
/// containment in a dominating `group_acl` bucket, role satisfaction,
/// and unexpired timing. One `must_get_valid_record` per call.
pub(super) fn verify_recipient_witness(
    witness: &RecipientWitness,
    group_acl: &AclByGroupGenesis,
    timestamp: &Timestamp,
) -> ExternResult<ValidateCallbackResult> {
    let (_membership_author, membership) = fetch_group_membership(&witness.membership_hash)?;
    if membership.for_agent != witness.pubkey {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveGroup recipient_witness membership {} grants role to {} \
             but witness claims pubkey {}",
            witness.membership_hash, membership.for_agent, witness.pubkey,
        )));
    }
    // The membership's group MUST live in a `group_acl` bucket that is
    // dominated by — or equal to — the witness bucket. Owner-bucket
    // witnesses only accept Owner-bucket groups; Reader-bucket
    // witnesses accept any bucket (admin ∪ writer ∪ reader ∪ owner).
    // Group-bucket acceptance follows AclBucket dominance
    // (Owner > Admin > Writer > Reader): a witness in bucket X
    // accepts a membership in any group_acl bucket Y where
    // X.dominates(Y). Owner-bucket witnesses require the owner group
    // exactly. Admin accepts owner∪admin; Writer accepts
    // owner∪admin∪writer; Reader accepts all four. This intentionally
    // diverges from the per-class link validator semantics in
    // `validate_create_link_humm_content_acl` (which gates on the
    // bucket only, e.g. HummContentAdmin → group_acl.admin only):
    // witnesses use dominance because a single high-bucket witness
    // must cover the corresponding lower-bucket PKA entries for the
    // same pubkey under humm-tauri's inclusive-listing convention.
    //
    // The match arms inline `.any()` over the chained iterator to
    // avoid materialising a Vec<&ActionHash> per witness call (bounded
    // worst case ~16KB of pointer allocations at HIVEGROUP_MAX_WITNESSES *
    // GROUP_ACL_MAX_GROUPS); the dominance ordering is preserved
    // because each higher bucket adds groups from the immediately
    // higher-authority bucket onto the same iterator chain.
    let group_in_accepted_bucket = match witness.bucket {
        AclBucket::Owner => group_acl.owner == membership.group_genesis_hash,
        AclBucket::Admin => std::iter::once(&group_acl.owner)
            .chain(group_acl.admin.iter())
            .any(|g| g == &membership.group_genesis_hash),
        AclBucket::Writer => std::iter::once(&group_acl.owner)
            .chain(group_acl.admin.iter())
            .chain(group_acl.writer.iter())
            .any(|g| g == &membership.group_genesis_hash),
        AclBucket::Reader => std::iter::once(&group_acl.owner)
            .chain(group_acl.admin.iter())
            .chain(group_acl.writer.iter())
            .chain(group_acl.reader.iter())
            .any(|g| g == &membership.group_genesis_hash),
    };
    if !group_in_accepted_bucket {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveGroup recipient_witness membership {} is for group {} \
             which is not in group_acl bucket {:?} or any dominating bucket",
            witness.membership_hash, membership.group_genesis_hash, witness.bucket,
        )));
    }
    // Role satisfies the bucket (Admin-bucket → group Admin+ etc.).
    let required = bucket_required_role(witness.bucket);
    if !role_satisfies(membership.role, required) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "HiveGroup recipient_witness membership {} grants role {:?}, \
             required {:?} for bucket {:?}",
            witness.membership_hash, membership.role, required, witness.bucket,
        )));
    }
    // Unexpired at the entry's timestamp.
    if let Some(expiry) = membership.expiry {
        if timestamp > &expiry {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "HiveGroup recipient_witness membership {} expired at {:?}; \
                 action timestamp {:?}",
                witness.membership_hash, expiry, timestamp,
            )));
        }
    }
    Ok(ValidateCallbackResult::Valid)
}

/// `AclSpec::DirectMessage` validator. The DM author must be one of
/// the recipients (no impersonation); the recipient set must be
/// 2..=`DM_MAX_RECIPIENTS` (no zero-recipient DMs, no over-cap
/// fanout) AND unique (no degenerate self-DM with duplicate keys);
/// `public_key_acl.reader` (as string-form pubkeys) MUST equal the
/// recipient set so I-A delete authority remains symmetric (either
/// party can delete by pubkey-match against `public_key_acl.reader`).
fn validate_directmessage_acl(
    author: &AgentPubKey,
    recipients: &[AgentPubKey],
    public_key_acl: &Acl,
) -> ExternResult<ValidateCallbackResult> {
    if recipients.len() < 2 {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "DirectMessage recipients.len() = {} (must be >= 2)",
            recipients.len(),
        )));
    }
    if recipients.len() > DM_MAX_RECIPIENTS {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "DirectMessage recipients.len() = {} exceeds DM_MAX_RECIPIENTS = {}",
            recipients.len(),
            DM_MAX_RECIPIENTS,
        )));
    }
    if !recipients.iter().any(|r| r == author) {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "DirectMessage author {author} is not in recipients",
        )));
    }
    // Uniqueness: reject duplicate recipients. Degenerate self-DM
    // (recipients = [me, me]) would otherwise pass the cardinality +
    // reader-equality checks and produce ambiguous on-DHT shape.
    let mut seen: std::collections::HashSet<&AgentPubKey> =
        std::collections::HashSet::with_capacity(recipients.len());
    for r in recipients.iter() {
        if !seen.insert(r) {
            return Ok(ValidateCallbackResult::Invalid(format!(
                "DirectMessage recipients contains duplicate pubkey {r}",
            )));
        }
    }
    // Reader bucket MUST equal recipients (set equality on string
    // form). Other buckets MUST be empty — DM has no owner/admin/writer
    // semantics, just the symmetric reader bucket.
    if !public_key_acl.owner.is_empty()
        || !public_key_acl.admin.is_empty()
        || !public_key_acl.writer.is_empty()
    {
        return Ok(ValidateCallbackResult::Invalid(
            "DirectMessage public_key_acl owner/admin/writer must be empty".into(),
        ));
    }
    let mut sorted_expected: Vec<String> = recipients.iter().map(|r| r.to_string()).collect();
    sorted_expected.sort();
    let mut sorted_actual = public_key_acl.reader.clone();
    sorted_actual.sort();
    if sorted_expected != sorted_actual {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "DirectMessage public_key_acl.reader {:?} does not match recipients {:?}",
            sorted_actual, sorted_expected,
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}
/// `AclSpec::OpenWrite` validator. No hive/group membership required —
/// this is the outsider-knock + cross-network-discovery contract. If
/// `target_hive_genesis_hash` is `Some`, that hash must resolve to a
/// real `HiveGenesis` (closes the fake-target attack #13).
fn validate_openwrite_acl(
    target_hive_genesis_hash: Option<&ActionHash>,
) -> ExternResult<ValidateCallbackResult> {
    let Some(target) = target_hive_genesis_hash else {
        return Ok(ValidateCallbackResult::Valid);
    };
    // fetch_genesis errors if the hash doesn't reference a real
    // HiveGenesis; we only care that it resolves, not WHO authored.
    crate::hive::fetch_genesis(target).map(|_| ValidateCallbackResult::Valid)
}

pub fn validate_create_encrypted_content(
    action: EntryCreationAction,
    encrypted_content: EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    run_content_validators(action.author(), action.timestamp(), &encrypted_content)
}

pub fn validate_update_encrypted_content(
    action: Update,
    encrypted_content: EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    // **M-1 fix.** Without this guard, any agent with Writer+ in the
    // same hive could commit an Update on their own source chain
    // pointing at *another agent's* EncryptedContent. The Holochain-
    // native update chain would then report the rogue update as part
    // of the original entry's revision graph, letting consumers that
    // resolve updates via app-level discovery be poisoned. The fix:
    // fetch the original record and reject if action.author does not
    // match the original action's author. This mirrors the
    // dispatch_delete_entry pattern in lib.rs (delete authority is
    // bound to the original-author + public_key_acl rule).
    let original = must_get_valid_record(action.original_action_address.clone())?;
    let original_author = original.action().author();
    if &action.author != original_author {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "EncryptedContent update author {} does not match original action author {}",
            action.author, original_author,
        )));
    }
    let original_content = original
        .entry()
        .to_app_option::<EncryptedContent>()
        .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?;
    let Some(original_content) = original_content else {
        return Ok(ValidateCallbackResult::Invalid(
            "update original is not an EncryptedContent".to_string(),
        ));
    };
    let continuity =
        validate_update_continuity(&original_content.header, &encrypted_content.header);
    if !matches!(continuity, ValidateCallbackResult::Valid) {
        return Ok(continuity);
    }
    run_content_validators(&action.author, &action.timestamp, &encrypted_content)
}

/// Author may always delete. Otherwise a `DirectMessage`'s `reader` bucket is
/// its recipient set (they may delete their copy); for every other scope only
/// owner/admin/writer may delete — a pure reader is read-only.
pub fn validate_delete_encrypted_content(
    action: Delete,
    original_action: EntryCreationAction,
    original_entry: EncryptedContent,
) -> ExternResult<ValidateCallbackResult> {
    if &action.author == original_action.author() {
        return Ok(ValidateCallbackResult::Valid);
    }
    let deleter = action.author.to_string();
    let pka = &original_entry.header.public_key_acl;
    let authorized = match &original_entry.header.acl_spec {
        AclSpec::DirectMessage { .. } => pka.reader.contains(&deleter),
        AclSpec::HiveGroup { .. } | AclSpec::Public { .. } | AclSpec::OpenWrite { .. } => {
            pka.owner == deleter || pka.admin.contains(&deleter) || pka.writer.contains(&deleter)
        }
    };
    if authorized {
        return Ok(ValidateCallbackResult::Valid);
    }
    Ok(ValidateCallbackResult::Invalid(format!(
        "delete by {} is not authorised for this content's ACL scope",
        action.author,
    )))
}
