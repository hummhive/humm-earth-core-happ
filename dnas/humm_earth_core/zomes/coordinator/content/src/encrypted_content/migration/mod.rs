//! Forward-pointer migration markers — let old-DNA clients detect moved data.

mod markers;
mod payload;
mod readers;
mod writers;

pub use markers::{
    MigrationMarker, MigrationMarkerV1, MigrationMarkerV2, MIGRATION_MARKER_CONTENT_TYPE_PREFIX,
    MIGRATION_MARKER_SCHEMA_TAG,
};
pub use payload::{build_marker_payload, build_marker_v2_payload};
pub use readers::{get_migration_marker, get_migration_marker_v2};
pub use writers::{mark_migrated, mark_migrated_v2, MarkMigratedInput, MarkMigratedV2Input};

#[cfg(test)]
mod tests;
