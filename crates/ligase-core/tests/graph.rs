//! M2: the dependency graph, built and checked without running anything.
//!
//! Two properties matter here and neither is about finding more errors.
//!
//! The first is **propagation**: a step's output carries the dimensions of
//! what went into it, re-sourced as coming from that step. That is what lets
//! a mistake three steps downstream still name the file that caused it.
//!
//! The second is **patience**. `docs/dimensions.md` allows an unknown
//! dimension unless a downstream step requires it. The graph is what makes
//! that sentence mean something: a sample set nobody joins on is nobody's
//! problem, and demanding it anyway would be a false alarm.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::fixture;
use ligase::check::ErrorKind;
use ligase::dim::{CoordinateSystem, Dimension, Source};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::load::{self, Declared};
use ligase::types::{DataKind, Dataset};

// ------------------------------------------------------------- builders --

fn reference(name: &str) -> Dataset {
    load::reference(
        &fixture(name),
        &Declared {
            organism: Some("HIV-1".into()),
            build: Some("tiny".into()),
            ..Declared::default()
        },
    )
    .unwrap()
}

fn alignment(name: &str) -> Dataset {
    load::alignment(&fixture(name), &Declared::default()).unwrap()
}

fn variants(name: &str) -> Dataset {
    load::variants(&fixture(name), &Declared::default()).unwrap()
}

fn reads(name: &str) -> Dataset {
    load::reads(&fixture(name), None, &Declared::default()).unwrap()
}

fn argument(param: &str, declared: DataKind, node: NodeId) -> Argument {
    Argument {
        param: param.to_owned(),
        declared,
        node,
    }
}

fn spec(name: &str, output: DataKind, arguments: Vec<Argument>) -> StepSpec {
    StepSpec {
        name: name.to_owned(),
        output,
        coordinates: None,
        arguments,
    }
}

/// The golden example's three steps over one sample, as a graph.
///
/// Returns the graph and the terminal node, so a test can check the whole
/// pipeline the way `ligase check` does.
fn golden(reference_fixture: &str) -> (Graph, NodeId) {
    let mut graph = Graph::new();
    let r1 = graph.add_input("reads", reads("reads_phred33_1.fastq"));
    let reference_node = graph.add_input("hxb2", reference(reference_fixture));

    let aligned = graph
        .add_step(spec(
            "align",
            DataKind::Alignment,
            vec![
                argument("reads", DataKind::Reads, r1),
                argument("ref", DataKind::Reference, reference_node),
            ],
        ))
        .unwrap();
    let called = graph
        .add_step(spec(
            "call",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, aligned),
                argument("ref", DataKind::Reference, reference_node),
            ],
        ))
        .unwrap();
    let report = graph
        .add_step(spec(
            "resistance",
            DataKind::ResistanceReport,
            vec![argument("v", DataKind::Variants, called)],
        ))
        .unwrap();

    (graph, report)
}

// ----------------------------------------------------------- the shape --

#[test]
fn the_golden_example_builds_a_graph_that_checks_clean() {
    let (graph, report) = golden("tiny_bare.fasta");

    assert_eq!(graph.step_names(&[report]), ["align", "call", "resistance"]);
    assert!(
        graph.check(&[report]).is_empty(),
        "errors were: {:#?}",
        graph.check(&[report])
    );
}

#[test]
fn an_argument_naming_a_node_that_does_not_exist_is_refused() {
    let mut graph = Graph::new();
    let error = graph
        .add_step(spec(
            "align",
            DataKind::Alignment,
            vec![argument("reads", DataKind::Reads, NodeId::from_index(7))],
        ))
        .expect_err("a step cannot depend on a node that was never added");

    let message = error.to_string();
    assert!(
        message.contains("align") && message.contains("reads"),
        "{message}"
    );
}

