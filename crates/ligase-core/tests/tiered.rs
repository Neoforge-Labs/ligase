//! M14: a local tier that consults a remote, and what it does when the
//! remote misbehaves.
//!
//! The remote here is a second `FsStore`, which exercises exactly the
//! tiering code an object store will run under and needs no container. The
//! `object_store` backend is tested against MinIO in CI; everything about
//! *tiering* is tested here, in every build.
//!
//! The interesting tests are not the hits. They are the three ways a second
//! tier is slowly wrong: an object that is not what its name says, a remote
//! that cannot be reached, and two machines writing the same object at once.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use ligase::hash::ContentHash;
use ligase::store::{BlobStore, FsStore, ObjectKey, StoreError, TieredStore};
use tempfile::TempDir;

/// A local tier and a remote tier, both directories.
fn tiers() -> (TempDir, TieredStore, FsStore) {
    let dir = tempfile::tempdir().unwrap();
    let remote_root = dir.path().join("remote");
    let remote = FsStore::open(&remote_root).unwrap();
    let store = TieredStore::open(
        &dir.path().join("local"),
        Some(&format!("file://{}", remote_root.display())),
    )
    .unwrap();
    (dir, store, remote)
}

fn key_for(bytes: &[u8], suffix: &str) -> ObjectKey {
    ObjectKey::new(ContentHash::of_bytes(bytes), suffix)
}

/// A backend that fails every call, the way a bucket behind a dropped VPN
/// does. Not a miss -- an error, which is the case the engine has to decide
/// about.
#[derive(Debug)]
struct Unreachable;

impl Unreachable {
    fn fail<T>() -> Result<T, StoreError> {
        Err(StoreError::Io {
            path: PathBuf::from("s3://unreachable"),
            source: io::Error::new(io::ErrorKind::TimedOut, "no route to host"),
        })
    }
}

impl BlobStore for Unreachable {
    fn has(&self, _: &ObjectKey) -> Result<bool, StoreError> {
        Self::fail()
    }
    fn get(&self, _: &ObjectKey) -> Result<Option<Vec<u8>>, StoreError> {
        Self::fail()
    }
    fn put(&self, _: &ObjectKey, _: &[u8]) -> Result<(), StoreError> {
        Self::fail()
    }
    fn put_file(&self, _: &std::path::Path, _: &str) -> Result<ObjectKey, StoreError> {
        Self::fail()
    }
    fn put_file_as(&self, _: &std::path::Path, _: &ObjectKey) -> Result<(), StoreError> {
        Self::fail()
    }
    fn local_path(&self, _: &ObjectKey) -> Result<Option<PathBuf>, StoreError> {
        Self::fail()
    }
}

fn unreachable_remote() -> (TempDir, TieredStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = TieredStore::with_remote(
        &dir.path().join("local"),
        Box::new(Unreachable),
        "s3://unreachable",
    )
    .unwrap();
    (dir, store)
}

// ----------------------------------------------------------------- tiers --

#[test]
fn a_store_with_no_remote_behaves_as_the_local_one_did() {
    let dir = tempfile::tempdir().unwrap();
    let store = TieredStore::open(&dir.path().join("local"), None).unwrap();
    let key = key_for(b"alone", ".txt");

    store.put(&key, b"alone").unwrap();

    assert!(store.has(&key).unwrap());
    assert_eq!(
        store.checked_get(&key).unwrap().as_deref(),
        Some(&b"alone"[..])
    );
    assert_eq!(store.remote_url(), None);
    assert_eq!(store.stats(), Default::default());
}

#[test]
fn a_local_hit_never_touches_the_remote() {
    let (_dir, store, remote) = tiers();
    let key = key_for(b"here", ".txt");
    store.put(&key, b"here").unwrap();

    // Remove it from the remote, so a read that consulted the remote would
    // notice. The local tier is authoritative on a hit.
    std::fs::remove_file(remote.path_of(&key)).unwrap();

    assert_eq!(store.get(&key).unwrap().as_deref(), Some(&b"here"[..]));
    assert_eq!(store.stats().fetched, 0);
}

#[test]
fn a_local_miss_is_served_from_the_remote_and_promoted() {
    let (_dir, store, remote) = tiers();
    let key = key_for(b"over there", ".txt");
    remote.put(&key, b"over there").unwrap();

    assert_eq!(
        store.get(&key).unwrap().as_deref(),
        Some(&b"over there"[..])
    );

    assert_eq!(store.stats().fetched, 1, "it was not counted");
    assert!(
        store.local().has(&key).unwrap(),
        "a promoted object stays promoted, or every step pays for it again"
    );
}

#[test]
fn a_put_is_written_through_to_the_remote() {
    let (_dir, store, remote) = tiers();
    let key = key_for(b"outward", ".txt");

    store.put(&key, b"outward").unwrap();

    assert_eq!(remote.get(&key).unwrap().as_deref(), Some(&b"outward"[..]));
    assert_eq!(store.stats().pushed, 1);
}

#[test]
fn a_staged_file_is_written_through_too() {
    let (dir, store, remote) = tiers();
    let staged = dir.path().join("staged.vcf.gz");
    std::fs::write(&staged, b"finished output").unwrap();

    let key = store.put_file(&staged, ".vcf.gz").unwrap();

    assert_eq!(key.suffix, ".vcf.gz");
    assert_eq!(
        remote.get(&key).unwrap().as_deref(),
        Some(&b"finished output"[..])
    );
}

