//! The conversion registry, and the axes a gene list is the first to claim.
//!
//! Two things are tested here. One is the registry itself: the first edge of
//! the conversion graph D5 describes, built so that `liftover` and
//! `rename_contigs` are later rows rather than a later redesign.
//!
//! The other is M7's standing requirement. Until a kind applied `release`,
//! `subject` or `identifier_namespace`, `joined_on` filtered each of them out
//! and `check_step` never reached their arms in `compare`, so no test could
//! catch one written against the wrong field — and the compiler could not
//! either, because `Release` and `Subject` are one type and so are `Organism`
//! and `SourceSet`. A gene list claims three of those axes, so this is where
//! each one's arm, in `compare` and in `propagate`, is finally pinned.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::check::{Binding, check_step};
use ligase::convert::{Conversion, Target};
use ligase::dim::{
    Dim, Dimension, Dimensions, IdentifierNamespace, Organism, Release, Source, Subject,
};
use ligase::graph::{Argument, Converted, Graph, NodeId, StepSpec};
use ligase::types::{DataKind, Dataset};

// ----------------------------------------------------------- scaffold --

fn bind(name: &str, dataset: Dataset) -> Binding {
    Binding {
        name: name.to_owned(),
        dataset,
    }
}

fn entity_set(dimensions: Dimensions) -> Dataset {
    Dataset::new(DataKind::EntitySet, Some("genes.tsv".into()), dimensions)
}

/// A gene list declaring only the space its identifiers are in.
fn entity_set_in(namespace: &str) -> Dataset {
    entity_set(Dimensions {
        identifier_namespace: Dim::known(IdentifierNamespace::new(namespace), Source::Declared),
        ..Dimensions::default()
    })
}

/// A gene list declaring one axis, and the namespace every join needs.
fn entity_set_with(dimension: Dimension, value: &str) -> Dataset {
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
        other => panic!("{other} is not an axis a gene list claims"),
    }
    entity_set(dimensions)
}

