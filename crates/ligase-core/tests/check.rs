//! M1: `check_step` refuses to connect steps whose dimensions disagree.
//!
//! The error text is asserted verbatim. It is a deliverable, specified at the
//! end of `web/content/project/golden-example.mdx`: it names the step, both
//! conflicting values, where each value came from, and asks a leading
//! question. A message that does not say where a value came from cannot be
//! acted on.
//!
//! Datasets here are built in memory rather than loaded from files, so the
//! rendered paths are fixed and the assertions stay exact.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use ligase::check::{Binding, ErrorKind, check_step};
use ligase::dim::{
    Contig, ContigNaming, CoordinateSystem, Dim, Dimension, Dimensions, Organism, QualityEncoding,
    ReferenceId, SampleSet, Source,
};
use ligase::types::{DataKind, Dataset};

// ------------------------------------------------------------- builders --

const M1: &str = "a965a71aa3690f605935c54d320905ab";
const M2: &str = "89c2d6e069f386bafb1a0c811ba821f4";
const M1_OTHER: &str = "44e7acaffac988f931debb037a014055";

fn contig(name: &str, md5: &str) -> Contig {
    Contig {
        name: name.to_owned(),
        length: Some(20),
        md5: Some(md5.to_owned()),
    }
}

fn inferred(path: &str, detail: &'static str) -> Source {
    Source::Inferred {
        path: PathBuf::from(path),
        detail,
    }
}

/// An alignment carrying a reference, as inferred from a BAM `@SQ` header.
fn alignment(path: &str, label: Option<&str>, contigs: Vec<Contig>) -> Dataset {
    let source = inferred(path, "BAM @SQ header");
    let reference = ReferenceId::new(label.map(str::to_owned), contigs);
    let naming = reference.naming();
    let dimensions = Dimensions {
        reference: Dim::known(reference, source.clone()),
        contig_naming: Dim::known(naming, source.clone()),
        coordinates: Dim::known(CoordinateSystem::OneBasedClosed, source),
        ..Dimensions::default()
    };
    Dataset::new(DataKind::Alignment, Some(PathBuf::from(path)), dimensions)
}

/// A reference, as inferred from a FASTA.
fn reference(path: &str, label: Option<&str>, contigs: Vec<Contig>) -> Dataset {
    let reference = ReferenceId::new(label.map(str::to_owned), contigs);
    let naming = reference.naming();
    let dimensions = Dimensions {
        reference: Dim::known(reference, inferred(path, "FASTA contigs")),
        contig_naming: Dim::known(naming, inferred(path, "FASTA deflines")),
        ..Dimensions::default()
    };
    Dataset::new(DataKind::Reference, Some(PathBuf::from(path)), dimensions)
}

/// Genomic intervals, as inferred from a BED's records.
///
/// Contigs come from records, so they carry no length and no checksum, and
/// their order is not compared.
fn bed(path: &str, names: &[&str]) -> Dataset {
    let contigs = names
        .iter()
        .map(|name| Contig {
            name: (*name).to_owned(),
            length: None,
            md5: None,
        })
        .collect();
    let reference = ReferenceId::from_records(None, contigs);
    let naming = reference.naming();
    let dimensions = Dimensions {
        reference: Dim::known(reference, inferred(path, "BED records")),
        contig_naming: Dim::known(naming, inferred(path, "BED records")),
        coordinates: Dim::known(
            CoordinateSystem::ZeroBasedHalfOpen,
            inferred(path, "BED coordinate convention"),
        ),
        ..Dimensions::default()
    };
    Dataset::new(DataKind::Intervals, Some(PathBuf::from(path)), dimensions)
}

/// Variant calls, as inferred from a VCF header.
fn variants(path: &str, contigs: Vec<Contig>, samples: &[&str]) -> Dataset {
    let reference = ReferenceId::new(None, contigs);
    let naming = reference.naming();
    let dimensions = Dimensions {
        reference: Dim::known(reference, inferred(path, "VCF ##contig lines")),
        contig_naming: Dim::known(naming, inferred(path, "VCF ##contig lines")),
        coordinates: Dim::known(
            CoordinateSystem::OneBasedClosed,
            inferred(path, "VCF coordinate convention"),
        ),
        sample_set: Dim::known(
            SampleSet::new(samples.iter().map(|s| (*s).to_owned()).collect()),
            inferred(path, "VCF header sample columns"),
        ),
        ..Dimensions::default()
    };
    Dataset::new(DataKind::Variants, Some(PathBuf::from(path)), dimensions)
}

