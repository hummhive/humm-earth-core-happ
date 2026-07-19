//! Conductor proof for pass-7 M6 coordinator riders: opt-in per-root
//! liveness (B10) and reindex-on-update of Dynamic discovery links.

mod support;

use std::time::Duration;

use holo_hash::ActionHash;
use holochain::sweettest::{SweetConductor, SweetZome};
use holochain_types::prelude::UnsafeBytes;
use serde::{Deserialize, Serialize};
use support::{
	create_hive, owner_only_acl, single_conductor_cell_app, AclSpec, CreateEncryptedContentInput,
	CreateResponse, EncryptedContent, EncryptedContentHeader,
};

const POLL_ATTEMPTS: usize = 200;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Serialize)]
struct ListByAuthorInput {
	author: String,
	content_type: String,
	since_ts: Option<i64>,
	limit: Option<usize>,
	include_liveness: bool,
}

#[derive(Debug, Serialize)]
struct ListByDynamicLinkInput {
	hive_genesis_hash: ActionHash,
	content_type: String,
	dynamic_link: String,
	include_liveness: bool,
}

#[derive(Debug, Serialize)]
struct UpdateInput {
	previous_encrypted_content_hash: ActionHash,
	updated_encrypted_content: EncryptedContent,
	dynamic_links: Option<Vec<String>>,
	remove_dynamic_links: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct LivenessRecord {
	hash: String,
	original_hash: String,
	#[serde(default)]
	tombstoned: Option<bool>,
}

fn open_write_header(author: &str, id: &str, hive: &ActionHash) -> EncryptedContentHeader {
	EncryptedContentHeader {
		id: id.to_string(),
		display_hive_id: "sweettest-hive".to_string(),
		content_type: "post".to_string(),
		revision_author_signing_public_key: author.to_string(),
		acl_spec: AclSpec::OpenWrite {
			target_hive_genesis_hash: Some(hive.clone()),
		},
		public_key_acl: owner_only_acl(author),
	}
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

	let _: ActionHash = conductor
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

async fn dynamic_hashes(
	conductor: &SweetConductor,
	zome: &SweetZome,
	hive: &ActionHash,
	label: &str,
) -> Vec<String> {
	let records: Vec<LivenessRecord> = conductor
		.call(
			zome,
			"list_by_dynamic_link",
			ListByDynamicLinkInput {
				hive_genesis_hash: hive.clone(),
				content_type: "post".to_string(),
				dynamic_link: label.to_string(),
				include_liveness: false,
			},
		)
		.await;
	records.into_iter().map(|r| r.hash).collect()
}

async fn wait_until_dynamic_count(
	conductor: &SweetConductor,
	zome: &SweetZome,
	hive: &ActionHash,
	label: &str,
	expected: usize,
) -> Vec<String> {
	for _ in 0..POLL_ATTEMPTS {
		let hashes = dynamic_hashes(conductor, zome, hive, label).await;
		if hashes.len() == expected {
			return hashes;
		}
		tokio::time::sleep(POLL_INTERVAL).await;
	}
	panic!("dynamic label {label} never reached {expected} records");
}

#[tokio::test(flavor = "multi_thread")]
async fn update_reindexes_dynamic_links_exclusively() {
	let (conductor, _cell, zome) = single_conductor_cell_app().await;
	let hive = create_hive(&conductor, &zome, "reindex-hive").await;
	let author = zome.cell_id().agent_pubkey().to_string();

	let created = create_fixed_bytes(
		&conductor,
		&zome,
		&hive,
		"reindex",
		vec![1u8],
		Some(vec!["e1".to_string()]),
	)
	.await;
	wait_until_dynamic_count(&conductor, &zome, &hive, "e1", 1).await;

	let updated_content = EncryptedContent {
		header: open_write_header(&author, "reindex", &hive),
		bytes: UnsafeBytes::from(vec![2u8]).into(),
	};
	let _: CreateResponse = conductor
		.call(
			&zome,
			"update_encrypted_content",
			UpdateInput {
				previous_encrypted_content_hash: ActionHash::try_from(created.as_str()).unwrap(),
				updated_encrypted_content: updated_content,
				dynamic_links: Some(vec!["e2".to_string()]),
				remove_dynamic_links: Some(vec!["e1".to_string()]),
			},
		)
		.await;

	wait_until_dynamic_count(&conductor, &zome, &hive, "e2", 1).await;
	wait_until_dynamic_count(&conductor, &zome, &hive, "e1", 0).await;
}
