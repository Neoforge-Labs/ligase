//! The rows a cache hit needs, published into the store.
//!
//! # Why this exists, and what D43 got wrong
//!
//! D43 says the SQLite index "is rebuildable from the store, which it can
//! be because everything in it derives from content hashes". Held against
//! `cache.rs`, that is **false**. A step's key is a hash of its
//! ingredients — code, parameters, input hashes, tool versions — and no
//! object in the store carries any of them, so a bag of objects cannot say
//! which key produced which. `remotes` is keyed by a locator, which is not
//! a hash of anything either.
//!
//! The consequence is the whole of this module. A clean machine pointed at
//! a shared bucket computes exactly the right step key, finds the output
//! sitting in the bucket under exactly the right name, and still has no
//! way to know they belong together. So the rows are published too: each
//! one an object, named for the key it answers to.
//!
//! # These are claims, and that is the one thing to be clear about
//!
//! An object's name is the hash of its contents, so an object's name is a
//! claim that can be checked, and [`BlobStore::checked_get`] checks it. A
//! row is different. It says "key K produced object O", and nothing short
//! of running the step can check that. A row is therefore **the one object
//! class whose name is not the hash of its contents**, and it gets the
//! verification it can have rather than the one it cannot:
//!
//! - the body restates the table and the key, and a body that restates the
//!   wrong ones is refused, so a row cannot be served in answer to a
//!   question it was not written for;
//! - the object it points at is still verified byte for byte on the way in
//!   ([`super::tiered`]), so a shared store can never hand over corrupt
//!   data.
//!
//! What remains is that **a shared store is as trustworthy as write access
//! to it**. Someone who can write your bucket can claim that step K
//! produced some other intact object, and you would believe them. That is
//! the same trust boundary a Nix binary cache has — Nix answers it with
//! signatures, which v0.2 does not have — and it is written down here,
//! in the architecture page and in D43 rather than left to be discovered.
//!
//! The practical reading: point Ligase at a bucket your lab writes, not at
//! one the internet writes.

use crate::hash::ContentHash;

use super::{BlobStore, ObjectKey, ROW_SUFFIX, StoreError};

/// Separates a row's name from every other hash in the engine.
const DOMAIN: &str = "ligase.published-row";

/// The format of a row's body.
///
/// A row written by another version is *ignored* rather than read, exactly
/// as a cache entry from another key format is: it describes something
/// that was put together differently, and the cost of ignoring it is one
/// recomputation.
const ROW_VERSION: u32 = 1;

/// Step entries: which key produced which output.
pub const STEPS: &str = "steps";

/// Pinned remotes: which locator resolved to which objects.
pub const REMOTES: &str = "remotes";

/// Where a row lives in the store.
///
/// Not the hash of the row's contents — the hash of the *question* it
/// answers, because the question is all a cold machine has. The `.row`
/// suffix keeps the two classes apart in any listing, and the domain
/// string keeps a row from ever colliding with an object.
#[must_use]
pub fn key_of(table: &str, key: &str) -> ObjectKey {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DOMAIN.as_bytes());
    hasher.update(&[0]);
    hasher.update(table.as_bytes());
    hasher.update(&[0]);
    hasher.update(key.as_bytes());
    ObjectKey::new(
        ContentHash::from_bytes(*hasher.finalize().as_bytes()),
        ROW_SUFFIX,
    )
}

/// Publish a row, so a machine with an empty index can learn it.
///
/// Write-once like every other object: a row already published is left
/// alone. That is right rather than merely convenient — a step key fixes
/// its ingredients, so two rows under one key would have to agree.
///
/// # Errors
///
/// If the row cannot be written.
pub fn publish(
    store: &dyn BlobStore,
    table: &str,
    key: &str,
    fields: &[String],
) -> Result<(), StoreError> {
    // Escaped like every other field: a `remotes` key carries a tab
    // between the locator and the kind, and an unescaped one would split
    // the header and make every row look like an impostor.
    let mut body = format!("{ROW_VERSION}\t{}\t{}", escape(table), escape(key));
    for field in fields {
        body.push('\t');
        body.push_str(&escape(field));
    }
    body.push('\n');
    store.put(&key_of(table, key), body.as_bytes())
}

/// Read a row back, refusing one that answers a different question.
///
/// `None` for a row that is not there, and for one written by a version
/// this build does not read.
///
/// # Errors
///
/// [`StoreError::Impostor`] if the row restates a table or key other than
/// the one it was found under — which means the store holds a row someone
/// wrote by hand or by accident, and serving it would be worse than
/// recomputing. Or an I/O failure.
pub fn fetch(
    store: &dyn BlobStore,
    table: &str,
    key: &str,
) -> Result<Option<Vec<String>>, StoreError> {
    let object = key_of(table, key);
    let Some(bytes) = store.get_row(&object)? else {
        return Ok(None);
    };
    let body = String::from_utf8_lossy(&bytes);
    let line = body.lines().next().unwrap_or_default();

    let mut fields = line.split('\t');
    let (Some(version), Some(found_table), Some(found_key)) =
        (fields.next(), fields.next(), fields.next())
    else {
        return Err(impostor(&object, "a row with no header"));
    };
    if version != ROW_VERSION.to_string() {
        return Ok(None);
    }
    let (found_table, found_key) = (unescape(found_table), unescape(found_key));
    if found_table != table || found_key != key {
        return Err(impostor(
            &object,
            &format!("a `{found_table}` row for {found_key}"),
        ));
    }

    Ok(Some(fields.map(unescape).collect()))
}

fn impostor(at: &ObjectKey, claims: &str) -> StoreError {
    StoreError::Impostor {
        at: at.to_string(),
        claims: claims.to_owned(),
    }
}

/// Tabs, newlines and backslashes, so no field can shift a column.
fn escape(field: &str) -> String {
    field
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn unescape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut characters = field.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}
