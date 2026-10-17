//! M1 acceptance tests, verbatim from `docs/milestones.md`:
//!
//! - HXB2 FASTA and a sample BAM are loaded with dimensions inferred from
//!   headers.
//! - A BAM aligned to a different reference fails `check()` with a clear
//!   `DimensionError`.
//! - A `chr1` vs `1` mismatch raises a distinct error.
//!
//! These go through real files end to end: parse a header, build a dataset,
//! run the checker.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{bam_from_sam, demo_alignment, fixture, hxb2_reference};
use ligase::check::{Binding, ErrorKind, check_step};
use ligase::dim::{ContigNaming, Dimension};
use ligase::load::{self, Declared};

fn bind(name: &str, dataset: ligase::types::Dataset) -> Binding {
    Binding {
        name: name.to_owned(),
        dataset,
    }
}

#[test]
fn a_reference_and_a_sample_bam_load_with_dimensions_inferred_from_headers() {
    let reference = load::reference(
        &fixture("tiny_bare.fasta"),
        &Declared {
            organism: Some("HIV-1".into()),
            build: Some("tiny".into()),
            ..Declared::default()
        },
    )
    .unwrap();
    let (_dir, bam) = bam_from_sam("tiny_bare.sam");
    let alignment = load::alignment(&bam, &Declared::default()).unwrap();

    // Inferred, not declared: the reference identity and the sample set came
    // out of the headers.
    assert!(reference.dimensions().reference.is_known());
    assert!(alignment.dimensions().reference.is_known());
    assert_eq!(
        alignment
            .dimensions()
            .sample_set
            .value()
            .unwrap()
            .as_slice(),
        ["sample_a"]
    );

    // And they agree, so the step is sound.
    let errors = check_step("call", &[bind("aln", alignment), bind("ref", reference)]);
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn a_bam_aligned_to_a_different_reference_fails_check_with_a_dimension_error() {
    // `tiny_other` has the same contig names and lengths as `tiny_bare` but
    // different sequence. Only the checksum separates them -- exactly the
    // hg19/hg38 class of bug, which never crashes and always produces
    // plausible numbers.
    let reference = load::reference(&fixture("tiny_bare.fasta"), &Declared::default()).unwrap();
    let (_dir, bam) = bam_from_sam("tiny_other.sam");
    let alignment = load::alignment(&bam, &Declared::default()).unwrap();

    let errors = check_step("call", &[bind("aln", alignment), bind("ref", reference)]);

    assert_eq!(errors.len(), 1, "got: {errors:#?}");
    assert_eq!(errors[0].kind(), ErrorKind::Dimension);
    assert_eq!(errors[0].dimension(), Dimension::Reference);

    let message = errors[0].to_string();
    assert!(
        message.starts_with("DimensionError in step `call`:"),
        "{message}"
    );
    // Both values, where each came from, and a leading question.
    assert!(message.contains("aln.reference"), "{message}");
    assert!(message.contains("ref.reference"), "{message}");
    assert!(message.contains("BAM @SQ header"), "{message}");
    assert!(message.contains("FASTA contigs"), "{message}");
    assert!(
        message.contains("Did you pass the wrong reference?"),
        "{message}"
    );
}

#[test]
fn a_chr1_versus_1_mismatch_raises_an_error_distinct_from_a_reference_mismatch() {
    let reference = load::reference(&fixture("tiny_bare.fasta"), &Declared::default()).unwrap();
    let (_dir, bam) = bam_from_sam("tiny_prefixed.sam");
    let alignment = load::alignment(&bam, &Declared::default()).unwrap();

    assert_eq!(
        alignment.dimensions().contig_naming.value(),
        Some(&ContigNaming::Prefixed)
    );
    assert_eq!(
        reference.dimensions().contig_naming.value(),
        Some(&ContigNaming::Bare)
    );

    let errors = check_step("call", &[bind("aln", alignment), bind("ref", reference)]);

    assert_eq!(errors.len(), 1, "got: {errors:#?}");
    // Distinct from the reference mismatch above: a different kind, a
    // different dimension, and a message that points at the real fix.
    assert_eq!(errors[0].kind(), ErrorKind::ContigNaming);
    assert_ne!(errors[0].kind(), ErrorKind::Dimension);
    assert_eq!(errors[0].dimension(), Dimension::ContigNaming);
    assert!(
        errors[0].to_string().contains("renaming step"),
        "{}",
        errors[0]
    );
}

#[test]
fn a_contig_absent_from_the_reference_is_caught() {
    let reference = load::reference(&fixture("tiny_bare.fasta"), &Declared::default()).unwrap();
    let (_dir, bam) = bam_from_sam("tiny_unknown_contig.sam");
    let alignment = load::alignment(&bam, &Declared::default()).unwrap();

    let errors = check_step("call", &[bind("aln", alignment), bind("ref", reference)]);
    assert_eq!(errors.len(), 1, "got: {errors:#?}");
    assert_eq!(errors[0].dimension(), Dimension::Reference);
    assert!(errors[0].to_string().contains('9'), "{}", errors[0]);
}

#[test]
fn a_vcf_naming_only_some_contigs_is_accepted() {
    // Decision D10. This is the false-alarm case: correct input that strict
    // equality would reject, teaching users to stop trusting `check()`.
    let reference = load::reference(&fixture("tiny_superset.fasta"), &Declared::default()).unwrap();
    let variants = load::variants(&fixture("tiny_subset.vcf"), &Declared::default()).unwrap();

    let errors = check_step("resistance", &[bind("v", variants), bind("ref", reference)]);
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

// --------------------------------------------- the real HIV-1 pipeline --

#[test]
fn the_real_demo_alignment_agrees_with_the_real_hxb2_reference() {
    // The reference demo from CLAUDE.md, on real data. `data/` is
    // gitignored, so this skips rather than fails in CI.
    let (Some(fasta), Some(bam)) = (hxb2_reference(), demo_alignment()) else {
        eprintln!(
            "skipping: run scripts/fetch_test_data.sh and scripts/make_demo_alignment.sh \
             to produce data/refs/HXB2.fasta and data/alignments/*.bam"
        );
        return;
    };

    let reference = load::reference(
        &fasta,
        &Declared {
            organism: Some("HIV-1".into()),
            build: Some("HXB2".into()),
            ..Declared::default()
        },
    )
    .unwrap();
    let alignment = load::alignment(&bam, &Declared::default()).unwrap();

    // The aligner wrote an @SQ M5; we computed ours from the FASTA. That
    // they agree is the whole point.
    let from_bam = alignment.dimensions().reference.value().unwrap();
    let from_fasta = reference.dimensions().reference.value().unwrap();
    assert_eq!(from_bam.fingerprint(), from_fasta.fingerprint());
    assert_eq!(from_bam.contigs()[0].name, "K03455.1");
    assert_eq!(from_bam.contigs()[0].length, Some(9719));

    let errors = check_step("call", &[bind("aln", alignment), bind("ref", reference)]);
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}
