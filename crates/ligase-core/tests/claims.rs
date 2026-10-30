//! M16: binding a published number to the computation behind it.
//!
//! Provenance runs one way today — from a result back to how it was
//! produced — and these tests are about the other direction: from a sentence
//! in a manuscript to the step, the key and the output hash that stand behind
//! it. What they are really pinning is D45's two halves. A claim binds to the
//! *computation* rather than to the occasion, so a step served from the cache
//! backs its claim completely; and a claim never reaches a cache key, so
//! editing a manuscript reruns nothing.
//!
//! The other property under test is the one that keeps the feature from being
//! decoration (D42's rule, from M13): every part of a claim's declaration is
//! checked against the run, and a claim the engine cannot stand behind is
//! named rather than reprinted.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use ligase::cache::Cache;
use ligase::dim::Dimensions;
use ligase::exec::{self, Host, Options, Outcome, Recipe, RunReport, Stores, Task};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::load::{self, Declared};
use ligase::note::Note;
use ligase::prov::{self, Claim, ClaimVerdict, Prov, Restated, Unbacked, bundle};
use ligase::types::{DataKind, Dataset};
use tempfile::TempDir;

// ------------------------------------------------------------ scaffold --

/// A workspace: a cache, a provenance store, inputs, and a graph.
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

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// A text input, added to the graph as a reference.
    fn input(&mut self, name: &str, contents: &str) -> NodeId {
        let path = self.path(name);
        fs::write(&path, contents).unwrap();
        let dataset = Dataset::new(DataKind::Reference, Some(path), Dimensions::default());
        self.graph.add_input(name, dataset)
    }

    /// A reads input whose sample identifier is declared, so the nodes of
    /// one step have something to tell them apart.
    fn sample(&mut self, id: &str, sequence: &str) -> NodeId {
        let path = self.path(&format!("{id}.fastq"));
        let quality = "I".repeat(sequence.len());
        fs::write(&path, format!("@{id}.1\n{sequence}\n+\n{quality}\n")).unwrap();
        let dataset = load::reads(
            &path,
            None,
            &Declared {
                organism: Some(vec!["HIV-1".into()]),
                samples: Some(vec![id.to_owned()]),
                ..Declared::default()
            },
        )
        .unwrap();
        self.graph.add_input(id, dataset)
    }

    fn step(&mut self, name: &str, command: &str, inputs: &[(&str, NodeId)]) -> NodeId {
        self.step_with(name, command, inputs, &[], &[])
    }

    fn step_with(
        &mut self,
        name: &str,
        command: &str,
        inputs: &[(&str, NodeId)],
        parameters: &[(&str, &str)],
        tools: &[&str],
    ) -> NodeId {
        let node = self
            .graph
            .add_step(StepSpec {
                name: name.to_owned(),
                output: DataKind::Reference,
                coordinates: None,
                conversion: None,
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
            tools: tools.iter().map(|t| (*t).to_owned()).collect(),
            parameters: parameters
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            citations: Vec::new(),
            seed: None,
        });
        node
    }

    /// Give a step references of its own, as `@lg.step(cite=...)` does.
    ///
    /// Here so that a claim can be checked across the one edit that
    /// provably cannot change a number: `cite` never joins a cache key
    /// (D45), so the step is served from the cache and the artifact is the
    /// same bytes.
    fn cite(&mut self, node: NodeId, references: &[&str]) {
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.node == node)
            .unwrap();
        task.citations = references.iter().map(|text| (*text).to_owned()).collect();
    }

    fn run(&self, roots: &[NodeId]) -> RunReport {
        exec::run(
            &self.graph,
            roots,
            &self.tasks,
            &Stores {
                cache: &self.cache,
                prov: &self.prov,
            },
            &Options::default(),
            &Refusing,
        )
    }

    /// The write-up for a run, with these claims bound against it.
    fn methods(&self, roots: &[NodeId], report: &RunReport, claims: &[Claim]) -> prov::Methods {
        let mut methods =
            prov::methods(&self.graph, roots, &self.tasks, report, &self.prov).unwrap();
        methods.bind(claims);
        methods
    }
}

struct Refusing;