#[test]
fn check_covers_only_the_nodes_the_roots_reach() {
    let (mut graph, report) = golden("tiny_bare.fasta");

    // A second, broken branch that no root reaches.
    let other = graph.add_input("other", reference("tiny_other.fasta"));
    let stray = graph.add_input("stray", alignment("tiny_bare.sam"));
    let orphan = graph
        .add_step(spec(
            "orphan",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, stray),
                argument("ref", DataKind::Reference, other),
            ],
        ))
        .unwrap();

    assert!(graph.check(&[report]).is_empty());
    assert!(!graph.check(&[orphan]).is_empty());
    assert_eq!(graph.step_names(&[report]), ["align", "call", "resistance"]);
}

// -------------------------------------------------------- propagation --

#[test]
fn a_step_output_is_sourced_to_the_step_that_produced_it() {
    let (graph, report) = golden("tiny_bare.fasta");
    let output = graph.dataset(report).unwrap();

    // `Source::Derived` exists for exactly this: a value that travelled
    // through the graph says which step it came out of.
    assert_eq!(
        output.dimensions().organism.source(),
        Some(&Source::Derived {
            step: "resistance".to_owned()
        })
    );
}

#[test]
fn an_alignment_inherits_the_reference_from_the_reference_argument() {
    let (graph, _) = golden("tiny_bare.fasta");
    let aligned = graph.dataset(NodeId::from_index(2)).unwrap();

    assert_eq!(aligned.kind(), DataKind::Alignment);
    let inherited = aligned.dimensions().reference.value().unwrap();
    let declared = reference("tiny_bare.fasta");
    assert_eq!(Some(inherited), declared.dimensions().reference.value());
}

#[test]
fn an_alignment_inherits_pairing_and_samples_from_the_reads() {
    let (graph, _) = golden("tiny_bare.fasta");
    let aligned = graph.dataset(NodeId::from_index(2)).unwrap();

    // The reads fixture is single-end with a sample taken from its filename.
    // M1 could only report an alignment's pairing as unknown, because it is
    // in the SAM flags; a derived alignment knows it from the reads.
    assert!(aligned.dimensions().pairing.is_known());
    assert_eq!(
        aligned.dimensions().sample_set.value(),
        reads("reads_phred33_1.fastq")
            .dimensions()
            .sample_set
            .value()
    );
}

#[test]
fn an_alignment_output_is_one_based_closed() {
    let (graph, _) = golden("tiny_bare.fasta");
    let aligned = graph.dataset(NodeId::from_index(2)).unwrap();

    // Nothing upstream carries a coordinate system: FASTQ and FASTA have
    // none. It is intrinsic to the kind (decision D9).
    assert_eq!(
        aligned.dimensions().coordinates.value(),
        Some(&CoordinateSystem::OneBasedClosed)
    );
}

#[test]
fn a_dimension_survives_two_steps_of_propagation() {
    let (graph, report) = golden("tiny_bare.fasta");

    // organism was declared once, on the reference, and is three steps away.
    assert_eq!(
        graph
            .dataset(report)
            .unwrap()
            .dimensions()
            .organism
            .value()
            .map(ToString::to_string),
        Some("HIV-1".to_owned())
    );
}

#[test]
fn an_unknown_dimension_keeps_its_reason_as_it_propagates() {
    let mut graph = Graph::new();
    // No @RG lines, so the sample set is unknown and says why.
    let aligned = graph.add_input("aln", alignment("tiny_no_rg.sam"));
    let reference_node = graph.add_input("ref", reference("tiny_bare.fasta"));
    let called = graph
        .add_step(spec(
            "call",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, aligned),
                argument("ref", DataKind::Reference, reference_node),
            ],
        ))
        .unwrap();

    let reason = graph
        .dataset(called)
        .unwrap()
        .dimensions()
        .sample_set
        .reason()
        .expect("the gap must survive the step");
    assert!(reason.contains("@RG"), "the reason was lost: {reason}");
}

