//! The `KnowledgeGraph` kind, and the axis three milestones deferred to it.
//!
//! A knowledge graph is the first kind in Ligase that is *assembled* rather
//! than measured. What it holds is a function of which evidence sources were
//! folded into it, and no node or edge says what that list was — so two
//! graphs that differ only in their source composition diff, score and join
//! against each other without complaint, and the difference between them
//! reads as biology. That is the silent failure `source_set` exists to
//! break, and it is why this kind is admitted under
//! [D35](https://ligase.neoforgelabs.tech/project/decisions/).
//!
//! # M7's standing requirement lands here
//!
//! The first kind to claim an axis must pin that axis's `check::compare` and
//! `graph::propagate` arms with a named test, because until a kind applies
//! an axis `check_step` never reaches its arms. M8 discharged this for
//! `identifier_namespace`, `subject` and `release`; M8, M10 and M12 each
//! declined `source_set` and recorded that the obligation travels here. The
//! two tests under "M7's standing requirement" below are that obligation
//! met.
//!
//! # And so does D36's reserved right
//!
//! The shared set type folds members differing only in ASCII case, which is
//! right for Latin binomials and wrong for object keys. M10a settled that
//! `source_set` must **not** fold, and reserved the split for the milestone
//! that first applies the axis. `a_source_set_compares_exactly` and
//! `an_organism_still_folds_ascii_case` are the two halves of that split,
//! and they are a pair on purpose: the point is not that one rule changed
//! but that the two axes now have different rules.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use ligase::canon::{self, Form};
use ligase::check::{self, Binding, check_step};
use ligase::dim::{
    Dim, Dimension, Dimensions, IdentifierNamespace, Organism, Release, Source, SourceSet, Subject,
};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::hash::ContentHash;
use ligase::load::{self, Declared};
use ligase::note::Note;
use ligase::types::{DataKind, Dataset};

mod common;

use common::fixture;

// ----------------------------------------------------------- scaffold --

/// The canonical hash of a file read as `kind`.
fn canonical(kind: DataKind, path: &Path) -> ContentHash {
    let raw = ContentHash::of_file(path).unwrap();
    canon::hash(kind, path, raw).unwrap().hash
}

/// How a file read as `kind` was hashed, and what that noticed.
fn hashed(kind: DataKind, path: &Path) -> (Form, Vec<Note>) {
    let raw = ContentHash::of_file(path).unwrap();
    let canonical = canon::hash(kind, path, raw).unwrap();
    (canonical.form, canonical.notes)
}

fn bind(name: &str, dataset: Dataset) -> Binding {
    Binding {
        name: name.to_owned(),
        dataset,
    }
}

/// Load a committed graph fixture, declaring nothing.
fn graph(name: &str) -> Dataset {
    load::knowledge_graph(&fixture(name), &Declared::default()).unwrap()
}

/// A graph carrying one axis's value, and the namespace every join of two
/// entity-naming kinds needs.
fn graph_with(dimension: Dimension, value: &str) -> Dataset {
    let mut dimensions = Dimensions {
        identifier_namespace: Dim::known(
            IdentifierNamespace::new("Ensembl gene"),
            Source::Declared,
        ),
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
        Dimension::SourceSet => {
            dimensions.source_set = Dim::known(SourceSet::new(value), Source::Declared);
        }
        other => panic!("{other} is not an axis a knowledge graph claims"),
    }
    Dataset::new(DataKind::KnowledgeGraph, None, dimensions)
}

