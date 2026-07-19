//! Conductor proof for pass-7 M4/M5: cross-generation content lineage.
//! One agent installs the vendored pass-6 cell and the pass-7 cell on one
//! conductor, writes content in pass-6, then claims it from pass-7 with a
//! bridge-probed lineage link — and the reverse lookup resolves it.

mod support;

use holochain::prelude::CellId;
use holochain::sweettest::{SweetAgents, SweetCell, SweetConductor};
use holochain_types::prelude::UnsafeBytes;
use serde::{Deserialize, Serialize};
use support::{
	create_hive, create_open_write_content, load_dna, load_pass_6_dna, owner_only_acl, AclSpec,
	ContentRecord, CreateEncryptedContentInput,
};

#[derive(Debug, Serialize)]
struct ContentLineage {
	prior_dna_hash_b64: String,
	prior_action_hash_b64: String,
}

#[derive(Debug, Serialize)]
struct CreateWithLineageInput {
	create: CreateEncryptedContentInput,
	lineage: ContentLineage,
	prior_cell: Option<CellId>,
}

#[derive(Debug, Serialize)]
struct ResolveByPriorInput {
	prior_dna_hash_b64: String,
	prior_action_hash_b64: String,
}

#[derive(Debug, Deserialize)]
struct UpsertResponse {
	was_created: bool,
	was_updated: bool,
}

async fn two_generation_cells(conductor: &mut SweetConductor) -> (SweetCell, SweetCell) {
	let agent = SweetAgents::one(conductor.keystore()).await;
	let gen6 = conductor
		.setup_app_for_agent("gen6", agent.clone(), &[("humm_earth_core".into(), load_pass_6_dna().await)])
		.await
		.unwrap();
	let gen7 = conductor
		.setup_app_for_agent("gen7", agent, &[("humm_earth_core".into(), load_dna().await)])
		.await
		.unwrap();
	let (cell6,): (SweetCell,) = gen6.into_tuple();
	let (cell7,): (SweetCell,) = gen7.into_tuple();
	(cell6, cell7)
}

