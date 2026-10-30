//! The `RankedList` kind: a gene list that remembers its order.
//!
//! A `RankedList` is an `EntitySet` and one more thing, and the one more
//! thing is the whole milestone: its canonical form **preserves order**,
//! because for a ranking the order *is* the output
//! ([D41](https://ligase.neoforgelabs.tech/project/decisions/)). Every other
//! kind in the engine sorts its records, so the first test below is the
//! inverse of every other canonicalisation test in the suite and says so at
//! length.
//!
//! The dimensions are `EntitySet`'s, unchanged: the two kinds differ in
//! canonicalisation and in nothing else. So the refusals tested here are
//! M8's refusals reached from a new kind — a ranking in one identifier space
//! scored against a list in another, and a ranking from one release scored
//! against a list from a different one.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use ligase::canon::{self, Form};
use ligase::check::{self, Binding, check_step};
use ligase::convert::Conversion;
use ligase::dim::{
    Dim, Dimension, Dimensions, IdentifierNamespace, Organism, Release, Source, Subject,
};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::hash::ContentHash;
use ligase::types::{DataKind, Dataset};

mod common;

use common::fixture;
use ligase::load::{self, Declared};

// ----------------------------------------------------------- scaffold --

/// The canonical hash of a file read as `kind`.
fn canonical(kind: DataKind, path: &Path) -> ContentHash {
    let raw = ContentHash::of_file(path).unwrap();
    canon::hash(kind, path, raw).unwrap().hash
}

/// How a file read as `kind` was hashed.
fn form(kind: DataKind, path: &Path) -> Form {
    let raw = ContentHash::of_file(path).unwrap();
    canon::hash(kind, path, raw).unwrap().form
}

fn bind(name: &str, dataset: Dataset) -> Binding {
    Binding {
        name: name.to_owned(),
        dataset,
    }
}

/// A declaration naming the organism and nothing else.
fn declared_human() -> Declared {
    Declared {
        organism: Some(vec!["Homo sapiens".to_owned()]),
        ..Declared::default()
    }
}

/// A dataset of `kind` declaring one axis, and the namespace every join of
/// two named lists needs.
fn named_list_with(kind: DataKind, dimension: Dimension, value: &str) -> Dataset {
    let mut dimensions = Dimensions {
        identifier_namespace: Dim::known(IdentifierNamespace::new("HGNC symbol"), Source::Declared),
        ..Dimensions::default()
    };
    match dimension {
        Dimension::IdentifierNamespace => {
            dimensions.identifier_namespace =
                Dim::known(IdentifierNamespace::new(value), Source::Declared);
        }
        Dimension::Subject => {
            dimensions.subject = Dim::known(Subject::new(value), Source::Declared);
        }
        Dimension::Release => {
            dimensions.release = Dim::known(Release::new(value), Source::Declared);
        }
        Dimension::Organism => {
            dimensions.organism = Dim::known(Organism::new(value), Source::Declared);
        }
        other => panic!("{other} is not an axis a ranked list claims"),
    }
    Dataset::new(kind, Some("ranked.tsv".into()), dimensions)
}

/// A ranked list declaring every axis it claims.
fn ranked_list_declaring_everything() -> Dataset {
    Dataset::new(
        DataKind::RankedList,
        Some("ranked.tsv".into()),
        Dimensions {
            organism: Dim::known(Organism::new("Homo sapiens"), Source::Declared),
            identifier_namespace: Dim::known(
                IdentifierNamespace::new("HGNC symbol"),
                Source::Declared,
            ),
            subject: Dim::known(Subject::new("breast carcinoma"), Source::Declared),
            release: Dim::known(Release::new("HGNC 2026-09"), Source::Declared),
            ..Dimensions::default()
        },
    )
}

fn spec(name: &str, output: DataKind, inputs: &[(&str, NodeId)]) -> StepSpec {
    StepSpec {
        name: name.to_owned(),
        output,
        coordinates: None,
        conversion: None,
        arguments: inputs
            .iter()
            .map(|(param, node)| Argument {
                param: (*param).to_owned(),
                declared: output,
                node: *node,
            })
            .collect(),
    }
}