impl Host for Refusing {
    fn call(
        &self,
        _id: u64,
        _out: &Path,
        _inputs: &[(String, Vec<PathBuf>)],
    ) -> Result<Vec<Note>, String> {
        Err("no callbacks in these tests".to_owned())
    }
}

/// A claim about a step and nothing more: the shape with no value to compare,
/// which binds to a computation rather than being checked against one.
fn about(text: &str, step: &str) -> Claim {
    Claim {
        text: text.to_owned(),
        step: step.to_owned(),
        scope: None,
        restates: None,
    }
}

/// The one check in a write-up, which these tests always declare one of.
fn only(methods: &prov::Methods) -> &prov::ClaimCheck {
    assert_eq!(methods.claims.len(), 1, "one claim was declared");
    &methods.claims[0]
}

// ---------------------------------------------------------- binding --

#[test]
fn a_claim_naming_a_step_the_pipeline_runs_is_bound_to_its_key() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);
    let result = report.result(node).unwrap();

    let methods = bench.methods(&[node], &report, &[about("one line was copied", "copy")]);
    let check = only(&methods);

    assert_eq!(check.verdict, ClaimVerdict::Bound);
    assert_eq!(check.bound.len(), 1, "one computation stands behind it");
    assert_eq!(
        check.bound[0].key, result.key,
        "a claim binds to the step key, which is the identity of the \
         computation"
    );
    assert_eq!(check.bound[0].canonical, result.canonical);
    assert!(
        !check.bound[0].platform.is_empty(),
        "and to where that computation happened"
    );
    assert!(
        check.bound[0].recorded_at > 0,
        "and to when it happened, which is the record's own date"
    );
}

#[test]
fn a_claim_on_one_sample_binds_to_that_samples_own_computation() {
    let mut bench = Bench::new();
    let first = bench.sample("SRR1", "ACGTACGTAC");
    let second = bench.sample("SRR2", "TTTTACGTAC");
    let one = bench.step("copy", "cat {reads} > {out}", &[("reads", first)]);
    let two = bench.step("copy", "cat {reads} > {out}", &[("reads", second)]);
    let report = bench.run(&[one, two]);

    let mut claim = about("SRR2 carried four T bases", "copy");
    claim.scope = Some("SRR2".to_owned());
    let methods = bench.methods(&[one, two], &report, &[claim]);
    let check = only(&methods);

    assert_eq!(check.verdict, ClaimVerdict::Bound);
    assert_eq!(
        check.bound.len(),
        1,
        "a scoped claim binds to one sample's computation, not to the step's"
    );
    assert_eq!(check.bound[0].scope.as_deref(), Some("SRR2"));
    assert_eq!(
        check.bound[0].key,
        report.result(two).unwrap().key,
        "and to that sample's key rather than its sibling's"
    );
}

#[test]
fn a_claim_with_no_scope_binds_to_every_sample_the_step_ran_on() {
    let mut bench = Bench::new();
    let first = bench.sample("SRR1", "ACGTACGTAC");
    let second = bench.sample("SRR2", "TTTTACGTAC");
    let one = bench.step("copy", "cat {reads} > {out}", &[("reads", first)]);
    let two = bench.step("copy", "cat {reads} > {out}", &[("reads", second)]);
    let report = bench.run(&[one, two]);

    let methods = bench.methods(
        &[one, two],
        &report,
        &[about("every sample was copied", "copy")],
    );
    let check = only(&methods);

    assert_eq!(check.verdict, ClaimVerdict::Bound);
    assert_eq!(
        check.bound.len(),
        2,
        "the claim is about the step across both samples"
    );
    let mut scopes: Vec<&str> = check
        .bound
        .iter()
        .filter_map(|bound| bound.scope.as_deref())
        .collect();
    scopes.sort_unstable();
    assert_eq!(scopes, ["SRR1", "SRR2"]);
}

// ----------------------------------------- D45: the computation, not the run --

