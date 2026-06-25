//! Validated group-membership infrastructure.

mod authority;
mod links;
mod membership;
mod types;

pub use authority::*;
#[allow(unused_imports)]
pub(crate) use links::{link_authors_target_entry, require_link_author_is, target_action_hash};
pub use links::{
    validate_create_link_agent_to_group_memberships,
    validate_create_link_group_to_group_memberships, validate_create_link_hive_to_groups,
    validate_delete_group_link,
};
pub use membership::*;
pub use types::*;

#[cfg(test)]
use membership::enforce_grant_window;

#[cfg(test)]
mod tests;
