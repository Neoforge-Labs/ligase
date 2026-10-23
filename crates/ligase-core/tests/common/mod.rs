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

/// The real HXB2 reference, if `scripts/fetch_test_data.sh` has been run.
///
/// `data/` is gitignored, so every test touching it must skip when absent
/// rather than fail in CI.
pub fn hxb2_reference() -> Option<PathBuf> {
    let path = repo_root().join("data/refs/HXB2.fasta");
    path.is_file().then_some(path)
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
