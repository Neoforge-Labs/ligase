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
    // v0.2 ships one conversion. These two are the next entries and must
    // need no change to the registry beyond a row: an axis, the kinds it
    // converts, and a target read from the same parameter.
    assert_eq!(Conversion::ALL.len(), 1);
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
