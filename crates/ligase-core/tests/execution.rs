//! M3: running a checked graph, once.
//!
//! The four acceptance tests in `docs/milestones.md` are all one property
//! seen from four sides: a step runs when something that could change its
//! result has changed, and not otherwise. They are written here over shell
//! steps on text files, so the property is tested without waiting for an
//! aligner; `tests/test_demo.py` then runs the same four over the real
//! HIV-1 pipeline, which is what the milestone is actually signed off on.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ligase::cache::Cache;
use ligase::dim::Dimensions;
use ligase::exec::{self, Host, Options, Outcome, Recipe, RunReport, Stores, Task};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::prov::Prov;
use ligase::types::{DataKind, Dataset};
use tempfile::TempDir;

// ------------------------------------------------------------ scaffold --

/// A workspace: a cache, a place for inputs, and a graph being built.
struct Bench {
    dir: TempDir,
    cache: Cache,
    prov: Prov,
    graph: Graph,
    tasks: Vec<Task>,
}

impl Bench {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join(".ligase");
        let cache = Cache::open(&root).unwrap();
        let prov = Prov::open(&root).unwrap();
        Self {
            dir,
            cache,
            prov,
            graph: Graph::new(),
            tasks: Vec::new(),
        }
    }

    fn stores(&self) -> Stores<'_> {
        Stores {
            cache: &self.cache,
            prov: &self.prov,
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// An input file holding `contents`, added to the graph.
    fn input(&mut self, name: &str, contents: &str) -> NodeId {
        let path = self.path(name);
        fs::write(&path, contents).unwrap();
        let dataset = Dataset::new(DataKind::Reference, Some(path), Dimensions::default());
        self.graph.add_input(name, dataset)
    }

    /// A shell step over one input, bound to the parameter `in`.
    fn step(&mut self, name: &str, command: &str, input: NodeId) -> NodeId {
        self.shell(name, command, &[("in", input)], &[])
    }

    /// A step producing a FASTA, for the tests that need an output whose
    /// canonical form is more than its bytes.
    fn fasta(&mut self, name: &str, command: &str, input: NodeId) -> NodeId {
        self.shell_of(name, command, &[("in", input)], &[], DataKind::Reference)
    }

    fn shell(
        &mut self,
        name: &str,
        command: &str,
        inputs: &[(&str, NodeId)],
        parameters: &[(&str, &str)],
    ) -> NodeId {
        self.shell_of(name, command, inputs, parameters, DataKind::Alignment)
    }

    fn shell_of(
        &mut self,
        name: &str,
        command: &str,
        inputs: &[(&str, NodeId)],
        parameters: &[(&str, &str)],
        output: DataKind,
    ) -> NodeId {
        let node = self
            .graph
            .add_step(StepSpec {
                name: name.to_owned(),
                output,
                coordinates: None,
                arguments: inputs
                    .iter()
                    .map(|(param, node)| Argument {
                        param: (*param).to_owned(),
                        declared: DataKind::Reference,
                        node: *node,
                    })
                    .collect(),
            })
            .unwrap();

        self.tasks.push(Task {
            node,
            code: format!("def {name}(): ..."),
            recipe: Recipe::Shell(command.to_owned()),
            tools: Vec::new(),
            parameters: parameters
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        });
        node
    }

    /// Rewrite a step's recorded code, as editing its body would.
    fn edit(&mut self, node: NodeId, code: &str) {
        let task = self.tasks.iter_mut().find(|t| t.node == node).unwrap();
        task.code = code.to_owned();
    }

    /// Rewrite a step's parameter, as editing `min_freq` would.
    fn set(&mut self, node: NodeId, name: &str, value: &str) {
        let task = self.tasks.iter_mut().find(|t| t.node == node).unwrap();
        task.parameters = vec![(name.to_owned(), value.to_owned())];
    }

    fn run(&self, roots: &[NodeId]) -> RunReport {
        self.run_with(roots, &Options::default(), &Refusing)
    }

    fn run_with(&self, roots: &[NodeId], options: &Options, host: &dyn Host) -> RunReport {
        exec::run(
            &self.graph,
            roots,
            &self.tasks,
            &self.stores(),
            options,
            host,
        )
    }
}

/// A host with no callbacks: every step in these tests is a shell step.
struct Refusing;

