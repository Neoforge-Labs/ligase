//! M10: the blob tier, and the one rule a shared store cannot do without.
//!
//! The content-addressed store becomes a trait here (D39's store trait, the
//! one M14 implements over object storage). Only the filesystem backend
//! exists yet, so these tests are about the *contract* rather than about any
//! one backend: an object's name is a claim about its contents, and the
//! trait checks that claim on every read so that no backend author can
//! forget to.
//!
//! That check is not ceremony. A store a lab shares is a store other people
//! can write to, and a wrong answer served quickly from a cache is worse
//! than no cache at all.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use ligase::hash::ContentHash;
use ligase::store::{Backend, BlobStore, FsStore, ObjectKey, StoreError};
use tempfile::TempDir;

fn store() -> (TempDir, FsStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = FsStore::open(&dir.path().join("store")).unwrap();
    (dir, store)
}

fn key_for(bytes: &[u8], suffix: &str) -> ObjectKey {
    ObjectKey::new(ContentHash::of_bytes(bytes), suffix)
}

// ------------------------------------------------------------ round trip --

#[test]
fn put_then_get_returns_the_same_bytes() {
    let (_dir, store) = store();
    let bytes = b">K03455.1\nACGT\n";
    let key = key_for(bytes, ".fasta");

    assert!(!store.has(&key).unwrap());
    store.put(&key, bytes).unwrap();

    assert!(store.has(&key).unwrap());
    assert_eq!(store.get(&key).unwrap().as_deref(), Some(&bytes[..]));
}

#[test]
fn get_of_an_absent_key_is_none_not_an_error() {
    let (_dir, store) = store();
    // An object nobody has stored is a miss, and a miss is an ordinary
    // answer: it is what sends a step off to compute, or M14's second tier
    // off to look upstream.
    assert_eq!(store.get(&key_for(b"never stored", ".txt")).unwrap(), None);
}

#[test]
fn an_object_already_there_is_kept_as_it_is() {
    let (_dir, store) = store();
    let bytes = b"identical bytes are identical bytes";
    let key = key_for(bytes, ".txt");

    store.put(&key, bytes).unwrap();
    let first = fs::metadata(store.local_path(&key).unwrap().unwrap())
        .unwrap()
        .modified()
        .unwrap();

    // Objects are written once and never overwritten (D39). Storing the
    // same object again must not rewrite the file, because every cache key
    // derived from it would otherwise rest on a file that moved under them.
    store.put(&key, bytes).unwrap();
    let second = fs::metadata(store.local_path(&key).unwrap().unwrap())
        .unwrap()
        .modified()
        .unwrap();

    assert_eq!(first, second);
}

#[test]
fn a_stored_object_is_read_only() {
    let (_dir, store) = store();
    let key = key_for(b"evidence", ".txt");
    store.put(&key, b"evidence").unwrap();

    let path = store.local_path(&key).unwrap().unwrap();
    assert!(
        fs::metadata(&path).unwrap().permissions().readonly(),
        "the store is the cache's evidence; a step that wrote back through \
         one would invalidate every key derived from it"
    );
}

// --------------------------------------------------------- the one rule --

#[test]
fn an_object_corrupted_in_place_is_rejected_naming_the_hash_mismatch() {
    let (_dir, store) = store();
    let bytes = b"the answer the key promises";
    let key = key_for(bytes, ".txt");
    store.put(&key, bytes).unwrap();

    // Someone else's store, or a bit flip, or a bucket a colleague can
    // write to. Whatever did it, the object no longer is what its name says.
    let path = store.local_path(&key).unwrap().unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&path, b"a different answer entirely").unwrap();

    let error = store.checked_get(&key).unwrap_err();
    let message = error.to_string();
    assert!(
        matches!(error, StoreError::Mismatch { .. }),
        "expected a mismatch, got {message}"
    );
    assert!(
        message.contains(&key.hash.to_hex()),
        "the error must name the hash the object claimed: {message}"
    );
    assert!(
        message.contains(&ContentHash::of_bytes(b"a different answer entirely").to_hex()),
        "and the hash it actually has, so the two can be compared: {message}"
    );
}

#[test]
fn a_checked_get_of_an_untouched_object_returns_it() {
    let (_dir, store) = store();
    let bytes = b"unmolested";
    let key = key_for(bytes, ".txt");
    store.put(&key, bytes).unwrap();

    assert_eq!(
        store.checked_get(&key).unwrap().as_deref(),
        Some(&bytes[..])
    );
}

#[test]
fn a_checked_get_of_an_absent_object_is_a_miss_not_a_mismatch() {
    let (_dir, store) = store();
    // Nothing to verify is not a failed verification. Collapsing the two
    // would turn M14's cold cache into a hard error.
    assert_eq!(
        store.checked_get(&key_for(b"absent", ".txt")).unwrap(),
        None
    );
}

// ------------------------------------------------------------- suffixes --

#[test]
fn local_path_names_the_file_the_suffix_asked_for() {
    let (_dir, store) = store();
    let key = key_for(b"##fileformat=VCFv4.3\n", ".vcf.gz");
    store.put(&key, b"##fileformat=VCFv4.3\n").unwrap();

    let path = store.local_path(&key).unwrap().unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.ends_with(".vcf.gz"),
        "the suffix is functional, not cosmetic: bcftools dispatches on it, \
         so the key carries it. Got {name}"
    );
}