// ------------------------------------------- order is the canonical form --

#[test]
fn the_order_of_a_ranked_list_is_its_result() {
    // READ THIS BEFORE "FIXING" IT. The `assert_ne!` below is correct, and
    // it is the inverse of every other canonicalisation test in this
    // repository. Everywhere else -- alignments, variants, intervals,
    // reads, report rows, gene lists -- two files holding the same records
    // in a different order are the same file, because the order records
    // came off whatever produced them is not a fact about the biology.
    //
    // A ranking is the one kind where it is. The twelfth-ranked gene and
    // the first-ranked gene are different claims about the same two genes,
    // so a canonical form that sorted them would make two different
    // answers agree, and a cache built on that would serve the wrong one.
    // Order is the output, so order is hashed (D41).
    //
    // The contrast is asserted in the same breath, on the same two files,
    // so that this is visibly a difference between two kinds rather than a
    // difference between two files.
    let ordered = fixture("ranked_degs.tsv");
    let shuffled = fixture("ranked_degs_shuffled.tsv");

    assert_ne!(
        canonical(DataKind::RankedList, &ordered),
        canonical(DataKind::RankedList, &shuffled),
        "a ranking reordered is a different ranking"
    );
    assert_eq!(
        canonical(DataKind::EntitySet, &ordered),
        canonical(DataKind::EntitySet, &shuffled),
        "the same two files, read as a set, say the same thing"
    );
}

#[test]
fn a_ranking_and_a_set_over_one_file_are_two_different_claims() {
    // The kind is hashed into the canonical form, so reading one file two
    // ways cannot produce one hash. Without this, a step annotated to
    // return a ranking could be served the cached output of one annotated
    // to return a set -- the same bytes, a weaker promise.
    let one = fixture("ranked_degs.tsv");
    assert_ne!(
        canonical(DataKind::RankedList, &one),
        canonical(DataKind::EntitySet, &one)
    );
}

#[test]
fn a_ranked_list_with_different_columns_is_a_different_list() {
    // Which columns it carries is part of what it says, as for a gene list:
    // a ranking with a fold change is not the ranking with a p-value.
    assert_ne!(
        canonical(DataKind::RankedList, &fixture("ranked_degs.tsv")),
        canonical(DataKind::RankedList, &fixture("ranked_named_column.tsv"))
    );
}

#[test]
fn a_ranked_list_written_by_excel_means_what_a_unix_one_does() {
    // CRLF is a line ending, not part of a rank.
    assert_eq!(
        canonical(DataKind::RankedList, &fixture("ranked_crlf.tsv")),
        canonical(DataKind::RankedList, &fixture("ranked_cr_only.tsv"))
    );
}

#[test]
fn a_ragged_ranked_list_falls_back_to_its_bytes() {
    // A row the header cannot account for means this is not the table it
    // claims to be, so it hashes as itself rather than as the canonical
    // form of something it is not (D21).
    let dir = tempfile::tempdir().unwrap();
    let ragged = dir.path().join("a.tsv");
    std::fs::write(&ragged, "id\tscore\nTP53\n").unwrap();
    assert_eq!(form(DataKind::RankedList, &ragged), Form::Raw);
}

#[test]
fn what_a_ranked_list_keeps_is_described_as_keeping_its_order() {
    // `describe` is engine output a user reads to find out why two files
    // agreed or did not, so it must not claim the opposite rule from the
    // one the code implements. "Order" is the word that distinguishes this
    // kind from every other, so the description has to carry it.
    let said = canon::describe(DataKind::RankedList).unwrap();
    assert!(said.contains("order"), "{said}");
    assert!(!said.contains("sorted"), "{said}");
}