impl Host for Refusing {
    fn call(&self, id: u64, _out: &Path, _inputs: &[(String, Vec<PathBuf>)]) -> Result<(), String> {
        Err(format!("no callback {id} in this test"))
    }
}

fn outcome(report: &RunReport, node: NodeId) -> Outcome {
    report
        .result(node)
        .unwrap_or_else(|| {
            panic!(
                "node {} is missing from the report: {report:?}",
                node.index()
            )
        })
        .outcome
}

fn contents(report: &RunReport, node: NodeId) -> String {
    fs::read_to_string(&report.result(node).unwrap().output).unwrap()
}

/// How many times the commands in this bench have been run.
fn executions(bench: &Bench) -> usize {
    fs::read_to_string(bench.path("marker")).map_or(0, |log| log.lines().count())
}

/// A command that copies its input, recording that it ran.
fn copy(bench: &Bench, tag: &str) -> String {
    format!(
        "cat {{in}} > {{out}}; echo {tag} >> {}",
        bench.path("marker").display()
    )
}

// ------------------------------------------------------------- running --

#[test]
fn a_pipeline_runs_and_produces_its_outputs() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let one = bench.step("align", "tr acgt ACGT < {in} > {out}", reads);
    let two = bench.step("call", "cat {in} {in} > {out}", one);

    let report = bench.run(&[two]);

    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(outcome(&report, one), Outcome::Ran);
    assert_eq!(outcome(&report, two), Outcome::Ran);
    assert_eq!(contents(&report, two), "ACGT\nACGT\n");
}

#[test]
fn an_output_is_named_after_its_contents_and_kept_out_of_the_way() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", "cat {in} > {out}", reads);

    let report = bench.run(&[align]);
    let result = report.result(align).unwrap();

    assert!(
        result.output.starts_with(bench.cache.root()),
        "outputs belong in the store, not beside the inputs: {}",
        result.output.display()
    );
    assert_eq!(result.hash, ligase::hash::ContentHash::of_bytes(b"ACGT\n"));
}

#[test]
fn a_step_is_given_its_parameters_and_its_inputs_by_name() {
    let mut bench = Bench::new();
    let left = bench.input("left.txt", "left\n");
    let right = bench.input("right.txt", "right\n");
    let joined = bench.shell(
        "join",
        "cat {a} {b} > {out}; echo {label} >> {out}",
        &[("a", left), ("b", right)],
        &[("label", "done")],
    );

    let report = bench.run(&[joined]);

    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(contents(&report, joined), "left\nright\ndone\n");
}

#[test]
fn a_paired_sample_reaches_its_command_as_two_mates() {
    let mut bench = Bench::new();
    let r1 = bench.path("sample_1.fastq");
    let r2 = bench.path("sample_2.fastq");
    fs::write(&r1, "mate one\n").unwrap();
    fs::write(&r2, "mate two\n").unwrap();
    let reads = bench.graph.add_input(
        "reads",
        Dataset::new(DataKind::Reads, Some(r1), Dimensions::default()).with_mate(r2),
    );
    let align = bench.shell(
        "align",
        "cat {reads.r1} {reads.r2} > {out}",
        &[("reads", reads)],
        &[],
    );

    let report = bench.run(&[align]);

    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(contents(&report, align), "mate one\nmate two\n");
}

// --------------------------------------------------------------- cache --

#[test]
fn a_second_run_comes_entirely_from_cache() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let one = bench.step("align", &copy(&bench, "align"), reads);
    let two = bench.step("call", &copy(&bench, "call"), one);

    bench.run(&[two]);
    assert_eq!(executions(&bench), 2);

    let again = bench.run(&[two]);

    assert_eq!(outcome(&again, one), Outcome::Cached);
    assert_eq!(outcome(&again, two), Outcome::Cached);
    assert_eq!(executions(&bench), 2, "a cached step should not have run");
    assert_eq!(contents(&again, two), "ACGT\n");
}

#[test]
fn a_cache_survives_the_process_that_wrote_it() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", &copy(&bench, "align"), reads);
    bench.run(&[align]);

    let reopened = Cache::open(bench.cache.root()).unwrap();
    let report = exec::run(
        &bench.graph,
        &[align],
        &bench.tasks,
        &Stores {
            cache: &reopened,
            prov: &bench.prov,
        },
        &Options::default(),
        &Refusing,
    );

    assert_eq!(outcome(&report, align), Outcome::Cached);
    assert_eq!(executions(&bench), 1);
}

