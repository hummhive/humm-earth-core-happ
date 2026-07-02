# Security review A — humm-earth-core-happ dry-refactor

## 1. Title and scope

Full-repository security review pass A for `dry-refactor` / pass-6 candidate of `humm-earth-core-happ`.

Scope covered:

- Integrity zome validation authority, entry/link validation dispatch, `EntryTypes` / `LinkTypes` stability, private entries.
- Hive, Group, EncryptedContent ACL invariants, owner-handoff lineage, invite redemption.
- Coordinator cap grants, remote-signal provenance, migration marker semantics, inbox behavior, query tolerance.
- Migration script/security docs and pass-6 hash/wire-compatibility handoff.

No source fixes, build gates, formatters, lint, or tests were run, per assignment constraints.

## 2. Method / codewalk coverage

Read required context and security workflow material first: `skill://security-review`, `skill://holochain` Architecture/Patterns/ReviewZome, `POSTCOMPACTION.md`, `CLAUDE.md`, `README.md`, `AGENTS.md`, `.baseline-hashes.txt`, `docs/CODEMAPS/{architecture,backend,data,dependencies}.md`, `docs/PASS_6_DRY_REFACTOR_HANDOFF.md`, and the Humm Tauri integration/security handoffs most relevant to this review.

Codewalk coverage:

- Integrity zome: `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs`, `validation_dispatch/*`, `encrypted_content/{types,entry_validation,links/*}`, `hive/{types,authority,membership,owner}`, `group/{types,authority,membership,links}`, `inbox.rs`, `invite.rs`.
- Coordinator zome: `dnas/humm_earth_core/zomes/coordinator/content/src/lib.rs`, `encrypted_content/{mod,crud,get_helpers,queries,migration/*,signals/*}`, `hive/{crud,queries,owner}`, `group/{crud,queries}`, `inbox/{crud,queries}`, `invite.rs`, `linking/*` via the link-validator call chain.
- Migration/deployment/config: `scripts/migrate-dna.ts`, root and zome `Cargo.toml`, `workdir/happ.yaml`, `dnas/humm_earth_core/workdir/dna.yaml`, `package.json`.
- Targeted searches: production unwrap/silent-swallow patterns, forbidden NIST-curve terms, forbidden host calls in integrity validation, cap-grant exposure, wildcard arms, and security/TODO notes.

## 3. Executive verdict

**BLOCK before merge/release.** The pass-6 refactor appears to preserve the claimed entry/link/wire shapes, and the main ACL/owner/migration controls are generally well defended. However, two integrity-level issues are release-blocking: `OriginalHashPointer` remains an unvalidated link type while `update_encrypted_content` trusts the first network pointer result after already committing an update, and update validation can route a cross-entry-type update through the new `EncryptedContent` validator instead of the original immutable entry's validator.

Continued codewalking after the initial BLOCK found this second BLOCK plus additional WARN findings covering stale update indexing, unbounded routing/fan-out hints, migration artifact handling, and a cap-granted read panic surface. Notes below capture accepted/documentation residuals, not new blockers.

## 4. Findings

### BLOCK

#### A-BLOCK-1 — `OriginalHashPointer` links are forgeable/deletable while the update path trusts them

**Evidence**

