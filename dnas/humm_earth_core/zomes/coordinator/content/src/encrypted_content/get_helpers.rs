//! Generic DHT get helpers, factored out of the original
//! `encrypted_content.rs` (copied originally from
//! https://github.com/ddd-mtl/zome-utils/blob/main/src/get.rs while
//! waiting for the zome-utils to be updated for latest 0.3.0-beta).
//!
//! These are not zome externs; they are local helpers used by `crud.rs`
//! and `queries.rs` to resolve action hashes → entries / entry hashes.

use content_integrity::EncryptedContent;
use hdk::prelude::*;

/// Resolve an `ActionHash` to its `EntryHash` via a DHT get on the
/// associated `Record`. Errors if the record is not found OR if the
/// record's action has no entry hash (e.g. for non-entry actions).
pub fn get_eh(ah: ActionHash, options: GetOptions) -> ExternResult<EntryHash> {
    let record = get_record(AnyDhtHash::from(ah), options)?;
    let Some(eh) = record.action().entry_hash() else {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "ah_to_eh(): Record not found".to_string(),
        )));
    };
    Ok(eh.to_owned())
}

/// DHT-get with caller-selected options and a uniform error message.
pub fn get_record(dh: AnyDhtHash, options: GetOptions) -> ExternResult<Record> {
    let maybe_record = get(dh, options)?;
    let Some(record) = maybe_record else {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "no Record found at given hash".to_string(),
        )));
    };
    Ok(record)
}

pub type TypedEntryAndHash<T> = (T, ActionHash, EntryHash, Timestamp);
pub type OptionTypedEntryAndHash<T> = Option<TypedEntryAndHash<T>>;

/// Resolve an `EntryHash` to the latest live update of its typed entry.
///
/// Walks the entry's update chain, picks the latest (highest-timestamp)
/// update action, fetches that record, and returns the typed entry along
/// with the ORIGINAL action hash (so callers can reference the original
/// entry rather than the updated one) plus the SELECTED action's
/// timestamp (create's for a never-updated entry, the latest update's
/// otherwise — the fetched record IS the selected action). Returns
/// `Ok(None)` if the entry is dead (deleted) or not present locally.
pub fn get_latest_typed_from_eh<T: TryFrom<SerializedBytes, Error = SerializedBytesError>>(
    entry_hash: EntryHash,
    options: GetOptions,
) -> ExternResult<OptionTypedEntryAndHash<T>> {
    // First, make sure we DO have the latest action_hash address
    let maybe_maybe_details = get_details(entry_hash.clone(), options.clone())?;
    let Some(Details::Entry(details)) = maybe_maybe_details else {
        return Ok(None);
    };
    if details.entry_dht_status != EntryDhtStatus::Live {
        return Ok(None);
    }
    // No update: the create's entry is already in `details`; rebuild the
    // record locally instead of re-fetching the same address. An update
    // points at a distinct, updated entry, so fetch that one.
    let record = match details.updates.len() {
        0 => {
            let Some(create) = details.actions.first() else {
                return Ok(None);
            };
            Record::new(create.to_owned(), Some(details.entry.clone()))
        }
        _ => {
            let mut sortlist = details.updates.to_vec();
            sortlist.sort_by_key(|update| update.action().timestamp().as_micros());
            let Some(last) = sortlist.last() else {
                return Ok(None);
            };
            let Some(record) = get(sah_to_ah(last.to_owned()), options)? else {
                return Ok(None);
            };
            record
        }
    };
    let typed_entry = match record.entry().to_app_option::<T>() {
        Ok(Some(entry)) => entry,
        Ok(None) => return Ok(None),
        Err(e) => return Err(wasm_error!(WasmErrorInner::Serialize(e))),
    };
    let ah = match record.action() {
        // Return the ORIGINAL action for an update so callers reference the
        // original entry, not the updated one.
        Action::Update(update) => update.original_action_address.clone(),
        Action::Create(_) => record.action_address().clone(),
        // An entry only rides a Create or Update; anything else cannot resolve.
        _ => return Ok(None),
    };
    let Some(eh) = record.action().entry_hash() else {
        return Ok(None);
    };
    let eh = eh.to_owned();
    let ts = record.action().timestamp();
    Ok(Some((typed_entry, ah, eh, ts)))
}

/// Extract the action hash from a `SignedActionHashed`.
pub fn sah_to_ah(sah: SignedActionHashed) -> ActionHash {
    sah.as_hash().to_owned()
}

/// Tolerant `EncryptedContent` shape probe on a resolved record: `None`
/// for wrong-shape or absent entries instead of an error. Shape-safe:
/// no other entry type carries the nested `header` + `bytes` fields.
pub(crate) fn decode_encrypted_content(record: &Record) -> Option<EncryptedContent> {
    record
        .entry()
        .to_app_option::<EncryptedContent>()
        .ok()
        .flatten()
}