/// A gene list declaring every axis it claims.
fn entity_set_declaring_everything() -> Dataset {
    entity_set(Dimensions {
        organism: Dim::known(Organism::new("Homo sapiens"), Source::Declared),
        identifier_namespace: Dim::known(IdentifierNamespace::new("HGNC symbol"), Source::Declared),
        subject: Dim::known(Subject::new("breast carcinoma"), Source::Declared),
        release: Dim::known(Release::new("HGNC 2026-09"), Source::Declared),
        ..Dimensions::default()
    })
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

/// A step that crosses an axis, as the Python surface builds one.
fn converting_spec(name: &str, input: NodeId, to: &str) -> StepSpec {
    let conversion = Conversion::by_name(name).unwrap();
    StepSpec {
        conversion: Some(Converted {
            conversion,
            target: conversion.target(to).unwrap(),
        }),
        ..spec(name, DataKind::EntitySet, &[("entities", input)])
    }
}

// ----------------------------------------------------------- registry --

#[test]
fn the_registry_took_a_python_bodied_entry_without_changing_shape() {
    // The claim `liftover` will depend on, pinned rather than asserted in
    // prose. `map_identifiers` is implemented in this crate;
    // `standardize` is implemented in `ligase.chem`, because computing an
    // InChIKey needs RDKit and the engine has no chemistry (D49). If the
    // registry had needed a second shape for that -- a flag, an optional
    // body, a second table -- it would not be the row-per-conversion
    // design D5 asked for, and the next two entries would each be a
    // redesign.
    //
    // So: both entries satisfy the *same* contract, and nothing in it
    // mentions a body. A field added for one of them alone would fail
    // here.
    let entries: Vec<&str> = Conversion::ALL.iter().map(|entry| entry.name).collect();
    assert_eq!(entries, ["map_identifiers", "standardize"]);

    for conversion in &Conversion::ALL {
        // Found by name, and the only way across its axis.
        assert_eq!(Conversion::by_name(conversion.name), Some(conversion));
        assert_eq!(Conversion::across(conversion.axis), Some(conversion));
        // Converts at least one kind, and says what the user supplies.
        assert!(!conversion.kinds.is_empty(), "{}", conversion.name);
        assert!(!conversion.supplies.is_empty(), "{}", conversion.name);
        // Reads its destination from the one spelling, and refuses a blank
        // -- two blanks would compare equal, so a conversion that accepted
        // one would produce two outputs agreeing about being nowhere.
        assert!(conversion.target("").is_err(), "{}", conversion.name);
    }

    // And the axis each crosses has a target type, which is the part that
    // would have needed a new arm rather than a new row. A registry entry
    // whose axis has none is an error naming the axis, not a panic, so this
    // asserts the arm exists rather than that the call happens to work.
    assert!(
        Conversion::by_name("standardize")
            .expect("registered")
            .target("inchikey; salt-stripped")
            .is_ok()
    );
}

#[test]
fn the_registry_names_the_only_way_across_each_axis_it_covers() {
    for conversion in &Conversion::ALL {
        assert_eq!(Conversion::by_name(conversion.name), Some(conversion));
        assert_eq!(Conversion::across(conversion.axis), Some(conversion));
        // The leading question is where a user is sent, so it has to name
        // the step that gets them there. A conversion added without saying
        // so in its axis's question fails here.
        assert!(
            conversion
                .axis
                .leading_question()
                .contains(&format!("`{}`", conversion.name)),
            "{}",
            conversion.name
        );
        assert!(!conversion.kinds.is_empty(), "{}", conversion.name);
        assert!(!conversion.supplies.is_empty(), "{}", conversion.name);
    }
}

#[test]
fn the_registry_is_the_shape_liftover_and_rename_contigs_will_use() {
    // v0.2 shipped one conversion and v0.3 added `standardize`, which is
    // the first evidence that the claim holds: it needed a row and nothing
    // else, even though its body is Python rather than Rust. These two are
    // the next entries and must need no more than that either -- an axis,
    // the kinds it converts, and a target read from the same parameter.
    assert_eq!(Conversion::ALL.len(), 2);
    assert_eq!(Conversion::TARGET, "to");
    assert_eq!(Conversion::across(Dimension::Reference), None);
    assert_eq!(Conversion::across(Dimension::ContigNaming), None);
}

#[test]
fn map_identifiers_converts_a_gene_list_and_nothing_else() {
    let conversion = Conversion::by_name("map_identifiers").unwrap();
    assert!(conversion.converts(DataKind::EntitySet));
    assert!(!conversion.converts(DataKind::Variants));
    assert_eq!(conversion.axis, Dimension::IdentifierNamespace);
}

#[test]
fn a_conversion_nobody_registered_is_not_one() {
    assert_eq!(Conversion::by_name("liftover"), None);
    assert_eq!(Conversion::by_name(""), None);
}

#[test]
fn a_blank_target_is_not_a_namespace() {
    let conversion = Conversion::by_name("map_identifiers").unwrap();
    assert!(matches!(
        conversion.target("Ensembl gene"),
        Ok(Target::IdentifierNamespace(_))
    ));
    assert!(conversion.target("   ").is_err());
    assert!(conversion.target("").is_err());
}

// -------------------------------------------------------- the refusal --

#[test]
fn two_gene_lists_in_different_spaces_refuse_to_join() {
    let errors = check_step(
        "score",
        &[
            bind("truth", entity_set_in("HGNC symbol")),
            bind("found", entity_set_in("Ensembl gene")),
        ],
    );
    let found: Vec<_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::IdentifierNamespace))
        .collect();
    assert_eq!(found.len(), 1, "got: {errors:#?}");
    let said = found[0].to_string();
    assert!(said.contains("HGNC symbol"), "{said}");
    assert!(said.contains("Ensembl gene"), "{said}");
    assert!(said.contains("map_identifiers"), "{said}");
    assert!(said.contains("never renamed implicitly"), "{said}");
}

#[test]
fn two_gene_lists_in_one_space_agree() {
    let errors = check_step(
        "score",
        &[
            bind("truth", entity_set_in("HGNC symbol")),
            bind("found", entity_set_in("HGNC symbol")),
        ],
    );
    assert!(errors.is_empty(), "got: {errors:#?}");
}

#[test]
fn two_gene_lists_that_each_say_nothing_are_not_allowed_to_agree() {
    // The false agreement the dimension exists to break: a namespace is in
    // `required_pair` for this pair, so silence on both sides is an error
    // rather than a join.
    let errors = check_step(
        "score",
        &[
            bind("truth", entity_set(Dimensions::default())),
            bind("found", entity_set(Dimensions::default())),
        ],
    );
    assert!(
        errors
            .iter()
            .any(|error| error.dimension() == Some(Dimension::IdentifierNamespace)),
        "got: {errors:#?}"
    );
}

// ------------------------------- M7's standing requirement, discharged --

