//! M9: describing one file, with nothing to run.
//!
//! `ligase inspect` is the ten-second trial on files a scientist already has,
//! so what it says about a file is the first thing about Ligase most readers
//! will ever see. Two properties of that text are load-bearing enough to be
//! pinned here rather than left to the surface that prints it.
//!
//! The first is the **three states**. A dimension that applies and is known
//! is shown with its value and its source; one that applies and is unknown is
//! shown with the engine's reason; one that does not apply to the kind at all
//! is neither. The last is the trap: `Dimensions::reason` answers for an
//! inapplicable dimension as readily as for an applicable one, so a surface
//! that forgets to ask `DataKind::applies` first will report a FASTA as
//! *missing* a quality encoding, which is a category error rather than a gap.
//!
//! The second is that a description line and a diagnostic line are the **same
//! line**. `describe` renders through the helpers `check` renders its value
//! lines with, so the diagnostic a reader gets after the descriptions is
//! visibly a selection of the lines above it. That holds by construction, and
//! `a_description_line_is_the_line_a_diagnostic_quotes` is what stops the two
//! drifting apart again.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::fixture;
use ligase::check::{Binding, check_step};
use ligase::inspect::describe;
use ligase::load::{self, Declared};

/// A named binding over a FASTA, as a file on a command line becomes one.
fn reference(name: &str, file: &str) -> Binding {
    Binding {
        name: name.to_owned(),
        dataset: load::reference(&fixture(file), &Declared::default())
            .expect("the fixture should load"),
    }
}

fn alignment(name: &str, file: &str) -> Binding {
    Binding {
        name: name.to_owned(),
        dataset: load::alignment(&fixture(file), &Declared::default())
            .expect("the fixture should load"),
    }
}

fn entity_set(name: &str, file: &str) -> Binding {
    Binding {
        name: name.to_owned(),
        dataset: load::entity_set(&fixture(file), None, &Declared::default())
            .expect("the fixture should load"),
    }
}

#[test]
fn a_known_dimension_is_shown_with_its_value_and_its_source() {
    let described = describe(&reference("ref", "tiny_bare.fasta"));

    assert!(
        described.contains("ref.reference = 2 contigs, 40 bp (from FASTA contigs:"),
        "the value and where it came from, both: {described}"
    );
    assert!(
        described.contains("ref.contig_naming = bare (Ensembl/NCBI style) (from FASTA deflines:"),
        "naming is read off the deflines and says so: {described}"
    );
}

#[test]
fn an_applicable_dimension_that_is_unknown_is_shown_with_the_reason() {
    let described = describe(&reference("ref", "tiny_bare.fasta"));

    assert!(
        described
            .contains("ref.organism is unknown (a FASTA does not record its organism; declare it)"),
        "the reason is the actionable part: {described}"
    );
    assert!(
        described.contains("ref.release is unknown ("),
        "a reference has a release, and this one does not state it: {described}"
    );
}

#[test]
fn a_dimension_the_kind_has_no_business_with_is_never_called_unknown() {
    let described = describe(&reference("ref", "tiny_bare.fasta"));

    for dimension in [
        "quality_encoding",
        "sample_set",
        "pairing",
        "coordinates",
        "subject",
        "source_set",
        "identifier_namespace",
    ] {
        assert!(
            !described.contains(&format!("ref.{dimension} is unknown")),
            "a FASTA is not *missing* {dimension}; it has no such property \
             to have, and calling it unknown invites a user to go and declare \
             one: {described}"
        );
    }
}

#[test]
fn the_dimensions_that_do_not_apply_are_named_once_as_not_applying() {
    let described = describe(&reference("ref", "tiny_bare.fasta"));

    let line = described
        .lines()
        .find(|line| line.contains("do not apply"))
        .expect("the inapplicable dimensions are named: {described}");

    for dimension in [
        "coordinates",
        "sample_set",
        "pairing",
        "quality_encoding",
        "subject",
        "source_set",
        "identifier_namespace",
    ] {
        assert!(
            line.contains(dimension),
            "{dimension} does not apply to a Reference and belongs on this line: {line}"
        );
    }
    assert!(
        line.contains("Reference"),
        "the line says which kind they do not apply to: {line}"
    );
}

#[test]
fn a_gene_list_is_not_described_as_missing_a_reference() {
    // An Ensembl list rather than a symbol one, because since M19 a bare
    // symbol list states no space (D56) and this test is about what a gene
    // list is *not* asked for. The symbol case has its own test below.
    let described = describe(&entity_set("genes", "entities_ensembl.tsv"));

    assert!(
        described.contains("genes.identifier_namespace = Ensembl gene"),
        "the dimension a gene list exists for, read off the identifiers: {described}"
    );
    assert!(
        !described.contains("genes.reference is unknown"),
        "a list of gene names has no contigs to be missing: {described}"
    );
    assert!(
        !described.contains("genes.contig_naming is unknown"),
        "nor a naming convention for them: {described}"
    );
}

#[test]
fn a_bare_symbol_list_is_described_as_unknown_rather_than_guessed() {
    // What a consumer sees first, and the whole of what #32 asked for: the
    // command that reads a file's dimensions says it cannot tell, in the
    // language it already has for an unknown dimension, instead of printing
    // `HGNC symbol` as though the file had said so.
    let described = describe(&entity_set("genes", "entities_hgnc.tsv"));

    assert!(
        !described.contains("genes.identifier_namespace = HGNC symbol"),
        "0.3.0 printed this as a fact: {described}"
    );
    assert!(
        described.contains("genes.identifier_namespace is unknown"),
        "{described}"
    );
    assert!(
        described.contains("indistinguishable by pattern"),
        "the reason has to say why no pattern can settle it: {described}"
    );
}

#[test]
fn what_the_loader_noticed_is_part_of_the_description() {
    let described = describe(&entity_set("genes", "entities_excel_dates.tsv"));

    assert!(
        described.contains("is a date, not a name"),
        "a spreadsheet ate this file's symbols, and the whole point of \
         inspecting a file you already have is to be told so: {described}"
    );
}

#[test]
fn the_heading_names_the_kind_and_the_file() {
    let described = describe(&reference("ref", "tiny_bare.fasta"));
    let heading = described.lines().next().expect("a non-empty description");

    assert!(
        heading.contains("Reference"),
        "which kind it was read as: {heading}"
    );
    assert!(
        heading.contains("tiny_bare.fasta"),
        "and which file: {heading}"
    );
}

#[test]
fn a_description_line_is_the_line_a_diagnostic_quotes() {
    // The invariant the shared helpers exist for: what `inspect` prints about
    // a dimension, and what a diagnostic prints about that same dimension of
    // that same file, are one line rendered once. A reader who has read the
    // description has already read the error.
    let bare = reference("ref", "tiny_bare.fasta");
    let prefixed = alignment("aln", "tiny_prefixed.sam");

    let described = describe(&bare);
    let errors = check_step("inspect", &[bare, prefixed]);
    let [error] = errors.as_slice() else {
        panic!("one naming disagreement, got {}", errors.len());
    };

    let quoted = error
        .to_string()
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("ref.contig_naming"))
        .expect("the diagnostic names the reference's naming")
        .to_owned();

    assert!(
        described.lines().any(|line| line.trim() == quoted),
        "the diagnostic's line `{quoted}` should appear verbatim in the \
         description:\n{described}"
    );
}