/// Two contigs, as the tiny fixtures and the demo reference both have.
fn two_contigs() -> Vec<Contig> {
    vec![contig("1", M1), contig("2", M2)]
}

fn bind(name: &str, dataset: Dataset) -> Binding {
    Binding {
        name: name.to_owned(),
        dataset,
    }
}

// -------------------------------------------- the specified error shape --

#[test]
fn a_reference_mismatch_reads_like_the_golden_example() {
    let errors = check_step(
        "call",
        &[
            bind(
                "aln",
                alignment("data/sample.bam", Some("HXB2"), two_contigs()),
            ),
            bind(
                "ref",
                reference(
                    "refs/hg38.fasta",
                    Some("hg38"),
                    vec![contig("1", M1_OTHER), contig("2", M2)],
                ),
            ),
        ],
    );

    assert_eq!(errors.len(), 1, "expected exactly one error: {errors:#?}");
    assert_eq!(errors[0].kind(), ErrorKind::Dimension);
    assert_eq!(errors[0].dimension(), Some(Dimension::Reference));
    assert_eq!(
        errors[0].to_string(),
        "\
DimensionError in step `call`:
  aln.reference = HXB2 (from BAM @SQ header: data/sample.bam)
  ref.reference = hg38 (from FASTA contigs: refs/hg38.fasta)
  Contig `1`: same name and length but different sequence \
(MD5 a965a71a against 44e7acaf).
  These must match. Did you pass the wrong reference?"
    );
}

#[test]
fn a_contig_naming_mismatch_is_a_distinct_error() {
    // The third M1 acceptance criterion. Same contigs, same checksums, one
    // naming style apart -- so the fix is a renaming step, not a different
    // reference, and the message must say so.
    let errors = check_step(
        "call",
        &[
            bind(
                "aln",
                alignment(
                    "data/sample.bam",
                    None,
                    vec![contig("chr1", M1), contig("chr2", M2)],
                ),
            ),
            bind("ref", reference("refs/bare.fasta", None, two_contigs())),
        ],
    );

    assert_eq!(errors.len(), 1, "expected exactly one error: {errors:#?}");
    assert_eq!(errors[0].kind(), ErrorKind::ContigNaming);
    assert_eq!(errors[0].dimension(), Some(Dimension::ContigNaming));
    assert_eq!(
        errors[0].to_string(),
        "\
ContigNamingError in step `call`:
  aln.contig_naming = chr-prefixed (UCSC style) (from BAM @SQ header: data/sample.bam)
  ref.contig_naming = bare (Ensembl/NCBI style) (from FASTA deflines: refs/bare.fasta)
  The contigs are otherwise identical. Add a renaming step, or use one naming \
style throughout."
    );
}

#[test]
fn a_contig_order_mismatch_is_its_own_error() {
    let errors = check_step(
        "call",
        &[
            bind(
                "aln",
                alignment(
                    "data/sample.bam",
                    None,
                    vec![contig("2", M2), contig("1", M1)],
                ),
            ),
            bind("ref", reference("refs/bare.fasta", None, two_contigs())),
        ],
    );

    assert_eq!(errors.len(), 1, "expected exactly one error: {errors:#?}");
    assert_eq!(errors[0].kind(), ErrorKind::ContigOrder);
    assert!(
        errors[0].to_string().contains("GATK"),
        "the message should say who this breaks: {}",
        errors[0]
    );
}

