//! Generic DHT get helpers, factored out of the original
//! `encrypted_content.rs` (copied originally from
//! https://github.com/ddd-mtl/zome-utils/blob/main/src/get.rs while
//! waiting for the zome-utils to be updated for latest 0.3.0-beta).
//!
//! These are not zome externs; they are local helpers used by `crud.rs`
//! and `queries.rs` to resolve action hashes → entries / entry hashes.

use hdk::prelude::*;

/// Resolve an `ActionHash` to its `EntryHash` via a DHT get on the
/// associated `Record`. Errors if the record is not found OR if the
/// record's action has no entry hash (e.g. for non-entry actions).
pub fn get_eh(ah: ActionHash) -> ExternResult<EntryHash> {
    let record = get_record(AnyDhtHash::from(ah))?;
    let Some(eh) = record.action().entry_hash() else {
        return Err(wasm_error!(WasmErrorInner::Guest(format!(
            "ah_to_eh(): Record not found"
        ))));
    };
    Ok(eh.to_owned())
}

/// DHT-get wrapping `Network` strategy with a uniform error message.
pub fn get_record(dh: AnyDhtHash) -> ExternResult<Record> {
    let maybe_record = get(
        dh,
        GetOptions {
            strategy: GetStrategy::Network,
        },
    )?;
    let Some(record) = maybe_record else {
        return Err(wasm_error!(WasmErrorInner::Guest(format!(
            "no Record found at given hash"
        ))));
    };
    Ok(record)
}

pub type TypedEntryAndHash<T> = (T, ActionHash, EntryHash);
pub type OptionTypedEntryAndHash<T> = Option<TypedEntryAndHash<T>>;

/// Resolve an `EntryHash` to the latest live update of its typed entry.
///
/// Walks the entry's update chain, picks the latest (highest-timestamp)
/// update action, fetches that record, and returns the typed entry along
/// with the ORIGINAL action hash (so callers can reference the original
/// entry rather than the updated one). Returns `Ok(None)` if the entry
/// is dead (deleted) or not present locally.
pub fn get_latest_typed_from_eh<T: TryFrom<SerializedBytes, Error = SerializedBytesError>>(
    entry_hash: EntryHash,
) -> ExternResult<OptionTypedEntryAndHash<T>> {
    // First, make sure we DO have the latest action_hash address
    let maybe_maybe_details = get_details(
        entry_hash.clone(),
        GetOptions {
            strategy: GetStrategy::Network,
        },
    )?;
    let Some(Details::Entry(details)) = maybe_maybe_details else {
        return Ok(None);
    };
    if details.entry_dht_status != EntryDhtStatus::Live {
        return Ok(None);
    }
    let latest_ah = match details.updates.len() {
        // pass out the action associated with this entry
        0 => sah_to_ah(details.actions.first().unwrap().to_owned()),
        _ => {
            let mut sortlist = details.updates.to_vec();
            // unix timestamp should work for sorting
            sortlist.sort_by_key(|update| update.action().timestamp().as_micros());
            // sorts in ascending order, so take the last Record
            let last = sortlist.last().unwrap().to_owned();
            sah_to_ah(last)
        }
    };
    // Second, go and get that Record, and return its entry and action_address
    let Some(record) = get(
        latest_ah,
        GetOptions {
            strategy: GetStrategy::Network,
        },
    )?
    else {
        return Ok(None);
    };
    let maybe_maybe_typed_entry = record.entry().to_app_option::<T>();
    if let Err(e) = maybe_maybe_typed_entry {
        return Err(wasm_error!(WasmErrorInner::Serialize(e)));
    }
    let Some(typed_entry) = maybe_maybe_typed_entry.unwrap() else {
        return Ok(None);
    };
    let ah = match record.action() {
        // we DO want to return the action for the original instead of the updated
        Action::Update(update) => update.original_action_address.clone(),
        Action::Create(_) => record.action_address().clone(),
        _ => unreachable!("Can't have returned a action for a nonexistent entry"),
    };
    let eh = record.action().entry_hash().unwrap().to_owned();
    Ok(Some((typed_entry, ah, eh)))
}

/// Extract the action hash from a `SignedActionHashed`.
pub fn sah_to_ah(sah: SignedActionHashed) -> ActionHash {
    sah.as_hash().to_owned()
}
