//! M1: dimensions are inferred from file headers, per `docs/dimensions.md`.
//!
//! Every assertion here is a claim about what a real header says. Where a
//! dimension cannot be inferred, the test asserts it comes back *unknown*
//! rather than guessed — a wrong guess in this field is worse than an honest
//! gap, because a guess silently passes `check()`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{bam_from_sam, fixture, hxb2_reference};
use ligase::dim::{ContigNaming, CoordinateSystem, Pairing, QualityEncoding};
use ligase::load::{self, Declared};
use ligase::types::DataKind;

// ----------------------------------------------------------------- FASTA --

#[test]
fn fasta_infers_contigs_with_lengths_and_computed_md5() {
    let dataset = load::reference(&fixture("tiny_bare.fasta"), &Declared::default()).unwrap();

    assert_eq!(dataset.kind(), DataKind::Reference);
    let reference = dataset.dimensions().reference.value().unwrap();
    let contigs = reference.contigs();

    assert_eq!(contigs.len(), 2);
    assert_eq!(contigs[0].name, "1");
    assert_eq!(contigs[0].length, Some(20));
    // MD5 of the uppercased, whitespace-stripped sequence, as the SAM
    // specification defines @SQ M5. We compute it; we do not read it.
    assert_eq!(
        contigs[0].md5.as_deref(),
        Some("a965a71aa3690f605935c54d320905ab")
    );
    assert_eq!(contigs[1].name, "2");
    assert_eq!(
        contigs[1].md5.as_deref(),
        Some("89c2d6e069f386bafb1a0c811ba821f4")
    );
}

#[test]
fn fasta_contig_name_is_the_first_defline_token_not_the_description() {
    let dataset = load::reference(&fixture("tiny_bare.fasta"), &Declared::default()).unwrap();
    let reference = dataset.dimensions().reference.value().unwrap();

    // The defline is ">1 contig one"; the name is "1".
    assert_eq!(reference.contigs()[0].name, "1");
}

