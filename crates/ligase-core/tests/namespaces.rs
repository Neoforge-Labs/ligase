//! The identifier-namespace axis, and the catalogue inference reads.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::dim::{Dimension, KnownNamespace};

#[test]
fn the_axis_is_appended_so_earlier_reports_keep_their_order() {
    assert_eq!(
        Dimension::ALL.map(Dimension::name),
        [
            "organism",
            "reference",
            "contig_naming",
            "coordinates",
            "sample_set",
            "pairing",
            "quality_encoding",
            "release",
            "subject",
            "source_set",
            "identifier_namespace",
        ]
    );
}

#[test]
fn the_leading_question_sends_a_user_to_a_conversion_not_a_rename() {
    assert_eq!(
        Dimension::IdentifierNamespace.leading_question(),
        "These must match. Add a `map_identifiers` step; identifiers are never renamed \
         implicitly."
    );
}

#[test]
fn each_namespace_claims_its_own_identifiers() {
    for (identifier, expected) in [
        ("ENSG00000141510", "Ensembl gene"),
        ("ENSMUSG00000059552", "Ensembl gene"),
        ("P04637", "UniProt accession"),
        ("Q9Y6K9", "UniProt accession"),
        ("NM_000546", "RefSeq"),
        ("NM_000546.6", "RefSeq"),
        ("HGNC:11998", "HGNC ID"),
        ("CHEMBL1824", "ChEMBL target"),
        ("7157", "Entrez gene"),
        ("TP53", "HGNC symbol"),
        ("NKX2-1", "HGNC symbol"),
        ("SEPT7", "HGNC symbol"),
    ] {
        assert_eq!(
            KnownNamespace::resolve(identifier).map(KnownNamespace::name),
            Some(expected),
            "{identifier}"
        );
    }
}

#[test]
fn a_uniprot_accession_is_not_read_as_a_gene_symbol() {
    // The symbol shape would claim `P04637` too. It is last for this reason.
    assert_eq!(
        KnownNamespace::resolve("P04637").map(KnownNamespace::name),
        Some("UniProt accession")
    );
}

#[test]
fn a_file_whose_identifiers_agree_infers_that_namespace() {
    assert_eq!(
        KnownNamespace::infer(["ENSG00000141510", "ENSG00000012048"]).map(KnownNamespace::name),
        Some("Ensembl gene")
    );
}

#[test]
fn a_file_that_mixes_two_namespaces_infers_neither() {
    assert_eq!(KnownNamespace::infer(["TP53", "ENSG00000141510"]), None);
}

#[test]
fn a_file_with_no_identifiers_infers_nothing() {
    // Every pattern matches all zero identifiers, so a vacuous answer here
    // would declare a namespace for a file that names nothing.
    assert_eq!(KnownNamespace::infer(Vec::<&str>::new()), None);
}

#[test]
fn an_identifier_nothing_claims_stops_inference() {
    // `2-Sep` is what Excel does to SEPT7. Guessing past it would declare a
    // namespace for a file that is partly corrupt.
    assert_eq!(KnownNamespace::infer(["TP53", "2-Sep"]), None);
}
