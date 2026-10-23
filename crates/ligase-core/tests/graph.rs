//! M2: the dependency graph, built and checked without running anything.
//!
//! Two properties matter here and neither is about finding more errors.
//!
//! The first is **propagation**: a step's output carries the dimensions of
//! what went into it, re-sourced as coming from that step. That is what lets
//! a mistake three steps downstream still name the file that caused it.
//!
//! The second is **patience**. `web/content/reference/dimensions.mdx` allows
//! an unknown dimension unless a downstream step requires it. The graph is
//! what makes that sentence mean something: a sample set nobody joins on is
//! nobody's problem, and demanding it anyway would be a false alarm.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::fixture;
use ligase::check::{self, ErrorKind, Scope};
use ligase::dim::{CoordinateSystem, Dimension, Source};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::load::{self, Declared};
use ligase::types::{DataKind, Dataset};

// ------------------------------------------------------------- builders --

fn reference(name: &str) -> Dataset {
    load::reference(
        &fixture(name),
        &Declared {
            organism: Some(vec!["HIV-1".into()]),
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
        conversion: None,
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
    // in this pipeline joins two sample sets, so there is nothing to declare
    // and nothing to report. `web/content/reference/dimensions.mdx`: unknown
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

// -------------------------------------------- reporting under fan-out --

/// Reads with a declared sample identifier, so a graph can fan out.
fn sample(id: &str) -> Dataset {
    load::reads(
        &fixture("reads_phred33_1.fastq"),
        None,
        &Declared {
            samples: Some(vec![id.to_owned()]),
            ..Declared::default()
        },
    )
    .unwrap()
}

/// `n` samples aligned to one reference and called against another, so every
/// `call` node fails the same way. This is what per-sample fan-out looks like
/// when the pipeline is wrong.
fn broken_fan_out(ids: &[&str]) -> (Graph, Vec<NodeId>) {
    let mut graph = Graph::new();
    let right = graph.add_input("hxb2", reference("tiny_bare.fasta"));
    let wrong = graph.add_input("other", reference("tiny_other.fasta"));

    let roots = ids
        .iter()
        .map(|id| {
            let reads = graph.add_input("reads", sample(id));
            let aligned = graph
                .add_step(spec(
                    "align",
                    DataKind::Alignment,
                    vec![
                        argument("reads", DataKind::Reads, reads),
                        argument("ref", DataKind::Reference, right),
                    ],
                ))
                .unwrap();
            graph
                .add_step(spec(
                    "call",
                    DataKind::Variants,
                    vec![
                        argument("aln", DataKind::Alignment, aligned),
                        argument("ref", DataKind::Reference, wrong),
                    ],
                ))
                .unwrap()
        })
        .collect();

    (graph, roots)
}

#[test]
fn the_summary_counts_failing_nodes_not_step_names() {
    // Regression. Four distinct `call` nodes failed, one per sample. Counting
    // the names they share reported a cohort-wide failure as a single
    // localised one -- the opposite of what a user needs to decide whether to
    // re-run one sample or rethink the pipeline.
    let (graph, roots) = broken_fan_out(&["s1", "s2", "s3", "s4"]);
    let errors = graph.check(&roots);

    assert_eq!(errors.len(), 4, "errors were: {errors:#?}");
    assert_eq!(check::summarise(&errors), "4 errors in 4 steps.");
}

#[test]
fn two_problems_at_one_node_still_read_as_one_step() {
    // The other side of that fix: the count is of places, not of errors, so
    // one node with two problems must not read as two steps.
    let mut graph = Graph::new();
    let aligned = graph.add_input("aln", alignment("tiny_bare.sam"));
    let wrong = graph.add_input("ref", {
        load::reference(
            &fixture("tiny_other.fasta"),
            &Declared {
                organism: Some(vec!["human".into()]),
                build: Some("other".into()),
                ..Declared::default()
            },
        )
        .unwrap()
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

    // The alignment declares no organism, so only the reference disagrees...
    let errors = graph.check(&[called]);
    assert_eq!(errors.len(), 1, "errors were: {errors:#?}");
    assert_eq!(check::summarise(&errors), "1 error in 1 step.");
}

#[test]
fn an_error_at_a_fan_out_node_names_its_sample() {
    // Regression. Four byte-identical messages left no way to tell whether
    // you were looking at one problem or four, or which samples were hit.
    let (graph, roots) = broken_fan_out(&["patient_a", "patient_b"]);
    let errors = graph.check(&roots);

    assert_eq!(
        errors[0].scope().map(Scope::describe),
        Some("sample patient_a".to_owned())
    );
    assert!(
        errors[0]
            .to_string()
            .starts_with("DimensionError in step `call` (sample patient_a):"),
        "{}",
        errors[0]
    );
    assert!(
        errors[1].to_string().contains("(sample patient_b)"),
        "{}",
        errors[1]
    );
}

#[test]
fn a_step_over_many_samples_is_not_labelled_with_one_of_them() {
    // `merge` spans every sample, so there is no one sample to blame and
    // naming one would be worse than saying nothing.
    let mut graph = Graph::new();
    let first = graph.add_input("a", variants("tiny_two_samples.vcf"));
    let second = graph.add_input("b", variants("tiny_swapped_samples.vcf"));
    let merged = graph
        .add_step(spec(
            "merge",
            DataKind::Variants,
            vec![
                argument("a", DataKind::Variants, first),
                argument("b", DataKind::Variants, second),
            ],
        ))
        .unwrap();

    let errors = graph.check(&[merged]);
    assert_eq!(errors.len(), 1, "errors were: {errors:#?}");
    assert_eq!(errors[0].scope(), None);
    assert!(
        errors[0]
            .to_string()
            .starts_with("DimensionError in step `merge`:"),
        "{}",
        errors[0]
    );
}

#[test]
fn a_report_states_one_problem_once_and_names_every_sample_it_affects() {
    let (graph, roots) = broken_fan_out(&["s1", "s2", "s3", "s4"]);
    let errors = graph.check(&roots);
    let report = check::render(&errors);

    // The problem is explained once, against the first sample...
    assert_eq!(
        report
            .matches("These must match. Did you pass the wrong reference?")
            .count(),
        1,
        "the report repeats itself:\n{report}"
    );
    assert!(report.contains("(sample s1):"), "{report}");
    // ...and every other sample it affects is named, so nothing has to be
    // expanded to find out who was hit.
    assert!(
        report.contains("The same problem affects 3 other samples: s2, s3, s4."),
        "{report}"
    );
    // The count is still of nodes, not of paragraphs.
    assert!(report.ends_with("4 errors in 4 steps."), "{report}");
}

#[test]
fn a_report_keeps_problems_that_differ_apart() {
    let (mut graph, mut roots) = broken_fan_out(&["s1", "s2"]);

    // A third sample with a *different* problem: aligned to a chr-prefixed
    // reference, then called against a bare one.
    let prefixed = graph.add_input("prefixed", reference("tiny_prefixed.fasta"));
    let bare = graph.add_input("bare", reference("tiny_bare.fasta"));
    let reads = graph.add_input("reads", sample("s3"));
    let aligned = graph
        .add_step(spec(
            "align",
            DataKind::Alignment,
            vec![
                argument("reads", DataKind::Reads, reads),
                argument("ref", DataKind::Reference, prefixed),
            ],
        ))
        .unwrap();
    roots.push(
        graph
            .add_step(spec(
                "call",
                DataKind::Variants,
                vec![
                    argument("aln", DataKind::Alignment, aligned),
                    argument("ref", DataKind::Reference, bare),
                ],
            ))
            .unwrap(),
    );

    let errors = graph.check(&roots);
    let report = check::render(&errors);

    // The two samples that share a problem are stated once together...
    assert!(
        report.contains("The same problem affects 1 other sample: s2."),
        "{report}"
    );
    // ...and the one that differs keeps its own paragraph.
    assert!(
        report.contains("ContigNamingError in step `call` (sample s3):"),
        "{report}"
    );
    assert!(report.ends_with("3 errors in 3 steps."), "{report}");
}

#[test]
fn errors_that_cannot_name_themselves_are_never_collapsed() {
    // Two identical problems at two unnamed steps. Collapsing them would
    // hide one a user cannot otherwise identify, so each is kept.
    let mut graph = Graph::new();
    let first = graph.add_input("a", variants("tiny_two_samples.vcf"));
    let second = graph.add_input("b", variants("tiny_swapped_samples.vcf"));
    let roots: Vec<NodeId> = ["merge_one", "merge_two"]
        .into_iter()
        .map(|name| {
            graph
                .add_step(spec(
                    name,
                    DataKind::Variants,
                    vec![
                        argument("a", DataKind::Variants, first),
                        argument("b", DataKind::Variants, second),
                    ],
                ))
                .unwrap()
        })
        .collect();

    let errors = graph.check(&roots);
    assert_eq!(errors.len(), 2, "errors were: {errors:#?}");
    assert!(errors.iter().all(|error| error.scope().is_none()));

    let report = check::render(&errors);
    assert_eq!(
        report.matches("These must match, in order.").count(),
        2,
        "an unnamed problem must not be hidden behind another:\n{report}"
    );
}

// ------------------------------------- when a step needs naming, and when not --

#[test]
fn a_unique_step_name_reads_exactly_as_the_specification_says() {
    // Regression. `web/content/project/golden-example.mdx` fixes this shape
    // and the project treats it as fixed:
    //
    //     DimensionError in step `call`:
    //       aln.reference = ...
    //
    // One `call` step and no fan-out means there is nothing a user needed
    // help identifying, so a sample in the heading is noise on the commonest
    // path. This goes through real files, where the in-memory fixtures in
    // `check.rs` carry no sample set and so never exercised it.
    let mut graph = Graph::new();
    let aligned = graph.add_input("aln", alignment("tiny_other.sam"));
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

    // The alignment does have a sample set -- `@RG SM:sample_a` -- so this
    // is about ambiguity, not about whether a sample is known.
    assert!(
        graph
            .dataset(aligned)
            .unwrap()
            .dimensions()
            .sample_set
            .is_known()
    );

    let errors = graph.check(&[called]);
    assert_eq!(errors.len(), 1, "errors were: {errors:#?}");
    assert_eq!(errors[0].scope(), None);
    assert!(
        errors[0]
            .to_string()
            .starts_with("DimensionError in step `call`:\n  aln.reference = "),
        "{}",
        errors[0]
    );
}

#[test]
fn a_shared_name_whose_samples_match_is_told_apart_by_its_input() {
    // Regression. Two `call` nodes fed different alignments that happen to
    // share a sample both read "(sample sample_a)", which tells a user
    // nothing while looking exactly as though it did. A suffix that repeats
    // is worse than no suffix.
    let mut graph = Graph::new();
    let bare = graph.add_input("ref", reference("tiny_bare.fasta"));
    let roots: Vec<NodeId> = ["tiny_prefixed.sam", "tiny_other.sam"]
        .into_iter()
        .map(|name| {
            let aligned = graph.add_input("aln", alignment(name));
            graph
                .add_step(spec(
                    "call",
                    DataKind::Variants,
                    vec![
                        argument("aln", DataKind::Alignment, aligned),
                        argument("ref", DataKind::Reference, bare),
                    ],
                ))
                .unwrap()
        })
        .collect();

    let errors = graph.check(&roots);
    assert_eq!(errors.len(), 2, "errors were: {errors:#?}");

    // Both nodes are `sample_a`, so the sample is not a discriminator and
    // the file that distinguishes them is used instead.
    let scopes: Vec<Option<String>> = errors
        .iter()
        .map(|error| error.scope().map(Scope::describe))
        .collect();
    assert_eq!(
        scopes,
        [
            Some("file tests/fixtures/tiny_prefixed.sam".to_owned()),
            Some("file tests/fixtures/tiny_other.sam".to_owned()),
        ],
        "errors were: {errors:#?}"
    );
    // Whatever the discriminator is, the two headings must differ.
    assert_ne!(
        errors[0].to_string().lines().next(),
        errors[1].to_string().lines().next()
    );
}

#[test]
fn a_shared_name_nothing_distinguishes_is_left_unannotated() {
    // Two `call` nodes over the same two in-memory datasets: same name, same
    // sample, same inputs. Nothing tells them apart, so nothing is claimed
    // to.
    let mut graph = Graph::new();
    let aligned = graph.add_input("aln", alignment("tiny_other.sam"));
    let reference_node = graph.add_input("ref", reference("tiny_bare.fasta"));
    let roots: Vec<NodeId> = (0..2)
        .map(|_| {
            graph
                .add_step(spec(
                    "call",
                    DataKind::Variants,
                    vec![
                        argument("aln", DataKind::Alignment, aligned),
                        argument("ref", DataKind::Reference, reference_node),
                    ],
                ))
                .unwrap()
        })
        .collect();

    let errors = graph.check(&roots);
    assert_eq!(errors.len(), 2, "errors were: {errors:#?}");
    assert!(errors.iter().all(|error| error.scope().is_none()));

    // And without a name they are not collapsed, so neither is hidden.
    let report = check::render(&errors);
    assert_eq!(
        report
            .matches("These must match. Did you pass the wrong reference?")
            .count(),
        2,
        "{report}"
    );
    assert!(report.ends_with("2 errors in 2 steps."), "{report}");
}

#[test]
fn an_unreachable_twin_does_not_make_a_name_ambiguous() {
    // Ambiguity is judged over the pipeline being checked, not the whole
    // module: a second `call` built for another pipeline must not start
    // annotating this one's.
    let (mut graph, _) = broken_fan_out(&["s1"]);
    let roots = vec![NodeId::from_index(graph.len() - 1)];
    let elsewhere = graph.add_input("aln", alignment("tiny_other.sam"));
    let bare = graph.add_input("ref", reference("tiny_bare.fasta"));
    graph
        .add_step(spec(
            "call",
            DataKind::Variants,
            vec![
                argument("aln", DataKind::Alignment, elsewhere),
                argument("ref", DataKind::Reference, bare),
            ],
        ))
        .unwrap();

    let errors = graph.check(&roots);
    assert_eq!(errors.len(), 1, "errors were: {errors:#?}");
    assert_eq!(errors[0].scope(), None);
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
                organism: Some(vec!["HIV-1".into()]),
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
