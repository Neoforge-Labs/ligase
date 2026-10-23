//! M10: a pinned remote, and what it means to pin one.
//!
//! A `PinnedRemote` promises the same bytes forever *from the source*. The
//! promise is kept by a digest, and a digest is only worth having if the
//! engine refuses to proceed when it does not match — before any step runs,
//! not after a step has produced a result from the wrong input.
//!
//! The digest is over what the file *means*, not over its bytes (D4, D21).
//! That is the one choice here worth arguing about, and the argument is in
//! `scripts/fetch_test_data.sh`'s own comment, which this milestone deletes:
//! NCBI may reflow lines or reword a defline without the biology changing,
//! and an alarm that fires on a cosmetic reformat teaches people to bump
//! pins without reading — after which the one that matters gets bumped too.
//!
//! Everything here runs against a real socket on localhost, so "with the
//! server down" means a closed port.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;

use common::mock_http::{MockServer, Reply};
use ligase::cache::Cache;
use ligase::hash::ContentHash;
use ligase::note::Note;
use ligase::remote::Locator;
use ligase::remote::fetch::{FetchError, Fetched, Network, Offline, Policy, Transport};
use ligase::remote::pinned::{self, RemoteError, Source};
use ligase::types::DataKind;
use tempfile::TempDir;

const HXB2: &[u8] = b">K03455.1 HIV-1\nACGTACGTAC\nGTACGTACGT\n";

/// The same sequence, reflowed and the defline reworded — which is what NCBI
/// does from time to time, and which changes no biology at all.
const HXB2_REFLOWED: &[u8] = b">K03455.1 Human immunodeficiency virus 1\nACGTACGTACGTACGTACGT\n";

fn setup() -> (TempDir, Cache, MockServer) {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open(&dir.path().join(".ligase")).unwrap();
    (dir, cache, MockServer::start())
}

fn transport() -> Network {
    Network::new(Policy {
        attempts: 1,
        backoff: std::time::Duration::from_millis(1),
        ..Policy::default()
    })
}

/// The digest the engine would give these bytes, read as a reference.
///
/// A fold over the files in order, even for one file, so a single file and a
/// pair are described the same way.
fn digest_of(cache: &Cache, bytes: &[u8], suffix: &str) -> ContentHash {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("probe{suffix}"));
    fs::write(&path, bytes).unwrap();
    ContentHash::of_hashes(&[cache.semantic_hash(DataKind::Reference, &path).unwrap()])
}

/// Resolve `locator` as a reference, with whatever digest was declared.
fn resolve(
    cache: &Cache,
    locator: &str,
    declared: Option<ContentHash>,
) -> Result<pinned::Resolved, RemoteError> {
    pinned::resolve(
        &Locator::parse(locator).unwrap(),
        DataKind::Reference,
        declared,
        cache,
        &transport(),
    )
}

// ------------------------------------------------------- the first fetch --

#[test]
fn a_first_resolution_fetches_once_and_records_what_it_got() {
    let (_dir, cache, server) = setup();
    server.on("/HXB2.fasta", HXB2);
    let locator = server.url("/HXB2.fasta");

    let resolved = resolve(&cache, &locator, None).unwrap();

    assert_eq!(resolved.from, Source::Network);
    assert_eq!(server.hits("/HXB2.fasta"), 1);
    assert_eq!(resolved.paths.len(), 1);
    assert_eq!(fs::read(&resolved.paths[0]).unwrap(), HXB2.to_vec());
    assert!(
        resolved.record.fetched_at > 0,
        "the methods paragraph says *when*, so the time is recorded"
    );
    assert_eq!(resolved.record.locator, locator);
}

#[test]
fn an_undeclared_digest_records_what_arrived_and_says_so() {
    let (_dir, cache, server) = setup();
    server.on("/HXB2.fasta", HXB2);

    let resolved = resolve(&cache, &server.url("/HXB2.fasta"), None).unwrap();

    // Nothing was promised, so nothing is broken — but the digest is now on
    // record, which is what a user copies into `digest=` to make the next
    // run a checked one.
    assert!(!resolved.digest.to_hex().is_empty());
    assert_eq!(resolved.record.digest, resolved.digest);
}

