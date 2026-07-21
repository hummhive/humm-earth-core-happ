//! Conductor proof for pass-7 M3: per-author system-role `GroupGenesis`
//! uniqueness. A second live `(hive, hive_wide_role)` on one chain is
//! rejected; custom groups and different-hive/different-role system groups
//! are exempt; a self-delete frees the slot; find-or-create converges.

mod support;

use holo_hash::ActionHash;
use support::{
	create_hive, single_conductor_cell_app, wait_for_group_visible, CreateGroupGenesisInput,
	FindOrCreateGenesisResponse, GenesisResponse,
};

const UNIQUENESS_REJECT: &str =
	"a GroupGenesis for this hive and hive-wide role already exists on your chain";
const DISPLAY_ID_REJECT: &str =
	"a system-role GroupGenesis with this display_id already exists in this hive on your chain";
const DISPLAY_ID_BOUNDS_REJECT: &str = "system-role GroupGenesis display_id must be 1-256 chars";

fn system_role_group(hive: &ActionHash, role: &str) -> CreateGroupGenesisInput {
	system_role_group_with_display(hive, role, &format!("{role}-group"))
}

fn system_role_group_with_display(
	hive: &ActionHash,
	role: &str,
	display_id: &str,
) -> CreateGroupGenesisInput {
	CreateGroupGenesisInput {
		hive_genesis_hash: hive.clone(),
		display_id: display_id.to_string(),
		hive_wide_role: Some(role.to_string()),
		creator_hive_membership_hash: None,
	}
}

fn custom_group(hive: &ActionHash, display_id: &str) -> CreateGroupGenesisInput {
	CreateGroupGenesisInput {
		hive_genesis_hash: hive.clone(),
		display_id: display_id.to_string(),
		hive_wide_role: None,
		creator_hive_membership_hash: None,
	}
}


