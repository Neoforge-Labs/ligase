//! M1: what makes two references the same reference.
//!
//! This is the heart of the milestone.
//! `web/content/reference/dimensions.mdx`: identity is contig names plus
//! lengths plus MD5 where available, *never* the filename. Decision D10: two
//! references compared with each other must be identical, but data checked
//! against a reference need only be contained in it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::dim::{Contig, ContigNaming, ReferenceId, Relation, Verdict, relate};

fn contig(name: &str, length: u64, md5: Option<&str>) -> Contig {
    Contig {
        name: name.to_owned(),
        length: Some(length),
        md5: md5.map(str::to_owned),
    }
}

/// A contig whose length the file did not record, as a `##contig=<ID=1>`
/// line leaves it.
fn lengthless(name: &str, md5: Option<&str>) -> Contig {
    Contig {
        name: name.to_owned(),
        length: None,
        md5: md5.map(str::to_owned),
    }
}

const M1: &str = "a965a71aa3690f605935c54d320905ab";
const M2: &str = "89c2d6e069f386bafb1a0c811ba821f4";
const M3: &str = "61bb649e97c09f49b40b225b9f863777";
const M1_OTHER: &str = "44e7acaffac988f931debb037a014055";

fn bare() -> ReferenceId {
    ReferenceId::new(
        None,
        vec![contig("1", 20, Some(M1)), contig("2", 20, Some(M2))],
    )
}

fn prefixed() -> ReferenceId {
    ReferenceId::new(
        None,
        vec![contig("chr1", 20, Some(M1)), contig("chr2", 20, Some(M2))],
    )
}

// ------------------------------------------------------------- identity --

#[test]
fn identity_ignores_the_label() {
    // A label is for humans. Two references with different labels and the
    // same contigs are the same reference; the converse is the dangerous
    // case and is covered below.
    let labelled = ReferenceId::new(Some("hg38".into()), bare().contigs().to_vec());
    let differently_labelled = ReferenceId::new(Some("GRCh38".into()), bare().contigs().to_vec());

    assert_eq!(
        relate(&labelled, &differently_labelled, Relation::Identical),
        Verdict::Agree
    );
}

#[test]
fn same_names_and_lengths_but_different_md5_is_a_reference_mismatch() {
    // The silent killer: two files both called hg38, both with chr1 at the
    // same length, holding different sequence. Only the checksum tells them
    // apart, which is why identity cannot rest on names alone.
    let left = ReferenceId::new(
        Some("hg38".into()),
        vec![contig("1", 20, Some(M1)), contig("2", 20, Some(M2))],
    );
    let right = ReferenceId::new(
        Some("hg38".into()),
        vec![contig("1", 20, Some(M1_OTHER)), contig("2", 20, Some(M2))],
    );

    match relate(&left, &right, Relation::Identical) {
        Verdict::ContigConflict { name, .. } => assert_eq!(name, "1"),
        other => panic!("expected a contig conflict, got {other:?}"),
    }
}

#[test]
fn same_names_but_different_lengths_is_a_reference_mismatch() {
    let left = ReferenceId::new(None, vec![contig("1", 20, None)]);
    let right = ReferenceId::new(None, vec![contig("1", 21, None)]);

    match relate(&left, &right, Relation::Identical) {
        Verdict::ContigConflict { name, .. } => assert_eq!(name, "1"),
        other => panic!("expected a contig conflict, got {other:?}"),
    }
}

#[test]
fn a_missing_length_on_one_side_is_not_invented() {
    // `##contig=<ID=1>` with no length is legal. A zero stored in its place
    // would compare equal to another zero and wave a real mismatch through.
    let sized = ReferenceId::new(None, vec![contig("1", 20, Some(M1))]);
    let unsized_ = ReferenceId::new(None, vec![lengthless("1", Some(M1))]);

    assert_eq!(
        relate(&sized, &unsized_, Relation::Identical),
        Verdict::Agree
    );

    // The checksum still has to agree.
    let wrong = ReferenceId::new(None, vec![lengthless("1", Some(M1_OTHER))]);
    assert!(matches!(
        relate(&sized, &wrong, Relation::Identical),
        Verdict::ContigConflict { .. }
    ));
}