#[test]
fn a_matching_digest_is_accepted() {
    let (_dir, cache, server) = setup();
    server.on("/HXB2.fasta", HXB2);
    let locator = server.url("/HXB2.fasta");

    let first = resolve(&cache, &locator, None).unwrap();
    let again = resolve(&cache, &locator, Some(first.digest)).unwrap();

    assert_eq!(again.digest, first.digest);
}

// ------------------------------------------------- the second resolution --

#[test]
fn a_second_resolution_serves_from_the_store_with_the_server_down() {
    let (_dir, cache, mut server) = setup();
    server.on("/HXB2.fasta", HXB2);
    let locator = server.url("/HXB2.fasta");

    let first = resolve(&cache, &locator, None).unwrap();
    server.shutdown();

    let again = resolve(&cache, &locator, Some(first.digest)).unwrap();

    assert_eq!(
        again.from,
        Source::Store,
        "no network, because none is needed"
    );
    assert_eq!(again.digest, first.digest);
    assert_eq!(fs::read(&again.paths[0]).unwrap(), HXB2.to_vec());
}

// -------------------------------------------------------- a wrong digest --

#[test]
fn a_wrong_digest_fails_naming_the_locator_the_expected_and_the_got() {
    let (_dir, cache, server) = setup();
    server.on("/HXB2.fasta", HXB2);
    let locator = server.url("/HXB2.fasta");
    let wrong = ContentHash::of_bytes(b"not what is there");

    let error = resolve(&cache, &locator, Some(wrong)).unwrap_err();

    let message = error.to_string();
    assert!(matches!(error, RemoteError::Moved { .. }), "{message}");
    assert!(message.contains(&locator), "{message}");
    assert!(
        message.contains(&wrong.to_hex()),
        "the expected digest, so it can be compared: {message}"
    );
    assert!(
        message.contains("moved"),
        "and the diagnosis in a word a user can act on: {message}"
    );
}

#[test]
fn a_wrong_digest_stores_nothing_under_the_name_it_claimed() {
    let (_dir, cache, server) = setup();
    server.on("/HXB2.fasta", HXB2);
    let locator = server.url("/HXB2.fasta");
    let wrong = ContentHash::of_bytes(b"not what is there");

    let _ = resolve(&cache, &locator, Some(wrong));

    // Nothing may be left behind that a later run could mistake for a
    // verified input, and nothing may be recorded that a later run would
    // serve without asking.
    let error = resolve(&cache, &locator, Some(wrong)).unwrap_err();
    assert!(matches!(error, RemoteError::Moved { .. }));
    assert_eq!(
        server.hits("/HXB2.fasta"),
        2,
        "it asked again, which it would not do if it had kept the first answer"
    );
}

#[test]
fn a_digest_the_user_bumped_asks_the_source_again() {
    let (_dir, cache, server) = setup();
    server.on("/x.fasta", HXB2);
    let locator = server.url("/x.fasta");
    let first = resolve(&cache, &locator, None).unwrap();

    // The source legitimately changed -- a different sequence, not a
    // reflow -- and the user updated the pin to the new one. The record no
    // longer satisfies the declaration, so the record is not the answer:
    // ask the source.
    const CHANGED: &[u8] = b">K03455.2 HIV-1\nTTTTTTTTTT\n";
    server.replies("/x.fasta", vec![Reply::ok(CHANGED)]);
    let bumped = digest_of(&cache, CHANGED, ".fasta");
    assert_ne!(bumped, first.digest, "the fixture must really differ");

    let resolved = resolve(&cache, &locator, Some(bumped)).unwrap();

    assert_eq!(resolved.from, Source::Network);
    assert_eq!(resolved.digest, bumped);
}

// ------------------------------------------- bytes moved, meaning did not --

