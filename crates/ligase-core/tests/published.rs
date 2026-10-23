//! M14: the rows a cache hit needs, published into the store.
//!
//! A bag of objects cannot say which step key produced which output, so a
//! clean machine pointed at a shared bucket would find every object it
//! needed and still rerun everything. The rows close that gap — and they
//! are the one object class whose name is not the hash of its contents,
//! which is why most of these tests are about what happens when a row is
//! not what it should be.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::cache::{Cache, Stored};
use ligase::key::{Ingredients, StepKey};
use ligase::store::published::{self, REMOTES, STEPS};
use ligase::store::{BlobStore, FsStore};
use ligase::types::DataKind;
use tempfile::TempDir;

fn store() -> (TempDir, FsStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = FsStore::open(&dir.path().join("store")).unwrap();
    (dir, store)
}

fn fields(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

// ------------------------------------------------------------ round trip --

#[test]
fn a_published_row_reads_back() {
    let (_dir, store) = store();
    published::publish(&store, STEPS, "abc123", &fields(&["out", ".bam", "41"])).unwrap();

    assert_eq!(
        published::fetch(&store, STEPS, "abc123").unwrap(),
        Some(fields(&["out", ".bam", "41"]))
    );
}

#[test]
fn a_row_that_was_never_published_is_a_miss() {
    let (_dir, store) = store();
    assert_eq!(published::fetch(&store, STEPS, "nothing").unwrap(), None);
}

#[test]
fn a_tab_in_a_field_cannot_shift_a_column() {
    let (_dir, store) = store();
    let awkward = fields(&["has\ta tab", "has\na newline", r"has\a backslash"]);
    published::publish(&store, STEPS, "awkward", &awkward).unwrap();

    assert_eq!(
        published::fetch(&store, STEPS, "awkward").unwrap(),
        Some(awkward)
    );
}

#[test]
fn two_tables_do_not_share_a_key_space() {
    let (_dir, store) = store();
    published::publish(&store, STEPS, "same", &fields(&["a step"])).unwrap();

    assert_eq!(published::fetch(&store, REMOTES, "same").unwrap(), None);
}

// -------------------------------------------------------------- impostors --

#[test]
fn a_row_that_restates_another_key_is_refused() {
    // The only check a row can carry. Someone who writes a row by hand
    // into a shared bucket must not be able to have it served in answer to
    // a question it was not written for.
    let (_dir, store) = store();
    published::publish(&store, STEPS, "the real key", &fields(&["out"])).unwrap();

    // Move the body to the place another key's row would live.
    let body = store
        .get(&published::key_of(STEPS, "the real key"))
        .unwrap()
        .unwrap();
    store
        .put(&published::key_of(STEPS, "another key"), &body)
        .unwrap();

    let error = published::fetch(&store, STEPS, "another key").unwrap_err();
    let message = error.to_string();
    assert!(message.contains("the real key"), "{message}");
    assert!(message.contains("refused"), "{message}");
}

#[test]
fn a_row_from_another_format_is_ignored_rather_than_misread() {
    let (_dir, store) = store();
    let object = published::key_of(STEPS, "from the future");
    store
        .put(&object, b"99\tsteps\tfrom the future\tsomething\n")
        .unwrap();

    assert_eq!(
        published::fetch(&store, STEPS, "from the future").unwrap(),
        None,
        "a row this version does not understand costs one recomputation, \
         and reading it half-way would cost a wrong answer"
    );
}

// ----------------------------------------------------- rebuilding an index --

/// A step key, and an output already in the store.
fn a_step(cache: &Cache, dir: &TempDir, body: &[u8]) -> (StepKey, Stored) {
    let key = Ingredients {
        step: "align".to_owned(),
        code: "def align(): ...".to_owned(),
        recipe: "minimap2 {ref}".to_owned(),
        output: DataKind::Alignment,
        parameters: Vec::new(),
        inputs: Vec::new(),
        tools: Vec::new(),
    }
    .key();

    let staged = dir.path().join("staged.bam");
    std::fs::write(&staged, body).unwrap();
    let stored = cache.store(&staged).unwrap();
    (key, stored)
}

#[test]
fn deleting_the_index_rebuilds_it_from_the_store() {
    // The done-when clause. Everything the index holds that a hit depends
    // on is published, so losing `cache.sqlite` costs one store read per
    // step rather than a whole pipeline.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(".ligase");

    let (key, stored) = {
        let cache = Cache::open(&root).unwrap();
        let (key, stored) = a_step(&cache, &dir, b"an alignment");
        cache
            .record(key, "align", &stored, "minimap2 2.28", 41, &[])
            .unwrap();
        (key, stored)
    };

    std::fs::remove_file(root.join("cache.sqlite")).unwrap();

    let cache = Cache::open(&root).unwrap();
    let entry = cache
        .lookup(key)
        .unwrap()
        .expect("the entry was published, so it comes back");

    assert_eq!(entry.output.hash, stored.hash);
    assert!(entry.output.path.is_file());
}

#[test]
fn an_entry_whose_output_is_only_in_the_remote_is_materialised() {
    // The clean-machine clause in miniature: nothing local at all, and a
    // remote holding both the row and the object.
    let dir = tempfile::tempdir().unwrap();
    let bucket = dir.path().join("bucket");
    let url = format!("file://{}", bucket.display());

    let (key, hash) = {
        let cache = Cache::open_with(&dir.path().join("first"), Some(&url)).unwrap();
        let (key, stored) = a_step(&cache, &dir, b"an alignment");
        cache
            .record(key, "align", &stored, "minimap2 2.28", 41, &[])
            .unwrap();
        (key, stored.hash)
    };

    let second = Cache::open_with(&dir.path().join("second"), Some(&url)).unwrap();
    let entry = second
        .lookup(key)
        .unwrap()
        .expect("a second machine on the same bucket finds the work done");

    assert_eq!(entry.output.hash, hash);
    assert!(
        entry.output.path.is_file(),
        "a step below it is handed this path, so it has to be a real file"
    );
    assert!(entry.output.path.starts_with(dir.path().join("second")));
    assert_eq!(second.objects().stats().fetched, 1);
}

#[test]
fn a_remote_row_is_published_so_a_second_machine_does_not_refetch() {
    use ligase::cache::{RemoteFile, RemoteRow};
    use ligase::hash::ContentHash;
    use ligase::store::ObjectKey;

    let dir = tempfile::tempdir().unwrap();
    let bucket = dir.path().join("bucket");
    let url = format!("file://{}", bucket.display());

    let row = RemoteRow {
        digest: ContentHash::of_bytes(b"meaning"),
        bytes: ContentHash::of_bytes(b"bytes"),
        files: vec![RemoteFile {
            key: ObjectKey::new(ContentHash::of_bytes(b"HXB2"), ".fasta"),
            url: "https://example.invalid/K03455.1".to_owned(),
            bytes: 9719,
        }],
        fetched_at: 1_760_000_000,
    };

    let first = Cache::open_with(&dir.path().join("first"), Some(&url)).unwrap();
    first
        .record_remote("ncbi:K03455.1", DataKind::Reference, &row)
        .unwrap();

    let second = Cache::open_with(&dir.path().join("second"), Some(&url)).unwrap();
    assert_eq!(
        second.remote("ncbi:K03455.1", DataKind::Reference).unwrap(),
        Some(row),
        "otherwise a clean checkout downloads the reads again and calls it \
         a cache hit"
    );
}

#[test]
fn a_corrupt_object_behind_a_good_row_reruns_the_step_rather_than_serving_it() {
    // The clause: corrupted in place, refused on read, and the step reruns.
    // `lookup` returning `None` is how it reruns; the refusal itself is
    // counted and reported, because a silent rerun is the thing to avoid.
    let dir = tempfile::tempdir().unwrap();
    let bucket = dir.path().join("bucket");
    let url = format!("file://{}", bucket.display());

    let key = {
        let cache = Cache::open_with(&dir.path().join("first"), Some(&url)).unwrap();
        let (key, stored) = a_step(&cache, &dir, b"an alignment");
        cache
            .record(key, "align", &stored, "minimap2 2.28", 41, &[])
            .unwrap();
        key
    };

    // Corrupt the object in the bucket, leaving its name alone.
    let remote = FsStore::open(&bucket).unwrap();
    let object = remote.path_of(&ligase::store::ObjectKey::new(
        ligase::hash::ContentHash::of_bytes(b"an alignment"),
        ".bam",
    ));
    let mut permissions = std::fs::metadata(&object).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&object, permissions).unwrap();
    std::fs::write(&object, b"not an alignment at all").unwrap();

    let second = Cache::open_with(&dir.path().join("second"), Some(&url)).unwrap();
    assert_eq!(second.lookup(key).unwrap(), None, "the step must rerun");

    let stats = second.objects().stats();
    assert_eq!(stats.rejected, 1);
    let refusal = second.objects().rejection().expect("it is reported");
    assert!(refusal.contains("hash"), "{refusal}");
}

#[test]
fn a_store_with_no_remote_still_publishes_rows_locally() {
    // Which is what makes the "delete the index" clause work with no
    // network at all, and what makes `ligase store push` have something to
    // push later.
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open(&dir.path().join(".ligase")).unwrap();
    let (key, _) = a_step(&cache, &dir, b"local only");
    let stored = cache.store(&{
        let staged = dir.path().join("again.bam");
        std::fs::write(&staged, b"local only too").unwrap();
        staged
    });
    cache
        .record(key, "align", &stored.unwrap(), "minimap2", 1, &[])
        .unwrap();

    assert!(
        published::fetch(cache.objects(), STEPS, &key.to_hex())
            .unwrap()
            .is_some()
    );
}