#[test]
fn local_path_promotes_so_a_tool_can_be_handed_a_path() {
    let (_dir, store, remote) = tiers();
    let key = key_for(b"needs a path", ".bam");
    remote.put(&key, b"needs a path").unwrap();

    let path = store.local_path(&key).unwrap().unwrap();

    assert!(path.is_file());
    assert!(path.to_string_lossy().ends_with(".bam"));
    assert_eq!(std::fs::read(&path).unwrap(), b"needs a path");
}

// ------------------------------------------------------------ corruption --

/// Corrupt an object in place, as a bucket someone else can write to is
/// corrupted: the name stays, the bytes change.
fn corrupt(remote: &FsStore, key: &ObjectKey, with: &[u8]) {
    let path = remote.path_of(key);
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&path, permissions).unwrap();
    std::fs::write(&path, with).unwrap();
}

#[test]
fn a_corrupt_remote_object_is_refused_and_names_both_hashes() {
    let (_dir, store, remote) = tiers();
    let key = key_for(b"the real thing", ".txt");
    remote.put(&key, b"the real thing").unwrap();
    corrupt(&remote, &key, b"something else entirely");

    let error = store.get(&key).unwrap_err();

    assert!(matches!(error, StoreError::Mismatch { .. }));
    let message = error.to_string();
    assert!(message.contains(&key.hash.to_hex()), "{message}");
    assert!(
        message.contains(&ContentHash::of_bytes(b"something else entirely").to_hex()),
        "{message}"
    );
    assert_eq!(store.stats().rejected, 1);
}

#[test]
fn a_corrupt_remote_object_does_not_poison_the_local_tier() {
    let (_dir, store, remote) = tiers();
    let key = key_for(b"the real thing", ".txt");
    remote.put(&key, b"the real thing").unwrap();
    corrupt(&remote, &key, b"something else entirely");

    let _ = store.get(&key);

    assert!(
        !store.local().has(&key).unwrap(),
        "the local tier is trusted, so promotion has to verify BEFORE it \
         installs -- otherwise the mismatch is caught once and never again"
    );
}

#[test]
fn local_path_verifies_before_it_installs_too() {
    let (_dir, store, remote) = tiers();
    let key = key_for(b"the real thing", ".bam");
    remote.put(&key, b"the real thing").unwrap();
    corrupt(&remote, &key, b"wrong");

    assert!(matches!(
        store.local_path(&key).unwrap_err(),
        StoreError::Mismatch { .. }
    ));
    assert!(!store.local().has(&key).unwrap());
}

// ---------------------------------------------------------- unreachable --

#[test]
fn an_unreachable_remote_is_a_miss_rather_than_a_failure() {
    let (_dir, store) = unreachable_remote();
    let key = key_for(b"never stored", ".txt");

    // A remote is a substituter, not a source of truth: falling back to
    // computing is always correct, and failing the run would make the
    // engine worse than having no remote at all.
    assert!(!store.has(&key).unwrap());
    assert_eq!(store.get(&key).unwrap(), None);
    assert_eq!(store.local_path(&key).unwrap(), None);
    assert!(
        store.stats().unreachable >= 3,
        "a fallback nobody is told about is how an hour disappears"
    );
}

#[test]
fn an_unreachable_remote_does_not_fail_a_put() {
    let (_dir, store) = unreachable_remote();
    let key = key_for(b"computed anyway", ".txt");

    store.put(&key, b"computed anyway").unwrap();

    assert_eq!(
        store.get(&key).unwrap().as_deref(),
        Some(&b"computed anyway"[..]),
        "the object is local, so the run is right whatever the network did"
    );
    assert!(store.stats().unreachable > 0);
    assert_eq!(store.stats().pushed, 0);
}

#[test]
fn a_miss_on_an_unreachable_remote_is_told_apart_from_a_miss_on_a_reachable_one() {
    let (_dir, store) = unreachable_remote();
    assert_eq!(store.get(&key_for(b"a", ".txt")).unwrap(), None);
    assert!(store.stats().unreachable > 0);

    let (_dir, warm, _remote) = tiers();
    assert_eq!(warm.get(&key_for(b"a", ".txt")).unwrap(), None);
    assert_eq!(
        warm.stats().unreachable,
        0,
        "an honest miss must not be reported as a network problem"
    );
}

// ------------------------------------------------------------------ races --

#[test]
fn many_threads_putting_identical_bytes_all_succeed() {
    // Two machines pushing the same object is the normal case for a lab, and
    // content addressing is supposed to make it benign. This is the proof
    // rather than the assumption: the bytes under a key are fixed by the
    // key, so the only question is whether the writes trip over each other.
    let (_dir, store, remote) = tiers();
    let store = Arc::new(store);
    let key = key_for(b"the same answer", ".txt");

    let mut threads = Vec::new();
    for _ in 0..8 {
        let store = Arc::clone(&store);
        let key = key.clone();
        threads.push(std::thread::spawn(move || {
            store.put(&key, b"the same answer")
        }));
    }
    for thread in threads {
        thread.join().unwrap().expect("a benign race failed a put");
    }

    assert_eq!(
        store.checked_get(&key).unwrap().as_deref(),
        Some(&b"the same answer"[..])
    );
    assert_eq!(
        remote.checked_get(&key).unwrap().as_deref(),
        Some(&b"the same answer"[..])
    );
}

#[test]
fn an_object_already_in_the_remote_is_not_written_again() {
    // Write-once. Every cache key derived from an object rests on it still
    // being what it was, so a second put must not move it -- and must not
    // cost an upload either.
    let (_dir, store, remote) = tiers();
    let key = key_for(b"already there", ".txt");
    remote.put(&key, b"already there").unwrap();

    store.put(&key, b"already there").unwrap();

    assert_eq!(store.stats().pushed, 0, "it was already held");
}