/// A graph declaring every axis it claims, for the propagation test.
fn graph_declaring_everything() -> Dataset {
    Dataset::new(
        DataKind::KnowledgeGraph,
        None,
        Dimensions {
            organism: Dim::known(Organism::new("Homo sapiens"), Source::Declared),
            identifier_namespace: Dim::known(
                IdentifierNamespace::new("Ensembl gene"),
                Source::Declared,
            ),
            subject: Dim::known(Subject::new("breast carcinoma"), Source::Declared),
            release: Dim::known(Release::new("Open Targets 24.09"), Source::Declared),
            source_set: Dim::known(
                SourceSet::of(["open-targets", "disgenet"]),
                Source::Declared,
            ),
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

// -------------------------------------------------------- the dimensions --

#[test]
fn a_knowledge_graph_has_no_coordinate_system() {
    // The category error `types.rs` warns about. A graph of genes and
    // diseases has no contigs, no coordinate convention, no read pairing,
    // no quality scores and no samples -- not as gaps to be filled, but
    // because a graph has no such properties to have.
    for dimension in [
        Dimension::ContigNaming,
        Dimension::Coordinates,
        Dimension::Pairing,
        Dimension::QualityEncoding,
        Dimension::Reference,
        Dimension::SampleSet,
    ] {
        assert!(!DataKind::KnowledgeGraph.applies(dimension), "{dimension}");
    }
}

#[test]
fn a_knowledge_graph_claims_five_axes() {
    for dimension in [
        Dimension::Organism,
        Dimension::IdentifierNamespace,
        Dimension::Subject,
        Dimension::Release,
        Dimension::SourceSet,
    ] {
        assert!(DataKind::KnowledgeGraph.applies(dimension), "{dimension}");
    }
}

#[test]
fn source_set_is_the_axis_this_kind_was_admitted_for() {
    // The axis M8, M10 and M12 each declined and recorded as travelling
    // here. It is this kind's alone: nothing that holds sequence is
    // assembled from sources, and the two named lists are read from one
    // file each. D35's test for the kind is that it brings an axis no other
    // kind brings, so a second kind appearing here is a question, not a
    // detail.
    for kind in DataKind::ALL {
        assert_eq!(
            kind.applies(Dimension::SourceSet),
            kind == DataKind::KnowledgeGraph,
            "{kind:?}"
        );
    }
}

#[test]
fn two_graphs_must_both_say_which_space_they_are_named_in() {
    // M12's lesson, applied before it can become a gap: the mixed pairs are
    // written out, not left to a catch-all. Joining a graph to a curated
    // gene list is the commonest reason to join those two kinds at all, and
    // a catch-all would have demanded nothing of it.
    for pair in [
        (DataKind::KnowledgeGraph, DataKind::KnowledgeGraph),
        (DataKind::KnowledgeGraph, DataKind::EntitySet),
        (DataKind::EntitySet, DataKind::KnowledgeGraph),
        (DataKind::KnowledgeGraph, DataKind::RankedList),
        (DataKind::RankedList, DataKind::KnowledgeGraph),
    ] {
        assert!(
            check::required_pair(pair.0, pair.1).contains(&Dimension::IdentifierNamespace),
            "{pair:?}"
        );
    }
}

#[test]
fn a_source_set_is_never_demanded() {
    // The catalogue's rule for `release`, `subject` and `source_set`: each
    // is compared wherever both sides state it, and a file recording none
    // is still loadable. A graph assembled by hand records no sources, and
    // a demand a user cannot always meet is worse than a missed check.
    for kind in DataKind::ALL {
        assert!(
            !check::required_pair(DataKind::KnowledgeGraph, kind).contains(&Dimension::SourceSet),
            "{kind:?}"
        );
    }

    let errors = check_step(
        "score",
        &[
            bind("a", graph_with(Dimension::SourceSet, "open-targets")),
            bind("b", graph("graph_no_sources.json")),
        ],
    );
    assert!(
        errors
            .iter()
            .all(|error| error.dimension() != Some(Dimension::SourceSet)),
        "got: {errors:#?}"
    );
}

#[test]
fn a_knowledge_graph_round_trips_through_its_name() {
    assert_eq!(DataKind::KnowledgeGraph.name(), "KnowledgeGraph");
    assert_eq!(
        DataKind::from_name("KnowledgeGraph"),
        Some(DataKind::KnowledgeGraph)
    );
}

#[test]
fn a_knowledge_graph_is_written_as_json() {
    // Tools read a format off the name, so the suffix is not cosmetic.
    assert_eq!(
        ligase::exec::output_suffix(DataKind::KnowledgeGraph),
        ".json"
    );
}

// ------------------------------------------------------- the refusal --

#[test]
fn two_graphs_assembled_from_different_sources_refuse_to_join() {
    // The milestone's headline. Both graphs hold the same three genes and
    // the same two edges; one of them also folded in DisGeNET. Everything
    // downstream of them -- an edge count, a neighbourhood, a propagated
    // score -- differs because of that and not because of biology, and
    // until now nothing said so.
    let errors = check_step(
        "compare",
        &[
            bind("a", graph("graph_targets.json")),
            bind("b", graph("graph_targets_disgenet.json")),
        ],
    );

    let found: Vec<_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::SourceSet))
        .collect();
    assert_eq!(found.len(), 1, "got: {errors:#?}");

    let said = found[0].to_string();
    // Both sets in full, the member that makes them differ named, and the
    // question a user can act on.
    assert!(said.contains("open-targets"), "{said}");
    assert!(said.contains("b also lists disgenet"), "{said}");
    assert!(
        said.contains("Was a source added or removed upstream?"),
        "{said}"
    );
    // And where each value came from, which is what makes it actionable:
    // nobody declared these, the engine read them off the edges.
    assert!(
        said.contains("the sources named on its nodes and edges"),
        "{said}"
    );
}

// ------------------------------- D36's reserved right, carried out --

#[test]
fn a_source_set_compares_exactly() {
    // M10a settled this and left it unimplemented because no kind applied
    // the axis. `s3://evidence/A.json` and `s3://evidence/a.json` are two
    // objects *by specification*, and a URL path is case-sensitive wherever
    // it is served. Folding the case would drop one of them without a word,
    // turning two graphs that differ into two that agree -- the false
    // agreement this engine exists to refuse. Not folding it can only turn
    // agreement into a refusal, which is loud and costs one consistent
    // spelling.
    assert_ne!(
        SourceSet::of(["s3://evidence/A.json"]),
        SourceSet::of(["s3://evidence/a.json"])
    );
    assert_eq!(
        SourceSet::of(["  open-targets "]).as_slice(),
        ["open-targets"],
        "surrounding space is still forgiven; only case is not"
    );
    assert_eq!(
        SourceSet::of(["open-targets", "open-targets"]).as_slice(),
        ["open-targets"],
        "an exact repeat is still one source"
    );
    assert_eq!(
        SourceSet::of(["open-targets", "Open-Targets"]).as_slice(),
        ["open-targets", "Open-Targets"],
        "two spellings are two sources, which is the whole of the split"
    );

    let errors = check_step(
        "compare",
        &[
            bind("a", graph("graph_source_upper.json")),
            bind("b", graph("graph_source_lower.json")),
        ],
    );
    assert!(
        errors
            .iter()
            .any(|error| error.dimension() == Some(Dimension::SourceSet)),
        "got: {errors:#?}"
    );
}

#[test]
fn an_organism_still_folds_ascii_case() {
    // The other half of the split, and the reason the two are one test
    // apart. `organism` folds because Latin binomials are a closed
    // conventional vocabulary in which `HIV-1` and `hiv-1` are one
    // organism; sources are not that. Were this to start failing, the split
    // has been applied to the wrong axis.
    assert_eq!(Organism::of(["HIV-1"]), Organism::of(["hiv-1"]));
    assert_eq!(
        Organism::of(["HIV-1", "hiv-1"]).as_slice(),
        ["HIV-1"],
        "and a repeat differing only in case is still one organism"
    );
}

// ------------------------------- M7's standing requirement, for it --

#[test]
fn each_axis_a_knowledge_graph_claims_compares_its_own_field() {
    // M7's standing requirement. `Release` and `Subject` are one type, so
    // an arm in `check::compare` written against the wrong field compiles,
    // and only a test over a kind that *applies* the axis can reach it.
    // `source_set` has never been reached by any kind until now, which is
    // exactly the condition the requirement exists to prevent.
    for (dimension, left, right) in [
        (
            Dimension::IdentifierNamespace,
            "Ensembl gene",
            "HGNC symbol",
        ),
        (Dimension::Subject, "breast carcinoma", "lung carcinoma"),
        (
            Dimension::Release,
            "Open Targets 24.06",
            "Open Targets 24.09",
        ),
        (Dimension::Organism, "Homo sapiens", "Mus musculus"),
        (Dimension::SourceSet, "open-targets", "disgenet"),
    ] {
        let errors = check_step(
            "compare",
            &[
                bind("a", graph_with(dimension, left)),
                bind("b", graph_with(dimension, right)),
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
fn a_step_over_a_knowledge_graph_carries_every_axis_it_claims() {
    // The other half of the standing requirement. `graph::propagate` has an
    // arm per axis and each names a field by hand; a `source_set` arm
    // reading `organism` would have compiled and nothing could have caught
    // it, because no kind's output applied the axis.
    let mut graph = Graph::new();
    let input = graph.add_input("kg", graph_declaring_everything());
    let node = graph
        .add_step(spec("annotate", DataKind::KnowledgeGraph, &[("kg", input)]))
        .unwrap();
    let dimensions = graph.dataset(node).unwrap().dimensions();

    for (dimension, expected) in [
        (Dimension::IdentifierNamespace, "Ensembl gene"),
        (Dimension::Subject, "breast carcinoma"),
        (Dimension::Release, "Open Targets 24.09"),
        (Dimension::Organism, "Homo sapiens"),
        (Dimension::SourceSet, "open-targets and disgenet"),
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
            Some("from step `annotate`"),
            "{dimension}"
        );
    }
}

// ----------------------------------------------------------- loading --

#[test]
fn a_graphs_sources_are_read_off_its_own_edges() {
    // D44. The sources are in the file, on the edges, and refusing to read
    // them would be the engine asking for something it is holding.
    let loaded = graph("graph_targets_disgenet.json");
    assert_eq!(
        loaded.dimensions().render(Dimension::SourceSet).as_deref(),
        Some("open-targets and disgenet")
    );
    assert_eq!(
        loaded
            .dimensions()
            .source(Dimension::SourceSet)
            .map(Source::describe)
            .as_deref()
            .map(|said| said.contains("the sources named on its nodes and edges")),
        Some(true)
    );
}

#[test]
fn a_graph_may_record_its_assembly_once_at_the_top() {
    // A graph assembled by a pipeline often says what went into it in one
    // place rather than on every edge. Both are read, and the dimension is
    // their union.
    let loaded = graph("graph_assembly_at_the_top.json");
    assert_eq!(
        loaded.dimensions().render(Dimension::SourceSet).as_deref(),
        Some("open-targets and disgenet")
    );
}

#[test]
fn a_graph_naming_no_sources_leaves_the_axis_unknown() {
    // Not a known, empty set. Two graphs that each recorded no sources
    // would otherwise *agree* on the axis, which is the false agreement the
    // dimension exists to break -- the same rule `load::organism` holds for
    // an empty organism declaration.
    let loaded = graph("graph_no_sources.json");
    assert!(!loaded.dimensions().is_known(Dimension::SourceSet));
    assert_eq!(
        loaded.dimensions().reason(Dimension::SourceSet),
        Some(
            "neither the graph nor any of its nodes and edges names a source -- declare \
             source_set"
        )
    );
}

#[test]
fn a_declared_source_set_is_taken_as_given() {
    // Declared wins over inference, as it does for a namespace, and for the
    // same reason: a declaration is a user telling the engine something it
    // cannot read, and a graph's own `sources` may name a mirror rather
    // than the source a reader would recognise.
    let loaded = load::knowledge_graph(
        &fixture("graph_targets.json"),
        &Declared {
            source_set: Some(vec!["Open Targets 24.09".to_owned()]),
            ..Declared::default()
        },
    )
    .unwrap();
    assert_eq!(
        loaded.dimensions().render(Dimension::SourceSet).as_deref(),
        Some("Open Targets 24.09")
    );
    assert_eq!(
        loaded.dimensions().source(Dimension::SourceSet),
        Some(&Source::Declared)
    );
}

#[test]
fn a_graphs_namespace_is_read_off_its_node_ids() {
    // The same inference a gene list gets, over node ids: every id matches
    // one known space or the dimension stays unknown.
    let loaded = graph("graph_targets.json");
    assert_eq!(
        loaded
            .dimensions()
            .render(Dimension::IdentifierNamespace)
            .as_deref(),
        Some("Ensembl gene")
    );

    // A graph of genes and diseases names its entities in two spaces, which
    // is normal for a graph and is why inference declines rather than
    // guessing. An EFO term is no Ensembl gene.
    let mixed = graph("graph_mixed_ids.json");
    assert!(!mixed.dimensions().is_known(Dimension::IdentifierNamespace));
}

#[test]
fn a_graph_does_not_record_its_organism_subject_or_release() {
    let loaded = graph("graph_targets.json");
    for (dimension, expected) in [
        (
            Dimension::Organism,
            "a knowledge graph does not record which organisms its entities belong to",
        ),
        (
            Dimension::Subject,
            "a knowledge graph does not record what it is a graph of",
        ),
        (
            Dimension::Release,
            "a knowledge graph does not record which release its sources were drawn from",
        ),
    ] {
        assert_eq!(
            loaded.dimensions().reason(dimension),
            Some(expected),
            "{dimension}"
        );
    }
}

#[test]
fn a_node_without_an_id_is_not_a_graph() {
    // Strict, like every loader here. A node with no id cannot be an edge
    // endpoint, so a file holding one is not the thing it claims to be.
    let error = load::knowledge_graph(
        &fixture("graph_node_without_an_id.json"),
        &Declared::default(),
    )
    .unwrap_err();
    let said = error.to_string();
    assert!(said.contains("id"), "{said}");
}

// -------------------------------------------------- the canonical form --

#[test]
fn the_order_of_a_graphs_nodes_and_edges_says_nothing() {
    // The ordinary rule, not the ranked list's exception: the order a
    // graph's nodes came out of whatever assembled them is not a fact about
    // the biology. The two fixtures hold the same three nodes and two
    // edges, with the arrays in different orders, the keys inside each
    // object in different orders, and one node's `ids` reversed.
    assert_eq!(
        canonical(DataKind::KnowledgeGraph, &fixture("graph_targets.json")),
        canonical(
            DataKind::KnowledgeGraph,
            &fixture("graph_targets_reordered.json")
        )
    );
}

#[test]
fn adding_a_source_to_one_edge_changes_what_the_graph_means() {
    // The other side of the same coin, and the reason the canonical form
    // cannot simply drop provenance: a source added upstream is a different
    // graph, and a cache that served the old answer for it would be serving
    // the wrong one.
    assert_ne!(
        canonical(DataKind::KnowledgeGraph, &fixture("graph_targets.json")),
        canonical(
            DataKind::KnowledgeGraph,
            &fixture("graph_targets_disgenet.json")
        )
    );
}

#[test]
fn a_servers_signature_is_dropped_from_the_top_level_and_noticed() {
    // D22: a `generatedAt` says how the answer was served, not what it
    // says. The same closed set `remote::response` drops from a recorded
    // body, at the top level only for D26's reason -- a `timestamp` inside
    // a record is what the record says, and recursing would silently delete
    // a time series. Each drop is noticed, in case the key was the user's
    // own data after all.
    assert_eq!(
        canonical(DataKind::KnowledgeGraph, &fixture("graph_volatile.json")),
        canonical(DataKind::KnowledgeGraph, &fixture("graph_targets.json"))
    );

    let (form, notes) = hashed(DataKind::KnowledgeGraph, &fixture("graph_volatile.json"));
    assert_eq!(form, Form::Canonical);
    assert_eq!(notes.len(), 2, "{notes:#?}");
    let said: Vec<String> = notes.iter().map(ToString::to_string).collect();
    assert!(
        said.iter().any(|note| note.contains("generatedAt")),
        "{said:#?}"
    );
    assert!(
        said.iter().any(|note| note.contains("request_id")),
        "{said:#?}"
    );
}

#[test]
fn a_file_that_is_not_a_graph_hashes_as_its_own_bytes() {
    // `canon`'s rule, and the conservative direction: a file that does not
    // parse as its kind falls back to its bytes, tagged so the two can
    // never collide.
    let (form, notes) = hashed(
        DataKind::KnowledgeGraph,
        &fixture("graph_node_without_an_id.json"),
    );
    assert_eq!(form, Form::Raw);
    assert!(notes.is_empty(), "{notes:#?}");

    // And a TSV is not a graph either, however well it parses as a TSV.
    assert_eq!(
        hashed(DataKind::KnowledgeGraph, &fixture("entities_hgnc.tsv")).0,
        Form::Raw
    );
}

#[test]
fn one_file_read_as_two_kinds_gives_two_hashes() {
    // The kind is hashed into the stream, so a step annotated to return a
    // graph can never be served the cached output of one annotated to
    // return something else -- even where both fell back to the bytes.
    assert_ne!(
        canonical(DataKind::KnowledgeGraph, &fixture("graph_targets.json")),
        canonical(DataKind::EntitySet, &fixture("graph_targets.json"))
    );
}

#[test]
fn a_graph_says_what_its_canonical_form_keeps() {
    let said = canon::describe(DataKind::KnowledgeGraph).unwrap();
    assert!(said.contains("nodes"), "{said}");
    assert!(said.contains("edges"), "{said}");
}