#[test]
fn changing_an_input_reruns_the_steps_that_read_it() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", &copy(&bench, "align"), reads);
    bench.run(&[align]);

    fs::write(bench.path("reads.txt"), "TGCA\n").unwrap();
    let report = bench.run(&[align]);

    assert_eq!(outcome(&report, align), Outcome::Ran);
    assert_eq!(contents(&report, align), "TGCA\n");
}

#[test]
fn changing_a_parameter_reruns_only_what_is_below_it() {
    // The milestone's third acceptance test, in miniature: `min_freq` moves,
    // `call` and `resistance` rerun, `align` does not.
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", &copy(&bench, "align"), reads);
    let call = bench.shell(
        "call",
        &format!(
            "sed s/A/{{min_freq}}/ {{in}} > {{out}}; {}",
            touch_marker(&bench, "call")
        ),
        &[("in", align)],
        &[("min_freq", "0.01")],
    );
    let resistance = bench.step("resistance", &copy(&bench, "resistance"), call);
    bench.run(&[resistance]);

    bench.set(call, "min_freq", "0.05");
    let report = bench.run(&[resistance]);

    assert_eq!(outcome(&report, align), Outcome::Cached);
    assert_eq!(outcome(&report, call), Outcome::Ran);
    assert_eq!(outcome(&report, resistance), Outcome::Ran);
    assert_eq!(contents(&report, resistance), "0.05CGT\n");
}

#[test]
fn an_added_sample_is_the_only_work() {
    // The milestone's fourth acceptance test. Each sample is its own branch
    // of the graph, so adding one adds nodes rather than changing any.
    let mut bench = Bench::new();
    let first = bench.input("a.txt", "a\n");
    let second = bench.input("b.txt", "b\n");
    let align_a = bench.step("align", &copy(&bench, "a"), first);
    let align_b = bench.step("align", &copy(&bench, "b"), second);
    bench.run(&[align_a, align_b]);
    assert_eq!(executions(&bench), 2);

    let third = bench.input("c.txt", "c\n");
    let align_c = bench.step("align", &copy(&bench, "c"), third);
    let report = bench.run(&[align_a, align_b, align_c]);

    assert_eq!(outcome(&report, align_a), Outcome::Cached);
    assert_eq!(outcome(&report, align_b), Outcome::Cached);
    assert_eq!(outcome(&report, align_c), Outcome::Ran);
    assert_eq!(executions(&bench), 3, "only the new sample should have run");
}

#[test]
fn two_samples_with_identical_data_share_one_result() {
    // Content addressing, seen from the side: the same bytes through the
    // same step is the same work, whoever asked for it.
    let mut bench = Bench::new();
    let first = bench.input("a.txt", "same\n");
    let second = bench.input("b.txt", "same\n");
    let command = copy(&bench, "align");
    let align_a = bench.step("align", &command, first);
    let align_b = bench.step("align", &command, second);

    let report = bench.run(&[align_a, align_b]);

    assert_eq!(
        executions(&bench),
        1,
        "the same bytes through the same step is one piece of work"
    );
    assert_eq!(
        report.result(align_a).unwrap().output,
        report.result(align_b).unwrap().output
    );
}

#[test]
fn forcing_a_run_ignores_the_cache() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", &copy(&bench, "align"), reads);
    bench.run(&[align]);

    let options = Options {
        force: true,
        ..Options::default()
    };
    let report = bench.run_with(&[align], &options, &Refusing);

    assert_eq!(outcome(&report, align), Outcome::Ran);
    assert_eq!(executions(&bench), 2);
}

// -------------------------------------------------------- early cutoff --

#[test]
fn an_unchanged_output_stops_the_rerun_there() {
    // A step reran -- its code changed -- but produced the same bytes. Every
    // key below it is therefore unchanged, and the work below it is not
    // redone. In M3 that means byte-identical; M4 widens it to
    // scientifically identical.
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", &copy(&bench, "align"), reads);
    let call = bench.step("call", &copy(&bench, "call"), align);
    bench.run(&[call]);
    assert_eq!(executions(&bench), 2);

    // A comment-only edit: different code, identical output.
    bench.edit(align, "def align(): ...  # now with a comment");
    let report = bench.run(&[call]);

    assert_eq!(outcome(&report, align), Outcome::Ran);
    assert_eq!(
        outcome(&report, call),
        Outcome::Skipped,
        "the output did not change, so `call` had nothing to redo"
    );
    assert_eq!(executions(&bench), 3, "only `align` should have rerun");
}

