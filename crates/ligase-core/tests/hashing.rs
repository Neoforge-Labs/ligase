//! M3: content hashing and the content-addressed store.
//!
//! Timestamps lie — a copy, a clone, a `touch` all move them without
//! changing a byte — so nothing here keys on one (D2). What a timestamp is
//! allowed to do is say that a file need not be read again, which is the
//! whole of the prefilter (D3): it can only ever save work, never decide an
//! answer.
//!
//! The store is content-addressed, so two steps that produced identical
//! bytes hold one object between them, and an object's name is a claim about
//! its contents that can be checked.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use ligase::cache::Cache;
use ligase::hash::ContentHash;
use tempfile::TempDir;

fn write(dir: &Path, name: &str, contents: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

/// Move a file's mtime without touching a byte of it.
fn touch(path: &Path) {
    let when = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(when))
        .unwrap();
}

// ------------------------------------------------------------- hashing --

#[test]
fn identical_bytes_hash_identically() {
    assert_eq!(
        ContentHash::of_bytes(b"ACGT"),
        ContentHash::of_bytes(b"ACGT")
    );
    assert_ne!(
        ContentHash::of_bytes(b"ACGT"),
        ContentHash::of_bytes(b"ACGA")
    );
}

#[test]
fn a_hash_survives_a_round_trip_through_hex() {
    let hash = ContentHash::of_bytes(b"ACGT");
    let hex = hash.to_hex();

    assert_eq!(hex.len(), 64, "BLAKE3 is 32 bytes: {hex}");
    assert_eq!(ContentHash::parse(&hex), Some(hash));
    assert_eq!(ContentHash::parse("not a hash"), None);
}

#[test]
fn a_file_hashes_as_its_contents_do() {
    let dir = TempDir::new().unwrap();
    let path = write(dir.path(), "reads.txt", "ACGT");

    assert_eq!(
        ContentHash::of_file(&path).unwrap(),
        ContentHash::of_bytes(b"ACGT")
    );
}

#[test]
fn hashing_a_missing_file_is_an_error_rather_than_a_hash_of_nothing() {
    assert!(ContentHash::of_file(Path::new("/nonexistent/reads.txt")).is_err());
}

// ---------------------------------------------------------- prefilter --

#[test]
fn an_unchanged_file_is_not_read_twice() {
    let dir = TempDir::new().unwrap();
    let cache = Cache::open(&dir.path().join(".ligase")).unwrap();
    let path = write(dir.path(), "reads.txt", "ACGT");

    let first = cache.content_hash(&path).unwrap();
    let second = cache.content_hash(&path).unwrap();

    assert_eq!(first, second);
    assert_eq!(cache.stats().hashed, 1, "the second call reread the file");
    assert_eq!(cache.stats().prefiltered, 1);
}

#[test]
fn a_changed_file_is_rehashed() {
    let dir = TempDir::new().unwrap();
    let cache = Cache::open(&dir.path().join(".ligase")).unwrap();
    let path = write(dir.path(), "reads.txt", "ACGT");

    let before = cache.content_hash(&path).unwrap();
    write(dir.path(), "reads.txt", "ACGTACGT");
    let after = cache.content_hash(&path).unwrap();

    assert_ne!(before, after);
    assert_eq!(cache.stats().hashed, 2);
}

#[test]
fn a_touched_file_hashes_the_same_as_before() {
    // The prefilter misses — the mtime moved — so the file is read again.
    // What must not happen is the *hash* moving, which is the whole of D2:
    // a touched file has not changed, whatever its timestamp says.
    let dir = TempDir::new().unwrap();
    let cache = Cache::open(&dir.path().join(".ligase")).unwrap();
    let path = write(dir.path(), "reads.txt", "ACGT");

    let before = cache.content_hash(&path).unwrap();
    touch(&path);
    let after = cache.content_hash(&path).unwrap();

    assert_eq!(before, after);
    assert_eq!(
        cache.stats().hashed,
        2,
        "a moved mtime must miss the filter"
    );
}

#[test]
fn the_prefilter_survives_reopening_the_cache() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join(".ligase");
    let path = write(dir.path(), "reads.txt", "ACGT");

    let first = Cache::open(&root).unwrap();
    let before = first.content_hash(&path).unwrap();
    drop(first);

    let second = Cache::open(&root).unwrap();
    let after = second.content_hash(&path).unwrap();

    assert_eq!(before, after);
    assert_eq!(
        second.stats().hashed,
        0,
        "a cache that forgets on reopen rehashes every input of every run"
    );
}

// -------------------------------------------------------------- store --

#[test]
fn an_object_is_stored_under_the_hash_of_its_contents() {
    let dir = TempDir::new().unwrap();
    let cache = Cache::open(&dir.path().join(".ligase")).unwrap();
    let staged = write(dir.path(), "out.bam", "ACGT");

    let stored = cache.store(&staged).unwrap();

    assert_eq!(stored.hash, ContentHash::of_bytes(b"ACGT"));
    assert!(stored.path.is_file());
    // An object's name is a claim about its contents that can be checked by
    // hand. It is fanned out over the first byte of the hash, so that a
    // cohort's worth of objects is not one directory of a million files.
    let hex = stored.hash.to_hex();
    assert_eq!(
        stored.path.parent().and_then(Path::file_name).unwrap(),
        &hex[..2]
    );
    assert_eq!(
        stored.path.file_name().unwrap().to_string_lossy(),
        format!("{}.bam", &hex[2..])
    );
    assert_eq!(fs::read_to_string(&stored.path).unwrap(), "ACGT");
    assert!(!staged.exists(), "the staged file should have been moved");
}

#[test]
fn an_object_keeps_the_extension_tools_infer_formats_from() {
    let dir = TempDir::new().unwrap();
    let cache = Cache::open(&dir.path().join(".ligase")).unwrap();

    let bam = cache.store(&write(dir.path(), "out.bam", "ACGT")).unwrap();
    let vcf = cache
        .store(&write(dir.path(), "out.vcf.gz", "ACGTA"))
        .unwrap();

    assert_eq!(bam.path.extension().unwrap(), "bam");
    assert!(
        vcf.path.to_string_lossy().ends_with(".vcf.gz"),
        "samtools and bcftools read the format off the name: {}",
        vcf.path.display()
    );
}

#[test]
fn identical_outputs_share_one_object() {
    let dir = TempDir::new().unwrap();
    let cache = Cache::open(&dir.path().join(".ligase")).unwrap();

    let first = cache.store(&write(dir.path(), "a.bam", "ACGT")).unwrap();
    let second = cache.store(&write(dir.path(), "b.bam", "ACGT")).unwrap();

    assert_eq!(first.path, second.path);
    assert_eq!(first.hash, second.hash);
}

#[test]
fn a_stored_object_is_read_only() {
    // The store is the cache's evidence. A step that writes back through its
    // input path would invalidate every key derived from it, silently.
    let dir = TempDir::new().unwrap();
    let cache = Cache::open(&dir.path().join(".ligase")).unwrap();

    let stored = cache.store(&write(dir.path(), "out.bam", "ACGT")).unwrap();

    let mode = fs::metadata(&stored.path).unwrap().permissions();
    assert!(mode.readonly(), "a stored object should not be writable");
}