- `OriginalHashPointer` is a registered public link type: `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs:42-49`.
- Integrity validation accepts both create and delete for `OriginalHashPointer` unconditionally: `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/links.rs:48-50` and `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/links.rs:128-130`.
- The coordinator creates a self-pointer on content create: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:69-75`.
- `update_encrypted_content` commits the update first, then queries `LinkTypes::OriginalHashPointer` with `GetStrategy::Network`, and uses `original_hash_link[0]` as the original hash: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:173-195`.
- The same function then creates `EncryptedContentUpdates` and a new `OriginalHashPointer` from that trusted result: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:196-207`.
- The code already documents that a non-member peer can plant a poison `OriginalHashPointer`: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:154-163` and the regression test comment at `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:258-262`.
- The `EncryptedContentUpdates` validator binds base and target authors, but does not prove the target is a native update of the base/root chain: `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/links/updates.rs:15-23` and `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/links/updates.rs:37-78`.
- Migration marker writers inherit this update plumbing through `update_encrypted_content`: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/migration/writers.rs:20-37` and `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/migration/writers.rs:45-52`.

**Why it matters**

A modified-coordinator adversary can publish or delete `OriginalHashPointer` links at arbitrary bases because integrity validation returns `Valid`. Since the coordinator reads the network link set and trusts index 0 after `update_entry` has already committed, a poisoned or deleted pointer can make a legitimate caller partially mutate their source chain and then fail before link/signal plumbing. If the poison target is another victim-authored `EncryptedContent` action hash, the victim can also be induced into publishing an app-level `EncryptedContentUpdates` link under the wrong original hash.

This does not appear to bypass the `EncryptedContent` update author check, but it is a release-blocking graph-integrity and availability flaw in update/migration behavior.

**Suggested remediation**

- Stop treating `OriginalHashPointer` as an unvalidated/deletable utility link.
- Validate create links: base and target must be `ActionHash`es for `EncryptedContent`, link author must equal the relevant content author, and the base/target relation must match the native update-chain relation (`create` self-pointer or update-to-root pointer).
- Reject `OriginalHashPointer` deletes outright, or at minimum author-gate them; immutable is safer because this pointer is update plumbing.
- In the coordinator, avoid `get_links(...)[0]` as a trust boundary. Prefer deriving the root from native action headers, or require exactly one validated pointer with deterministic handling of duplicates.
- Add tests for third-party create/delete/poison attempts and for the partial-commit failure case. This is an integrity behavior change, so recapture pass-6 DNA/artifact hashes after fixing.

#### A-BLOCK-2 — Cross-entry-type updates can bypass immutable-entry update validators

**Evidence**

- `validate_op` dispatches update validation by the **new** `app_entry` type for `StoreEntry`, `RegisterUpdate`, and `StoreRecord` update ops: `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/mod.rs:22-29` and `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/mod.rs:58-60`.
- The per-entry update dispatcher matches only the new `EntryTypes` variant: `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/entry.rs:34-60`.
- The special `StoreRecord::UpdateEntry` path for `EncryptedContent` reruns create validation and then `validate_update_encrypted_content`, but still does not classify the original entry type: `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/entry.rs:63-80`.
- `validate_update_encrypted_content` fetches the original record and checks only original action author equality before rerunning content validators: `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/entry_validation.rs:503-526`.
- Several other public entry types explicitly claim update immutability only in their own same-type update validators, e.g. `HiveGenesis`: `dnas/humm_earth_core/zomes/integrity/content/src/hive/membership.rs:20-27`, `HiveMembership`: `dnas/humm_earth_core/zomes/integrity/content/src/hive/membership.rs:216-223`, `GroupGenesis`: `dnas/humm_earth_core/zomes/integrity/content/src/group/membership.rs:30-40`, owner-handoff entries: `dnas/humm_earth_core/zomes/integrity/content/src/hive/owner.rs:60-67` and `dnas/humm_earth_core/zomes/integrity/content/src/hive/owner.rs:95-102`.

**Why it matters**

The confirmed code path validates an update according to the new app-entry type, not the original app-entry type. [INFERENCE] A custom coordinator can therefore author an `Update` whose new entry type is `EncryptedContent` while the original action hash points at one of the same-author immutable entry types; the current validator then applies `EncryptedContent` rules instead of the original entry type's immutable-update rejection. This does not appear to let a peer alter another agent's authority records, because existing authority reads fetch concrete action hashes rather than trusting a latest-update projection. It is still an integrity-rule violation: entries documented as immutable can acquire valid native update edges that future consumers or tooling could misread.

**Suggested remediation**

For every public app-entry update, assert that the original record deserializes to the same `EntryTypes` variant as the new entry before running variant-specific update validation. For `EncryptedContent`, reject updates whose original action is not an `EntryTypes::EncryptedContent`.

### WARN

#### A-WARN-1 — Cap-granted read path still has production `unwrap()` assumptions

**Evidence**

- `get_latest_typed_from_eh` unwraps `details.actions.first()`, `sortlist.last()`, `maybe_maybe_typed_entry`, and `record.action().entry_hash()`: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/get_helpers.rs:56-86`.
- `get_encrypted_content` uses this helper: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:118-131`.
- `get_encrypted_content` and list/query externs that transitively call it are in the unrestricted cap grant read surface: `dnas/humm_earth_core/zomes/coordinator/content/src/lib.rs:76-85`.

**Why it matters**

The unwraps likely rely on Holochain `EntryDetails` invariants, but a panic in a zome traps the WASM guest. Since these read externs are remotely cap-callable, malformed/unexpected host-returned details should degrade to `Err`/`None`, not a panic. This is an availability hardening issue, not an observed authority bypass.

**Suggested remediation**

Replace the unwraps with explicit `ok_or_else` / `match` branches and return `ExternResult` errors or `Ok(None)` with clear log context, matching the rest of the coordinator’s tolerant-read style.

#### A-WARN-2 — `Public` / `OpenWrite` `public_key_acl.reader` is unbounded while coordinator fan-out iterates it

**Evidence**

- `AclSpec::Public` explicitly leaves the recipient set unconstrained and treats `public_key_acl.reader` as `['*']` or empty routing hint: `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/types.rs:209-216`.
- `AclSpec::OpenWrite` validates only author/header binding and optional target HiveGenesis existence: `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/types.rs:217-224` and `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/entry_validation.rs:481-493`.
- The validator enforces bounded recipient/fetch cost for `DirectMessage` (`DM_MAX_RECIPIENTS`) and `HiveGroup` witnesses (`HIVEGROUP_MAX_WITNESSES`), but those checks apply only to those variants: `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/types.rs:134-164` and `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/entry_validation.rs:208-228`.
- The coordinator fans out create/update/delete signals by iterating `public_key_acl.reader`: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:50-67`, `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:216-223`, and `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:239-247`.
- That fan-out path decodes every reader string and sends to every valid recipient except self: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/signals/outbound.rs:30-60`.

**Why it matters**

The runtime path is best-effort and local-author initiated, so this is not a remote authority bypass. It is still an avoidable resource-amplification surface: a valid writer can commit a `Public` or `OpenWrite` entry with a very large reader vector, bloating the public entry header and making an honest coordinator allocate/decode/fan out over that list. The code already recognizes the same class of risk for DMs and HiveGroup witnesses; the routing-hint variants should have an explicit cap or an explicit ignore rule.

**Suggested remediation**

Either enforce a shared maximum total `public_key_acl` size for every `AclSpec` variant, or make the coordinator ignore `public_key_acl.reader` for `Public` and `OpenWrite` signals except for a small, validated sentinel such as `'*'`.

#### A-WARN-3 — `EncryptedContent` updates can change ACL/index fields without reindexing discovery links

**Evidence**

- `create_encrypted_content` creates the hive-scoped `Hive`, `HummContentId`, `Dynamic`, and HummContent* ACL link bundle when the new entry binds a hive/group context: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:91-113`.
- `update_encrypted_content` only writes the native update, creates `EncryptedContentUpdates` and `OriginalHashPointer` links, then emits signals; it does not recreate hive/dynamic/ACL/content-id links for the updated header: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/crud.rs:173-225`.
- The integrity update validator checks original action author equality and then reruns content validators on the new `EncryptedContent`; it does not require `acl_spec`, hive context, content type, content id, `public_key_acl`, or dynamic labels to stay index-compatible with the original link bundle: `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/entry_validation.rs:503-526`.
- The repo's own integration handoff documents the mismatch: `update_encrypted_content` accepts ACL changes but leaves the entry under old query paths, while `get_encrypted_content(original_hash)` returns the latest content through the update chain: `docs/HUMM_TAURI_ACLSPEC_MUTATION_ON_UPDATE.md:9-18`, `docs/HUMM_TAURI_ACLSPEC_MUTATION_ON_UPDATE.md:27-37`, and `docs/HUMM_TAURI_ACLSPEC_MUTATION_ON_UPDATE.md:56-82`.

**Why it matters**

This is not a direct write-authority bypass because only the original content author may update. It is still security-relevant for access-scope changes: stale ACL/dynamic/content-id links can keep surfacing the latest updated entry under the old discovery paths, or make the new scope undiscoverable under the new paths. [INFERENCE] If a caller treats update-in-place as a re-share/re-key primitive, old readers or old group paths may continue receiving metadata and latest-action pointers even after the author intended to narrow scope.

**Suggested remediation**

Either make ACL/index-load-bearing fields immutable across `update_encrypted_content` and require re-author + delete for sharing-scope changes, or teach update to delete/recreate every affected discovery link atomically enough for the repo's consistency model. The existing docs already recommend re-authoring; enforce that invariant in code or make the coordinator reject incompatible updates with a clear error.

#### A-WARN-4 — Migration JSON artifacts are written with default filesystem permissions

**Evidence**

- Export bundles contain decoded `EncryptedContent` headers and bytes: `scripts/migrate-dna.ts:364-397`, then write the JSON bundle with `writeFile(..., "utf8")`: `scripts/migrate-dna.ts:715-745`.
- Hive bundles contain member pubkeys, roles, membership hashes, owner pubkey, and old/new hive hashes: `scripts/migrate-dna.ts:434-470`, then `saveHiveBundle` writes with default `writeFile`: `scripts/migrate-dna.ts:540-542`.
- Remap files contain source/target app ids, source/target agent pubkeys, and old/new action-hash mappings: `scripts/migrate-dna.ts:420-431`, then write with default `writeFile`: `scripts/migrate-dna.ts:1231-1232`. Marker failure augmentation rewrites the same remap path with default `writeFile`: `scripts/migrate-dna.ts:1394-1399`.

**Why it matters**

Node's `writeFile` creates files with process-default permissions (subject to umask), not an explicit private mode. The bundle bytes may still be encrypted at the application layer, but the files expose social graph metadata, action-hash remaps, content ids/types, and potentially ciphertext that operators should treat as sensitive during a DNA migration.

**Suggested remediation**

Create migration output files with owner-only permissions (`mode: 0o600`), preserve restrictive mode on rewrites, and document that bundles/remaps are sensitive operational artifacts to share only over trusted channels.

### NOTE

#### A-NOTE-1 — Owner transfer residual remains accepted and documented

**Evidence**

- Current repo state documents the accepted residual: `POSTCOMPACTION.md:34-40`.
- Humm Tauri integration docs state that `is_lineage_owner` is an ever-owner predicate and a malicious past owner can fork/re-seize governance ownership: `docs/HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md:69-89`.
- Integrity code matches that model: `is_lineage_owner` proves genesis root or prior accepted handoff recipient, not current owner: `dnas/humm_earth_core/zomes/integrity/content/src/hive/owner.rs:19-35`.
- Coordinator code resolves current owner deterministically and detects contested forks: `dnas/humm_earth_core/zomes/coordinator/content/src/hive/owner.rs:100-151` and `dnas/humm_earth_core/zomes/coordinator/content/src/hive/owner.rs:168-216`.
- The current-owner precheck for stock `Admin` grants lives in the coordinator: `dnas/humm_earth_core/zomes/coordinator/content/src/hive/crud.rs:90-98`.

**Why it matters**

This is not a new pass-6 regression. It is a governance residual inherent to the chosen agent-centric owner-handoff model; it should remain visible in release and UI wording.

**Suggested remediation**

Keep the existing honest microcopy and contested-owner surfacing. If the product later requires final removal of a past owner, this needs a different authority model, not a patch to the current refactor.

#### A-NOTE-2 — Invite `max_uses` is an advisory soft cap, not hard authority

**Evidence**

- `InviteRedemption` create is intentionally permissionless/advisory: `dnas/humm_earth_core/zomes/integrity/content/src/invite.rs:14-20`.
- Coordinator `redeem_invite_grant` counts redemption links, writes a marker if not already present, and then delegates to validated `create_hive_membership`: `dnas/humm_earth_core/zomes/coordinator/content/src/invite.rs:20-61`.
- Pass-5 integration docs call `max_uses` advisory and identify the validated `HiveMembership` as the real authority: `docs/HUMM_TAURI_OWNER_ROLE_AND_ACL_INTEGRATION.md:215-218`.

**Why it matters**

Concurrent redeems or malicious marker behavior can affect UX/accounting around the soft cap, but cannot mint a valid hive role without passing `HiveMembership` validation.

**Suggested remediation**

Do not present `max_uses` as a cryptographic quota. If a hard quota becomes a requirement, design a new integrity-level authority mechanism.

#### A-NOTE-3 — `DNA_MIGRATION_GUIDE.md` security-model text is stale relative to current update validation

**Evidence**

- The guide still says the integrity update validator does not enforce original-entry authorship: `docs/DNA_MIGRATION_GUIDE.md:474-482`.
- Current code does enforce original action author equality for `EncryptedContent` updates: `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/entry_validation.rs:503-526`.

**Why it matters**

Runtime behavior is stronger than the doc claims, so this is not a code vulnerability. It can still mislead downstream migration/security reviewers about which layer is load-bearing.

**Suggested remediation**

Update the migration guide in a later docs pass to state that pass-5/pass-6 integrity rejects cross-author `EncryptedContent` updates, while marker readers still retain author filtering as defense-in-depth and compatibility with older pass explanations.

#### A-NOTE-4 — Legacy Tryorama/docs drift is verification risk, not a runtime ACL bypass

**Evidence**

- The top-level README still advertises `npm test` as the backend test path and `@holochain/tryorama` as the test framework: `README.md:25-29` and `README.md:52-58`.
- The active conductor harness docs say Tryorama cannot boot on this hc 0.6.x line and Sweettest is the in-process conductor path: `crates/sweettest/README.md:3-12`.
- Some integration docs still call fetch-dependent branches "tryorama only": `docs/HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md:338-349` and `docs/HUMM_TAURI_CORE_HAPP_BDD_SANITY_CHECKS.md:464-470`.
- The legacy TS test helpers still use `any` and stale `hive_id` / `acl` create payload fields: `tests/src/humm_earth_core/content/common.ts:15-18`, `tests/src/humm_earth_core/content/common.ts:44-85`, while current `CreateEncryptedContentInput` requires `display_hive_id`, `acl_spec`, and `public_key_acl`: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/mod.rs:41-68`.

