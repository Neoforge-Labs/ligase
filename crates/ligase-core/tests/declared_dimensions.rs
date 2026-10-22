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