#[test]
fn a_step_that_produces_intervals_must_say_which_coordinates_it_writes() {
    let mut graph = Graph::new();
    let called = graph.add_input("v", variants("tiny_bare.vcf"));

    // Without a declaration, the output's coordinate system is unknown
    // rather than inherited: a tool that writes BED from a VCF changes the
    // convention, and inheriting 1-based closed here would be a silent lie.
    let silent = graph
        .add_step(spec(
            "regions",
            DataKind::Intervals,
            vec![argument("v", DataKind::Variants, called)],
        ))
        .unwrap();
    assert!(
        !graph
            .dataset(silent)
            .unwrap()
            .dimensions()
            .coordinates
            .is_known()
    );

    let declared = graph
        .add_step(StepSpec {
            coordinates: Some(CoordinateSystem::ZeroBasedHalfOpen),
            ..spec(
                "regions_bed",
                DataKind::Intervals,
                vec![argument("v", DataKind::Variants, called)],
            )
        })
        .unwrap();
    assert_eq!(
        graph
            .dataset(declared)
            .unwrap()
            .dimensions()
            .coordinates
            .value(),
        Some(&CoordinateSystem::ZeroBasedHalfOpen)
    );
}

// ------------------------------------------------------------- checking --

#[test]
fn a_reference_mismatch_is_reported_at_the_step_that_joins_them() {
    let mut graph = Graph::new();
    let aligned = graph.add_input("aln", alignment("tiny_bare.sam"));
    let wrong = graph.add_input("ref", reference("tiny_other.fasta"));
    let called = graph
        .add_step(spec(
            "call",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, aligned),
                argument("ref", DataKind::Reference, wrong),
            ],
        ))
        .unwrap();

    let errors = graph.check(&[called]);
    assert_eq!(errors.len(), 1, "errors were: {errors:#?}");
    assert_eq!(errors[0].step(), "call");
    assert_eq!(errors[0].dimension(), Some(Dimension::Reference));
}

#[test]
fn every_error_in_the_pipeline_is_reported_in_one_pass() {
    // Two independent mistakes, two steps apart. A user who has to rerun to
    // find the second one is living the problem Ligase exists to remove.
    let mut graph = Graph::new();
    let aligned = graph.add_input("aln", alignment("tiny_bare.sam"));
    let wrong = graph.add_input("ref", reference("tiny_other.fasta"));
    let prefixed = graph.add_input("regions", {
        load::intervals(&fixture("tiny_prefixed.bed"), None, &Declared::default()).unwrap()
    });

    let called = graph
        .add_step(spec(
            "call",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, aligned),
                argument("ref", DataKind::Reference, wrong),
            ],
        ))
        .unwrap();
    let restricted = graph
        .add_step(spec(
            "restrict",
            DataKind::Variants,
            vec![
                argument("v", DataKind::Variants, called),
                argument("regions", DataKind::Intervals, prefixed),
            ],
        ))
        .unwrap();

    let errors = graph.check(&[restricted]);
    let steps: Vec<&str> = errors.iter().map(ligase::check::CheckError::step).collect();
    assert_eq!(
        steps,
        ["call", "restrict", "restrict"],
        "errors were: {errors:#?}"
    );
    let kinds: Vec<ErrorKind> = errors.iter().map(ligase::check::CheckError::kind).collect();
    assert_eq!(
        kinds,
        [
            // The wrong reference, where it was joined.
            ErrorKind::Dimension,
            // `chr1` against `1`, which is a renaming step, not a reference.
            ErrorKind::ContigNaming,
            // 0-based half-open intervals against 1-based closed calls.
            ErrorKind::Dimension
        ],
        "errors were: {errors:#?}"
    );
    assert_eq!(errors[2].dimension(), Some(Dimension::Coordinates));
}

#[test]
fn passing_the_wrong_kind_of_data_is_a_step_type_error() {
    let mut graph = Graph::new();
    let aligned = graph.add_input("aln", alignment("tiny_bare.sam"));
    let calls = graph.add_input("v", variants("tiny_bare.vcf"));

    // `call` wants a Reference and was handed Variants.
    let called = graph
        .add_step(spec(
            "call",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, aligned),
                argument("ref", DataKind::Reference, calls),
            ],
        ))
        .unwrap();

    let errors = graph.check(&[called]);
    assert_eq!(errors.len(), 1, "errors were: {errors:#?}");
    assert_eq!(errors[0].kind(), ErrorKind::StepType);
    assert_eq!(errors[0].dimension(), None);

    let message = errors[0].to_string();
    assert!(
        message.contains("StepTypeError in step `call`"),
        "{message}"
    );
    assert!(message.contains("Reference"), "{message}");
    assert!(message.contains("Variants"), "{message}");
}

