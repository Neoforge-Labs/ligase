//! M2: BED and GFF, where the `coordinates` dimension starts doing real work.
//!
//! A 0-based half-open BED fed to a tool expecting 1-based closed coordinates
//! is the bug that dimension exists for, and it is the one mismatch in the
//! catalogue that no amount of checksum comparison would catch.
//!
//! The other half of these tests is about *not* crying wolf. An interval
//! file's contigs come from its records, which carry no sequence dictionary,
//! so the order they happen to appear in means nothing and must never be
//! reported as a contig order mismatch.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::fixture;
use ligase::check::{Binding, ErrorKind, check_step};
use ligase::dim::{ContigNaming, CoordinateSystem, Dimension};
use ligase::load::{self, Declared, IntervalFormat};
use ligase::types::{DataKind, Dataset};

fn intervals(name: &str) -> Dataset {
    load::intervals(&fixture(name), None, &Declared::default()).unwrap()
}

fn binding(name: &str, dataset: Dataset) -> Binding {
    Binding {
        name: name.to_owned(),
        dataset,
    }
}

// ------------------------------------------------------------- inference --

#[test]
fn bed_is_zero_based_half_open() {
    let dataset = intervals("tiny_bare.bed");

    assert_eq!(dataset.kind(), DataKind::Intervals);
    assert_eq!(
        dataset.dimensions().coordinates.value(),
        Some(&CoordinateSystem::ZeroBasedHalfOpen)
    );
}

#[test]
fn gff_is_one_based_closed() {
    let dataset = intervals("tiny_bare.gff");

    assert_eq!(
        dataset.dimensions().coordinates.value(),
        Some(&CoordinateSystem::OneBasedClosed)
    );
}

#[test]
fn gtf_is_one_based_closed_too() {
    let dataset = intervals("tiny_bare.gtf");

    assert_eq!(
        dataset.dimensions().coordinates.value(),
        Some(&CoordinateSystem::OneBasedClosed)
    );
}

#[test]
fn bed_takes_contig_names_from_records() {
    let dataset = intervals("tiny_bare.bed");
    let reference = dataset.dimensions().reference.value().unwrap();

    let names: Vec<&str> = reference
        .contigs()
        .iter()
        .map(|contig| contig.name.as_str())
        .collect();
    assert_eq!(names, ["1", "2"]);

    // A BED records no length and no checksum. Inventing either would be the
    // lie this whole system guards against.
    assert_eq!(reference.contigs()[0].length, None);
    assert_eq!(reference.contigs()[0].md5, None);
}

#[test]
fn bed_skips_track_lines_and_comments() {
    // The fixture opens with a `track` line and a `#` comment; neither is a
    // contig called `track` or `#`.
    let dataset = intervals("tiny_bare.bed");
    let reference = dataset.dimensions().reference.value().unwrap();

    assert_eq!(reference.contigs().len(), 2);
}

#[test]
fn bed_mentioning_a_contig_twice_lists_it_once() {
    // `tiny_bare.bed` has two intervals on contig 1 and one on contig 2.
    let dataset = intervals("tiny_bare.bed");
    let reference = dataset.dimensions().reference.value().unwrap();

    let names: Vec<&str> = reference
        .contigs()
        .iter()
        .map(|contig| contig.name.as_str())
        .collect();
    assert_eq!(names, ["1", "2"]);
}

#[test]
fn bed_infers_contig_naming_from_records() {
    let bare = intervals("tiny_bare.bed");
    let prefixed = intervals("tiny_prefixed.bed");

    assert_eq!(
        bare.dimensions().contig_naming.value(),
        Some(&ContigNaming::Bare)
    );
    assert_eq!(
        prefixed.dimensions().contig_naming.value(),
        Some(&ContigNaming::Prefixed)
    );
}

#[test]
fn gff_takes_lengths_from_sequence_region_directives() {
    let dataset = intervals("tiny_bare.gff");
    let reference = dataset.dimensions().reference.value().unwrap();

    assert_eq!(reference.contigs()[0].name, "1");
    assert_eq!(reference.contigs()[0].length, Some(20));
    // A GFF never records a checksum.
    assert_eq!(reference.contigs()[0].md5, None);
}

