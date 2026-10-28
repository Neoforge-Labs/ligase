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
///
/// Version 2 (M18) adds the output's canonical hash to a `steps` row, so
/// that a worker whose publish was dropped can tell at no cost whether the
/// machine that got there first *means* the same thing it does (D52).
///
/// This number is in [`key_of`]'s hash, and that is not decoration — see
/// the note there. It was not, until the first bump needed it.
const ROW_VERSION: u32 = 2;

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
///
/// # Why [`ROW_VERSION`] is in here
///
/// Because objects are **write-once**, and a format version that is not in
/// the name of the thing it versions can only be discovered by the reader
/// — which in a write-once store is already too late.
///
/// Until M18 this hashed the domain, the table and the key alone, so every
/// format competed for one address. A bucket holding a version 1 row would
/// have refused every version 2 row written to that key: the `put` is
/// dropped because something is already there, the read finds a version
/// this build will not parse, [`fetch`] answers `None`, and that step key
/// becomes **permanently unpublishable** by anyone, for as long as the
/// bucket exists. Not one recomputation — all of them, for ever.
///
/// The bug was latent in M14 rather than introduced here: it is
/// unreachable until the first format bump, and M18 is the first format
/// bump. With the version in the address each format addresses its own
/// rows, old rows are never read instead of being read wrongly, and the
/// next bump costs one recomputation as the doc comment above promises.
#[must_use]
pub fn key_of(table: &str, key: &str) -> ObjectKey {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DOMAIN.as_bytes());
    hasher.update(&[0]);
    hasher.update(&ROW_VERSION.to_le_bytes());
    hasher.update(&[0]);
    hasher.update(table.as_bytes());
    hasher.update(&[0]);
    hasher.update(key.as_bytes());
    ObjectKey::new(
        ContentHash::from_bytes(*hasher.finalize().as_bytes()),
        ROW_SUFFIX,
    )
}

/// What the store holds for a row once it has been published.
///
/// Write-once means a `put` cannot say whether it wrote: an object already
/// held is kept, and the call succeeds either way. So publishing reads
/// back, and this is what it found.
///
/// The distinction exists for M18. A step key fixes its ingredients, so two
/// machines publishing one key have done the same work — but not
/// necessarily to the same bytes, and in the cases D52 names not even to
/// the same *meaning*. Which of those it is has to be decided by somebody,
/// and the store is the right somebody: it is the only layer that knows
/// what is actually there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Publication {
    /// The store holds exactly what was published — because this call
    /// wrote it, or because a row identical to it was already there.
    /// Identical bytes are not somebody disagreeing.
    Ours,
    /// A different row was already there, and it is the one in force.
    /// Its fields, which are the store's answer and therefore the
    /// authoritative one.
    Theirs(Vec<String>),
}

/// Write a row, so a machine with an empty index can learn it.
///
/// Write-once like every other object: a row already published is left
/// alone. That is right rather than merely convenient — a step key fixes
/// its ingredients, so two rows under one key would have to agree.
///
/// Says nothing about what was already there. Use [`publish`] where that
/// matters, and note what it says about *which* store to ask.
///
/// # Errors
///
/// If the row cannot be written.
pub fn put(
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

/// Write a row and say what the store holds for it afterwards.
///
/// # Ask the right store
///
/// The read-back is only worth anything against the store **every machine
/// sees** ([`super::TieredStore::shared`]). Asked of a tiered store, it
/// reads this machine's own local tier, finds this machine's own row, and
/// answers [`Publication::Ours`] every time — which is how M18's first
/// implementation managed to be blind to every divergence while looking
/// correct.
///
/// # Errors
///
/// If the row cannot be written, or cannot be read back.
pub fn publish(
    store: &dyn BlobStore,
    table: &str,
    key: &str,
    fields: &[String],
) -> Result<Publication, StoreError> {
    put(store, table, key, fields)?;

    // Read back rather than trusting the write. `put` cannot say whether it
    // wrote — write-once means an object already there is kept and the call
    // succeeds either way — and every store this runs against is
    // read-after-write consistent for a new object: S3 since 2020, GCS,
    // Azure, and a filesystem trivially. That is what makes this a check
    // the store can honour, unlike the compare-and-swap it would want.
    match fetch(store, table, key)? {
        Some(found) if found == fields => Ok(Publication::Ours),
        Some(found) => Ok(Publication::Theirs(found)),
        // Nothing there, having just written it. A row this build cannot
        // read occupies the address -- which, with the version in the
        // address, can only be someone else's doing. Treat it as theirs
        // and unreadable: recomputing is the documented cost.
        None => Ok(Publication::Theirs(Vec::new())),
    }
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