#[test]
fn an_undeclared_required_dimension_asks_the_user_to_declare_it() {
    let reads = Dataset::new(
        DataKind::Reads,
        Some("data/r1.fastq".into()),
        Dimensions {
            sample_set: Dim::known(SampleSet::new(vec!["sample_a".into()]), Source::Declared),
            ..Dimensions::default()
        },
    );
    let variants = Dataset::new(
        DataKind::Variants,
        Some("out/calls.vcf".into()),
        Dimensions {
            sample_set: Dim::unknown("no sample columns in the VCF header"),
            reference: Dim::known(
                ReferenceId::new(None, two_contigs()),
                inferred("out/calls.vcf", "VCF ##contig lines"),
            ),
            ..Dimensions::default()
        },
    );

    let errors = check_step("report", &[bind("reads", reads), bind("v", variants)]);

    let undeclared: Vec<_> = errors
        .iter()
        .filter(|error| error.kind() == ErrorKind::Undeclared)
        .collect();
    assert_eq!(undeclared.len(), 1, "got: {errors:#?}");
    assert_eq!(undeclared[0].dimension(), Some(Dimension::SampleSet));
    assert_eq!(
        undeclared[0].to_string(),
        "\
UndeclaredDimensionError in step `report`:
  v.sample_set is unknown (no sample columns in the VCF header)
  reads.sample_set = [sample_a] (declared)
  `report` needs sample_set to agree. Declare it where you load the data."
    );
}

// ---------------------------------------------------- required vs checked --

