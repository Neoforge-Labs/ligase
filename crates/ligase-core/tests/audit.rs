//! M4: running a step twice and saying whether it is reproducible.
//!
//! `CLAUDE.md` asks for three outcomes, not two. A step can be
//! byte-identical, or it can produce different bytes that mean the same
//! thing — which is the ordinary case for any tool that records its own
//! command line — or it can produce a different result. Telling the second
//! from the third is the whole point: the second is fine and the third ends
//! a paper.
//!
//! The steps here write FASTA, because a FASTA's description line is a real
//! volatile field: the canonical form is the sequence, so a timestamp in a
//! description changes the bytes and nothing else.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use ligase::audit::{self, Verdict};
use ligase::cache::Cache;
use ligase::dim::{Dim, Dimensions, SampleSet, Source};
use ligase::exec::{Host, Options, Recipe, Stores, Task};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::note::Note;
use ligase::prov::Prov;
use ligase::types::{DataKind, Dataset};
use tempfile::TempDir;

// ------------------------------------------------------------ scaffold --

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

    /// A FASTA input, added to the graph.
    fn input(&mut self, name: &str, contents: &str) -> NodeId {
        let path = self.dir.path().join(name);
        fs::write(&path, contents).unwrap();
        let dataset = Dataset::new(DataKind::Reference, Some(path), Dimensions::default());
        self.graph.add_input(name, dataset)
    }

    /// Reads belonging to one named sample, so the graph can tell two nodes
    /// of the same step apart by sample the way a real pipeline does.
    fn sample(&mut self, sample: &str) -> NodeId {
        let path = self.dir.path().join(format!("{sample}.fastq"));
        fs::write(&path, "@read1\nACGT\n+\nIIII\n").unwrap();
        let dataset = Dataset::new(
            DataKind::Reads,
            Some(path),
            Dimensions {
                sample_set: Dim::known(SampleSet::new(vec![sample.to_owned()]), Source::Declared),
                ..Dimensions::default()
            },
        );
        self.graph.add_input(sample, dataset)
    }

    /// A step producing a FASTA from one FASTA, bound to `in`.
    fn step(&mut self, name: &str, command: &str, input: NodeId) -> NodeId {
        self.step_of(name, command, input, DataKind::Reference)
    }

    fn step_of(&mut self, name: &str, command: &str, input: NodeId, output: DataKind) -> NodeId {
        let declared = self
            .graph
            .dataset(input)
            .map_or(DataKind::Reference, Dataset::kind);
        let node = self
            .graph
            .add_step(StepSpec {
                name: name.to_owned(),
                output,
                coordinates: None,
                conversion: None,
                arguments: vec![Argument {
                    param: "in".to_owned(),
                    declared,
                    node: input,
                }],
            })
            .unwrap();
        self.tasks.push(Task {
            node,
            code: format!("def {name}(): ..."),
            recipe: Recipe::Shell(command.to_owned()),
            tools: Vec::new(),
            parameters: Vec::new(),
            citations: Vec::new(),
        });
        node
    }

    fn audit(&self, roots: &[NodeId], step: &str) -> audit::AuditReport {
        self.audit_sample(roots, step, None)
    }

    fn audit_sample(
        &self,
        roots: &[NodeId],
        step: &str,
        sample: Option<&str>,
    ) -> audit::AuditReport {
        audit::audit(
            &self.graph,
            roots,
            &self.tasks,
            &Stores {
                cache: &self.cache,
                prov: &self.prov,
            },
            &Options::default(),
            &Refusing,
            &audit::Subject { step, sample },
        )
        .unwrap()
    }
}

struct Refusing;

impl Host for Refusing {
    fn call(
        &self,
        id: u64,
        _out: &Path,
        _inputs: &[(String, Vec<PathBuf>)],
    ) -> Result<Vec<Note>, String> {
        Err(format!("no callback {id} in this test"))
    }
}

/// A command whose output is the same every time.
const STEADY: &str = "printf '>1\\nACGT\\n' > {out}";

/// How many times a command has run, as a shell expression.
///
/// A counter file rather than a clock: `date` has no sub-second field on
/// every platform the tests run on, and two runs of a fast step inside one
/// second would make a wobble test pass by agreeing.
fn counter(bench: &Bench, tag: &str) -> String {
    let path = bench.dir.path().join(format!("{tag}.count"));
    format!(
        "echo . >> '{}'; n=$(wc -l < '{}')",
        path.display(),
        path.display()
    )
}