#[test]
fn a_missing_md5_on_one_side_falls_back_to_name_and_length() {
    // VCF ##contig lines often omit md5. Refusing to compare at all would
    // make the check useless; pretending the absent digest matches would be
    // a lie. Comparing what we have, and saying so, is the honest middle.
    let with_md5 = ReferenceId::new(None, vec![contig("1", 20, Some(M1))]);
    let without = ReferenceId::new(None, vec![contig("1", 20, None)]);

    assert_eq!(
        relate(&with_md5, &without, Relation::Identical),
        Verdict::Agree
    );
}

// --------------------------------------------------------- contig naming --

#[test]
fn chr_prefixes_are_a_naming_difference_not_a_reference_difference() {
    // The acceptance criterion: `chr1` against `1` must be distinct from a
    // reference mismatch, because the fix is a renaming step, not a
    // different reference.
    match relate(&prefixed(), &bare(), Relation::Identical) {
        Verdict::NamingDiffers { left, right, .. } => {
            assert_eq!(left, ContigNaming::Prefixed);
            assert_eq!(right, ContigNaming::Bare);
        }
        other => panic!("expected a naming difference, got {other:?}"),
    }
}

#[test]
fn naming_style_is_detected_from_the_contig_names() {
    assert_eq!(bare().naming(), ContigNaming::Bare);
    assert_eq!(prefixed().naming(), ContigNaming::Prefixed);

    let mixed = ReferenceId::new(
        None,
        vec![contig("chr1", 20, Some(M1)), contig("2", 20, Some(M2))],
    );
    assert_eq!(mixed.naming(), ContigNaming::Mixed);
}

#[test]
fn chr_m_and_mt_name_the_same_contig_only_when_the_sequence_agrees() {
    // UCSC calls it chrM, Ensembl calls it MT. They are the same contig in
    // GRCh38 -- but in hg19 chrM is the older NC_001807 at 16571 bp while
    // GRCh37's MT is the rCRS at 16569 bp. Normalising the name is only safe
    // because length and MD5 are still compared.
    let ucsc = ReferenceId::new(None, vec![contig("chrM", 16569, Some(M3))]);
    let ensembl = ReferenceId::new(None, vec![contig("MT", 16569, Some(M3))]);
    assert!(matches!(
        relate(&ucsc, &ensembl, Relation::Identical),
        Verdict::NamingDiffers { .. }
    ));

    let hg19 = ReferenceId::new(None, vec![contig("chrM", 16571, Some(M1))]);
    match relate(&hg19, &ensembl, Relation::Identical) {
        Verdict::ContigConflict { .. } => {}
        other => panic!("differing mitochondrial sequence must not pass as naming: {other:?}"),
    }
}

// ------------------------------------------------------------ containment --

#[test]
fn data_mentioning_a_subset_of_contigs_is_contained_in_the_reference() {
    // Decision D10: a VCF whose header lists only chr20 is correct against a
    // whole-genome reference. Rejecting it would be a false alarm.
    let data = ReferenceId::new(None, vec![contig("1", 20, Some(M1))]);
    let whole = ReferenceId::new(
        None,
        vec![
            contig("1", 20, Some(M1)),
            contig("2", 20, Some(M2)),
            contig("3", 20, Some(M3)),
        ],
    );

    assert_eq!(relate(&data, &whole, Relation::DataWithin), Verdict::Agree);
}