#[test]
fn gff_without_directives_knows_names_but_no_lengths() {
    let dataset = intervals("tiny_no_region.gff");
    let reference = dataset.dimensions().reference.value().unwrap();

    assert_eq!(reference.contigs().len(), 1);
    assert_eq!(reference.contigs()[0].name, "1");
    assert_eq!(reference.contigs()[0].length, None);
}

#[test]
fn gff_stops_reading_at_the_fasta_section() {
    // `tiny_no_region.gff` ends with `##FASTA` and a `>1 contig one` defline.
    // A parser that kept going would report a contig named `>1`.
    let dataset = intervals("tiny_no_region.gff");
    let reference = dataset.dimensions().reference.value().unwrap();

    let names: Vec<&str> = reference
        .contigs()
        .iter()
        .map(|contig| contig.name.as_str())
        .collect();
    assert_eq!(names, ["1"]);
}

#[test]
fn an_empty_bed_reports_its_reference_as_unknown() {
    let dataset = intervals("tiny_empty.bed");

    assert!(!dataset.dimensions().reference.is_known());
    let reason = dataset.dimensions().reference.reason().unwrap();
    assert!(
        reason.contains("no intervals"),
        "unhelpful reason: {reason}"
    );
    // Coordinates are a property of the format, not of the records, so an
    // empty BED still knows its coordinate system.
    assert_eq!(
        dataset.dimensions().coordinates.value(),
        Some(&CoordinateSystem::ZeroBasedHalfOpen)
    );
}

#[test]
fn organism_on_intervals_is_declared_not_inferred() {
    let undeclared = intervals("tiny_bare.bed");
    assert!(!undeclared.dimensions().organism.is_known());

    let declared = load::intervals(
        &fixture("tiny_bare.bed"),
        None,
        &Declared {
            organism: Some(vec!["HIV-1".into()]),
            ..Declared::default()
        },
    )
    .unwrap();
    assert_eq!(
        declared
            .dimensions()
            .organism
            .value()
            .map(ToString::to_string),
        Some("HIV-1".to_owned())
    );
}

#[test]
fn sample_set_does_not_apply_to_intervals() {
    // `web/content/reference/dimensions.mdx` marks sample_set n/a for BED and
    // GFF: an interval file describes regions, not samples.
    assert!(!DataKind::Intervals.applies(Dimension::SampleSet));
    assert!(!DataKind::Intervals.applies(Dimension::Pairing));
    assert!(!DataKind::Intervals.applies(Dimension::QualityEncoding));
    assert!(DataKind::Intervals.applies(Dimension::Coordinates));
    assert!(DataKind::Intervals.applies(Dimension::Reference));
    assert!(DataKind::Intervals.applies(Dimension::ContigNaming));
    assert!(DataKind::Intervals.applies(Dimension::Organism));
}

// ------------------------------------------------------- format handling --

#[test]
fn an_unrecognised_extension_asks_for_the_format() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("regions.txt");
    std::fs::write(&path, "1\t0\t10\n").unwrap();

    let error = load::intervals(&path, None, &Declared::default())
        .expect_err("an unknown extension must not be guessed at");
    let message = error.to_string();
    assert!(
        message.contains("bed") && message.contains("gff"),
        "the message should name the formats it understands: {message}"
    );
}

#[test]
fn an_explicit_format_overrides_the_extension() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("regions.txt");
    std::fs::write(&path, "1\t0\t10\n").unwrap();

    let dataset = load::intervals(&path, Some(IntervalFormat::Bed), &Declared::default()).unwrap();

    assert_eq!(
        dataset.dimensions().coordinates.value(),
        Some(&CoordinateSystem::ZeroBasedHalfOpen)
    );
}

// --------------------------------------------------------------- checking --

