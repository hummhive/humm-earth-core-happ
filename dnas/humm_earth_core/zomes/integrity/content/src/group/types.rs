use hdi::prelude::*;

use crate::hive::Role;

/// The per-group root-of-trust entry. Its **action hash** is the group's
/// cryptographic identity throughout the DNA (mirroring [`HiveGenesis`]
/// at the hive level).
///
/// Immutable: update + delete both reject. To deprecate a group, stop
/// granting memberships and let existing ones expire.
///
/// ## Fields
///
/// - `hive_genesis_hash` — the parent hive. Binds the group into a hive
///   trust domain so the hive owner is sovereign over it. MUST resolve
///   to a real [`HiveGenesis`].
/// - `display_id` — the legacy humm-tauri group squuid. For a *system
///   role group* this is LOAD-BEARING (pass-7): present (1-256 chars)
///   and unique per hive on the author's chain — the owner-attested
///   anchor that role-SS re-keying resolves a role by. For a custom
///   group it stays routing/display only.
/// - `hive_wide_role` — `Some(role)` marks a hive-wide *system role
///   group* (the admin/writer/reader groups created at hive setup);
///   `None` marks an ordinary custom group. Load-bearing: a system role
///   group may only be created by the hive Owner; a custom group needs
///   hive Admin+.
/// - `creator_hive_membership_hash` — the creator's authorising
///   [`crate::hive::HiveMembership`] in `hive_genesis_hash`. `None` =
///   the creator IS the hive genesis author (implicit Owner). Persisted
///   so the create validator can re-walk hive authority.
/// - `created_at_microseconds` — informational only (UI ordering); not
///   compared against `action.timestamp` by any validator.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct GroupGenesis {
    pub hive_genesis_hash: ActionHash,
    pub display_id: String,
    pub hive_wide_role: Option<Role>,
    pub creator_hive_membership_hash: Option<ActionHash>,
    pub created_at_microseconds: i64,
}

/// Bounds for a system-role `GroupGenesis.display_id` (the squuid
/// anchor); enforced only when `hive_wide_role.is_some()`.
pub const GROUP_DISPLAY_ID_MAX_CHARS: usize = 256;

/// A group role grant. Mirrors [`crate::hive::HiveMembership`] exactly,
/// plus the parent-hive witness needed for the hive-sovereign grant
/// route (Path B of [`check_group_authority`]).
///
/// Immutable; revocation is via `expiry` (issue a fresh membership for
/// the same agent with a past `expiry` — consumers use the most-recent
/// valid one). See [`crate::hive::HiveMembership`] for the rationale.
///
/// ## Fields
///
/// - `group_genesis_hash` — the group this grant applies in.
/// - `for_agent` — the grantee. May live in a *different hive* — the
///   field is just a holohash, which is what makes cross-hive group
///   membership (group chat across hives) representable.
/// - `role` — `Owner` / `Admin` / `Writer` / `Reader`.
/// - `grantor_membership_hash` — grantor's authorising
///   [`GroupMembership`] (Path C). `None` = grantor proved authority via
///   the group-author (Path A) or hive-sovereign (Path B) route.
/// - `grantor_hive_membership_hash` — grantor's authorising
///   [`crate::hive::HiveMembership`] in the group's parent hive (Path
///   B). `None` = not relying on a hive membership witness (uses Path A,
///   Path C, or the hive-genesis-author implicit-Owner case).
/// - `expiry` — `None` = permanent; `Some(ts)` = invalid once
///   `timestamp > ts`.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct GroupMembership {
    pub group_genesis_hash: ActionHash,
    pub for_agent: AgentPubKey,
    pub role: Role,
    pub grantor_membership_hash: Option<ActionHash>,
    pub grantor_hive_membership_hash: Option<ActionHash>,
    pub expiry: Option<Timestamp>,
}
