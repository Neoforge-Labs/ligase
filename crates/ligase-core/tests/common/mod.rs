//! Shared helpers for the dimension tests.
//!
//! Fixtures are committed as text (SAM, VCF, FASTA, FASTQ) so a reviewer can
//! read an `@SQ M5` and check it by hand. BAM, being binary, is written here
//! from a committed SAM's header: the text fixture stays the source of truth,
//! so a BAM test is not merely noodles' writer agreeing with noodles' reader
//! about a header neither of them was given.
#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

pub mod mock_http;

use noodles::sam::alignment::io::Write as _;
use noodles::{bam, sam};

use ligase::cache::Cache;
use ligase::store::BlobStore as _;
use ligase::types::DataKind;
use tempfile::TempDir;

/// Path to a committed fixture.
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// A BAM written from a committed SAM fixture, with its records carried over.
///
/// The returned [`TempDir`] owns the file; keep it alive for as long as the
/// path is used.
pub fn bam_from_sam(sam_fixture: &str) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let out = dir
        .path()
        .join(sam_fixture.strip_suffix(".sam").unwrap().to_owned() + ".bam");

    let mut reader =
        sam::io::Reader::new(BufReader::new(File::open(fixture(sam_fixture)).unwrap()));
    let header = reader.read_header().unwrap();

    let mut writer = bam::io::Writer::new(File::create(&out).unwrap());
    writer.write_alignment_header(&header).unwrap();
    for result in reader.records() {
        let record = result.unwrap();
        writer.write_alignment_record(&header, &record).unwrap();
    }
    writer.try_finish().unwrap();

    (dir, out)
}

/// The real HXB2 reference, if the demo's store holds it.
///
/// Since M10 the demo does not keep its inputs in `data/`: it declares
/// where they come from and the engine keeps them in its content-addressed
/// store, named by the hash of their bytes. So this does not look for a
/// path — it asks the engine where its own input is, which is exactly what
/// `cache::Cache::remote` is for.
///
/// `None` where the store is cold, so every test touching real data skips
/// rather than fails. Warm it with `python scripts/fetch_demo_data.py`.
pub fn hxb2_reference() -> Option<PathBuf> {
    let cache = Cache::open(&demo_store()).ok()?;
    let row = cache.remote("ncbi:K03455.1", DataKind::Reference).ok()??;
    let key = &row.files.first()?.key;
    cache.objects().local_path(key).ok()?
}

/// Where the demo's fetched inputs live.
///
/// The repository's own cache directory unless `LIGASE_DEMO_STORE` says
/// otherwise, which is the same default the Python tests read — so one
/// warm-up serves both languages.
fn demo_store() -> PathBuf {
    std::env::var_os("LIGASE_DEMO_STORE")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join(".ligase"))
}

/// The real HXB2 alignment, if `scripts/make_demo_alignment.sh` has been run.
pub fn demo_alignment() -> Option<PathBuf> {
    let dir = repo_root().join("data/alignments");
    let mut bams: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "bam"))
        .collect();
    bams.sort();
    bams.into_iter().next()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