#[test]
fn a_claim_bound_to_a_cached_step_names_the_computation_that_produced_it() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);

    // The computation happens here.
    let first = bench.run(&[node]);
    assert_eq!(
        first.result(node).unwrap().outcome,
        Outcome::Ran,
        "the first run is the one that computes"
    );
    let produced = bench.prov.lookup(first.result(node).unwrap().key).unwrap();
    let recorded = produced.expect("the first run recorded it").recorded_at;

    // And this run is served from the cache.
    let second = bench.run(&[node]);
    assert_eq!(
        second.result(node).unwrap().outcome,
        Outcome::Cached,
        "the second run is served from the cache"
    );

    let methods = bench.methods(&[node], &second, &[about("one line was copied", "copy")]);
    let check = only(&methods);

    assert_eq!(
        check.verdict,
        ClaimVerdict::Bound,
        "a cached step backs its claim completely: the key is the identity \
         of the computation, so there is nothing weaker about it"
    );
    assert_eq!(check.bound[0].key, second.result(node).unwrap().key);
    assert_eq!(
        check.bound[0].recorded_at, recorded,
        "and the date is when the computation happened, not when this run \
         fetched it (D45)"
    );
}

#[test]
fn a_claim_never_enters_a_step_key() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);

    let first = bench.run(&[node]);
    let key = first.result(node).unwrap().key;
    let _ = bench.methods(&[node], &first, &[about("one line was copied", "copy")]);

    // The manuscript is edited: a claim added, one reworded, one removed.
    let second = bench.run(&[node]);
    let methods = bench.methods(
        &[node],
        &second,
        &[
            about("exactly one line was copied", "copy"),
            about("the copy preserved every byte", "copy"),
        ],
    );

    assert_eq!(
        second.result(node).unwrap().key,
        key,
        "editing a manuscript must not move a step key (D45)"
    );
    assert_eq!(
        second.result(node).unwrap().outcome,
        Outcome::Cached,
        "and must therefore rerun nothing"
    );
    assert_eq!(methods.claims.len(), 2);
}

// -------------------------------------------------------- unbacked --

#[test]
fn a_claim_naming_a_step_nothing_produces_is_unbacked() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);

    let methods = bench.methods(
        &[node],
        &report,
        &[about("three variants were called", "call")],
    );
    let check = only(&methods);

    match &check.verdict {
        ClaimVerdict::Unbacked(Unbacked::NoSuchStep { produced }) => {
            assert_eq!(
                produced,
                &["copy".to_owned()],
                "the complaint names what the pipeline does produce, so a \
                 renamed step is a one-line fix rather than a hunt"
            );
        }
        other => panic!("expected no such step, got {other:?}"),
    }
    assert!(check.bound.is_empty(), "nothing stands behind it");
}

#[test]
fn a_claim_whose_step_is_a_gap_is_unbacked_rather_than_bound() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);

    // A cache populated before provenance was kept: the output is there and
    // the record is not, so `copy` is a gap.
    let elsewhere = TempDir::new().unwrap();
    let empty = Prov::open(elsewhere.path()).unwrap();
    let mut methods = prov::methods(&bench.graph, &[node], &bench.tasks, &report, &empty).unwrap();
    methods.bind(&[about("one line was copied", "copy")]);
    let check = only(&methods);

    assert_eq!(methods.gaps.len(), 1, "the step itself is a gap");
    assert_eq!(
        check.verdict,
        ClaimVerdict::Unbacked(Unbacked::NoRecord),
        "a claim bound to a step nothing is known about is bound to nothing, \
         and reporting it as bound would be the plausible wrongness this \
         whole mechanism is against"
    );
}

#[test]
fn a_claim_naming_a_sample_the_step_did_not_run_on_is_unbacked() {
    let mut bench = Bench::new();
    let first = bench.sample("SRR1", "ACGTACGTAC");
    let second = bench.sample("SRR2", "TTTTACGTAC");
    let one = bench.step("copy", "cat {reads} > {out}", &[("reads", first)]);
    let two = bench.step("copy", "cat {reads} > {out}", &[("reads", second)]);
    let report = bench.run(&[one, two]);

    let mut claim = about("SRR3 carried four T bases", "copy");
    claim.scope = Some("SRR3".to_owned());
    let methods = bench.methods(&[one, two], &report, &[claim]);
    let check = only(&methods);

    match &check.verdict {
        ClaimVerdict::Unbacked(Unbacked::NoSuchScope { ran_on }) => {
            assert_eq!(
                ran_on,
                &["SRR1".to_owned(), "SRR2".to_owned()],
                "a dropped sample is the commonest reason to land here, so \
                 the complaint names the samples there were"
            );
        }
        other => panic!("expected no such scope, got {other:?}"),
    }
    assert!(methods.text().contains("SRR3"), "{}", methods.text());
}