/// A command whose *description line* changes every time, as a tool that
/// records its own command line does.
fn volatile_description(bench: &Bench) -> String {
    format!(
        "{}; printf '>1 produced by run %s\\nACGT\\n' \"$n\" > {{out}}",
        counter(bench, "description")
    )
}

/// A command whose *sequence* changes every time: a genuine failure.
///
/// The sequence is not DNA, which the canonical form does not check and
/// does not need to: what is being tested is that a changed sequence is a
/// changed result.
fn volatile_sequence(bench: &Bench) -> String {
    format!(
        "{}; printf '>1\\nACG%s\\n' \"$n\" > {{out}}",
        counter(bench, "sequence")
    )
}

// ------------------------------------------------------------ verdicts --

#[test]
fn a_step_that_writes_the_same_bytes_is_deterministic() {
    let mut bench = Bench::new();
    let reference = bench.input("ref.fasta", ">1\nACGT\n");
    let step = bench.step("align", STEADY, reference);

    let report = bench.audit(&[step], "align");

    assert_eq!(report.audited.len(), 1, "{report:?}");
    assert_eq!(report.audited[0].verdict, Verdict::Deterministic);
    assert_eq!(report.verdict(), Verdict::Deterministic);
}

#[test]
fn a_step_whose_bytes_move_but_whose_meaning_does_not_is_reproducible() {
    // The ordinary case for a real tool, and the one M4 exists to tell
    // apart from a genuine failure.
    let mut bench = Bench::new();
    let reference = bench.input("ref.fasta", ">1\nACGT\n");
    let step = bench.step("align", &volatile_description(&bench), reference);

    let report = bench.audit(&[step], "align");

    let audited = &report.audited[0];
    assert_eq!(audited.verdict, Verdict::Reproducible);
    assert_ne!(audited.first, audited.second, "the bytes should differ");
    assert_eq!(
        audited.first_canonical, audited.second_canonical,
        "what they mean should not"
    );
}

#[test]
fn a_step_whose_result_moves_is_not_reproducible() {
    let mut bench = Bench::new();
    let reference = bench.input("ref.fasta", ">1\nACGT\n");
    let step = bench.step("align", &volatile_sequence(&bench), reference);

    let report = bench.audit(&[step], "align");

    let audited = &report.audited[0];
    assert_eq!(audited.verdict, Verdict::NotReproducible);
    assert_ne!(audited.first_canonical, audited.second_canonical);
}

#[test]
fn the_report_takes_the_worst_verdict_of_its_steps() {
    // One irreproducible sample in a cohort is an irreproducible step: a
    // summary that averaged them would bury the thing a user needs.
    let mut bench = Bench::new();
    let first = bench.input("a.fasta", ">1\nACGT\n");
    let second = bench.input("b.fasta", ">1\nTGCA\n");
    let steady = bench.step("align", STEADY, first);
    let moving = bench.step("align", &volatile_sequence(&bench), second);

    let report = bench.audit(&[steady, moving], "align");

    assert_eq!(report.audited.len(), 2, "{report:?}");
    assert_eq!(report.verdict(), Verdict::NotReproducible);
}

// -------------------------------------------------------------- choice --

#[test]
fn auditing_a_step_audits_every_node_of_it() {
    // One written step is one step per sample, so "audit align" means all of
    // them; a verdict from one sample is not a verdict about the step.
    let mut bench = Bench::new();
    let first = bench.input("a.fasta", ">1\nACGT\n");
    let second = bench.input("b.fasta", ">1\nTGCA\n");
    let one = bench.step("align", STEADY, first);
    let two = bench.step("align", STEADY, second);
    let elsewhere = bench.step("call", STEADY, first);

    let report = bench.audit(&[one, two, elsewhere], "align");

    assert_eq!(report.audited.len(), 2, "{report:?}");
    assert!(report.audited.iter().all(|step| step.step == "align"));
}