#[test]
fn a_score_column_is_compared_exactly() {
    // Spec §4.2: exact by default, and no tolerance until audit mode has
    // measured drift. This asserts something *absent*, which is why it is
    // written down -- a tolerance added on a hunch would otherwise arrive
    // silently and merge two rankings a user would treat differently.
    assert!(canon::tolerance(DataKind::RankedList, "score").is_none());
    for field in ["score", "rank", "log2fc", "id"] {
        assert!(
            canon::tolerance(DataKind::RankedList, field).is_none(),
            "{field}"
        );
    }
}

// ------------------------------------------------------- the dimensions --

#[test]
fn a_ranked_list_has_no_coordinate_system() {
    // The category error `types.rs` warns about, for the second kind that
    // holds no sequence: a ranking of genes has no coordinates, no contigs,
    // no pairing and no quality scores, and demanding one would be a gap
    // nobody can fill.
    for dimension in [
        Dimension::ContigNaming,
        Dimension::Coordinates,
        Dimension::Pairing,
        Dimension::QualityEncoding,
        Dimension::Reference,
        Dimension::SampleSet,
    ] {
        assert!(!DataKind::RankedList.applies(dimension), "{dimension}");
    }
}

#[test]
fn a_ranked_list_carries_organism_namespace_subject_and_release() {
    for dimension in [
        Dimension::Organism,
        Dimension::IdentifierNamespace,
        Dimension::Subject,
        Dimension::Release,
    ] {
        assert!(DataKind::RankedList.applies(dimension), "{dimension}");
    }
}

#[test]
fn a_ranked_list_and_a_gene_list_apply_exactly_the_same_dimensions() {
    // The point of D41 stated as a test: the two kinds differ in their
    // canonical form and in nothing else. A dimension added to one and not
    // the other would mean the difference had spread.
    for dimension in Dimension::ALL {
        assert_eq!(
            DataKind::RankedList.applies(dimension),
            DataKind::EntitySet.applies(dimension),
            "{dimension}"
        );
    }
}

#[test]
fn a_ranked_list_claims_no_source_set() {
    // M12 declined the axis and left D36's dedup question open, since spec
    // §4.2 gives a ranked list four dimensions and that is not among them.
    // M15 answered the question on the kind that needed it: a
    // `KnowledgeGraph` applies the axis and `SourceSet` now compares
    // exactly. A ranking still claims none -- it is one file, not an
    // assembly -- which is why this assertion is unchanged while the loop
    // that once accompanied it has moved to
    // `knowledge_graph.rs::source_set_is_the_axis_this_kind_was_admitted_for`.
    assert!(!DataKind::RankedList.applies(Dimension::SourceSet));
}

#[test]
fn a_ranked_list_round_trips_through_its_name() {
    assert_eq!(DataKind::RankedList.name(), "RankedList");
    assert_eq!(
        DataKind::from_name("RankedList"),
        Some(DataKind::RankedList)
    );
}

#[test]
fn a_ranked_list_is_written_as_a_table() {
    // Tools read a format off the name, so the suffix is not cosmetic.
    assert_eq!(ligase::exec::output_suffix(DataKind::RankedList), ".tsv");
}

// ---------------------------------------------------------- the refusals --

#[test]
fn a_ranked_list_in_one_space_refuses_to_join_a_gene_list_in_another() {
    // The second done-when clause. A ranked result scored against a
    // curated truth set that spells its genes differently gives a precision
    // of zero that reads as a failed method.
    let errors = check_step(
        "score",
        &[
            bind(
                "ranked",
                named_list_with(
                    DataKind::RankedList,
                    Dimension::IdentifierNamespace,
                    "HGNC symbol",
                ),
            ),
            bind(
                "truth",
                named_list_with(
                    DataKind::EntitySet,
                    Dimension::IdentifierNamespace,
                    "Ensembl gene",
                ),
            ),
        ],
    );

    let found: Vec<_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::IdentifierNamespace))
        .collect();
    assert_eq!(found.len(), 1, "{errors:#?}");
    let said = found[0].to_string();
    assert!(said.contains("HGNC symbol"), "{said}");
    assert!(said.contains("Ensembl gene"), "{said}");
    // And it says what to do about it, which is the only way across.
    assert!(said.contains("map_identifiers"), "{said}");
}