#[test]
fn a_changed_output_does_not_stop_the_rerun() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.shell(
        "align",
        "sed s/A/{base}/ {in} > {out}",
        &[("in", reads)],
        &[("base", "A")],
    );
    let call = bench.step("call", &copy(&bench, "call"), align);
    bench.run(&[call]);

    bench.set(align, "base", "T");
    let report = bench.run(&[call]);

    assert_eq!(outcome(&report, align), Outcome::Ran);
    assert_eq!(outcome(&report, call), Outcome::Ran);
    assert_eq!(contents(&report, call), "TCGT\n");
}

// --------------------------------------------------- semantic hashing --

#[test]
fn a_volatile_header_does_not_stop_the_cutoff() {
    // The case that byte equality could not reach, and the reason M4
    // exists. Every real tool writes its own command line into its output
    // -- and that command line holds the output path, which is the staging
    // file named after the step's own key. So the step's bytes move
    // whenever its key does, and before canonicalisation the rerun went all
    // the way to the bottom of the pipeline however identical the science.
    //
    // Here a FASTA's description line stands in for the `@PG` line: the
    // canonical form of a reference is its sequence, so a description that
    // records the output path is exactly as volatile.
    let mut bench = Bench::new();
    let reads = bench.input("reads.fasta", ">1\nACGT\n");
    let align = bench.fasta(
        "align",
        &format!(
            "printf '>1 written to %s\\nACGT\\n' {{out}} > {{out}}; echo align >> {}",
            bench.path("marker").display()
        ),
        reads,
    );
    let call = bench.fasta("call", &copy(&bench, "call"), align);
    bench.run(&[call]);
    assert_eq!(executions(&bench), 2);

    // A comment-only edit: a different key, a different staging path, and
    // so different bytes -- from a step that computed the same sequence.
    bench.edit(align, "def align(): ...  # now with a comment");
    let report = bench.run(&[call]);

    let aligned = report.result(align).expect("align is in the report");
    assert_eq!(aligned.outcome, Outcome::Ran);
    assert_eq!(
        outcome(&report, call),
        Outcome::Skipped,
        "the sequence did not change, so `call` had nothing to redo"
    );
    assert_eq!(executions(&bench), 3, "only `align` should have rerun");
}

#[test]
fn the_report_gives_both_what_an_output_is_and_what_it_means() {
    // The bytes name the object in the store; the canonical hash is what
    // the steps below were keyed on. A run that reported only one of them
    // could not show that a rerun changed the first and not the second.
    let mut bench = Bench::new();
    let reads = bench.input("reads.fasta", ">1\nACGT\n");
    // The description records where the step wrote, as a tool's `@PG CL`
    // does -- so the bytes move whenever the key does.
    let align = bench.fasta(
        "align",
        "printf '>1 written to %s\\nACGT\\n' {out} > {out}",
        reads,
    );

    let first = bench.run(&[align]);
    let before = first.result(align).expect("align is in the report").clone();

    // A different description, the same sequence.
    bench.edit(align, "def align(): ...  # edited");
    let second = bench.run(&[align]);
    let after = second.result(align).expect("align is in the report");

    assert_ne!(before.hash, after.hash, "the bytes should have moved");
    assert_eq!(
        before.canonical, after.canonical,
        "what they mean should not have"
    );
}

#[test]
fn an_input_is_hashed_by_what_it_means_and_not_by_its_layout() {
    // An input rewritten with different line wrapping is the same
    // reference, so nothing downstream of it should rerun.
    let mut bench = Bench::new();
    let reference = bench.input("ref.fasta", ">1\nACGTACGTAC\n");
    let align = bench.fasta("align", &copy(&bench, "align"), reference);
    bench.run(&[align]);
    assert_eq!(executions(&bench), 1);

    // The same sequence, wrapped at five columns and lowercased.
    fs::write(bench.path("ref.fasta"), ">1 rewrapped\nacgta\ncgtac\n").unwrap();
    let report = bench.run(&[align]);

    assert_eq!(outcome(&report, align), Outcome::Cached);
    assert_eq!(executions(&bench), 1, "the reference did not change");
}

// ------------------------------------------------------------ failures --