#[test]
fn each_axis_an_entity_set_claims_compares_its_own_field() {
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
                bind("a", entity_set_with(dimension, left)),
                bind("b", entity_set_with(dimension, right)),
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
fn a_step_over_a_gene_list_carries_every_axis_it_claims() {
    let mut graph = Graph::new();
    let input = graph.add_input("genes", entity_set_declaring_everything());
    let node = graph
        .add_step(spec("filter", DataKind::EntitySet, &[("genes", input)]))
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
        // And each is re-sourced to the step, so a mistake found three
        // steps downstream can still be traced back to where it came from.
        assert_eq!(
            dimensions
                .source(dimension)
                .map(Source::describe)
                .as_deref(),
            Some("from step `filter`"),
            "{dimension}"
        );
    }
}

// -------------------------------------------------- crossing the axis --

#[test]
fn a_conversion_moves_one_axis_and_carries_the_rest() {
    let mut graph = Graph::new();
    let input = graph.add_input("genes", entity_set_declaring_everything());
    let node = graph
        .add_step(converting_spec("map_identifiers", input, "Ensembl gene"))
        .unwrap();
    let dimensions = graph.dataset(node).unwrap().dimensions();

    assert_eq!(
        dimensions.render(Dimension::IdentifierNamespace).as_deref(),
        Some("Ensembl gene")
    );
    assert_eq!(
        dimensions
            .source(Dimension::IdentifierNamespace)
            .map(Source::describe)
            .as_deref(),
        Some("from step `map_identifiers`")
    );
    // Everything else is still the input's: a conversion crosses one axis
    // and carries the others, or it would quietly forget what it was given.
    for (dimension, expected) in [
        (Dimension::Subject, "breast carcinoma"),
        (Dimension::Release, "HGNC 2026-09"),
        (Dimension::Organism, "Homo sapiens"),
    ] {
        assert_eq!(
            dimensions.render(dimension).as_deref(),
            Some(expected),
            "{dimension}"
        );
    }
}

#[test]
fn a_mapped_list_joins_the_list_it_was_mapped_to() {
    // The join refused above succeeds once a conversion is in the way,
    // which is the whole point of the dimension.
    let mut graph = Graph::new();
    let genes = graph.add_input("genes", entity_set_in("HGNC symbol"));
    let target = graph.add_input("ensembl", entity_set_in("Ensembl gene"));
    let mapped = graph
        .add_step(converting_spec("map_identifiers", genes, "Ensembl gene"))
        .unwrap();
    let scored = graph
        .add_step(spec(
            "score",
            DataKind::EntitySet,
            &[("left", mapped), ("right", target)],
        ))
        .unwrap();
    assert!(
        graph.check(&[scored]).is_empty(),
        "{:#?}",
        graph.check(&[scored])
    );
}

// ----------------------------------------------- mapping, in the engine --

mod common;

use common::fixture;
use ligase::convert;
use ligase::note::Note;
use tempfile::TempDir;

/// Map `entities_hgnc.tsv` through the committed table, into a temp file.
fn mapped(entities: &str, column: &str) -> (TempDir, std::path::PathBuf, Vec<Note>) {
    let directory = TempDir::new().unwrap();
    let out = directory.path().join("mapped.tsv");
    let notes = convert::map_identifiers(
        &fixture(entities),
        "id",
        &fixture("map_hgnc_to_ensembl.tsv"),
        "id",
        column,
        &out,
    )
    .unwrap();
    (directory, out, notes)
}

#[test]
fn mapping_rewrites_what_it_can_and_names_what_it_cannot() {
    let (_kept, out, notes) = mapped("entities_hgnc.tsv", "ensembl_gene_id");
    let written = std::fs::read_to_string(&out).unwrap();

    // The header is the input's, and the attribute columns are carried.
    assert!(written.starts_with("id\tlog2fc\n"), "{written}");
    assert!(written.contains("ENSG00000141510\t2.4"), "{written}");
    assert!(written.contains("ENSG00000012048\t-1.8"), "{written}");

    // TP53 and BRCA1 crossed. SEPT7 is in no row of the table, and NKX2-1
    // is in two, so neither is in the output under any name.
    assert!(!written.contains("TP53"), "{written}");
    assert!(!written.contains("SEPT7"), "{written}");
    assert!(!written.contains("NKX2-1"), "{written}");

    let said: Vec<String> = notes.iter().map(ToString::to_string).collect();
    assert!(
        notes.iter().any(|note| matches!(
            note,
            Note::IdentifierUnmapped { identifier } if identifier == "SEPT7"
        )),
        "{said:?}"
    );
    assert!(
        notes.iter().any(|note| matches!(
            note,
            Note::IdentifierAmbiguous { identifier } if identifier == "NKX2-1"
        )),
        "{said:?}"
    );
    // Two named, and the total beside them.
    assert_eq!(notes.len(), 3, "{said:?}");
    assert!(
        notes
            .iter()
            .any(|note| matches!(note, Note::IdentifiersDidNotCross { count: 2 })),
        "{said:?}"
    );
}

