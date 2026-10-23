//! M14: the `object_store` backend, against a real S3 server.
//!
//! Everything about *tiering* is in `tiered.rs`, where the remote is a
//! second directory and the tests run in every build. This file is about
//! the backend itself — signing, multipart, what a 404 looks like — which
//! only a real bucket can answer, and which a local filesystem standing in
//! for one would answer wrongly and convincingly.
//!
//! It runs only with the `remote-store` feature **and** `LIGASE_TEST_STORE`
//! naming a bucket; CI's `store` job sets both against an S3 server in a
//! container. Nothing here runs otherwise, because a test that silently
//! does not is better than one that silently tests a directory.
//!
//! Nothing here is MinIO-specific, which is just as well: the spec asked
//! for MinIO and MinIO is no longer freely obtainable (see the `store` job
//! in `.github/workflows/ci.yml`). Any server that speaks S3 will do, and
//! the tests name only the protocol.
//!
//! Each test works under its own prefix. That is not a dodge of the
//! substitutability question — the rest of the suite answers that, by
//! running against this same bucket — it is how you test a store at all:
//! two tests sharing a content-addressed namespace are two tests sharing
//! their answers.
#![cfg(feature = "remote-store")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use ligase::hash::ContentHash;
use ligase::store::{BlobStore, ObjectKey, StoreError, TieredStore};
use tempfile::TempDir;

/// The bucket CI points us at, or nothing, in which case every test here
/// returns without asserting.
fn bucket() -> Option<String> {
    std::env::var("LIGASE_TEST_STORE")
        .ok()
        .filter(|url| !url.trim().is_empty())
}

/// A two-tier store whose remote is this test's own corner of the bucket.
fn tiers(prefix: &str) -> Option<(TempDir, TieredStore)> {
    let url = format!("{}/{prefix}", bucket()?.trim_end_matches('/'));
    let dir = tempfile::tempdir().unwrap();
    let store = TieredStore::open(&dir.path().join("local"), Some(&url)).unwrap();
    Some((dir, store))
}

/// A second machine: nothing local, the same corner of the bucket.
fn elsewhere(prefix: &str) -> (TempDir, TieredStore) {
    let url = format!(
        "{}/{prefix}",
        bucket()
            .expect("checked by the caller")
            .trim_end_matches('/')
    );
    let dir = tempfile::tempdir().unwrap();
    let store = TieredStore::open(&dir.path().join("elsewhere"), Some(&url)).unwrap();
    (dir, store)
}

fn key_for(bytes: &[u8], suffix: &str) -> ObjectKey {
    ObjectKey::new(ContentHash::of_bytes(bytes), suffix)
}

#[test]
fn an_object_written_here_is_readable_there() {
    let Some((_dir, store)) = tiers("round-trip") else {
        return;
    };
    let key = key_for(b"written to a bucket", ".txt");

    store.put(&key, b"written to a bucket").unwrap();

    let (_second, second) = elsewhere("round-trip");
    assert_eq!(
        second.checked_get(&key).unwrap().as_deref(),
        Some(&b"written to a bucket"[..])
    );
}

#[test]
fn an_object_that_is_not_there_is_a_miss_and_not_an_error() {
    // A 404 from S3 has to come back as `Ok(None)`, or every cold cache
    // would be a failed run.
    let Some((_dir, store)) = tiers("absent") else {
        return;
    };
    let key = key_for(b"never written anywhere", ".txt");

    assert!(!store.has(&key).unwrap());
    assert_eq!(store.get(&key).unwrap(), None);
    assert_eq!(
        store.stats().unreachable,
        0,
        "a miss is not a network problem"
    );
}