#[tokio::test(flavor = "multi_thread")]
async fn duplicate_system_role_genesis_rejects_on_one_chain() {
	let (conductor, _cell, zome) = single_conductor_cell_app().await;
	let hive = create_hive(&conductor, &zome, "uniqueness-hive").await;

	let first: GenesisResponse = conductor
		.call(&zome, "create_group_genesis", system_role_group(&hive, "Admin"))
		.await;
	wait_for_group_visible(&conductor, &zome, &hive, &first.hash).await;

	let duplicate: Result<GenesisResponse, _> = conductor
		.call_fallible(&zome, "create_group_genesis", system_role_group(&hive, "Admin"))
		.await;
	let err = format!("{:?}", duplicate.expect_err("second system-role create must reject"));
	assert!(err.contains(UNIQUENESS_REJECT), "unexpected error: {err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn same_role_in_different_hives_is_accepted() {
	let (conductor, _cell, zome) = single_conductor_cell_app().await;
	let hive_a = create_hive(&conductor, &zome, "hive-a").await;
	let hive_b = create_hive(&conductor, &zome, "hive-b").await;

	let _: GenesisResponse = conductor
		.call(&zome, "create_group_genesis", system_role_group(&hive_a, "Admin"))
		.await;
	let second: Result<GenesisResponse, _> = conductor
		.call_fallible(&zome, "create_group_genesis", system_role_group(&hive_b, "Admin"))
		.await;
	assert!(
		second.is_ok(),
		"same role in a different hive must be accepted: {second:?}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn custom_groups_are_exempt_from_uniqueness() {
	let (conductor, _cell, zome) = single_conductor_cell_app().await;
	let hive = create_hive(&conductor, &zome, "custom-hive").await;

	let _: GenesisResponse = conductor
		.call(&zome, "create_group_genesis", custom_group(&hive, "dupe-display"))
		.await;
	let second: Result<GenesisResponse, _> = conductor
		.call_fallible(&zome, "create_group_genesis", custom_group(&hive, "dupe-display"))
		.await;
	assert!(
		second.is_ok(),
		"two custom groups (identical display) must both be accepted: {second:?}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn self_delete_frees_the_system_role_slot() {
	let (conductor, _cell, zome) = single_conductor_cell_app().await;
	let hive = create_hive(&conductor, &zome, "delete-recreate-hive").await;

	let original: GenesisResponse = conductor
		.call(&zome, "create_group_genesis", system_role_group(&hive, "Writer"))
		.await;
	wait_for_group_visible(&conductor, &zome, &hive, &original.hash).await;

	let _: ActionHash = conductor
		.call(&zome, "delete_group_genesis", original.hash.clone())
		.await;

	let recreated: Result<GenesisResponse, _> = conductor
		.call_fallible(&zome, "create_group_genesis", system_role_group(&hive, "Writer"))
		.await;
	assert!(
		recreated.is_ok(),
		"recreating a self-deleted system-role group must succeed: {recreated:?}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn find_or_create_system_role_converges() {
	let (conductor, _cell, zome) = single_conductor_cell_app().await;
	let hive = create_hive(&conductor, &zome, "find-or-create-hive").await;

	let first: FindOrCreateGenesisResponse = conductor
		.call(
			&zome,
			"find_or_create_group_genesis",
			system_role_group(&hive, "Admin"),
		)
		.await;
	assert!(first.was_created);
	wait_for_group_visible(&conductor, &zome, &hive, &first.response.hash).await;

	let second: FindOrCreateGenesisResponse = conductor
		.call(
			&zome,
			"find_or_create_group_genesis",
			system_role_group(&hive, "Admin"),
		)
		.await;
	assert!(!second.was_created, "second find_or_create must find, not create");
	assert_eq!(second.response.hash, first.response.hash);
}

#[tokio::test(flavor = "multi_thread")]
async fn same_squuid_across_roles_rejects_in_one_hive() {
	let (conductor, _cell, zome) = single_conductor_cell_app().await;
	let hive = create_hive(&conductor, &zome, "squuid-hive").await;

	let first: GenesisResponse = conductor
		.call(
			&zome,
			"create_group_genesis",
			system_role_group_with_display(&hive, "Admin", "shared-squuid"),
		)
		.await;
	wait_for_group_visible(&conductor, &zome, &hive, &first.hash).await;

	let second: Result<GenesisResponse, _> = conductor
		.call_fallible(
			&zome,
			"create_group_genesis",
			system_role_group_with_display(&hive, "Writer", "shared-squuid"),
		)
		.await;
	let err = format!(
		"{:?}",
		second.expect_err("same-squuid system-role create must reject")
	);
	assert!(err.contains(DISPLAY_ID_REJECT), "unexpected error: {err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn distinct_squuids_across_roles_accept_in_one_hive() {
	let (conductor, _cell, zome) = single_conductor_cell_app().await;
	let hive = create_hive(&conductor, &zome, "squuid-distinct-hive").await;

	let first: GenesisResponse = conductor
		.call(
			&zome,
			"create_group_genesis",
			system_role_group_with_display(&hive, "Admin", "squuid-a"),
		)
		.await;
	wait_for_group_visible(&conductor, &zome, &hive, &first.hash).await;
	let second: Result<GenesisResponse, _> = conductor
		.call_fallible(
			&zome,
			"create_group_genesis",
			system_role_group_with_display(&hive, "Writer", "squuid-b"),
		)
		.await;
	assert!(
		second.is_ok(),
		"distinct squuids must both be accepted: {second:?}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn custom_group_may_reuse_a_system_role_squuid() {
	let (conductor, _cell, zome) = single_conductor_cell_app().await;
	let hive = create_hive(&conductor, &zome, "squuid-custom-hive").await;

	let first: GenesisResponse = conductor
		.call(
			&zome,
			"create_group_genesis",
			system_role_group_with_display(&hive, "Admin", "reused-squuid"),
		)
		.await;
	wait_for_group_visible(&conductor, &zome, &hive, &first.hash).await;
	let custom: Result<GenesisResponse, _> = conductor
		.call_fallible(&zome, "create_group_genesis", custom_group(&hive, "reused-squuid"))
		.await;
	assert!(
		custom.is_ok(),
		"custom create skips the squuid walk: {custom:?}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_squuid_rejects_on_system_role_create() {
	let (conductor, _cell, zome) = single_conductor_cell_app().await;
	let hive = create_hive(&conductor, &zome, "squuid-empty-hive").await;

	let rejected: Result<GenesisResponse, _> = conductor
		.call_fallible(
			&zome,
			"create_group_genesis",
			system_role_group_with_display(&hive, "Admin", ""),
		)
		.await;
	let err = format!("{:?}", rejected.expect_err("empty squuid must reject"));
	assert!(err.contains(DISPLAY_ID_BOUNDS_REJECT), "unexpected error: {err}");
}
