use content_integrity::*;
use hdk::prelude::*;

use super::crud::{create_encrypted_content, get_encrypted_content};
use super::paging::canonical_lowest_hash;
use super::service_records::UpsertContentResponse;
use super::{CreateEncryptedContentInput, EncryptedContentResponse};

/// Input to [`create_encrypted_content_with_lineage`]: a normal create
/// plus the prior-generation provenance and an optional prior cell to
/// probe for authorship. `create.lineage` is overwritten by `lineage`.
#[derive(Serialize, Deserialize, Debug)]
pub struct CreateWithLineageInput {
    pub create: CreateEncryptedContentInput,
    pub lineage: ContentLineage,
    pub prior_cell: Option<CellId>,
}

/// Input to [`resolve_by_prior_generation`]: the prior-generation pair to
/// resolve forward into this generation's content.
#[derive(Serialize, Deserialize, Debug)]
pub struct ResolveByPriorInput {
    pub prior_dna_hash_b64: String,
    pub prior_action_hash_b64: String,
}

fn guest(message: &str) -> WasmError {
    wasm_error!(WasmErrorInner::Guest(message.to_string()))
}

fn lineage_base(
    prior_dna_hash_b64: &str,
    prior_action_hash_b64: &str,
) -> ExternResult<AnyLinkableHash> {
    Ok(Path::from(vec![
        Component::from(prior_dna_hash_b64.to_string()),
        Component::from(prior_action_hash_b64.to_string()),
    ])
    .path_entry_hash()?
    .into())
}

fn resolve_lineage_records(
    prior_dna_hash_b64: &str,
    prior_action_hash_b64: &str,
    author: Option<&AgentPubKey>,
) -> ExternResult<Vec<EncryptedContentResponse>> {
    let base = lineage_base(prior_dna_hash_b64, prior_action_hash_b64)?;
    let mut query = LinkQuery::try_new(base, LinkTypes::Lineage)?;
    if let Some(me) = author {
        query = query.author(me.clone());
    }
    let mut records = Vec::new();
    for link in get_links(query, GetStrategy::Network)? {
        if author.is_some_and(|me| link.author != *me) {
            continue;
        }
        let Some(target) = link.target.into_action_hash() else {
            continue;
        };
        if let Ok(record) = get_encrypted_content(target) {
            records.push(record);
        }
    }
    Ok(records)
}

/// Create content carrying a cross-generation lineage claim, verifying the
/// caller authored the cited prior record when `prior_cell` is supplied.
///
/// Find-wins: a prior caller-authored claim for the same pair returns the
/// canonical existing record with no new write. `prior_cell: None` stores
/// the claim with NO verification status — a stamped "verified" flag would
/// be forgeable through a raw create, so provenance-sensitive readers must
/// re-derive through their own prior cell. NOT cap-granted (mutator).
#[hdk_extern]
pub fn create_encrypted_content_with_lineage(
    input: CreateWithLineageInput,
) -> ExternResult<UpsertContentResponse> {
    let CreateWithLineageInput {
        mut create,
        lineage,
        prior_cell,
    } = input;

    let me = agent_info()?.agent_initial_pubkey;
    let existing = resolve_lineage_records(
        &lineage.prior_dna_hash_b64,
        &lineage.prior_action_hash_b64,
        Some(&me),
    )?;
    if let Some(found) = canonical_lowest_hash(existing) {
        return Ok(UpsertContentResponse {
            response: found,
            was_created: false,
            was_updated: false,
        });
    }

    if let Some(cell) = prior_cell {
        probe_prior_authorship(cell, &lineage.prior_action_hash_b64, &me)?;
    }

    create.lineage = Some(lineage);
    let response = create_encrypted_content(create)?;
    Ok(UpsertContentResponse {
        response,
        was_created: true,
        was_updated: false,
    })
}

/// Bridge-call the prior cell's `get_encrypted_content` and require the
/// cited record's latest header names the caller. Same-conductor,
/// same-agent call needs no cap secret. A missing record, an unreachable
/// cell, and a foreign author are three distinct hard errors — never a
/// silent downgrade to an unprobed write.
fn probe_prior_authorship(
    prior_cell: CellId,
    prior_action_hash_b64: &str,
    me: &AgentPubKey,
) -> ExternResult<()> {
    let prior_action = ActionHash::try_from(prior_action_hash_b64)
        .map_err(|_| guest("lineage prior action hash is not a valid action hash"))?;
    match call(
        CallTargetCell::OtherCell(prior_cell),
        "content",
        "get_encrypted_content".into(),
        None,
        prior_action,
    ) {
        Ok(ZomeCallResponse::Ok(io)) => {
            let record: EncryptedContentResponse = io
                .decode()
                .map_err(|e| wasm_error!(WasmErrorInner::Serialize(e)))?;
            if record
                .encrypted_content
                .header
                .revision_author_signing_public_key
                != me.to_string()
            {
                return Err(guest("lineage prior record was not authored by the caller"));
            }
            Ok(())
        }
        Ok(_) => Err(guest(
            "lineage prior cell is not reachable on this conductor",
        )),
        Err(e) if format!("{e:?}").contains("Could not find the EncryptedContent") => Err(guest(
            "lineage prior record did not resolve in the prior-generation cell",
        )),
        Err(_) => Err(guest(
            "lineage prior cell is not reachable on this conductor",
        )),
    }
}

/// Resolve the prior-generation pair forward into every content in THIS
/// generation claiming descent from it. Public DHT-link reader
/// (cap-granted); tolerant — unresolvable targets are dropped.
#[hdk_extern]
pub fn resolve_by_prior_generation(
    input: ResolveByPriorInput,
) -> ExternResult<Vec<EncryptedContentResponse>> {
    resolve_lineage_records(
        &input.prior_dna_hash_b64,
        &input.prior_action_hash_b64,
        None,
    )
}