#[test]
fn a_ranked_list_refuses_to_score_against_a_gene_list_from_another_release() {
    // The third done-when clause. A symbol can mean different genes in two
    // HGNC releases, so comparing a ranking named by one against a set
    // named by another is a comparison of two different things.
    let errors = check_step(
        "score",
        &[
            bind(
                "ranked",
                named_list_with(DataKind::RankedList, Dimension::Release, "HGNC 2026-09"),
            ),
            bind(
                "truth",
                named_list_with(DataKind::EntitySet, Dimension::Release, "HGNC 2026-03"),
            ),
        ],
    );

    let found: Vec<_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::Release))
        .collect();
    assert_eq!(found.len(), 1, "{errors:#?}");
    let said = found[0].to_string();
    assert!(said.contains("HGNC 2026-09"), "{said}");
    assert!(said.contains("HGNC 2026-03"), "{said}");
}

#[test]
fn two_named_lists_must_both_say_which_space_they_are_in() {
    // Over all four pairings, because the mixed pair is the one the
    // existing `(EntitySet, _) => NONE` arm would otherwise swallow -- and
    // it is exactly the join this milestone exists to refuse. Unknown on
    // both sides is the false agreement the dimension is for: two lists
    // that say nothing would join happily and overlap in nothing.
    for left in [DataKind::RankedList, DataKind::EntitySet] {
        for right in [DataKind::RankedList, DataKind::EntitySet] {
            assert!(
                check::required_pair(left, right).contains(&Dimension::IdentifierNamespace),
                "{left:?} against {right:?}"
            );
        }
    }
}

#[test]
fn two_silent_named_lists_are_refused_rather_than_agreeing() {
    let silent = |kind| {
        Dataset::new(
            kind,
            Some("list.tsv".into()),
            Dimensions {
                identifier_namespace: Dim::unknown("not declared"),
                ..Dimensions::default()
            },
        )
    };
    let errors = check_step(
        "score",
        &[
            bind("ranked", silent(DataKind::RankedList)),
            bind("truth", silent(DataKind::EntitySet)),
        ],
    );
    assert!(
        errors
            .iter()
            .any(|error| error.dimension() == Some(Dimension::IdentifierNamespace)),
        "{errors:#?}"
    );
}

#[test]
fn a_ranked_list_demands_nothing_of_a_sequence_kind() {
    // A knowledge graph is not a sequence kind either: it names entities,
    // so the namespace is demanded between the two of them.
    for kind in DataKind::ALL {
        if !matches!(
            kind,
            DataKind::RankedList | DataKind::EntitySet | DataKind::KnowledgeGraph
        ) {
            assert!(
                check::required_pair(DataKind::RankedList, kind).is_empty(),
                "{kind:?}"
            );
            assert!(
                check::required_pair(kind, DataKind::RankedList).is_empty(),
                "{kind:?}"
            );
        }
    }
}

// ------------------------------------- M7's standing requirement, for it --

#[test]
fn each_axis_a_ranked_list_claims_compares_its_own_field() {
    // M7's standing requirement. `Release` and `Subject` are one type (and
    // `Organism` and `SourceSet` were, until M15 split them), so an arm in
    // `compare` written
    // against the wrong field compiles and only a test over a kind that
    // *applies* the axis can reach it. M8 pinned these arms for a gene
    // list; this pins that a ranked list reaches the same ones.
    for (dimension, left, right) in [
        (
            Dimension::IdentifierNamespace,
            "HGNC symbol",
            "Ensembl gene",
        ),
        (Dimension::Subject, "breast carcinoma", "lung carcinoma"),
        (Dimension::Release, "HGNC 2026-03", "HGNC 2026-09"),
        (Dimension::Organism, "Homo sapiens", "Mus musculus"),
    ] {
        let errors = check_step(
            "score",
            &[
                bind("a", named_list_with(DataKind::RankedList, dimension, left)),
                bind("b", named_list_with(DataKind::RankedList, dimension, right)),
            ],
        );
        let found: Vec<_> = errors
            .iter()
            .filter(|error| error.dimension() == Some(dimension))
            .collect();
        assert_eq!(found.len(), 1, "{dimension}: {errors:#?}");
        let said = found[0].to_string();
        assert!(said.contains(left), "{dimension}: {said}");
        assert!(said.contains(right), "{dimension}: {said}");
    }
}