#[test]
fn a_claim_naming_a_sample_of_a_step_that_ran_once_is_told_there_are_none() {
    let mut bench = Bench::new();
    let only_sample = bench.sample("SRR1", "ACGTACGTAC");
    let node = bench.step("copy", "cat {reads} > {out}", &[("reads", only_sample)]);
    let report = bench.run(&[node]);

    let mut claim = about("SRR2 carried four T bases", "copy");
    claim.scope = Some("SRR2".to_owned());
    let methods = bench.methods(&[node], &report, &[claim]);

    assert_eq!(
        only(&methods).verdict,
        ClaimVerdict::Unbacked(Unbacked::NoSuchScope { ran_on: Vec::new() }),
        "a step with one node has no scope at all: the graph gives one only \
         where something tells siblings apart (D15)"
    );
    let text = methods.text();
    assert!(
        text.contains("nothing tells its nodes apart"),
        "so the advice is to drop the sample, not to correct it: {text}"
    );
}

#[test]
fn a_claim_naming_a_parameter_the_step_does_not_take_is_unbacked() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "call",
        "cat {in} > {out}",
        &[("in", input)],
        &[("min_freq", "0.01")],
        &[],
    );
    let report = bench.run(&[node]);

    let mut claim = about("variants were called above 1%", "call");
    claim.restates = Some(Restated::Parameter {
        name: "min_frequency".to_owned(),
        value: "0.01".to_owned(),
    });
    let methods = bench.methods(&[node], &report, &[claim]);
    let check = only(&methods);

    match &check.verdict {
        ClaimVerdict::Unbacked(Unbacked::NoSuchParameter { takes }) => {
            assert_eq!(takes, &["min_freq".to_owned()]);
        }
        other => panic!("expected no such parameter, got {other:?}"),
    }
}

#[test]
fn a_claim_naming_a_tool_the_step_did_not_run_is_unbacked() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "align",
        "cat {in} > {out}",
        &[("in", input)],
        &[],
        &["python3"],
    );
    let report = bench.run(&[node]);

    let mut claim = about("reads were aligned with bwa", "align");
    claim.restates = Some(Restated::Tool {
        name: "bwa".to_owned(),
        version: "0.7.18".to_owned(),
    });
    let methods = bench.methods(&[node], &report, &[claim]);
    let check = only(&methods);

    match &check.verdict {
        ClaimVerdict::Unbacked(Unbacked::NoSuchTool { ran }) => {
            assert_eq!(ran, &["python3".to_owned()]);
        }
        other => panic!("expected no such tool, got {other:?}"),
    }
}

// ----------------------------------------------------- contradicted --

#[test]
fn a_claim_restating_a_parameter_the_run_disagrees_with_is_contradicted() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "call",
        "cat {in} > {out}",
        &[("in", input)],
        &[("min_freq", "0.05")],
        &[],
    );
    let report = bench.run(&[node]);

    let mut claim = about("variants were called above 1% allele frequency", "call");
    claim.restates = Some(Restated::Parameter {
        name: "min_freq".to_owned(),
        value: "0.01".to_owned(),
    });
    let methods = bench.methods(&[node], &report, &[claim]);
    let check = only(&methods);

    assert_eq!(
        check.verdict,
        ClaimVerdict::Contradicted {
            stated: "0.01".to_owned(),
            recorded: "0.05".to_owned(),
        },
        "the paper says 1% and the run says 5%, and both numbers are stated \
         so the fix needs no investigation"
    );
    assert!(
        !check.bound.is_empty(),
        "a contradicted claim is bound — to a computation that disagrees \
         with it, which is why it is not merely unbacked"
    );
}