#[test]
fn a_kind_error_replaces_the_step_s_dimension_errors_rather_than_adding_to_them() {
    // The bindings are not what the step declared, so checking their
    // dimensions would describe a step the user never wrote. One error per
    // root cause.
    let mut graph = Graph::new();
    let aligned = graph.add_input("aln", alignment("tiny_bare.sam"));
    let other = graph.add_input("v", variants("tiny_other.vcf"));

    let called = graph
        .add_step(spec(
            "call",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, aligned),
                argument("ref", DataKind::Reference, other),
            ],
        ))
        .unwrap();

    let errors = graph.check(&[called]);
    assert_eq!(errors.len(), 1, "errors were: {errors:#?}");
    assert_eq!(errors[0].kind(), ErrorKind::StepType);
}

// ------------------------------------------- requirements, driven by the graph --

#[test]
fn an_unknown_dimension_nothing_requires_is_nobody_s_problem() {
    // The alignment has no @RG lines, so its sample set is unknown. Nothing
    // in this pipeline joins two sample sets, so there is nothing to
    // declare and nothing to report. `docs/dimensions.md`: unknown
    // dimensions are allowed *if no downstream step requires them*.
    let mut graph = Graph::new();
    let aligned = graph.add_input("aln", alignment("tiny_no_rg.sam"));
    let reference_node = graph.add_input("ref", reference("tiny_bare.fasta"));
    let called = graph
        .add_step(spec(
            "call",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, aligned),
                argument("ref", DataKind::Reference, reference_node),
            ],
        ))
        .unwrap();
    let report = graph
        .add_step(spec(
            "resistance",
            DataKind::ResistanceReport,
            vec![argument("v", DataKind::Variants, called)],
        ))
        .unwrap();

    assert!(
        graph.check(&[report]).is_empty(),
        "errors were: {:#?}",
        graph.check(&[report])
    );
}

#[test]
fn an_unknown_dimension_a_later_step_requires_is_reported_against_its_origin() {
    // The same unknown sample set, but now two sets of calls are merged, and
    // merging demands them. The error must name the BAM -- the file the user
    // has to change -- not only the step that tripped over it two hops later.
    let mut graph = Graph::new();
    let aligned = graph.add_input("aln", alignment("tiny_no_rg.sam"));
    let reference_node = graph.add_input("ref", reference("tiny_bare.fasta"));
    let other = graph.add_input("other", variants("tiny_bare.vcf"));

    let called = graph
        .add_step(spec(
            "call",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, aligned),
                argument("ref", DataKind::Reference, reference_node),
            ],
        ))
        .unwrap();
    let merged = graph
        .add_step(spec(
            "merge",
            DataKind::Variants,
            vec![
                argument("a", DataKind::Variants, called),
                argument("b", DataKind::Variants, other),
            ],
        ))
        .unwrap();

    let errors = graph.check(&[merged]);
    assert_eq!(errors.len(), 1, "errors were: {errors:#?}");
    assert_eq!(errors[0].kind(), ErrorKind::Undeclared);
    assert_eq!(errors[0].dimension(), Some(Dimension::SampleSet));
    assert_eq!(errors[0].step(), "merge");

    let message = errors[0].to_string();
    assert!(message.contains("tiny_no_rg.sam"), "{message}");
    assert!(message.contains("@RG"), "{message}");
    // The route matters: the user is looking at a step called `merge` and
    // the gap is in a file two steps upstream.
    assert!(message.contains("call"), "{message}");
}