#[test]
fn a_reflowed_file_whose_meaning_is_unchanged_succeeds_and_is_noticed() {
    let (_dir, cache, server) = setup();
    server.on("/HXB2.fasta", HXB2);
    let locator = server.url("/HXB2.fasta");

    let first = resolve(&cache, &locator, None).unwrap();

    // The store is cleared — a CI cache miss, say — while the record of what
    // was fetched survives. Then NCBI reflows the file.
    for path in &first.paths {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).unwrap();
        fs::remove_file(path).unwrap();
    }
    server.replies("/HXB2.fasta", vec![Reply::ok(HXB2_REFLOWED)]);

    let again = resolve(&cache, &locator, Some(first.digest)).unwrap();

    assert_eq!(
        again.digest, first.digest,
        "the sequence is the same sequence, so the pin still holds"
    );
    assert_ne!(
        again.record.bytes, first.record.bytes,
        "and the bytes are not the same bytes"
    );
    assert!(
        again
            .notes
            .iter()
            .any(|note| matches!(note, Note::RemoteRewritten { .. })),
        "which is worth saying rather than failing on: {:?}",
        again.notes
    );
}

// ------------------------------------------------------- what 200 can be --

#[test]
fn an_html_error_page_served_as_two_hundred_is_refused_naming_the_locator() {
    let (_dir, cache, server) = setup();
    // NCBI answers a bad accession with 200 and an HTML page. Nothing about
    // the status says so, and with no digest declared there is nothing to
    // compare against either — so the only thing standing between that page
    // and a pipeline treating it as a reference is reading it.
    server.on(
        "/bad.fasta",
        b"<!DOCTYPE html><html><body>Error: ID list is empty</body></html>",
    );
    let locator = server.url("/bad.fasta");

    let error = resolve(&cache, &locator, None).unwrap_err();

    let message = error.to_string();
    assert!(matches!(error, RemoteError::Unreadable { .. }), "{message}");
    assert!(message.contains(&locator), "{message}");
}

// --------------------------------------------------------------- a pair --

/// A transport that sends every URL to the mock server instead.
///
/// The `sra:` scheme resolves to ENA's own host, and a test must not reach
/// it. Rewriting the host rather than canning the response means the
/// resolver, the locator and ENA's directory rule are all still the real
/// ones -- only the machine at the end is local.
struct ViaMock {
    base: String,
    inner: Network,
}

impl Transport for ViaMock {
    fn get(&self, url: &str, into: &std::path::Path) -> Result<Fetched, FetchError> {
        let path = url
            .split_once("://")
            .and_then(|(_, rest)| rest.split_once('/'))
            .map_or_else(String::new, |(_, path)| format!("/{path}"));
        self.inner.get(&format!("{}{path}", self.base), into)
    }
}