#[test]
fn a_step_no_node_is_named_after_is_refused() {
    let mut bench = Bench::new();
    let reference = bench.input("ref.fasta", ">1\nACGT\n");
    let step = bench.step("align", STEADY, reference);

    let refused = audit::audit(
        &bench.graph,
        &[step],
        &bench.tasks,
        &Stores {
            cache: &bench.cache,
            prov: &bench.prov,
        },
        &Options::default(),
        &Refusing,
        &audit::Subject {
            step: "allign",
            sample: None,
        },
    );

    let message = refused.expect_err("a misspelled step should be refused");
    // The misspelling is the likely cause, so the names that exist are what
    // the user needs to see.
    assert!(message.contains("allign"), "{message}");
    assert!(message.contains("align"), "{message}");
}

#[test]
fn a_sample_narrows_the_audit_to_one_node() {
    let mut bench = Bench::new();
    let first = bench.sample("sample_a");
    let second = bench.sample("sample_b");
    let one = bench.step_of("align", STEADY, first, DataKind::Alignment);
    let two = bench.step_of("align", STEADY, second, DataKind::Alignment);

    let report = bench.audit_sample(&[one, two], "align", Some("sample_a"));

    assert_eq!(report.audited.len(), 1, "{report:?}");
    assert_eq!(report.audited[0].node, one);
}

#[test]
fn a_sample_no_node_is_for_is_refused() {
    let mut bench = Bench::new();
    let first = bench.sample("sample_a");
    let node = bench.step_of("align", STEADY, first, DataKind::Alignment);

    let refused = audit::audit(
        &bench.graph,
        &[node],
        &bench.tasks,
        &Stores {
            cache: &bench.cache,
            prov: &bench.prov,
        },
        &Options::default(),
        &Refusing,
        &audit::Subject {
            step: "align",
            sample: Some("sample_z"),
        },
    );

    let message = refused.expect_err("a sample with no node should be refused");
    assert!(message.contains("sample_z"), "{message}");
}

// ------------------------------------------------------- side effects --

#[test]
fn an_audit_records_nothing_in_the_cache() {
    // An audit observes. Recording either of its two runs would make the
    // cache hold a result nobody asked for, from a step known to wobble.
    let mut bench = Bench::new();
    let reference = bench.input("ref.fasta", ">1\nACGT\n");
    let step = bench.step("align", &volatile_description(&bench), reference);

    let before = objects(&bench.cache);
    bench.audit(&[step], "align");
    let after = objects(&bench.cache);

    // The one object is what the preliminary run stored, so that the audited
    // step had its inputs; neither of the two audit runs added anything.
    assert_eq!(
        after - before,
        1,
        "an audit added {} objects to the store",
        after - before
    );
}

#[test]
fn an_audit_leaves_no_scratch_files_behind() {
    let mut bench = Bench::new();
    let reference = bench.input("ref.fasta", ">1\nACGT\n");
    let step = bench.step("align", &volatile_description(&bench), reference);

    bench.audit(&[step], "align");

    let scratch: Vec<PathBuf> = fs::read_dir(bench.cache.root().join("tmp"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    assert!(scratch.is_empty(), "left behind: {scratch:?}");
}

// --------------------------------------------------------------- words --

#[test]
fn the_report_says_which_verdict_and_names_the_hashes() {
    let mut bench = Bench::new();
    let reference = bench.input("ref.fasta", ">1\nACGT\n");
    let step = bench.step("align", &volatile_description(&bench), reference);

    let text = bench.audit(&[step], "align").text();

    assert!(text.contains("reproducible"), "{text}");
    assert!(text.contains("align"), "{text}");
    // A user reading this wants to see for themselves that the bytes moved
    // and the meaning did not.
    assert!(text.contains("the bytes differ"), "{text}");
}

#[test]
fn a_failing_step_is_reported_rather_than_judged() {
    let mut bench = Bench::new();
    let reference = bench.input("ref.fasta", ">1\nACGT\n");
    let step = bench.step("align", "exit 3", reference);

    let report = bench.audit(&[step], "align");

    assert!(report.audited.is_empty(), "{report:?}");
    assert_eq!(report.failures.len(), 1, "{report:?}");
    assert!(report.failures[0].message.contains('3'), "{report:?}");
}

/// How many objects the store holds.
fn objects(cache: &Cache) -> usize {
    fn count(path: &Path) -> usize {
        fs::read_dir(path)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| {
                        if entry.path().is_dir() {
                            count(&entry.path())
                        } else {
                            1
                        }
                    })
                    .sum()
            })
            .unwrap_or(0)
    }
    count(&cache.root().join("store"))
}