#[test]
fn a_failing_step_is_reported_against_its_node() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", "echo 'no such contig' >&2; exit 3", reads);

    let report = bench.run(&[align]);

    assert!(report.result(align).is_none());
    let [failure] = report.failures.as_slice() else {
        panic!("expected one failure: {report:?}");
    };
    assert_eq!(failure.node, align);
    assert_eq!(failure.step, "align");
    assert!(
        failure.message.contains("no such contig"),
        "the tool's own words are what a user needs: {}",
        failure.message
    );
    assert!(failure.message.contains('3'), "{}", failure.message);
}

#[test]
fn one_sample_failing_does_not_stop_the_others() {
    // Four samples, one bad file. Losing three good results to one bad one
    // is the overnight-run problem this project exists to remove.
    let mut bench = Bench::new();
    let good = bench.input("good.txt", "ACGT\n");
    let bad = bench.input("bad.txt", "");
    let ok = bench.step("align", "cat {in} > {out}", good);
    let broken = bench.step("align", "exit 1", bad);

    let report = bench.run(&[ok, broken]);

    assert_eq!(outcome(&report, ok), Outcome::Ran);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].node, broken);
}

#[test]
fn a_step_below_a_failure_does_not_run() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", "exit 1", reads);
    let call = bench.step("call", &copy(&bench, "call"), align);

    let report = bench.run(&[call]);

    assert!(report.result(call).is_none());
    assert_eq!(executions(&bench), 0);
    assert_eq!(
        report.failures.len(),
        1,
        "one cause, one failure: {report:?}"
    );
}

#[test]
fn a_failed_step_is_not_remembered_as_done() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", "cat {in} > {out}; exit 1", reads);
    assert_eq!(bench.run(&[align]).failures.len(), 1);

    // Same key, second time: a half-written output must not be served as a
    // result, and the step must be attempted again.
    let report = bench.run(&[align]);

    assert_eq!(report.failures.len(), 1);
    assert!(report.result(align).is_none());
}

#[test]
fn a_step_with_no_task_is_a_failure_rather_than_a_silent_skip() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", "cat {in} > {out}", reads);
    bench.tasks.clear();

    let report = bench.run(&[align]);

    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].node, align);
}

#[test]
fn a_placeholder_that_names_nothing_is_a_failure() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", "cat {nope} > {out}", reads);

    let report = bench.run(&[align]);

    let [failure] = report.failures.as_slice() else {
        panic!("expected one failure: {report:?}");
    };
    assert!(
        failure.message.contains("nope"),
        "say which placeholder: {}",
        failure.message
    );
}

// ----------------------------------------------------------- callbacks --

/// A host that writes a fixed string, recording what it was asked for.
struct Writing {
    calls: Mutex<Vec<(u64, Vec<String>)>>,
}

impl Host for Writing {
    fn call(&self, id: u64, out: &Path, inputs: &[(String, Vec<PathBuf>)]) -> Result<(), String> {
        let seen = inputs
            .iter()
            .map(|(param, paths)| format!("{param}={}", paths.len()))
            .collect();
        self.calls
            .lock()
            .map_err(|e| e.to_string())?
            .push((id, seen));
        fs::write(out, "interpreted\n").map_err(|e| e.to_string())
    }
}

#[test]
fn a_python_step_is_run_by_the_host_and_cached_like_any_other() {
    let mut bench = Bench::new();
    let variants = bench.input("calls.vcf", "##fileformat=VCFv4.3\n");
    let node = bench
        .graph
        .add_step(StepSpec {
            name: "resistance".into(),
            output: DataKind::ResistanceReport,
            coordinates: None,
            arguments: vec![Argument {
                param: "v".into(),
                declared: DataKind::Reference,
                node: variants,
            }],
        })
        .unwrap();
    bench.tasks.push(Task {
        node,
        code: "def resistance(v): ...".into(),
        recipe: Recipe::Callback(7),
        tools: Vec::new(),
        parameters: Vec::new(),
    });
    let host = Writing {
        calls: Mutex::new(Vec::new()),
    };

    let first = bench.run_with(&[node], &Options::default(), &host);
    let second = bench.run_with(&[node], &Options::default(), &host);

    assert_eq!(outcome(&first, node), Outcome::Ran);
    assert_eq!(contents(&first, node), "interpreted\n");
    assert_eq!(outcome(&second, node), Outcome::Cached);
    assert_eq!(
        *host.calls.lock().unwrap(),
        vec![(7, vec!["v=1".to_owned()])],
        "the host should have been asked exactly once"
    );
}