#[test]
fn a_claim_restating_a_parameter_the_run_agrees_with_is_bound() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "call",
        "cat {in} > {out}",
        &[("in", input)],
        &[("min_freq", "0.01")],
        &[],
    );
    let report = bench.run(&[node]);

    let mut claim = about("variants were called above 1% allele frequency", "call");
    claim.restates = Some(Restated::Parameter {
        name: "min_freq".to_owned(),
        value: "0.01".to_owned(),
    });
    let methods = bench.methods(&[node], &report, &[claim]);

    assert_eq!(only(&methods).verdict, ClaimVerdict::Bound);
}

#[test]
fn a_parameter_is_compared_without_the_quotes_a_host_rendered_it_with() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "call",
        "cat {in} > {out}",
        &[("in", input)],
        // A Python string arrives rendered, exactly as `restating` meets it.
        &[("region", "'pol'")],
        &[],
    );
    let report = bench.run(&[node]);

    let mut claim = about("only pol was called", "call");
    claim.restates = Some(Restated::Parameter {
        name: "region".to_owned(),
        value: "pol".to_owned(),
    });
    let methods = bench.methods(&[node], &report, &[claim]);

    assert_eq!(
        only(&methods).verdict,
        ClaimVerdict::Bound,
        "a user writes `pol`, not `'pol'`, and one layer of rendering quotes \
         is not a disagreement"
    );
}

// ------------------------------------------------- tool versions --

#[test]
fn a_tool_version_is_matched_against_what_the_tool_printed() {
    use ligase::prov::version_states;

    // `samtools --version` prints its own name first; `minimap2 --version`
    // prints the version alone.
    assert!(version_states("samtools", "samtools 1.24", "1.24"));
    assert!(version_states("minimap2", "2.31-r1287", "2.31"));
    assert!(
        version_states("samtools", "samtools 1.24\nUsing htslib 1.24\n", "1.24"),
        "only the first line is the tool's own version"
    );
    assert!(
        !version_states("minimap2", "2.41-r1287", "2.4"),
        "a prefix must end at a component boundary, or 2.4 would claim 2.41"
    );
    assert!(!version_states("samtools", "samtools 1.24", "1.21"));
}

#[test]
fn a_claim_restating_a_tool_version_the_run_disagrees_with_is_contradicted() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "align",
        "cat {in} > {out}",
        &[("in", input)],
        &[],
        &["python3"],
    );
    let report = bench.run(&[node]);

    let recorded = {
        let methods = bench.methods(&[node], &report, &[]);
        methods.groups[0].record.tools[0].1.clone()
    };

    let mut claim = about("run under Python 2.7", "align");
    claim.restates = Some(Restated::Tool {
        name: "python3".to_owned(),
        version: "2.7".to_owned(),
    });
    let methods = bench.methods(&[node], &report, &[claim]);

    assert_eq!(
        only(&methods).verdict,
        ClaimVerdict::Contradicted {
            stated: "2.7".to_owned(),
            recorded,
        },
        "the version a paper names is checked against the version that ran — \
         the sentence D42 found the engine getting wrong for bwa"
    );
}

// ----------------------------------------------- what the write-up says --

#[test]
fn an_unbacked_claim_is_named_in_the_write_up_the_way_an_uncited_tool_is() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);

    let methods = bench.methods(
        &[node],
        &report,
        &[
            about("one line was copied", "copy"),
            about("three variants were called", "call"),
        ],
    );
    let text = methods.text();

    assert!(
        text.contains("Claims"),
        "the claims have a section of their own: {text}"
    );
    assert!(
        text.contains("three variants were called"),
        "the sentence a reader will grep for is quoted back: {text}"
    );
    assert!(
        text.contains("has nothing behind it") && text.contains("produces `call`"),
        "and the complaint is the shape D24's uncited line is: {text}"
    );
    assert_eq!(methods.unbacked().len(), 1);
}