#[test]
fn one_hash_under_two_suffixes_is_two_objects() {
    let (_dir, store) = store();
    let bytes = b"same bytes, two names";
    let fasta = key_for(bytes, ".fasta");
    let text = key_for(bytes, ".txt");

    store.put(&fasta, bytes).unwrap();

    // The hash is the same, so this is not about content. It is about a tool
    // being handed a path it can read: a key is a hash *and* a name.
    assert!(store.has(&fasta).unwrap());
    assert!(!store.has(&text).unwrap());
}

#[test]
fn an_empty_suffix_is_a_legal_key() {
    let (_dir, store) = store();
    // A recorded response body has no extension to speak of, and must not
    // need an invented one.
    let key = key_for(b"{}", "");
    store.put(&key, b"{}").unwrap();
    assert_eq!(store.get(&key).unwrap().as_deref(), Some(&b"{}"[..]));
}

// ------------------------------------------------------- staged objects --

#[test]
fn a_staged_file_is_moved_in_under_the_hash_of_its_contents() {
    let (dir, store) = store();
    let staged = dir.path().join("step-output.vcf.gz");
    fs::write(&staged, b"what the step produced").unwrap();

    let key = store.put_file(&staged, ".vcf.gz").unwrap();

    assert_eq!(key.hash, ContentHash::of_bytes(b"what the step produced"));
    assert!(!staged.exists(), "the staged file is consumed");
    assert_eq!(
        store.get(&key).unwrap().as_deref(),
        Some(&b"what the step produced"[..])
    );
}

#[test]
fn a_staged_file_that_never_finished_acquires_no_name() {
    let (dir, store) = store();
    let staged = dir.path().join("half-written.vcf.gz");
    fs::write(&staged, b"the first half").unwrap();

    // What the caller *meant* to store, had the fetch or the step completed.
    let whole = key_for(b"the first half and the second", ".vcf.gz");

    let error = store.put_file_as(&staged, &whole).unwrap_err();
    assert!(
        matches!(error, StoreError::Mismatch { .. }),
        "a half file must never acquire a name that claims to be a whole \
         one: {error}"
    );
    assert!(
        !store.has(&whole).unwrap(),
        "and nothing may be left behind under that name"
    );
}

#[test]
fn put_file_as_accepts_a_staged_file_that_is_what_it_claims() {
    let (dir, store) = store();
    let bytes = b"exactly what was promised";
    let staged = dir.path().join("fetched.fasta");
    fs::write(&staged, bytes).unwrap();

    let key = key_for(bytes, ".fasta");
    store.put_file_as(&staged, &key).unwrap();

    assert_eq!(store.get(&key).unwrap().as_deref(), Some(&bytes[..]));
}

// ------------------------------------------------------------- store URLs --
//
// M14. A store is named by a URL before anything connects, so a value that
// cannot work is an error with a list in it rather than a failure at the
// first cache miss -- an hour into a run, on a machine the author is not
// sitting at.

#[test]
fn a_file_url_is_a_local_directory() {
    assert_eq!(
        Backend::parse("file:///srv/ligase/store").unwrap(),
        Backend::Local(PathBuf::from("/srv/ligase/store"))
    );
}

#[test]
fn an_unknown_scheme_names_the_ones_that_work() {
    let error = Backend::parse("ftp://host/bucket").unwrap_err().to_string();
    assert!(error.contains("ftp"), "{error}");
    for scheme in ligase::store::SCHEMES {
        assert!(error.contains(scheme), "{error} does not name {scheme}");
    }
}

#[test]
fn a_bare_path_is_not_a_store_url() {
    // A typo must not quietly become a directory in the working tree: the
    // store it names would be empty, every lookup would miss, and the run
    // would merely be slow.
    let error = Backend::parse("/srv/ligase/store").unwrap_err().to_string();
    assert!(error.contains("scheme"), "{error}");
}

#[test]
fn a_bucket_url_parses_in_every_build() {
    // Parsing is not opening. A default build must be able to say "this
    // build has no remote-store support" rather than "unknown scheme",
    // because the second sends a user to fix a URL that is correct.
    for url in ["s3://lab-cache", "gs://lab-cache/prefix", "az://container"] {
        assert!(
            matches!(Backend::parse(url), Ok(Backend::Remote { .. })),
            "{url} did not parse"
        );
    }
}

#[test]
fn an_empty_store_url_is_refused() {
    // `LIGASE_STORE=` is how a shell says "unset" by accident.
    assert!(Backend::parse("").is_err());
    assert!(Backend::parse("s3://").is_err());
}

// --------------------------------------------------------- a benign race --

#[test]
fn two_writers_of_one_object_do_not_trip_over_each_other() {
    // Regression. Two fan-out nodes recording the same response body, or
    // two machines pushing the same output, write the same key at the same
    // moment -- which content addressing is supposed to make harmless,
    // since the bytes under a key are fixed by the key.
    //
    // They shared a staging path, so the loser found its own partial file
    // already renamed into place by the winner and failed with `NotFound`:
    // an error on a put that had succeeded.
    let (dir, _) = store();
    let store = std::sync::Arc::new(FsStore::open(&dir.path().join("shared")).unwrap());
    let key = key_for(b"written twice at once", ".txt");

    let threads: Vec<_> = (0..8)
        .map(|_| {
            let store = std::sync::Arc::clone(&store);
            let key = key.clone();
            std::thread::spawn(move || store.put(&key, b"written twice at once"))
        })
        .collect();
    for thread in threads {
        let result = thread.join().unwrap();
        assert!(result.is_ok(), "{result:?}");
    }

    assert_eq!(
        store.checked_get(&key).unwrap().as_deref(),
        Some(&b"written twice at once"[..])
    );
}
