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
            // M17's, appended for the same reason this one was: `ALL` is
            // the order `check_step` walks, so it is the order a user's
            // multi-error report comes out in, and slotting an axis in
            // beside the ones it resembles would reorder every report that
            // already exists.
            "representation",
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

// --------------------------------- identifiers a biologist actually has --

/// Every identifier here must resolve to the space it belongs to, or to
/// nothing — never to a different space.
///
/// A wrong answer in this table is the failure M8 exists to prevent, caused
/// by the engine rather than by the user: two lists that each resolved to
/// `HGNC symbol` *agree*, and a join between them succeeds with an empty
/// overlap. One file of real-world identifiers is what catches that.
#[test]
fn the_identifiers_a_biologist_actually_has_resolve_to_their_own_space() {
    for (identifier, expected) in [
        // TrEMBL accessions are ten characters, and they are most of UniProt.
        ("A0A024R161", Some("UniProt accession")),
        ("A0A087WUL8", Some("UniProt accession")),
        // An isoform of a reviewed entry.
        ("P04637-2", Some("UniProt accession")),
        // What GENCODE, GTEx and featureCounts emit.
        ("ENSG00000141510.18", Some("Ensembl gene")),
        ("ENSMUSG00000059552.9", Some("Ensembl gene")),
        // Approved symbols are not all upper case.
        ("C9orf72", Some("HGNC symbol")),
        ("C1orf43", Some("HGNC symbol")),
        // Approved readthrough symbols are longer than twelve characters.
        ("TNFSF12-TNFSF13", Some("HGNC symbol")),
        ("TMEM189-UBE2V1", Some("HGNC symbol")),
        // RefSeq is more than six prefixes.
        ("NC_000017.11", Some("RefSeq")),
        ("NG_017013", Some("RefSeq")),
        ("YP_003024035.1", Some("RefSeq")),
        // And a date is in no space at all, whatever its shape.
        ("2-Sep", None),
        ("Sep-02", None),
        ("2-Sept", None),
    ] {
        assert_eq!(
            KnownNamespace::resolve(identifier).map(KnownNamespace::name),
            expected,
            "{identifier}"
        );
    }
}

#[test]
fn a_protein_list_never_agrees_with_a_gene_symbol_list() {
    // The concrete false agreement: before the ten-character accession was
    // recognised, a TrEMBL list inferred `HGNC symbol` and joined a symbol
    // list without a word.
    assert_ne!(
        KnownNamespace::infer(["A0A024R161", "A0A087WUL8"]).map(KnownNamespace::name),
        KnownNamespace::infer(["TP53", "BRCA1"]).map(KnownNamespace::name)
    );
    assert_eq!(
        KnownNamespace::infer(["A0A024R161", "A0A087WUL8"]).map(KnownNamespace::name),
        Some("UniProt accession")
    );
}

#[test]
fn an_ordinary_human_gene_list_still_infers() {
    // `C9orf72` is in most neurology gene lists and `C1orf43` is a standard
    // housekeeping gene. A list holding either used to infer nothing, which
    // sent the user to declare a dimension for a perfectly good file.
    assert_eq!(
        KnownNamespace::infer(["TP53", "C9orf72", "BRCA1", "C1orf43"]).map(KnownNamespace::name),
        Some("HGNC symbol")
    );
}

#[test]
fn a_versioned_ensembl_list_infers() {
    assert_eq!(
        KnownNamespace::infer(["ENSG00000141510.18", "ENSG00000012048.23"])
            .map(KnownNamespace::name),
        Some("Ensembl gene")
    );
}