fn gzipped(plain: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(plain).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn a_paired_run_resolves_to_two_objects_in_mate_order() {
    let (_dir, cache, server) = setup();
    let mate_one = gzipped(b"@read1\nACGT\n+\n!!!!\n");
    let mate_two = gzipped(b"@read1\nTGCA\n+\n####\n");
    server.on(
        "/vol1/fastq/SRR322/026/SRR32217726/SRR32217726_1.fastq.gz",
        &mate_one,
    );
    server.on(
        "/vol1/fastq/SRR322/026/SRR32217726/SRR32217726_2.fastq.gz",
        &mate_two,
    );

    let resolved = pinned::resolve(
        &Locator::parse("sra:SRR32217726").unwrap(),
        DataKind::Reads,
        None,
        &cache,
        &ViaMock {
            base: server.base(),
            inner: transport(),
        },
    )
    .unwrap();

    assert_eq!(resolved.paths.len(), 2, "a paired run is two files");
    assert_eq!(fs::read(&resolved.paths[0]).unwrap(), mate_one);
    assert_eq!(fs::read(&resolved.paths[1]).unwrap(), mate_two);
    assert_eq!(resolved.record.files.len(), 2);
    assert!(
        resolved.record.files[0].url.ends_with("_1.fastq.gz"),
        "mate order is the order the locator gave, and it is recorded"
    );
}

#[test]
fn swapping_a_pairs_mates_is_a_different_digest() {
    let (_dir, cache, server) = setup();
    let mate_one = gzipped(b"@read1\nACGT\n+\n!!!!\n");
    let mate_two = gzipped(b"@read1\nTGCA\n+\n####\n");

    let forward = {
        server.on(
            "/vol1/fastq/SRR322/026/SRR32217726/SRR32217726_1.fastq.gz",
            &mate_one,
        );
        server.on(
            "/vol1/fastq/SRR322/026/SRR32217726/SRR32217726_2.fastq.gz",
            &mate_two,
        );
        pinned::resolve(
            &Locator::parse("sra:SRR32217726").unwrap(),
            DataKind::Reads,
            None,
            &cache,
            &ViaMock {
                base: server.base(),
                inner: transport(),
            },
        )
        .unwrap()
        .digest
    };

    // The same two files, handed over in the other order. A paired-end
    // dataset with its mates swapped is a different dataset -- every read's
    // orientation is reversed -- so it must not share a digest with the
    // first one, and that is what folding in order buys.
    let (_other_dir, other_cache, other_server) = setup();
    other_server.on(
        "/vol1/fastq/SRR322/026/SRR32217726/SRR32217726_1.fastq.gz",
        &mate_two,
    );
    other_server.on(
        "/vol1/fastq/SRR322/026/SRR32217726/SRR32217726_2.fastq.gz",
        &mate_one,
    );
    let backward = pinned::resolve(
        &Locator::parse("sra:SRR32217726").unwrap(),
        DataKind::Reads,
        None,
        &other_cache,
        &ViaMock {
            base: other_server.base(),
            inner: transport(),
        },
    )
    .unwrap()
    .digest;

    assert_ne!(forward, backward);
}

// ---------------------------------------------------------- unreachable --

#[test]
fn a_source_that_cannot_be_reached_and_a_store_that_has_nothing_is_an_error() {
    let (_dir, cache, mut server) = setup();
    let locator = server.url("/never-there.fasta");
    server.shutdown();

    let error = resolve(&cache, &locator, None).unwrap_err();

    assert!(
        matches!(error, RemoteError::Fetch { .. }),
        "expected a fetch failure, got {error}"
    );
    assert!(error.to_string().contains(&locator));
}

// ---------------------------------------------------------- offline runs --

#[test]
fn an_offline_run_serves_what_the_store_holds() {
    let (_dir, cache, server) = setup();
    server.on("/HXB2.fasta", HXB2);
    let locator = server.url("/HXB2.fasta");
    let first = resolve(&cache, &locator, None).unwrap();

    let offline = pinned::resolve(
        &Locator::parse(&locator).unwrap(),
        DataKind::Reference,
        Some(first.digest),
        &cache,
        &Offline,
    )
    .unwrap();

    // Which is the point of the whole design: a reviewer with no
    // connection runs the pipeline and gets the same inputs.
    assert_eq!(offline.from, Source::Store);
    assert_eq!(offline.digest, first.digest);
}

#[test]
fn an_offline_run_that_needs_a_fetch_says_what_to_do_about_it() {
    let (_dir, cache, server) = setup();
    server.on("/HXB2.fasta", HXB2);

    let error = pinned::resolve(
        &Locator::parse(&server.url("/HXB2.fasta")).unwrap(),
        DataKind::Reference,
        None,
        &cache,
        &Offline,
    )
    .unwrap_err();

    let message = error.to_string();
    assert!(matches!(error, RemoteError::Fetch { .. }), "{message}");
    assert!(
        message.contains("may not fetch"),
        "and it says why rather than only that it failed: {message}"
    );
    assert!(
        message.contains("network available"),
        "and what to do instead: {message}"
    );
    assert_eq!(
        server.hits("/HXB2.fasta"),
        0,
        "offline means offline: nothing was asked"
    );
}