fn open_write_create(author: &str, id: &str) -> CreateEncryptedContentInput {
	CreateEncryptedContentInput {
		id: id.to_string(),
		display_hive_id: String::new(),
		content_type: "migrated-post".to_string(),
		revision_author_signing_public_key: author.to_string(),
		bytes: UnsafeBytes::from(id.as_bytes().to_vec()).into(),
		acl_spec: AclSpec::OpenWrite {
			target_hive_genesis_hash: None,
		},
		public_key_acl: owner_only_acl(author),
		dynamic_links: None,
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn lineage_roundtrip_resolves_prior_content_across_generations() {
	let mut conductor = SweetConductor::from_standard_config().await;
	let (cell6, cell7) = two_generation_cells(&mut conductor).await;
	let zome6 = cell6.zome("content");
	let zome7 = cell7.zome("content");
	let author = cell7.agent_pubkey().to_string();

	let hive6 = create_hive(&conductor, &zome6, "gen6-hive").await;
	let prior_action = create_open_write_content(&conductor, &zome6, hive6, "post", "c1", None).await;
	let prior_dna = cell6.dna_hash().to_string();

	let lineage_input = |prior_cell: Option<CellId>| CreateWithLineageInput {
		create: open_write_create(&author, "gen7-c1"),
		lineage: ContentLineage {
			prior_dna_hash_b64: prior_dna.clone(),
			prior_action_hash_b64: prior_action.clone(),
		},
		prior_cell,
	};

	let created: UpsertResponse = conductor
		.call(
			&zome7,
			"create_encrypted_content_with_lineage",
			lineage_input(Some(cell6.cell_id().clone())),
		)
		.await;
	assert!(created.was_created && !created.was_updated);

	let resolve = ResolveByPriorInput {
		prior_dna_hash_b64: prior_dna.clone(),
		prior_action_hash_b64: prior_action.clone(),
	};
	let found: Vec<ContentRecord> = conductor
		.call(&zome7, "resolve_by_prior_generation", resolve)
		.await;
	assert_eq!(found.len(), 1);

	let retried: UpsertResponse = conductor
		.call(
			&zome7,
			"create_encrypted_content_with_lineage",
			lineage_input(Some(cell6.cell_id().clone())),
		)
		.await;
	assert!(!retried.was_created, "retry must find-win, not create a duplicate");

	let resolve_again = ResolveByPriorInput {
		prior_dna_hash_b64: prior_dna,
		prior_action_hash_b64: prior_action,
	};
	let found_again: Vec<ContentRecord> = conductor
		.call(&zome7, "resolve_by_prior_generation", resolve_again)
		.await;
	assert_eq!(found_again.len(), 1, "find-wins must not duplicate the reverse-lookup");
}

#[tokio::test(flavor = "multi_thread")]
async fn lineage_probe_rejects_a_foreign_prior_author() {
	let mut conductor = SweetConductor::from_standard_config().await;
	let (cell6, _cell7) = two_generation_cells(&mut conductor).await;
	let zome6 = cell6.zome("content");

	let hive6 = create_hive(&conductor, &zome6, "gen6-hive").await;
	let prior_action = create_open_write_content(&conductor, &zome6, hive6, "post", "c1", None).await;
	let prior_dna = cell6.dna_hash().to_string();

	let other = SweetAgents::one(conductor.keystore()).await;
	let gen7b = conductor
		.setup_app_for_agent("gen7b", other.clone(), &[("humm_earth_core".into(), load_dna().await)])
		.await
		.unwrap();
	let (cell7b,): (SweetCell,) = gen7b.into_tuple();
	let zome7b = cell7b.zome("content");

	let claim = CreateWithLineageInput {
		create: open_write_create(&other.to_string(), "gen7b-c1"),
		lineage: ContentLineage {
			prior_dna_hash_b64: prior_dna,
			prior_action_hash_b64: prior_action,
		},
		prior_cell: Some(cell6.cell_id().clone()),
	};
	let result: Result<UpsertResponse, _> = conductor
		.call_fallible(&zome7b, "create_encrypted_content_with_lineage", claim)
		.await;
	let err = format!("{:?}", result.expect_err("foreign-author probe must reject"));
	assert!(
		err.contains("lineage prior record was not authored by the caller"),
		"unexpected error: {err}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn lineage_shape_rejects_malformed_and_self_referential_claims() {
	let mut conductor = SweetConductor::from_standard_config().await;
	let (_cell6, cell7) = two_generation_cells(&mut conductor).await;
	let zome7 = cell7.zome("content");
	let author = cell7.agent_pubkey().to_string();
	let gen7_dna = cell7.dna_hash().to_string();
	let valid_action = create_open_write_content(
		&conductor,
		&zome7,
		create_hive(&conductor, &zome7, "gen7-hive").await,
		"post",
		"seed",
		None,
	)
	.await;

	let swapped_prefix = CreateWithLineageInput {
		create: open_write_create(&author, "bad-dna"),
		lineage: ContentLineage {
			prior_dna_hash_b64: valid_action.clone(),
			prior_action_hash_b64: valid_action.clone(),
		},
		prior_cell: None,
	};
	let swapped: Result<UpsertResponse, _> = conductor
		.call_fallible(&zome7, "create_encrypted_content_with_lineage", swapped_prefix)
		.await;
	let swapped_err = format!("{:?}", swapped.expect_err("action-typed dna must reject"));
	assert!(
		swapped_err.contains("lineage prior dna hash is not a valid DNA hash"),
		"unexpected error: {swapped_err}"
	);

	let self_ref = CreateWithLineageInput {
		create: open_write_create(&author, "self-ref"),
		lineage: ContentLineage {
			prior_dna_hash_b64: gen7_dna,
			prior_action_hash_b64: valid_action,
		},
		prior_cell: None,
	};
	let selfie: Result<UpsertResponse, _> = conductor
		.call_fallible(&zome7, "create_encrypted_content_with_lineage", self_ref)
		.await;
	let self_err = format!("{:?}", selfie.expect_err("self-referential dna must reject"));
	assert!(
		self_err.contains("lineage must cite a prior generation, not this one"),
		"unexpected error: {self_err}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn lineage_unprobed_write_is_allowed_without_a_status_field() {
	let mut conductor = SweetConductor::from_standard_config().await;
	let (cell6, cell7) = two_generation_cells(&mut conductor).await;
	let zome6 = cell6.zome("content");
	let zome7 = cell7.zome("content");
	let author = cell7.agent_pubkey().to_string();

	let hive6 = create_hive(&conductor, &zome6, "gen6-hive").await;
	let prior_action = create_open_write_content(&conductor, &zome6, hive6, "post", "c1", None).await;
	let prior_dna = cell6.dna_hash().to_string();

	let unprobed = CreateWithLineageInput {
		create: open_write_create(&author, "gen7-unprobed"),
		lineage: ContentLineage {
			prior_dna_hash_b64: prior_dna.clone(),
			prior_action_hash_b64: prior_action.clone(),
		},
		prior_cell: None,
	};
	let created: UpsertResponse = conductor
		.call(&zome7, "create_encrypted_content_with_lineage", unprobed)
		.await;
	assert!(created.was_created && !created.was_updated);

	let resolve = ResolveByPriorInput {
		prior_dna_hash_b64: prior_dna,
		prior_action_hash_b64: prior_action,
	};
	let found: Vec<ContentRecord> = conductor
		.call(&zome7, "resolve_by_prior_generation", resolve)
		.await;
	assert_eq!(found.len(), 1, "an unprobed claim still writes and resolves");
}
