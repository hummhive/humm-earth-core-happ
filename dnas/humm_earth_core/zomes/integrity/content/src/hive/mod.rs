//! Validated hive-membership infrastructure.

mod authority;
mod links;
mod membership;
mod owner;
mod types;

pub use authority::*;
pub use links::{
    validate_create_link_hive_membership_index, validate_delete_link_hive_membership_index,
};
pub use membership::*;
pub use owner::*;
pub use types::*;

#[cfg(test)]
use membership::enforce_hive_grant_window;

#[cfg(test)]
mod tests;