#[test]
fn aligning_reads_does_not_demand_a_declared_organism() {
    // This is why "required" and "checked when known" are separate notions.
    // `lg.samples()` in the golden example takes no organism, and align() is
    // the step that introduces the reference -- so demanding an organism on
    // the FASTQs would break the documented API for no safety gain.
    let reads = Dataset::new(
        DataKind::Reads,
        Some("data/r1.fastq".into()),
        Dimensions::default(),
    );
    let reference = reference("refs/hxb2.fasta", Some("HXB2"), two_contigs());

    let errors = check_step("align", &[bind("reads", reads), bind("ref", reference)]);
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn a_known_dimension_is_checked_even_when_it_is_not_required() {
    // Organism is not required between reads and a reference, but if both
    // sides state one and they disagree, that is still a bug worth catching.
    let reads = Dataset::new(
        DataKind::Reads,
        Some("data/r1.fastq".into()),
        Dimensions {
            organism: Dim::known(Organism::new("human"), Source::Declared),
            ..Dimensions::default()
        },
    );
    let reference = Dataset::new(
        DataKind::Reference,
        Some("refs/hxb2.fasta".into()),
        Dimensions {
            reference: Dim::known(
                ReferenceId::new(Some("HXB2".into()), two_contigs()),
                inferred("refs/hxb2.fasta", "FASTA contigs"),
            ),
            organism: Dim::known(Organism::new("HIV-1"), Source::Declared),
            ..Dimensions::default()
        },
    );

    let errors = check_step("align", &[bind("reads", reads), bind("ref", reference)]);
    assert_eq!(errors.len(), 1, "got: {errors:#?}");
    assert_eq!(errors[0].dimension(), Some(Dimension::Organism));
    assert!(errors[0].to_string().contains("wrong organism"));
}

#[test]
fn organism_comparison_ignores_case_and_surrounding_space() {
    let left = Dimensions {
        organism: Dim::known(Organism::new("  HIV-1 "), Source::Declared),
        ..Dimensions::default()
    };
    let right = Dimensions {
        organism: Dim::known(Organism::new("hiv-1"), Source::Declared),
        ..Dimensions::default()
    };

    let errors = check_step(
        "report",
        &[
            bind(
                "a",
                Dataset::new(DataKind::Reference, Some("a.fasta".into()), left),
            ),
            bind(
                "b",
                Dataset::new(DataKind::Reference, Some("b.fasta".into()), right),
            ),
        ],
    );
    assert!(
        errors
            .iter()
            .all(|error| error.dimension() != Some(Dimension::Organism)),
        "got: {errors:#?}"
    );
}

#[test]
fn dimensions_that_do_not_apply_to_a_kind_are_not_checked() {
    // Quality encoding is meaningless for a reference, so a step joining
    // reads to a reference must never raise it.
    let reads = Dataset::new(
        DataKind::Reads,
        Some("data/r1.fastq".into()),
        Dimensions {
            quality_encoding: Dim::known(QualityEncoding::Phred64, Source::Declared),
            ..Dimensions::default()
        },
    );
    let reference = reference("refs/hxb2.fasta", Some("HXB2"), two_contigs());

    let errors = check_step("align", &[bind("reads", reads), bind("ref", reference)]);
    assert!(
        errors
            .iter()
            .all(|error| error.dimension() != Some(Dimension::QualityEncoding)),
        "got: {errors:#?}"
    );
}

#[test]
fn contig_naming_is_not_reported_twice_when_the_reference_is_known() {
    // Naming is derived from the reference, so checking the reference
    // subsumes it. Two errors for one root cause is noise.
    let errors = check_step(
        "call",
        &[
            bind(
                "aln",
                alignment(
                    "data/sample.bam",
                    None,
                    vec![contig("chr1", M1), contig("chr2", M2)],
                ),
            ),
            bind("ref", reference("refs/bare.fasta", None, two_contigs())),
        ],
    );

    assert_eq!(errors.len(), 1, "got: {errors:#?}");
}

// ------------------------------------------------------- one pass, D10 --

#[test]
fn every_error_is_reported_in_one_pass() {
    // M2 requires `ligase check` to report all type errors at once. The
    // checker already must not stop at the first.
    let aln = Dataset::new(
        DataKind::Alignment,
        Some("data/sample.bam".into()),
        Dimensions {
            reference: Dim::known(
                ReferenceId::new(Some("HXB2".into()), two_contigs()),
                inferred("data/sample.bam", "BAM @SQ header"),
            ),
            sample_set: Dim::known(
                SampleSet::new(vec!["sample_a".into()]),
                inferred("data/sample.bam", "BAM @RG header"),
            ),
            coordinates: Dim::known(CoordinateSystem::OneBasedClosed, Source::Declared),
            ..Dimensions::default()
        },
    );
    let vcf = Dataset::new(
        DataKind::Variants,
        Some("out/calls.vcf".into()),
        Dimensions {
            reference: Dim::known(
                ReferenceId::new(
                    Some("hg38".into()),
                    vec![contig("1", M1_OTHER), contig("2", M2)],
                ),
                inferred("out/calls.vcf", "VCF ##contig lines"),
            ),
            sample_set: Dim::known(
                SampleSet::new(vec!["sample_b".into()]),
                inferred("out/calls.vcf", "VCF header sample columns"),
            ),
            coordinates: Dim::known(CoordinateSystem::OneBasedClosed, Source::Declared),
            ..Dimensions::default()
        },
    );

    let errors = check_step("resistance", &[bind("aln", aln), bind("v", vcf)]);

    let dimensions: Vec<Option<Dimension>> = errors.iter().map(|error| error.dimension()).collect();
    assert!(
        dimensions.contains(&Some(Dimension::Reference)),
        "got: {dimensions:?}"
    );
    assert!(
        dimensions.contains(&Some(Dimension::SampleSet)),
        "got: {dimensions:?}"
    );
}

#[test]
fn sample_sets_must_match_in_order() {
    // The shuffled-sample-ID bug: same identities, wrong order, plausible
    // numbers, wrong answers.
    let left = Dimensions {
        sample_set: Dim::known(
            SampleSet::new(vec!["a".into(), "b".into()]),
            inferred("one.vcf", "VCF header sample columns"),
        ),
        ..Dimensions::default()
    };
    let right = Dimensions {
        sample_set: Dim::known(
            SampleSet::new(vec!["b".into(), "a".into()]),
            inferred("two.vcf", "VCF header sample columns"),
        ),
        ..Dimensions::default()
    };

    let errors = check_step(
        "merge",
        &[
            bind(
                "one",
                Dataset::new(DataKind::Variants, Some("one.vcf".into()), left),
            ),
            bind(
                "two",
                Dataset::new(DataKind::Variants, Some("two.vcf".into()), right),
            ),
        ],
    );

    let sample_errors: Vec<_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::SampleSet))
        .collect();
    assert_eq!(sample_errors.len(), 1, "got: {errors:#?}");
    assert!(
        sample_errors[0].to_string().contains("in order"),
        "the message should name order as the problem: {}",
        sample_errors[0]
    );
}

#[test]
fn a_single_binding_is_never_an_error() {
    // Nothing is being joined, so nothing is required.
    let errors = check_step(
        "report",
        &[bind(
            "v",
            Dataset::new(
                DataKind::Variants,
                Some("out/calls.vcf".into()),
                Dimensions::default(),
            ),
        )],
    );
    assert!(errors.is_empty(), "got: {errors:#?}");
}