#[test]
fn a_step_over_a_ranked_list_carries_every_axis_it_claims() {
    let mut graph = Graph::new();
    let input = graph.add_input("ranked", ranked_list_declaring_everything());
    let node = graph
        .add_step(spec("rerank", DataKind::RankedList, &[("ranked", input)]))
        .unwrap();
    let dimensions = graph.dataset(node).unwrap().dimensions();

    for (dimension, expected) in [
        (Dimension::IdentifierNamespace, "HGNC symbol"),
        (Dimension::Subject, "breast carcinoma"),
        (Dimension::Release, "HGNC 2026-09"),
        (Dimension::Organism, "Homo sapiens"),
    ] {
        assert_eq!(
            dimensions.render(dimension).as_deref(),
            Some(expected),
            "{dimension}"
        );
        assert_eq!(
            dimensions
                .source(dimension)
                .map(Source::describe)
                .as_deref(),
            Some("from step `rerank`"),
            "{dimension}"
        );
    }
}

#[test]
fn map_identifiers_crosses_for_a_ranked_list_too() {
    // Without this, a step declared `conversion="map_identifiers"` and
    // annotated to return a ranking would propagate the *input's*
    // namespace onto an output that has crossed, because
    // `graph::propagate` guards the override on `converts(output)`. That
    // guard exists for a kind with no such axis; a ranked list has one, so
    // leaving it out would turn a guard into a hiding place.
    let conversion = Conversion::across(Dimension::IdentifierNamespace).unwrap();
    assert!(conversion.converts(DataKind::RankedList));
    assert!(conversion.converts(DataKind::EntitySet));
}

// -------------------------------------------------------------- loading --

#[test]
fn a_ranked_list_carries_what_its_loader_read_and_what_was_declared() {
    // The space is declared here rather than read off the identifiers:
    // `ranked_degs.tsv` holds bare symbols, which state no space (D56).
    // This test is about the four axes a ranking carries, so each is
    // supplied the way a user would supply it.
    let declared = Declared {
        identifier_namespace: Some("HGNC symbol".to_owned()),
        subject: Some("breast carcinoma".to_owned()),
        release: Some("HGNC 2026-09".to_owned()),
        ..declared_human()
    };
    let loaded = load::ranked_list(&fixture("ranked_degs.tsv"), None, &declared).unwrap();

    assert_eq!(loaded.kind(), DataKind::RankedList);
    let dimensions = loaded.dimensions();
    assert_eq!(
        dimensions.render(Dimension::IdentifierNamespace).as_deref(),
        Some("HGNC symbol")
    );
    assert_eq!(
        dimensions.render(Dimension::Subject).as_deref(),
        Some("breast carcinoma")
    );
    assert_eq!(
        dimensions.render(Dimension::Release).as_deref(),
        Some("HGNC 2026-09")
    );
    assert_eq!(
        dimensions.render(Dimension::Organism).as_deref(),
        Some("Homo sapiens")
    );
}

#[test]
fn a_namespace_is_inferred_from_a_ranking_s_own_identifiers() {
    // `ranked_degs.tsv` was in this table until M19: bare symbols state no
    // space (D56), and the ranking inherits that from the loader it shares
    // with `EntitySet` — which is the point of sharing it.
    let file = "ranked_targets_ensembl.tsv";
    let loaded = load::ranked_list(&fixture(file), None, &declared_human()).unwrap();
    let dimensions = loaded.dimensions();
    assert_eq!(
        dimensions.render(Dimension::IdentifierNamespace).as_deref(),
        Some("Ensembl gene"),
        "{file}"
    );
    let source = dimensions
        .source(Dimension::IdentifierNamespace)
        .map(Source::describe)
        .unwrap_or_default();
    assert!(
        source.contains("the identifiers' own pattern"),
        "{file}: {source}"
    );
}