#[test]
fn fasta_infers_contig_naming_style() {
    let bare = load::reference(&fixture("tiny_bare.fasta"), &Declared::default()).unwrap();
    let prefixed = load::reference(&fixture("tiny_prefixed.fasta"), &Declared::default()).unwrap();

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
fn fasta_organism_is_declared_not_inferred() {
    let undeclared = load::reference(&fixture("tiny_bare.fasta"), &Declared::default()).unwrap();
    assert!(!undeclared.dimensions().organism.is_known());

    let declared = load::reference(
        &fixture("tiny_bare.fasta"),
        &Declared {
            organism: Some("HIV-1".into()),
            ..Declared::default()
        },
    )
    .unwrap();
    assert_eq!(
        declared.dimensions().organism.value().map(|o| o.as_str()),
        Some("HIV-1")
    );
}

#[test]
fn fasta_has_no_coordinate_system() {
    // A reference sequence has no records, so no coordinate convention.
    let dataset = load::reference(&fixture("tiny_bare.fasta"), &Declared::default()).unwrap();
    assert!(!dataset.dimensions().coordinates.is_known());
    assert!(!DataKind::Reference.applies(ligase::dim::Dimension::Coordinates));
}

// ------------------------------------------------------------- BAM / SAM --

#[test]
fn bam_infers_reference_from_sq_header() {
    let (_dir, bam) = bam_from_sam("tiny_bare.sam");
    let dataset = load::alignment(&bam, &Declared::default()).unwrap();

    assert_eq!(dataset.kind(), DataKind::Alignment);
    let reference = dataset.dimensions().reference.value().unwrap();
    assert_eq!(reference.contigs().len(), 2);
    assert_eq!(reference.contigs()[0].name, "1");
    assert_eq!(reference.contigs()[0].length, Some(20));
    assert_eq!(
        reference.contigs()[0].md5.as_deref(),
        Some("a965a71aa3690f605935c54d320905ab")
    );
}

#[test]
fn bam_and_sam_headers_yield_identical_dimensions() {
    // The committed SAM text is the source of truth. If BAM parsing drifts
    // from it, this fails -- and a lossless re-encoding must not change any
    // dimension (decision D9).
    let (_dir, bam) = bam_from_sam("tiny_bare.sam");
    let from_bam = load::alignment(&bam, &Declared::default()).unwrap();
    let from_sam = load::alignment(&fixture("tiny_bare.sam"), &Declared::default()).unwrap();

    let (b, s) = (from_bam.dimensions(), from_sam.dimensions());
    assert_eq!(b.reference.value(), s.reference.value());
    assert_eq!(b.contig_naming.value(), s.contig_naming.value());
    assert_eq!(b.sample_set.value(), s.sample_set.value());
    assert_eq!(b.coordinates.value(), s.coordinates.value());
}

#[test]
fn alignments_are_one_based_closed_in_both_containers() {
    // Decision D9: the dimension describes record semantics, not the BAM
    // binary encoding. `samtools view -b` must not change it.
    let (_dir, bam) = bam_from_sam("tiny_bare.sam");

    for path in [bam.as_path(), &fixture("tiny_bare.sam")] {
        let dataset = load::alignment(path, &Declared::default()).unwrap();
        assert_eq!(
            dataset.dimensions().coordinates.value(),
            Some(&CoordinateSystem::OneBasedClosed),
            "unexpected coordinate system for {}",
            path.display()
        );
    }
}

#[test]
fn bam_infers_sample_set_from_rg_in_header_order() {
    let (_dir, bam) = bam_from_sam("tiny_two_rg.sam");
    let dataset = load::alignment(&bam, &Declared::default()).unwrap();

    let samples = dataset.dimensions().sample_set.value().unwrap();
    assert_eq!(samples.as_slice(), ["sample_a", "sample_b"]);
}

#[test]
fn bam_without_read_groups_has_unknown_not_empty_sample_set() {
    // An empty sample set would compare equal to another empty one and let a
    // real mismatch through. Unknown is the honest answer.
    let (_dir, bam) = bam_from_sam("tiny_no_rg.sam");
    let dataset = load::alignment(&bam, &Declared::default()).unwrap();

    assert!(!dataset.dimensions().sample_set.is_known());
}

#[test]
fn unaligned_bam_has_unknown_reference() {
    // No @SQ lines at all: the file carries no reference, which is different
    // from carrying the wrong one.
    let (_dir, bam) = bam_from_sam("tiny_unaligned.sam");
    let dataset = load::alignment(&bam, &Declared::default()).unwrap();

    assert!(!dataset.dimensions().reference.is_known());
    assert!(!dataset.dimensions().contig_naming.is_known());
}

#[test]
fn bam_organism_is_unknown_unless_declared() {
    // `docs/dimensions.md` says a BAM's organism comes "from reference". We
    // cannot resolve a reference to an organism, and the free-text @SQ SP tag
    // is not a safe source: matching "Homo sapiens" against a declared
    // "human" would produce false mismatches. So: unknown unless declared.
    let (_dir, bam) = bam_from_sam("tiny_bare.sam");
    let dataset = load::alignment(&bam, &Declared::default()).unwrap();

    assert!(!dataset.dimensions().organism.is_known());
}

// ------------------------------------------------------------------- VCF --

#[test]
fn vcf_infers_contigs_from_contig_header_lines() {
    let dataset = load::variants(&fixture("tiny_bare.vcf"), &Declared::default()).unwrap();

    assert_eq!(dataset.kind(), DataKind::Variants);
    let reference = dataset.dimensions().reference.value().unwrap();
    assert_eq!(reference.contigs().len(), 2);
    assert_eq!(reference.contigs()[0].name, "1");
    assert_eq!(reference.contigs()[0].length, Some(20));
    assert_eq!(
        reference.contigs()[0].md5.as_deref(),
        Some("a965a71aa3690f605935c54d320905ab")
    );
}

#[test]
fn vcf_infers_sample_set_from_header_columns_in_order() {
    let dataset = load::variants(&fixture("tiny_two_samples.vcf"), &Declared::default()).unwrap();
    let samples = dataset.dimensions().sample_set.value().unwrap();

    assert_eq!(samples.as_slice(), ["sample_a", "sample_b"]);
}

#[test]
fn vcf_is_one_based_closed() {
    let dataset = load::variants(&fixture("tiny_bare.vcf"), &Declared::default()).unwrap();
    assert_eq!(
        dataset.dimensions().coordinates.value(),
        Some(&CoordinateSystem::OneBasedClosed)
    );
}

// ----------------------------------------------------------------- FASTQ --

#[test]
fn fastq_infers_phred33_from_quality_range() {
    let dataset = load::reads(
        &fixture("reads_phred33_1.fastq"),
        None,
        &Declared::default(),
    )
    .unwrap();

    assert_eq!(dataset.kind(), DataKind::Reads);
    assert_eq!(
        dataset.dimensions().quality_encoding.value(),
        Some(&QualityEncoding::Phred33)
    );
}

#[test]
fn fastq_infers_phred64_from_quality_range() {
    let dataset = load::reads(
        &fixture("reads_phred64_1.fastq"),
        None,
        &Declared::default(),
    )
    .unwrap();

    assert_eq!(
        dataset.dimensions().quality_encoding.value(),
        Some(&QualityEncoding::Phred64)
    );
}

#[test]
fn fastq_with_ambiguous_qualities_reports_unknown_encoding() {
    // Scores in the overlap of both encodings. Guessing here would silently
    // shift every quality value by 31.
    let dataset = load::reads(
        &fixture("reads_ambiguous_1.fastq"),
        None,
        &Declared::default(),
    )
    .unwrap();

    assert!(!dataset.dimensions().quality_encoding.is_known());
}

#[test]
fn fastq_pairing_follows_whether_a_mate_file_was_given() {
    let single = load::reads(
        &fixture("reads_phred33_1.fastq"),
        None,
        &Declared::default(),
    )
    .unwrap();
    let paired = load::reads(
        &fixture("reads_phred33_1.fastq"),
        Some(&fixture("reads_phred33_2.fastq")),
        &Declared::default(),
    )
    .unwrap();

    assert_eq!(single.dimensions().pairing.value(), Some(&Pairing::Single));
    assert_eq!(paired.dimensions().pairing.value(), Some(&Pairing::Paired));
}

#[test]
fn fastq_sample_name_comes_from_the_filename_with_the_mate_suffix_stripped() {
    let dataset = load::reads(
        &fixture("reads_phred33_1.fastq"),
        Some(&fixture("reads_phred33_2.fastq")),
        &Declared::default(),
    )
    .unwrap();

    let samples = dataset.dimensions().sample_set.value().unwrap();
    assert_eq!(samples.as_slice(), ["reads_phred33"]);
}

#[test]
fn declared_sample_overrides_the_filename() {
    let dataset = load::reads(
        &fixture("reads_phred33_1.fastq"),
        None,
        &Declared {
            samples: Some(vec!["patient_07".into()]),
            ..Declared::default()
        },
    )
    .unwrap();

    let samples = dataset.dimensions().sample_set.value().unwrap();
    assert_eq!(samples.as_slice(), ["patient_07"]);
}

#[test]
fn fastq_has_no_reference() {
    let dataset = load::reads(
        &fixture("reads_phred33_1.fastq"),
        None,
        &Declared::default(),
    )
    .unwrap();
    assert!(!dataset.dimensions().reference.is_known());
}

// --------------------------------------------------- the real HXB2 input --

#[test]
fn real_hxb2_fasta_loads_with_the_expected_identity() {
    // `data/` is gitignored, so this skips rather than fails in CI.
    let Some(path) = hxb2_reference() else {
        eprintln!("skipping: run scripts/fetch_test_data.sh to fetch data/refs/HXB2.fasta");
        return;
    };

    let dataset = load::reference(
        &path,
        &Declared {
            organism: Some("HIV-1".into()),
            build: Some("HXB2".into()),
            ..Declared::default()
        },
    )
    .unwrap();

    let reference = dataset.dimensions().reference.value().unwrap();
    assert_eq!(reference.contigs().len(), 1);
    assert_eq!(reference.contigs()[0].name, "K03455.1");
    assert_eq!(reference.contigs()[0].length, Some(9719));
    assert_eq!(
        reference.contigs()[0].md5.as_deref(),
        Some("c66838498aab02f786a6ec2f2d62cdc1")
    );
    assert_eq!(reference.label(), Some("HXB2"));
    assert_eq!(
        dataset.dimensions().contig_naming.value(),
        Some(&ContigNaming::Bare)
    );
}