#[test]
fn a_one_to_many_mapping_does_not_quietly_make_the_set_bigger() {
    // NKX2-1 has two Ensembl genes in the table. Emitting both would change
    // the set's cardinality without a word; picking one would be a guess.
    let (_kept, out, _notes) = mapped("entities_hgnc.tsv", "ensembl_gene_id");
    let rows = std::fs::read_to_string(&out).unwrap().lines().count();
    assert_eq!(
        rows, 3,
        "a header and the two identifiers that crossed once"
    );
}

#[test]
fn a_carriage_return_does_not_stop_an_identifier_being_found() {
    // The same list, written by Excel. Its TP53 must still cross.
    let (_kept, out, _notes) = mapped("entities_crlf.tsv", "ensembl_gene_id");
    let written = std::fs::read_to_string(&out).unwrap();
    assert!(written.contains("ENSG00000141510"), "{written}");
}

#[test]
fn mapping_a_column_onto_itself_is_refused() {
    // It would map every identifier to itself and then declare a space the
    // identifiers are not in: the implicit rename D38 forbids.
    let directory = TempDir::new().unwrap();
    let error = convert::map_identifiers(
        &fixture("entities_hgnc.tsv"),
        "id",
        &fixture("map_hgnc_to_ensembl.tsv"),
        "id",
        "id",
        &directory.path().join("mapped.tsv"),
    )
    .unwrap_err();
    assert!(error.contains("own identifier column"), "{error}");
}

#[test]
fn a_mapping_table_with_no_such_column_says_which_columns_it_has() {
    let directory = TempDir::new().unwrap();
    let error = convert::map_identifiers(
        &fixture("entities_hgnc.tsv"),
        "id",
        &fixture("map_hgnc_to_ensembl.tsv"),
        "id",
        "entrez_id",
        &directory.path().join("mapped.tsv"),
    )
    .unwrap_err();
    assert!(error.contains("entrez_id"), "{error}");
    assert!(error.contains("ensembl_gene_id"), "{error}");
}

#[test]
fn a_list_with_no_identifier_column_is_refused() {
    let directory = TempDir::new().unwrap();
    let error = convert::map_identifiers(
        &fixture("entities_hgnc.tsv"),
        "symbol",
        &fixture("map_hgnc_to_ensembl.tsv"),
        "id",
        "ensembl_gene_id",
        &directory.path().join("mapped.tsv"),
    )
    .unwrap_err();
    assert!(error.contains("symbol"), "{error}");
}

#[test]
fn a_mapped_list_is_in_the_space_it_was_mapped_to() {
    // The output is not merely smaller: every identifier in it now resolves
    // to the target space, which is what makes the output's declaration of
    // that space true rather than asserted.
    use ligase::dim::KnownNamespace;

    let (_kept, out, _notes) = mapped("entities_hgnc.tsv", "ensembl_gene_id");
    let written = std::fs::read_to_string(&out).unwrap();
    let identifiers: Vec<&str> = written
        .lines()
        .skip(1)
        .filter_map(|line| line.split('\t').next())
        .collect();
    assert!(!identifiers.is_empty());
    assert_eq!(
        KnownNamespace::infer(identifiers).map(KnownNamespace::name),
        Some("Ensembl gene")
    );
}

#[test]
fn the_engine_is_a_tool_whose_version_is_its_own() {
    // A conversion's work is code in this crate, not an external program:
    // there is nothing to probe with `--version`, and the engine's own
    // version is what changes its answers. Without this, a step declaring
    // `tool="ligase"` would be probed for a binary that does not exist, and
    // nothing about the engine would reach the cache key.
    let version = ligase::exec::tool_version("ligase").unwrap();
    assert!(version.contains("ligase"), "{version}");
    assert!(version.contains(env!("CARGO_PKG_VERSION")), "{version}");
}

// ----------------------------------- what the review of M8 turned up --