#[test]
fn a_ranking_with_a_header_and_no_rows_declines_to_infer() {
    // Every pattern fits all zero identifiers, so an answer here would
    // declare a space for a file that names nothing.
    let loaded =
        load::ranked_list(&fixture("ranked_header_only.tsv"), None, &declared_human()).unwrap();
    assert!(!loaded.dimensions().identifier_namespace.is_known());
    assert!(
        loaded
            .dimensions()
            .reason(Dimension::IdentifierNamespace)
            .is_some()
    );
}

#[test]
fn the_loader_notices_a_rank_whose_symbol_excel_turned_into_a_date() {
    let loaded =
        load::ranked_list(&fixture("ranked_excel_dates.tsv"), None, &declared_human()).unwrap();
    let said: Vec<String> = loaded.notes().iter().map(ToString::to_string).collect();
    assert_eq!(loaded.notes().len(), 2, "{said:?}");
    assert!(said.iter().any(|note| note.contains("2-Sep")), "{said:?}");
    assert!(said.iter().any(|note| note.contains("1-Mar")), "{said:?}");
    // And the reason the namespace is unknown is the one that tells the
    // user to re-export rather than to declare -- declaring it would make
    // the corruption permanent.
    let reason = loaded
        .dimensions()
        .reason(Dimension::IdentifierNamespace)
        .unwrap_or_default();
    assert!(reason.contains("re-export"), "{reason}");
}

#[test]
fn a_carriage_return_is_not_part_of_a_ranked_identifier() {
    // The reason rather than the value, since M19: `TP53` reaches the
    // symbol shape and declines for want of a marker (D56), where `TP53\r`
    // would reach no shape at all and decline for being unreadable. Only
    // one of those two sentences belongs to a file whose returns were
    // trimmed.
    for file in ["ranked_crlf.tsv", "ranked_cr_only.tsv"] {
        let loaded = load::ranked_list(&fixture(file), None, &declared_human()).unwrap();
        let why = loaded
            .dimensions()
            .reason(Dimension::IdentifierNamespace)
            .unwrap_or_default();
        assert!(why.contains("bare gene-symbol shapes"), "{file}: {why}");
        assert!(loaded.notes().is_empty(), "{file}");
    }
}

#[test]
fn the_identifier_column_of_a_ranking_can_be_named() {
    // A ranked table usually has a rank column of its own, so its
    // identifiers are rarely under `id`.
    let loaded = load::ranked_list(
        &fixture("ranked_named_column.tsv"),
        Some("gene"),
        &declared_human(),
    )
    .unwrap();
    // The named column was read, so its bare symbols declined for want of
    // a marker (D56) rather than for being unreadable; a column that was
    // not found would have errored before any of this.
    let why = loaded
        .dimensions()
        .reason(Dimension::IdentifierNamespace)
        .unwrap_or_default();
    assert!(why.contains("bare gene-symbol shapes"), "{why}");
}

#[test]
fn a_ranking_with_no_such_column_says_which_columns_it_has() {
    let error = load::ranked_list(&fixture("ranked_degs.tsv"), Some("gene"), &declared_human())
        .unwrap_err();
    let said = error.to_string();
    assert!(said.contains("gene"), "{said}");
    assert!(said.contains("score"), "{said}");
}

#[test]
fn a_declared_namespace_wins_over_what_a_ranking_s_identifiers_look_like() {
    let declared = Declared {
        identifier_namespace: Some("my curated panel".to_owned()),
        ..declared_human()
    };
    let loaded = load::ranked_list(&fixture("ranked_degs.tsv"), None, &declared).unwrap();
    assert_eq!(
        loaded
            .dimensions()
            .render(Dimension::IdentifierNamespace)
            .as_deref(),
        Some("my curated panel")
    );
    assert_eq!(
        loaded.dimensions().source(Dimension::IdentifierNamespace),
        Some(&Source::Declared)
    );
}