#[test]
fn a_bound_claim_names_the_step_and_the_computation_in_the_write_up() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "call",
        "cat {in} > {out}",
        &[("in", input)],
        &[("min_freq", "0.01")],
        &[],
    );
    let report = bench.run(&[node]);
    let key = report.result(node).unwrap().key.to_hex();

    let mut claim = about("variants were called above 1% allele frequency", "call");
    claim.restates = Some(Restated::Parameter {
        name: "min_freq".to_owned(),
        value: "0.01".to_owned(),
    });
    let methods = bench.methods(&[node], &report, &[claim]);
    let text = methods.text();

    assert!(text.contains("variants were called above 1% allele frequency"));
    assert!(
        text.contains("`call`"),
        "a bound claim names the step behind it: {text}"
    );
    assert!(
        text.contains("min_freq = 0.01"),
        "and the value it restates, as the run recorded it: {text}"
    );

    // Not the key. It is the identity of a *computation*, so it moves when
    // the computation's inputs do — including under edits that cannot
    // change the number, which is what makes it unsafe to print in a
    // manuscript (D59). The step key is still recorded, in provenance and
    // in the bundle's claims table.
    assert!(
        !text.contains(&key[..16]),
        "a manuscript must not be handed the identifier that moves: {text}"
    );
}

#[test]
fn a_bound_claim_names_the_digest_of_what_it_rests_on() {
    // D59, measured by a consumer against 0.3.0: adding `cite=` to a step
    // moved the claim's printed identifier while the artifact stayed
    // byte-identical, nothing reran, and both provenance rows carried the
    // same canonical digest. So the paper's identifier stopped resolving
    // for an edit that provably cannot change the result.
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with("call", "cat {in} > {out}", &[("in", input)], &[], &[]);
    let report = bench.run(&[node]);
    let canonical = report.result(node).unwrap().canonical.to_hex();

    let claim = about("one variant was called", "call");
    let text = bench.methods(&[node], &report, &[claim]).text();

    assert!(
        text.contains(&canonical[..12]),
        "the digest of the canonical form is what a reader can verify, and \
         it is what the sentence gives: {text}"
    );
}

#[test]
fn an_edit_that_cannot_change_the_number_does_not_move_what_a_claim_prints() {
    // The reported reproduction, as a test. `cite` never joins a cache key
    // (D45), so the step is served from the cache and the artifact is the
    // same bytes — and what the claim prints has to be the same too.
    let identifier = |cited: bool| {
        let mut bench = Bench::new();
        let input = bench.input("in.txt", "one\n");
        let node = bench.step_with("call", "cat {in} > {out}", &[("in", input)], &[], &[]);
        if cited {
            bench.cite(node, &["Van Rossum G (1995) Python reference manual."]);
        }
        let report = bench.run(&[node]);
        let canonical = report.result(node).unwrap().canonical.to_hex();
        let claim = about("one variant was called", "call");
        let text = bench.methods(&[node], &report, &[claim]).text();
        assert!(text.contains(&canonical[..12]), "{text}");
        canonical[..12].to_owned()
    };

    assert_eq!(
        identifier(false),
        identifier(true),
        "adding a citation cannot change a number, so it cannot change the \
         identifier a paper cites"
    );
}

#[test]
fn a_contradicted_claim_states_both_numbers_in_the_write_up() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "call",
        "cat {in} > {out}",
        &[("in", input)],
        &[("min_freq", "0.05")],
        &[],
    );
    let report = bench.run(&[node]);

    let mut claim = about("variants were called above 1% allele frequency", "call");
    claim.restates = Some(Restated::Parameter {
        name: "min_freq".to_owned(),
        value: "0.01".to_owned(),
    });
    let methods = bench.methods(&[node], &report, &[claim]);
    let text = methods.text();

    assert!(text.contains("0.01"), "what the paper says: {text}");
    assert!(text.contains("0.05"), "what the run did: {text}");
}

#[test]
fn the_summary_counts_the_claims_and_says_how_many_are_bound() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);

    let all_bound = bench.methods(
        &[node],
        &report,
        &[
            about("one line was copied", "copy"),
            about("the copy preserved every byte", "copy"),
        ],
    );
    assert_eq!(
        all_bound.summary(),
        "1 step, all accounted for. 2 claims, all bound."
    );

    let one_short = bench.methods(
        &[node],
        &report,
        &[
            about("one line was copied", "copy"),
            about("three variants were called", "call"),
        ],
    );
    assert_eq!(
        one_short.summary(),
        "1 step, all accounted for. 2 claims: 1 bound, 1 not."
    );
}