**Why it matters**

This is not a zome runtime vulnerability. It can still mislead reviewers or release operators into trusting the wrong verification path or stale wire-shape examples while assessing security-sensitive ACL behavior.

**Suggested remediation**

In the docs cleanup pass, make Sweettest the advertised conductor behavior gate, mark the legacy Tryorama workspace archival or update it, and remove stale `tryorama only` wording where Sweettest now covers the path.

## 5. Explicit passes / no-findings for checked risk categories

- **Entry/link/wire compatibility:** `EntryTypes` order and private `DmProbeLog` remain visible at `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs:18-40`; `LinkTypes` order remains at `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs:42-70`. Pass-6 handoff also states no entry/link/serde/wire changes: `docs/PASS_6_DRY_REFACTOR_HANDOFF.md:66-80`. No schema-compat finding.
- **Integrity dispatch coverage:** `validate` uses `op.flattened::<EntryTypes, LinkTypes>()?`: `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/mod.rs:16-89`. Entry dispatch handles all `EntryTypes` variants explicitly: `dnas/humm_earth_core/zomes/integrity/content/src/validation_dispatch/entry.rs:9-31`, `:38-60`, `:129-155`. No missing business-variant dispatch found.
- **Forbidden nondeterministic calls in integrity:** targeted search found no `get`, `get_links`, `agent_info`, `sys_time`, `query`, or mutating HDK calls in integrity validation code, beyond comments. Existing validation dependencies use `must_get_valid_record` / `must_get_action`, consistent with the repo’s validation model.
- **Hive authority / grant windows:** `check_hive_authority` binds genesis or membership to agent, hive, role, and expiry: `dnas/humm_earth_core/zomes/integrity/content/src/hive/authority.rs:67-120`. Hive membership creation blocks self-grant, Owner-via-membership, non-owner Admin grants at the integrity floor, founding-owner membership, and expiring-grantor window extension: `dnas/humm_earth_core/zomes/integrity/content/src/hive/membership.rs:64-130` and `:170-211`. No new bypass found beyond the documented owner residual.
- **Group authority / grant windows:** `check_group_authority` covers group author, hive Admin+, and explicit group membership paths: `dnas/humm_earth_core/zomes/integrity/content/src/group/authority.rs:58-128`. Group membership blocks self-grant, role escalation to Owner, and Path-C window extension: `dnas/humm_earth_core/zomes/integrity/content/src/group/membership.rs:77-131` and `:167-215`. No new bypass found.
- **EncryptedContent ACL invariants:** Author/header binding and AclSpec variant dispatch are enforced at `dnas/humm_earth_core/zomes/integrity/content/src/encrypted_content/entry_validation.rs:16-77`. HiveGroup enforces hive/group authority and cross-hive group consistency: `:109-181`. Recipient witnesses enforce bounded, bidirectional PKA coverage plus per-witness membership/role/expiry checks: `:202-414`. DirectMessage enforces cardinality, author inclusion, uniqueness, and `public_key_acl.reader` equality: `:423-480`. Delete keeps readers read-only outside DirectMessage: `:531-554`. No ACL-authority bypass found.
- **Validated discovery links except `OriginalHashPointer`:** Hive, Dynamic, HummContentId, ACL, EncryptedContentUpdates, Inbox, Group, owner-handoff, and invite-redemption link validators bind target type, author, base path, tags, or delete author as appropriate; examples include `encrypted_content/links/hive.rs:26-85`, `dynamic.rs:14-62`, `content_id.rs:9-53`, `acl.rs:44-139`, `updates.rs:26-91`, `inbox.rs:106-161`, `group/links.rs:61-218`, `hive/owner.rs:114-162`, and `invite.rs:42-64`.
- **Coordinator cap grants:** Mutators, local source-chain readers, marker writers, and sender-side signal reflectors are deliberately excluded, while public-DHT reads and `recv_remote_signal` are granted: `dnas/humm_earth_core/zomes/coordinator/content/src/lib.rs:42-71` and `:76-162`. No accidentally granted mutator found.
- **Remote signals:** Outbound signals are pre-encoded through one funnel: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/signals/outbound.rs:8-20`. ACL reader fan-out is best-effort and filters malformed reader pubkeys/self: `outbound.rs:22-60`. `recv_remote_signal` stamps `from_agent` from `call_info()?.provenance` for both content and DM signal families and errors on unknown payloads: `dnas/humm_earth_core/zomes/coordinator/content/src/lib.rs:237-275`. No spoofing regression found.
- **Migration markers:** Marker writers are local-only by cap policy and delegate to update plumbing: `dnas/humm_earth_core/zomes/coordinator/content/src/encrypted_content/migration/writers.rs:33-37`. Readers filter updates to the original/trusted author and return `Ok(None)` for non-marker/malformed marker bytes: `migration/readers.rs:22-68` and `:105-152`. V2 optional fields use `#[serde(default)]`: `migration/markers.rs:139-153`. No marker-forge issue found apart from A-BLOCK-1’s shared update-pointer substrate.
- **Private entries / sensitive local data:** `DmProbeLog` is declared private in `EntryTypes`: `dnas/humm_earth_core/zomes/integrity/content/src/lib.rs:26-30`; it is only created locally via `record_probe`: `dnas/humm_earth_core/zomes/coordinator/content/src/inbox/crud.rs:57-68`; `get_last_probe` is intentionally not cap-granted: `dnas/humm_earth_core/zomes/coordinator/content/src/lib.rs:153-158`. No private-entry exposure found.
- **Secrets / crypto / dependencies:** No hardcoded secrets or forbidden NIST-curve dependencies/usages were found by targeted search. The zome dependency set is small and pinned for Holochain (`hdi = "=0.7.1"`, `hdk = "=0.6.1"`, `holochain_serialized_bytes = "=0.0.57"`): `Cargo.toml:18-26`; coordinator adds only `base64 = "0.22"` for pubkey decode: `dnas/humm_earth_core/zomes/coordinator/content/Cargo.toml:15-22`.
- **Continued pass after initial BLOCK:** one additional BLOCK-level integrity validation issue was found (A-BLOCK-2). No other BLOCK-level authority bypass was found in Hive/Group membership validation, owner-handoff lineage beyond the documented residual, invite redemption, cap-grant mutator exclusion, private `DmProbeLog`, remote-signal provenance stamping, or migration-marker author filtering. Additional residuals found in the continued pass are tracked above as WARN/NOTE rather than BLOCK.

## 6. Open questions

1. Should `OriginalHashPointer` be retained after fixing validation, or should the coordinator derive the original root from native update-chain action headers and retire this link as trusted plumbing?
2. If A-BLOCK-1 or A-BLOCK-2 is fixed on `dry-refactor`, the integrity WASM and DNA hash will change again. The pass-6 candidate hash/docs should be recaptured after the fix, before any release decision.
3. Should cross-entry-type app updates be prohibited centrally in validation dispatch, or should each mutable entry validator own its original-type check?
4. Should `public_key_acl.reader` be globally capped, or should `Public` / `OpenWrite` stop using it for signal fan-out entirely?
5. Should migration bundle/remap paths be treated as secret-bearing by default in both the script and operator docs?
6. Should `update_encrypted_content` reject ACL/index-load-bearing field changes and force re-authoring, or should it become a full reindexing operation?