#[test]
fn matching_dimensions_produce_no_errors() {
    let errors = check_step(
        "call",
        &[
            bind(
                "aln",
                alignment("data/sample.bam", Some("HXB2"), two_contigs()),
            ),
            bind(
                "ref",
                reference("refs/hxb2.fasta", Some("HXB2"), two_contigs()),
            ),
        ],
    );
    assert!(errors.is_empty(), "got: {errors:#?}");
}

#[test]
fn contig_naming_renders_for_humans() {
    assert_eq!(
        ContigNaming::Prefixed.describe(),
        "chr-prefixed (UCSC style)"
    );
    assert_eq!(ContigNaming::Bare.describe(), "bare (Ensembl/NCBI style)");
    assert_eq!(ContigNaming::Mixed.describe(), "mixed");
}

// ------------------------------------------------------------ coordinates --

#[test]
fn a_coordinate_mismatch_names_both_conventions() {
    // The mismatch the `coordinates` dimension exists for: 0-based half-open
    // intervals against 1-based closed calls. Nothing crashes, no checksum
    // disagrees, and every result is off by one base.
    let errors = check_step(
        "restrict",
        &[
            bind("v", variants("out/calls.vcf", two_contigs(), &["sample_a"])),
            bind("regions", bed("data/regions.bed", &["1", "2"])),
        ],
    );

    assert_eq!(errors.len(), 1, "expected exactly one error: {errors:#?}");
    assert_eq!(errors[0].kind(), ErrorKind::Dimension);
    assert_eq!(errors[0].dimension(), Some(Dimension::Coordinates));
    assert_eq!(
        errors[0].to_string(),
        "\
DimensionError in step `restrict`:
  v.coordinates = 1-based closed (from VCF coordinate convention: out/calls.vcf)
  regions.coordinates = 0-based half-open (from BED coordinate convention: \
data/regions.bed)
  These must match. Did you mean to convert between coordinate systems?"
    );
}

#[test]
fn intervals_are_never_asked_to_declare_a_reference() {
    // An interval file's contigs come from its records, so an empty one
    // knows none and the user has no way to declare them. Demanding it would
    // be an unsatisfiable check.
    let empty = Dataset::new(
        DataKind::Intervals,
        Some(PathBuf::from("data/empty.bed")),
        Dimensions {
            coordinates: Dim::known(
                CoordinateSystem::ZeroBasedHalfOpen,
                inferred("data/empty.bed", "BED coordinate convention"),
            ),
            ..Dimensions::default()
        },
    );

    let errors = check_step(
        "restrict",
        &[
            bind("ref", reference("refs/tiny.fasta", None, two_contigs())),
            bind("regions", empty),
        ],
    );

    assert!(errors.is_empty(), "errors were: {errors:#?}");
}

// ------------------------------------------- naming between two datasets --

#[test]
fn two_datasets_sharing_no_contig_names_are_still_checked_for_naming() {
    // Regression. Neither side is the reference, so contigs the other lacks
    // are ignored by containment -- which once meant a `chr1` against `1`
    // mismatch between two VCFs agreed *vacuously*: there was nothing in
    // common left to disagree about. Normalising the names finds the overlap.
    let errors = check_step(
        "merge",
        &[
            bind("a", variants("out/bare.vcf", two_contigs(), &["sample_a"])),
            bind(
                "b",
                variants(
                    "out/prefixed.vcf",
                    vec![contig("chr1", M1), contig("chr2", M2)],
                    &["sample_a"],
                ),
            ),
        ],
    );

    assert_eq!(errors.len(), 1, "errors were: {errors:#?}");
    assert_eq!(errors[0].kind(), ErrorKind::ContigNaming);
}

#[test]
fn two_datasets_covering_different_contigs_are_not_a_mismatch() {
    // The other side of that fix. A chr20-only VCF and a chr21-only VCF are
    // both correct against the same reference, and saying otherwise would be
    // a false alarm.
    let errors = check_step(
        "merge",
        &[
            bind(
                "a",
                variants("out/one.vcf", vec![contig("1", M1)], &["sample_a"]),
            ),
            bind(
                "b",
                variants("out/two.vcf", vec![contig("2", M2)], &["sample_a"]),
            ),
        ],
    );

    assert!(errors.is_empty(), "errors were: {errors:#?}");
}