#[test]
fn one_missing_declaration_produces_one_error_however_many_steps_need_it() {
    let mut graph = Graph::new();
    let aligned = graph.add_input("aln", alignment("tiny_no_rg.sam"));
    let reference_node = graph.add_input("ref", reference("tiny_bare.fasta"));
    let other = graph.add_input("other", variants("tiny_bare.vcf"));

    let called = graph
        .add_step(spec(
            "call",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, aligned),
                argument("ref", DataKind::Reference, reference_node),
            ],
        ))
        .unwrap();
    let first = graph
        .add_step(spec(
            "merge_one",
            DataKind::Variants,
            vec![
                argument("a", DataKind::Variants, called),
                argument("b", DataKind::Variants, other),
            ],
        ))
        .unwrap();
    let second = graph
        .add_step(spec(
            "merge_two",
            DataKind::Variants,
            vec![
                argument("a", DataKind::Variants, first),
                argument("b", DataKind::Variants, other),
            ],
        ))
        .unwrap();

    // One file to fix, so one error -- not one per step the value flows
    // through. Repetition is how a report becomes unreadable.
    let errors = graph.check(&[second]);
    assert_eq!(errors.len(), 1, "errors were: {errors:#?}");
    assert_eq!(errors[0].step(), "merge_one");
}

#[test]
fn a_requirement_met_by_a_declaration_at_the_root_is_satisfied() {
    let mut graph = Graph::new();
    let aligned = graph.add_input(
        "aln",
        load::alignment(
            &fixture("tiny_no_rg.sam"),
            &Declared {
                samples: Some(vec!["sample_a".to_owned()]),
                ..Declared::default()
            },
        )
        .unwrap(),
    );
    let reference_node = graph.add_input("ref", reference("tiny_bare.fasta"));
    let other = graph.add_input("other", variants("tiny_bare.vcf"));

    let called = graph
        .add_step(spec(
            "call",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, aligned),
                argument("ref", DataKind::Reference, reference_node),
            ],
        ))
        .unwrap();
    let merged = graph
        .add_step(spec(
            "merge",
            DataKind::Variants,
            vec![
                argument("a", DataKind::Variants, called),
                argument("b", DataKind::Variants, other),
            ],
        ))
        .unwrap();

    assert!(
        graph.check(&[merged]).is_empty(),
        "errors were: {:#?}",
        graph.check(&[merged])
    );
}

// ------------------------------------------------- the real HIV-1 demo --

#[test]
fn the_demo_pipeline_builds_and_checks_on_real_data() {
    // `data/` is gitignored, so this skips rather than fails when the data
    // has not been fetched. Where it runs, the reference carried through two
    // steps is a checksum Ligase computed from the real HXB2 FASTA against
    // one `samtools dict` computed independently.
    let (Some(fasta), Some(bam)) = (common::hxb2_reference(), common::demo_alignment()) else {
        return;
    };

    let mut graph = Graph::new();
    let reference_node = graph.add_input(
        "hxb2",
        load::reference(
            &fasta,
            &Declared {
                organism: Some("HIV-1".into()),
                build: Some("HXB2".into()),
                ..Declared::default()
            },
        )
        .unwrap(),
    );
    let aligned = graph.add_input("aln", load::alignment(&bam, &Declared::default()).unwrap());

    let called = graph
        .add_step(spec(
            "call",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, aligned),
                argument("ref", DataKind::Reference, reference_node),
            ],
        ))
        .unwrap();
    let report = graph
        .add_step(spec(
            "resistance",
            DataKind::ResistanceReport,
            vec![argument("v", DataKind::Variants, called)],
        ))
        .unwrap();

    assert!(
        graph.check(&[report]).is_empty(),
        "errors were: {:#?}",
        graph.check(&[report])
    );

    // HXB2 travelled from the BAM header through `call` into the report's
    // organism, which the BAM itself never stated.
    assert_eq!(
        graph
            .dataset(report)
            .unwrap()
            .dimensions()
            .organism
            .value()
            .map(ToString::to_string),
        Some("HIV-1".to_owned())
    );
}