#[test]
fn an_object_corrupted_in_the_bucket_is_refused_and_names_both_hashes() {
    // The done-when clause, against a real bucket rather than a stand-in.
    //
    // Corrupting it is a plain `put` of the wrong bytes under the right
    // name: that is what someone with write access to a shared bucket can
    // do, deliberately or by accident, and it is the whole reason an
    // object's name is checked on arrival.
    let Some((_dir, store)) = tiers("corrupt") else {
        return;
    };
    let key = key_for(b"the real thing", ".bam");
    store.put(&key, b"something else entirely").unwrap();

    let (_second, second) = elsewhere("corrupt");
    let error = second.get(&key).unwrap_err();

    assert!(matches!(error, StoreError::Mismatch { .. }));
    let message = error.to_string();
    assert!(message.contains(&key.hash.to_hex()), "{message}");
    assert!(
        message.contains(&ContentHash::of_bytes(b"something else entirely").to_hex()),
        "{message}"
    );
    assert!(
        !second.local().has(&key).unwrap(),
        "and it must not have been promoted into the tier that is trusted"
    );
}

#[test]
fn a_large_object_goes_up_in_parts_and_comes_back_whole() {
    // Above the multipart threshold, so this takes the branch that does
    // not hold the body in memory -- the one a forty-gigabyte BAM needs
    // and that no small test would reach.
    let Some((dir, store)) = tiers("multipart") else {
        return;
    };
    let body: Vec<u8> = (0..20 * 1024 * 1024_usize)
        .map(|at| (at % 251) as u8)
        .collect();
    let staged = dir.path().join("big.bam");
    std::fs::write(&staged, &body).unwrap();

    let key = store.put_file(&staged, ".bam").unwrap();

    let (_second, second) = elsewhere("multipart");
    let read = second.checked_get(&key).unwrap().expect("it is there");
    assert_eq!(read.len(), body.len());
    assert_eq!(read, body);
}

#[test]
fn many_machines_writing_one_object_is_benign() {
    // Two people pushing the same alignment is the normal case for a lab,
    // and content addressing is supposed to make it harmless. Against a
    // real bucket, where the writes are concurrent HTTP requests rather
    // than renames.
    let Some((_dir, store)) = tiers("race") else {
        return;
    };
    let store = Arc::new(store);
    let key = key_for(b"the same answer, from eight machines", ".vcf");

    let threads: Vec<_> = (0..8)
        .map(|_| {
            let store = Arc::clone(&store);
            let key = key.clone();
            std::thread::spawn(move || store.put(&key, b"the same answer, from eight machines"))
        })
        .collect();
    for thread in threads {
        thread.join().unwrap().expect("a benign race failed a put");
    }

    let (_second, second) = elsewhere("race");
    assert_eq!(
        second.checked_get(&key).unwrap().as_deref(),
        Some(&b"the same answer, from eight machines"[..])
    );
}

#[test]
fn a_second_machine_takes_the_work_from_the_bucket() {
    // The clean-machine clause in its smallest honest form: one cache
    // records a step, a second cache with nothing in it finds the entry,
    // the output and the provenance.
    use ligase::cache::Cache;
    use ligase::key::Ingredients;
    use ligase::types::DataKind;

    let Some(bucket) = bucket() else { return };
    let url = format!("{}/handover", bucket.trim_end_matches('/'));
    let dir = tempfile::tempdir().unwrap();

    let key = Ingredients {
        step: "align".to_owned(),
        code: "def align(ref): ...".to_owned(),
        recipe: "minimap2 {ref}".to_owned(),
        output: DataKind::Alignment,
        parameters: Vec::new(),
        inputs: Vec::new(),
        tools: Vec::new(),
    }
    .key();

    let first = Cache::open_with(&dir.path().join("first"), Some(&url)).unwrap();
    let staged = dir.path().join("staged.bam");
    std::fs::write(&staged, b"an alignment from the first machine").unwrap();
    let stored = first.store(&staged).unwrap();
    first
        .record(key, "align", &stored, "minimap2 2.28", 41, &[])
        .unwrap();

    let second = Cache::open_with(&dir.path().join("second"), Some(&url)).unwrap();
    let entry = second.lookup(key).unwrap().expect("no step should run");

    assert_eq!(entry.output.hash, stored.hash);
    assert_eq!(
        std::fs::read(&entry.output.path).unwrap(),
        b"an alignment from the first machine"
    );
    assert!(entry.output.path.starts_with(dir.path().join("second")));
}