// --------------------------------------------------------------- tools --

#[test]
fn a_tool_reports_the_version_that_joins_the_key() {
    let dir = TempDir::new().unwrap();
    let tool = dir.path().join("fake-tool");
    fs::write(&tool, "#!/bin/sh\necho 'fake-tool 1.2.3'\n").unwrap();
    make_executable(&tool);

    let version = exec::tool_version(&tool.to_string_lossy()).unwrap();

    assert!(version.contains("1.2.3"), "{version}");
}

#[test]
fn a_tool_that_is_not_installed_is_named_rather_than_ignored() {
    // Hashing an absent tool as "no version" would let two machines with
    // different tools agree on a key.
    let error = exec::tool_version("ligase-no-such-tool").unwrap_err();

    assert!(error.contains("ligase-no-such-tool"), "{error}");
}

#[test]
fn a_step_whose_tool_is_missing_fails_before_it_runs() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", &copy(&bench, "align"), reads);
    bench.tasks[0].tools = vec!["ligase-no-such-tool".into()];

    let report = bench.run(&[align]);

    assert_eq!(report.failures.len(), 1);
    assert_eq!(executions(&bench), 0);
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn touch_marker(bench: &Bench, tag: &str) -> String {
    format!("echo {tag} >> {}", bench.path("marker").display())
}

// -------------------------------------------------------------- report --

#[test]
fn a_report_gives_each_step_that_ran_its_own_cost() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", "cat {in} > {out}", reads);
    let call = bench.step("call", "cat {in} > {out}", align);

    let report = bench.run(&[call]);
    let text = exec::render(&bench.graph, &[call], &report);

    assert!(text.contains("ran       `align`"), "{text}");
    assert!(text.contains("ran       `call`"), "{text}");
    assert!(text.trim_end().ends_with("2 steps: 2 ran."), "{text}");
}

#[test]
fn a_report_names_the_samples_a_step_was_cached_for() {
    // Four samples cached is one fact about four samples, and a user who
    // has to expand something to learn which four has a shorter report
    // rather than a better one.
    let mut bench = Bench::new();
    let command = copy(&bench, "align");
    let first = bench.input("a.txt", "a\n");
    let second = bench.input("b.txt", "b\n");
    let align_a = bench.step("align", &command, first);
    let align_b = bench.step("align", &command, second);
    bench.run(&[align_a, align_b]);

    let again = bench.run(&[align_a, align_b]);
    let text = exec::render(&bench.graph, &[align_a, align_b], &again);

    assert!(text.contains("cached    `align` for "), "{text}");
    assert!(text.contains("a.txt"), "{text}");
    assert!(text.contains("b.txt"), "{text}");
    assert!(text.contains("2 steps: 2 from cache."), "{text}");
}

#[test]
fn a_report_says_what_the_tool_said_when_a_step_failed() {
    let mut bench = Bench::new();
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", "echo 'no such contig' >&2; exit 3", reads);

    let report = bench.run(&[align]);
    let text = exec::render(&bench.graph, &[align], &report);

    assert!(text.contains("`align` failed:"), "{text}");
    assert!(text.contains("  no such contig"), "{text}");
    assert!(text.contains("1 step: 1 failed."), "{text}");
}

#[test]
fn a_report_reads_the_same_way_twice() {
    // Two identical runs that print different reports would be a strange
    // thing for a reproducibility tool to do, and under a thread pool the
    // finishing order is nobody's to predict.
    let mut bench = Bench::new();
    let roots: Vec<NodeId> = (0..6)
        .map(|index| {
            let input = bench.input(&format!("{index}.txt"), &format!("{index}\n"));
            bench.step("align", "cat {in} > {out}", input)
        })
        .collect();

    let first = exec::render(&bench.graph, &roots, &bench.run(&roots));
    let second = exec::render(&bench.graph, &roots, &bench.run(&roots));

    assert_eq!(
        first.lines().filter(|line| line.starts_with("ran")).count(),
        6
    );
    assert_eq!(
        second,
        exec::render(&bench.graph, &roots, &bench.run(&roots))
    );
}

#[test]
fn an_empty_run_says_so_rather_than_printing_nothing() {
    let bench = Bench::new();
    let report = bench.run(&[]);

    assert_eq!(exec::summarise(&report), "nothing to run.");
}