#[test]
fn a_subset_is_not_identical_when_two_references_are_compared() {
    let subset = ReferenceId::new(None, vec![contig("1", 20, Some(M1))]);

    match relate(&subset, &bare(), Relation::Identical) {
        Verdict::MissingContig { name, .. } => assert_eq!(name, "2"),
        other => panic!("expected a missing contig, got {other:?}"),
    }
}

#[test]
fn a_contig_absent_from_the_reference_is_a_mismatch() {
    let data = ReferenceId::new(
        None,
        vec![contig("1", 20, Some(M1)), contig("9", 20, Some(M3))],
    );

    match relate(&data, &bare(), Relation::DataWithin) {
        Verdict::MissingContig { name, .. } => assert_eq!(name, "9"),
        other => panic!("expected a missing contig, got {other:?}"),
    }
}

#[test]
fn containment_still_checks_the_checksum_of_shared_contigs() {
    // Containment is not permission to skip verification: a contig present
    // under the same name with different sequence is still wrong.
    let data = ReferenceId::new(None, vec![contig("1", 20, Some(M1_OTHER))]);

    match relate(&data, &bare(), Relation::DataWithin) {
        Verdict::ContigConflict { name, .. } => assert_eq!(name, "1"),
        other => panic!("expected a contig conflict, got {other:?}"),
    }
}

// ------------------------------------------------------------------ order --

#[test]
fn reordered_contigs_are_their_own_verdict() {
    // htslib matches contigs by name so bcftools does not care, but GATK
    // requires matching sequence dictionary order. Reporting this as a
    // reference mismatch would be misleading; staying silent would hide it.
    let reordered = ReferenceId::new(
        None,
        vec![contig("2", 20, Some(M2)), contig("1", 20, Some(M1))],
    );

    match relate(&reordered, &bare(), Relation::Identical) {
        Verdict::OrderDiffers { .. } => {}
        other => panic!("expected an order difference, got {other:?}"),
    }
}

#[test]
fn containment_of_a_subset_in_a_different_order_still_flags_the_order() {
    let data = ReferenceId::new(
        None,
        vec![contig("3", 20, Some(M3)), contig("1", 20, Some(M1))],
    );
    let whole = ReferenceId::new(
        None,
        vec![
            contig("1", 20, Some(M1)),
            contig("2", 20, Some(M2)),
            contig("3", 20, Some(M3)),
        ],
    );

    match relate(&data, &whole, Relation::DataWithin) {
        Verdict::OrderDiffers { .. } => {}
        other => panic!("expected an order difference, got {other:?}"),
    }
}

// ------------------------------------------------------------ description --

#[test]
fn a_label_describes_a_reference_when_one_was_declared() {
    let labelled = ReferenceId::new(
        Some("HXB2".into()),
        vec![contig("K03455.1", 9719, Some(M1))],
    );
    assert_eq!(labelled.describe(), "HXB2");
}

#[test]
fn an_unlabelled_reference_describes_itself_by_its_contigs() {
    let single = ReferenceId::new(None, vec![contig("K03455.1", 9719, Some(M1))]);
    assert_eq!(single.describe(), "K03455.1 (9719 bp)");

    assert_eq!(bare().describe(), "2 contigs, 40 bp");
}

#[test]
fn fingerprints_distinguish_references_that_describe_themselves_the_same_way() {
    // Two files both labelled hg38 must not render identically in an error
    // message, or the message cannot be acted on.
    let left = ReferenceId::new(Some("hg38".into()), vec![contig("1", 20, Some(M1))]);
    let right = ReferenceId::new(Some("hg38".into()), vec![contig("1", 20, Some(M1_OTHER))]);

    assert_eq!(left.describe(), right.describe());
    assert_ne!(left.fingerprint(), right.fingerprint());
}

#[test]
fn a_fingerprint_is_stable_across_naming_style() {
    // The fingerprint is identity, and naming style is a separate dimension,
    // so a chr-prefixed reference fingerprints the same as its bare twin.
    assert_eq!(bare().fingerprint(), prefixed().fingerprint());
}