#[test]
fn a_pipeline_that_declares_no_claims_writes_the_text_it_wrote_before() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "copy",
        "cat {in} > {out}",
        &[("in", input)],
        &[("min_freq", "0.01")],
        &["python3"],
    );
    let report = bench.run(&[node]);

    // The write-up as `methods` builds it, with nothing bound against it,
    // beside the same write-up with `bind` called over an empty list.
    let bare = prov::methods(&bench.graph, &[node], &bench.tasks, &report, &bench.prov).unwrap();
    let bound = bench.methods(&[node], &report, &[]);

    assert_eq!(
        bound.text(),
        bare.text(),
        "a pipeline with no claims must read exactly as it read before this \
         milestone — no empty section, no heading, nothing"
    );
    assert_eq!(bound.summary(), bare.summary());
    assert_eq!(bound.summary(), "1 step, all accounted for.");
    assert!(!bare.text().contains("Claims"));
}

// ------------------------------------------------------- the bundle --

#[test]
fn claims_travel_in_the_bundle_with_the_key_each_one_rests_on() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);
    let key = report.result(node).unwrap().key.to_hex();

    let methods = bench.methods(
        &[node],
        &report,
        &[
            about("one line was copied", "copy"),
            about("three variants were called", "call"),
        ],
    );

    let into = bench.path("bundle");
    let summary = bundle::write(
        &into,
        &bundle::Request {
            graph: &bench.graph,
            roots: &[node],
            report: &report,
            prov: &bench.prov,
            methods: &methods,
            include: &[],
            queries: None,
            thin: false,
        },
    )
    .unwrap();

    assert!(
        summary.files.contains(&"claims.tsv".to_owned()),
        "a bundle travels with the paper, so the paper's claims are in it: \
         {:?}",
        summary.files
    );
    let table = fs::read_to_string(into.join("claims.tsv")).unwrap();
    assert!(table.starts_with("claim\tverdict\tstep\tscope\tkey\tcanonical\n"));
    assert!(
        table.contains(&key),
        "the bound claim carries the key it rests on: {table}"
    );
    assert!(
        table.contains("unbacked"),
        "and the unbacked one is in the table rather than left out: {table}"
    );
}

#[test]
fn a_bundle_for_a_pipeline_with_no_claims_has_no_claims_table() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);
    let methods = bench.methods(&[node], &report, &[]);

    let into = bench.path("bundle");
    let summary = bundle::write(
        &into,
        &bundle::Request {
            graph: &bench.graph,
            roots: &[node],
            report: &report,
            prov: &bench.prov,
            methods: &methods,
            include: &[],
            queries: None,
            thin: false,
        },
    )
    .unwrap();

    assert!(
        !summary.files.contains(&"claims.tsv".to_owned()),
        "an empty table is a promise nobody made: {:?}",
        summary.files
    );
}

// --------------------------------------------- what the index records --

#[test]
fn the_write_up_indexes_every_result_by_the_computation_that_produced_it() {
    let mut bench = Bench::new();
    let first = bench.sample("SRR1", "ACGTACGTAC");
    let second = bench.sample("SRR2", "TTTTACGTAC");
    let one = bench.step("copy", "cat {reads} > {out}", &[("reads", first)]);
    let two = bench.step("copy", "cat {reads} > {out}", &[("reads", second)]);
    let report = bench.run(&[one, two]);

    let methods = bench.methods(&[one, two], &report, &[]);

    assert_eq!(
        methods.groups.len(),
        1,
        "two samples run the same way are one group"
    );
    assert_eq!(
        methods.produced.len(),
        2,
        "and two computations, because a group keeps one representative \
         record and a claim binds to a computation"
    );
    let keys: Vec<_> = methods.produced.iter().map(|row| row.key).collect();
    assert!(keys.contains(&report.result(one).unwrap().key));
    assert!(keys.contains(&report.result(two).unwrap().key));
}
