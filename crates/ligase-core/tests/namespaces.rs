//! The identifier-namespace axis, and the catalogue inference reads.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::dim::{Dimension, Inference, KnownNamespace};

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
        KnownNamespace::infer(["ENSG00000141510", "ENSG00000012048"])
            .settled()
            .map(KnownNamespace::name),
        Some("Ensembl gene")
    );
}

#[test]
fn a_file_that_mixes_two_namespaces_infers_neither() {
    assert_eq!(
        KnownNamespace::infer(["TP53", "ENSG00000141510"]).settled(),
        None
    );
}

#[test]
fn a_file_with_no_identifiers_infers_nothing() {
    // Every pattern matches all zero identifiers, so a vacuous answer here
    // would declare a namespace for a file that names nothing.
    assert_eq!(KnownNamespace::infer(Vec::<&str>::new()).settled(), None);
}

#[test]
fn an_identifier_nothing_claims_stops_inference() {
    // `2-Sep` is what Excel does to SEPT7. Guessing past it would declare a
    // namespace for a file that is partly corrupt.
    assert_eq!(KnownNamespace::infer(["TP53", "2-Sep"]).settled(), None);
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
        KnownNamespace::infer(["A0A024R161", "A0A087WUL8"])
            .settled()
            .map(KnownNamespace::name),
        KnownNamespace::infer(["TP53", "BRCA1"])
            .settled()
            .map(KnownNamespace::name)
    );
    assert_eq!(
        KnownNamespace::infer(["A0A024R161", "A0A087WUL8"])
            .settled()
            .map(KnownNamespace::name),
        Some("UniProt accession")
    );
}

#[test]
fn an_ordinary_human_gene_list_is_claimed_but_not_inferred() {
    // `C9orf72` is in most neurology gene lists and `C1orf43` is a standard
    // housekeeping gene, and the symbol *shape* must keep claiming both:
    // that is what lets inference say "these are bare symbols" rather than
    // "these hold a name I cannot read", and the two want different advice.
    for identifier in ["TP53", "C9orf72", "BRCA1", "C1orf43"] {
        assert_eq!(
            KnownNamespace::resolve(identifier).map(KnownNamespace::name),
            Some("HGNC symbol"),
            "{identifier}"
        );
    }

    // What changed in M19: this list used to *infer* `HGNC symbol`, and the
    // inference was a guess dressed as a fact. `DHFR-TS` is ChEMBL's name
    // for a bifunctional enzyme and fits this shape exactly, so a guess of
    // `HGNC symbol` over a synonym space is both confident and wrong -- and
    // two such guesses agreeing is how an undeclared cross-space join came
    // to pass `check` with exit 0 (D56).
    assert_eq!(
        KnownNamespace::infer(["TP53", "C9orf72", "BRCA1", "C1orf43"]).settled(),
        None
    );
}

#[test]
fn a_versioned_ensembl_list_infers() {
    assert_eq!(
        KnownNamespace::infer(["ENSG00000141510.18", "ENSG00000012048.23"])
            .settled()
            .map(KnownNamespace::name),
        Some("Ensembl gene")
    );
}

// ------------------- the shape that cannot discriminate, which is D56 --

#[test]
fn the_catalogue_says_what_tells_each_shape_from_the_others() {
    // The marker is a field rather than a remark in the module comment,
    // because `infer` reads it: a shape with no marker cannot tell its own
    // space from the several that look like it, so inference over one
    // declines instead of answering. Exactly one entry carries none, and it
    // is the fallback the resolution order already put last.
    let unmarked: Vec<&'static str> = KnownNamespace::ALL
        .iter()
        .filter(|namespace| !namespace.discriminates())
        .map(KnownNamespace::name)
        .collect();
    assert_eq!(unmarked, ["HGNC symbol"]);
    assert_eq!(
        KnownNamespace::ALL.last().map(KnownNamespace::name),
        Some("HGNC symbol"),
        "the shape with no marker is the one that must be tried last"
    );
}

#[test]
fn a_list_of_bare_gene_symbols_infers_nothing() {
    // Measured by a consumer against 0.3.0: `DHFR-TS` and `PPPK-DHPS` are
    // ChEMBL component synonyms for *P. falciparum* bifunctional enzymes,
    // and 0.3.0 called them `HGNC symbol` with no hedge. They are not HGNC
    // symbols. No matcher can separate them from `MT-CYB`, `HLA-A` or
    // `NAT8B-AS1`, which are, so the honest answer is that the identifiers
    // do not settle the question.
    assert_eq!(
        KnownNamespace::infer(["DHFR-TS", "PPPK-DHPS"]).settled(),
        None
    );
    assert_eq!(KnownNamespace::infer(["TP53", "BRCA1"]).settled(), None);
}

#[test]
fn inference_distinguishes_a_shape_it_cannot_discriminate_from_names_it_cannot_read() {
    // Two declines with two different remedies, so they are two answers
    // rather than one `None`. Bare symbols want "declare the space"; a file
    // holding `2-Sep` wants "re-export the column as text", and declaring a
    // space over that file would make the corruption permanent. The loaders
    // read this to choose which sentence a user gets.
    assert!(matches!(
        KnownNamespace::infer(["DHFR-TS", "PPPK-DHPS"]),
        Inference::Indistinguishable(shape) if shape.name() == "HGNC symbol"
    ));
    assert!(matches!(
        KnownNamespace::infer(["TP53", "2-Sep"]),
        Inference::Unsettled
    ));
    assert!(matches!(
        KnownNamespace::infer(["TP53", "ENSG00000141510"]),
        Inference::Unsettled
    ));
    assert!(matches!(
        KnownNamespace::infer(Vec::<&str>::new()),
        Inference::Unsettled
    ));
}

#[test]
fn every_marker_bearing_shape_still_infers() {
    // The convenience is kept wherever it is sound. Each of these spaces is
    // recognisable by a prefix, an underscore, a colon, or by being digits
    // throughout, and a list of them states its own space.
    for (identifiers, expected) in [
        (["ENSG00000141510", "ENSG00000012048"], "Ensembl gene"),
        (["P04637", "Q9Y6K9"], "UniProt accession"),
        (["NM_000546", "NM_007294"], "RefSeq"),
        (["HGNC:11998", "HGNC:1100"], "HGNC ID"),
        (["CHEMBL1824", "CHEMBL203"], "ChEMBL target"),
        (["7157", "672"], "Entrez gene"),
    ] {
        assert_eq!(
            KnownNamespace::infer(identifiers)
                .settled()
                .map(KnownNamespace::name),
            Some(expected),
            "{identifiers:?}"
        );
    }
}