#[test]
fn two_identifiers_that_arrive_at_one_gene_do_not_quietly_become_one() {
    // The mirror of the one-to-many case, and the same test applies: the
    // set's cardinality must not change without a word. `TP53` and `TP53P1`
    // both map to ENSG00000141510 here, so an output naming that gene twice
    // with two different fold changes would be a malformed set, and one
    // naming it once would be a guess about which row to keep.
    let directory = TempDir::new().unwrap();
    let out = directory.path().join("mapped.tsv");
    let notes = convert::map_identifiers(
        &fixture("entities_collide.tsv"),
        "id",
        &fixture("map_many_to_one.tsv"),
        "id",
        "ensembl_gene_id",
        &out,
    )
    .unwrap();

    let written = std::fs::read_to_string(&out).unwrap();
    let said: Vec<String> = notes.iter().map(ToString::to_string).collect();

    // BRCA1 crossed on its own and is there exactly once.
    assert_eq!(written.matches("ENSG00000012048").count(), 1, "{written}");
    // The two that collided are in neither the output nor silence.
    assert_eq!(written.matches("ENSG00000141510").count(), 0, "{written}");
    for colliding in ["TP53", "TP53P1"] {
        assert!(
            notes.iter().any(|note| matches!(
                note,
                Note::IdentifierCollides { identifier } if identifier == colliding
            )),
            "{colliding} not named in {said:?}"
        );
    }
}

#[test]
fn a_conversion_counts_what_did_not_cross_as_well_as_naming_it() {
    // Spec §5.1 asks for unmapped identifiers noticed, *counted* and listed.
    // The count is what a methods paragraph can state and what a user reads
    // first; naming four thousand identifiers and no total is not a report.
    let (_kept, _out, notes) = mapped("entities_hgnc.tsv", "ensembl_gene_id");
    let counted: Vec<&Note> = notes
        .iter()
        .filter(|note| matches!(note, Note::IdentifiersDidNotCross { .. }))
        .collect();
    assert_eq!(counted.len(), 1, "{notes:#?}");
    assert!(
        matches!(counted[0], Note::IdentifiersDidNotCross { count: 2 }),
        "{:?}",
        counted[0]
    );
    assert!(counted[0].to_string().contains('2'), "{}", counted[0]);
}

#[test]
fn naming_what_did_not_cross_is_bounded() {
    // A 20,000-row list through a table covering 80% of it produces 4,000
    // identifiers that did not cross. Naming every one of them puts 4,000
    // rows in provenance and interpolates all of them into one methods
    // sentence, so the named ones are capped and the count carries the rest.
    let directory = TempDir::new().unwrap();
    let list = directory.path().join("many.tsv");
    let table = directory.path().join("none.tsv");
    let mut rows = String::from("id\tlog2fc\n");
    for n in 0..500 {
        rows.push_str(&format!("GENE{n}\t1.0\n"));
    }
    std::fs::write(&list, rows).unwrap();
    std::fs::write(&table, "id\tensembl_gene_id\nTP53\tENSG00000141510\n").unwrap();

    let notes = convert::map_identifiers(
        &list,
        "id",
        &table,
        "id",
        "ensembl_gene_id",
        &directory.path().join("mapped.tsv"),
    )
    .unwrap();

    let named = notes
        .iter()
        .filter(|note| matches!(note, Note::IdentifierUnmapped { .. }))
        .count();
    assert!(named > 0 && named <= 20, "named {named}");
    assert!(
        notes
            .iter()
            .any(|note| matches!(note, Note::IdentifiersDidNotCross { count: 500 })),
        "{notes:#?}"
    );
}

#[test]
fn a_conversion_cannot_cross_an_axis_for_a_kind_it_does_not_convert() {
    // `Conversion::converts` exists for this. A step declaring
    // `conversion="map_identifiers"` but returning something that is not a
    // gene list would otherwise get a *known* identifier_namespace on a kind
    // whose `applies()` says it has none -- inconsistent state, even where
    // nothing reads it today.
    let mut graph = Graph::new();
    let input = graph.add_input("genes", entity_set_declaring_everything());
    let conversion = Conversion::by_name("map_identifiers").unwrap();
    let node = graph
        .add_step(StepSpec {
            conversion: Some(Converted {
                conversion,
                target: conversion.target("Ensembl gene").unwrap(),
            }),
            ..spec("misdeclared", DataKind::Variants, &[("genes", input)])
        })
        .unwrap();
    let dimensions = graph.dataset(node).unwrap().dimensions();
    assert!(!dimensions.identifier_namespace.is_known());
}