#[test]
fn a_bed_against_a_one_based_vcf_is_a_coordinate_mismatch() {
    let regions = intervals("tiny_bare.bed");
    let calls = load::variants(&fixture("tiny_bare.vcf"), &Declared::default()).unwrap();

    let errors = check_step(
        "restrict",
        &[binding("v", calls), binding("regions", regions)],
    );

    let coordinate: Vec<&_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::Coordinates))
        .collect();
    assert_eq!(coordinate.len(), 1, "errors were: {errors:#?}");
    assert_eq!(coordinate[0].kind(), ErrorKind::Dimension);

    // The exact wording is pinned in `check.rs`, against in-memory datasets
    // whose rendered paths do not depend on the working directory.
    let message = coordinate[0].to_string();
    assert!(
        message.contains("v.coordinates = 1-based closed"),
        "{message}"
    );
    assert!(
        message.contains("regions.coordinates = 0-based half-open"),
        "{message}"
    );
    assert!(
        message.contains("Did you mean to convert between coordinate systems?"),
        "{message}"
    );
}

#[test]
fn a_gff_against_a_vcf_agrees_on_coordinates() {
    let regions = intervals("tiny_bare.gff");
    let calls = load::variants(&fixture("tiny_bare.vcf"), &Declared::default()).unwrap();

    let errors = check_step(
        "restrict",
        &[binding("v", calls), binding("regions", regions)],
    );

    assert!(
        errors
            .iter()
            .all(|error| error.dimension() != Some(Dimension::Coordinates)),
        "errors were: {errors:#?}"
    );
}

#[test]
fn a_bed_is_contained_in_a_reference_that_holds_more() {
    let regions = intervals("tiny_bare.bed");
    let reference = load::reference(&fixture("tiny_superset.fasta"), &Declared::default()).unwrap();

    let errors = check_step(
        "restrict",
        &[binding("regions", regions), binding("ref", reference)],
    );

    assert!(
        errors
            .iter()
            .all(|error| error.dimension() != Some(Dimension::Reference)),
        "a BED naming a subset of the reference's contigs is correct input: \
         {errors:#?}"
    );
}

#[test]
fn contigs_observed_in_records_are_never_an_order_mismatch() {
    // `tiny_reordered.bed` names contig 2 before contig 1. A BED's records
    // carry no sequence dictionary, so that order means nothing. Reporting it
    // would be a false alarm, and false alarms are how a correctness tool
    // gets switched off.
    let regions = intervals("tiny_reordered.bed");
    let reference = load::reference(&fixture("tiny_bare.fasta"), &Declared::default()).unwrap();

    let errors = check_step(
        "restrict",
        &[binding("regions", regions), binding("ref", reference)],
    );

    assert!(errors.is_empty(), "errors were: {errors:#?}");
}

#[test]
fn a_prefixed_bed_against_a_bare_reference_is_a_naming_error() {
    let regions = intervals("tiny_prefixed.bed");
    let reference = load::reference(&fixture("tiny_bare.fasta"), &Declared::default()).unwrap();

    let errors = check_step(
        "restrict",
        &[binding("regions", regions), binding("ref", reference)],
    );

    assert_eq!(errors.len(), 1, "errors were: {errors:#?}");
    assert_eq!(errors[0].kind(), ErrorKind::ContigNaming);
}

#[test]
fn a_bed_naming_a_contig_the_reference_lacks_is_a_reference_error() {
    let regions = intervals("tiny_unknown_contig.bed");
    let reference = load::reference(&fixture("tiny_bare.fasta"), &Declared::default()).unwrap();

    let errors = check_step(
        "restrict",
        &[binding("regions", regions), binding("ref", reference)],
    );

    assert_eq!(errors.len(), 1, "errors were: {errors:#?}");
    assert_eq!(errors[0].kind(), ErrorKind::Dimension);
    assert_eq!(errors[0].dimension(), Some(Dimension::Reference));
    assert!(
        errors[0].to_string().contains("Contig `9` is not in"),
        "{}",
        errors[0]
    );
}
