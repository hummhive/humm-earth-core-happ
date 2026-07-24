//! Conductor proof for the coordinator's opt-in per-root liveness rider (B10).

mod support;


use holo_hash::ActionHash;
use holochain::sweettest::{SweetConductor, SweetZome};
use holochain_types::prelude::UnsafeBytes;
use serde::{Deserialize, Serialize};
use support::{
	create_hive, owner_only_acl, single_conductor_cell_app, AclSpec, CreateEncryptedContentInput,
	CreateResponse,
};


#[derive(Debug, Serialize)]
struct ListByAuthorInput {
	author: String,
	content_type: String,
	since_ts: Option<i64>,
	limit: Option<usize>,
	include_liveness: bool,
}

#[derive(Debug, Deserialize)]
struct LivenessRecord {
	original_hash: String,
	#[serde(default)]
	tombstoned: Option<bool>,
}


async fn create_fixed_bytes(
	conductor: &SweetConductor,
	zome: &SweetZome,
	hive: &ActionHash,
	id: &str,
	bytes: Vec<u8>,
	dynamic_links: Option<Vec<String>>,
) -> String {
	let author = zome.cell_id().agent_pubkey().to_string();
	let response: CreateResponse = conductor
		.call(
			zome,
			"create_encrypted_content",
			CreateEncryptedContentInput {
				id: id.to_string(),
				display_hive_id: "sweettest-hive".to_string(),
				content_type: "post".to_string(),
				revision_author_signing_public_key: author.clone(),
				bytes: UnsafeBytes::from(bytes).into(),
				acl_spec: AclSpec::OpenWrite {
					target_hive_genesis_hash: Some(hive.clone()),
				},
				public_key_acl: owner_only_acl(&author),
				dynamic_links,
			},
		)
		.await;
	response.hash
}

#[tokio::test(flavor = "multi_thread")]
async fn liveness_flag_marks_live_roots_and_defaults_off() {
	let (conductor, _cell, zome) = single_conductor_cell_app().await;
	let hive = create_hive(&conductor, &zome, "b10-hive").await;
	let author = zome.cell_id().agent_pubkey().to_string();
	let live = create_fixed_bytes(&conductor, &zome, &hive, "live", vec![1u8, 1], None).await;

	let author_input = |include_liveness: bool| ListByAuthorInput {
		author: author.clone(),
		content_type: "post".to_string(),
		since_ts: None,
		limit: None,
		include_liveness,
	};

	let plain: Vec<LivenessRecord> = conductor
		.call(&zome, "list_by_author", author_input(false))
		.await;
	assert!(
		plain.iter().all(|r| r.tombstoned.is_none()),
		"default read leaves every tombstoned None"
	);
	assert!(
		plain.iter().any(|r| r.original_hash == live),
		"live content is listed"
	);

	let flagged: Vec<LivenessRecord> = conductor
		.call(&zome, "list_by_author", author_input(true))
		.await;
	let live_record = flagged
		.iter()
		.find(|r| r.original_hash == live)
		.expect("live content resolves under the liveness flag");
	assert_eq!(
		live_record.tombstoned,
		Some(false),
		"a live never-deleted root flags not-tombstoned"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_content_is_absent_not_flagged_tombstoned() {
	let (conductor, _cell, zome) = single_conductor_cell_app().await;
	let hive = create_hive(&conductor, &zome, "b10-delete-hive").await;
	let author = zome.cell_id().agent_pubkey().to_string();
	let doomed = create_fixed_bytes(&conductor, &zome, &hive, "doomed", vec![2u8, 2], None).await;

	let _delete_action_hash: ActionHash = conductor
		.call(
			&zome,
			"delete_encrypted_content",
			ActionHash::try_from(doomed.as_str()).unwrap(),
		)
		.await;

	let flagged: Vec<LivenessRecord> = conductor
		.call(
			&zome,
			"list_by_author",
			ListByAuthorInput {
				author,
				content_type: "post".to_string(),
				since_ts: None,
				limit: None,
				include_liveness: true,
			},
		)
		.await;
	assert!(
		flagged.iter().all(|r| r.original_hash != doomed),
		"ordinarily deleted content is absent, never present with tombstoned Some(true)"
	);
}
