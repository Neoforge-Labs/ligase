//! Declaring a set of organisms, and a reference's release.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use ligase::dim::Dimension;
use ligase::load::{self, Declared};

use common::fixture;

fn declared(organism: Option<Vec<String>>) -> Declared {
    Declared {
        organism,
        ..Declared::default()
    }
}

fn names(value: &[&str]) -> Option<Vec<String>> {
    Some(value.iter().map(|name| (*name).to_owned()).collect())
}

#[test]
fn a_reference_can_declare_two_organisms() {
    let found = load::reference(
        &fixture("tiny_bare.fasta"),
        &declared(names(&["HIV-1", "Homo sapiens"])),
    )
    .expect("loads");

    assert_eq!(
        found.dimensions().render(Dimension::Organism).as_deref(),
        Some("HIV-1 and Homo sapiens")
    );
}

#[test]
fn one_organism_still_renders_exactly_as_it_did() {
    let found =
        load::reference(&fixture("tiny_bare.fasta"), &declared(names(&["HIV-1"]))).expect("loads");

    assert_eq!(
        found.dimensions().render(Dimension::Organism).as_deref(),
        Some("HIV-1")
    );
}

// An empty list is a gap, not an empty set — two datasets that each
// declared nothing would otherwise *agree* on organism.
#[test]
fn an_empty_organism_list_is_unknown() {
    for empty in [names(&[]), names(&["", "  "])] {
        let found = load::reference(&fixture("tiny_bare.fasta"), &declared(empty)).expect("loads");
        assert!(!found.dimensions().organism.is_known());
        assert!(found.dimensions().reason(Dimension::Organism).is_some());
    }
}

#[test]
fn a_reference_can_declare_its_release() {
    let found = load::reference(
        &fixture("tiny_bare.fasta"),
        &Declared {
            release: Some("GRCh38.p14".into()),
            ..Declared::default()
        },
    )
    .expect("loads");

    assert_eq!(
        found.dimensions().render(Dimension::Release).as_deref(),
        Some("GRCh38.p14")
    );
}

// A blank is a gap too: a blank that compares equal to another blank is a
// false agreement.
#[test]
fn a_blank_release_is_unknown() {
    let found = load::reference(
        &fixture("tiny_bare.fasta"),
        &Declared {
            release: Some("   ".into()),
            ..Declared::default()
        },
    )
    .expect("loads");

    assert!(!found.dimensions().release.is_known());
    assert!(found.dimensions().reason(Dimension::Release).is_some());
}

#[test]
fn every_loader_accepts_a_set_of_organisms() {
    let two = names(&["HIV-1", "Homo sapiens"]);
    let expected = Some("HIV-1 and Homo sapiens".to_owned());

    let alignment =
        load::alignment(&fixture("tiny_bare.sam"), &declared(two.clone())).expect("loads");
    assert_eq!(alignment.dimensions().render(Dimension::Organism), expected);

    let variants =
        load::variants(&fixture("tiny_bare.vcf"), &declared(two.clone())).expect("loads");
    assert_eq!(variants.dimensions().render(Dimension::Organism), expected);

    let reads = load::reads(
        &fixture("reads_phred33_1.fastq"),
        None,
        &declared(two.clone()),
    )
    .expect("loads");
    assert_eq!(reads.dimensions().render(Dimension::Organism), expected);

    let intervals =
        load::intervals(&fixture("tiny_bare.bed"), None, &declared(two)).expect("loads");
    assert_eq!(intervals.dimensions().render(Dimension::Organism), expected);
}

// Three published pages quote these words, so the text and which format says
// it are both pinned in full.
#[test]
fn each_loader_says_in_its_own_words_why_the_organism_is_unknown() {
    let none = Declared::default();

    let fasta = load::reference(&fixture("tiny_bare.fasta"), &none).expect("loads");
    assert_eq!(
        fasta.dimensions().reason(Dimension::Organism),
        Some("a FASTA does not record its organism; declare it")
    );

    let alignment = load::alignment(&fixture("tiny_bare.sam"), &none).expect("loads");
    assert_eq!(
        alignment.dimensions().reason(Dimension::Organism),
        Some("not recorded in SAM/BAM headers; declare it")
    );

    let variants = load::variants(&fixture("tiny_bare.vcf"), &none).expect("loads");
    assert_eq!(
        variants.dimensions().reason(Dimension::Organism),
        Some("a VCF does not record its organism; declare it")
    );

    let reads = load::reads(&fixture("reads_phred33_1.fastq"), None, &none).expect("loads");
    assert_eq!(
        reads.dimensions().reason(Dimension::Organism),
        Some("a FASTQ does not record its organism; declare it")
    );

    let intervals = load::intervals(&fixture("tiny_bare.bed"), None, &none).expect("loads");
    assert_eq!(
        intervals.dimensions().reason(Dimension::Organism),
        Some("an interval file does not record its organism; declare it")
    );
}

// Release belongs to a reference alone. The other loaders leave it unknown
// because it does not apply to their kind, which is a different thing from a
// gap the user could fill: `applies` is what tells the two apart, since
// `Dimensions::reason` reads the field and so still carries the default
// "not set" text for a dimension that does not apply.
#[test]
fn only_a_reference_carries_a_release() {
    let none = Declared::default();

    let fasta = load::reference(&fixture("tiny_bare.fasta"), &none).expect("loads");
    assert!(fasta.kind().applies(Dimension::Release));
    assert!(!fasta.dimensions().release.is_known());
    assert_eq!(
        fasta.dimensions().reason(Dimension::Release),
        Some("a FASTA does not record which release it came from; declare it")
    );

    let others = [
        load::alignment(&fixture("tiny_bare.sam"), &none).expect("loads"),
        load::variants(&fixture("tiny_bare.vcf"), &none).expect("loads"),
        load::reads(&fixture("reads_phred33_1.fastq"), None, &none).expect("loads"),
        load::intervals(&fixture("tiny_bare.bed"), None, &none).expect("loads"),
    ];
    for other in &others {
        assert!(!other.kind().applies(Dimension::Release));
        assert!(!other.dimensions().release.is_known());
    }

    // Even when declared, a non-reference loader does not take it up.
    let declared = Declared {
        release: Some("GRCh38.p14".into()),
        ..Declared::default()
    };
    let sam = load::alignment(&fixture("tiny_bare.sam"), &declared).expect("loads");
    assert!(!sam.dimensions().release.is_known());
}
