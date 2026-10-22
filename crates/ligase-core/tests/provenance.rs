//! M5: what a run records about how it produced things.
//!
//! The property under test is the one D22 gave up when it dropped `@PG`
//! lines from the hash: a cache hit can no longer prove which command
//! produced an object, so something else has to. These tests are about that
//! something else — that it is written when a step runs, that it is still
//! there when the step is served from the cache, and that it survives the
//! cache being thrown away, because a key format and a methods section want
//! different lifetimes.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use ligase::cache::Cache;
use ligase::dim::Dimensions;
use ligase::exec::{self, Host, Options, Recipe, RunReport, Stores, Task};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::hash::ContentHash;
use ligase::load::{self, Declared};
use ligase::note::Note;
use ligase::prov::{self, Prov, bundle};
use ligase::types::{DataKind, Dataset};
use tempfile::TempDir;

// ------------------------------------------------------------ scaffold --

/// A workspace: a cache, a provenance store, inputs, and a graph.
struct Bench {
    dir: TempDir,
    root: PathBuf,
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
            root,
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

    /// A FASTA input loaded the way `lg.reference` loads one, so its contigs
    /// and their checksums are inferred rather than invented.
    fn reference(&mut self, name: &str, contents: &str) -> NodeId {
        let path = self.path(name);
        fs::write(&path, contents).unwrap();
        let dataset = load::reference(
            &path,
            &Declared {
                organism: Some(vec!["HIV-1".into()]),
                build: Some("HXB2".into()),
                ..Declared::default()
            },
        )
        .unwrap();
        self.graph.add_input(name, dataset)
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
        });
        node
    }

    /// Rewrite a step's recorded code, as editing a comment in it would.
    fn edit(&mut self, node: NodeId, code: &str) {
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.node == node)
            .unwrap();
        task.code = code.to_owned();
    }

    /// Rewrite a step's command, as changing what it does would.
    fn retemplate(&mut self, node: NodeId, command: &str) {
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.node == node)
            .unwrap();
        task.recipe = Recipe::Shell(command.to_owned());
    }

    /// Give a step references of its own, as `@lg.step(cite=...)` does.
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

/// A one-contig FASTA whose name is an accession, as `K03455.1` is.
const HXB2: &str = ">K03455.1 Human immunodeficiency virus type 1 (HXB2)\nACGTACGTAC\nGTAC\n";

// -------------------------------------------------- what a run records --

#[test]
fn a_step_that_runs_records_the_command_that_ran() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);

    let report = bench.run(&[node]);
    let key = report.result(node).unwrap().key;

    let record = bench.prov.lookup(key).unwrap().expect("a record");
    let command = record.command.expect("a shell step records its command");
    assert!(
        command.starts_with("cat ") && command.contains("in.txt"),
        "the command should be the one that ran, filled in: {command}"
    );
    assert_eq!(record.step, "copy");
    assert_eq!(record.code, "def copy(): ...");
    assert_eq!(record.engine, ligase::version());
    assert!(!record.platform.is_empty(), "the platform is recorded");
}

#[test]
fn the_record_holds_the_output_both_ways() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);

    let report = bench.run(&[node]);
    let result = report.result(node).unwrap();
    let record = bench.prov.lookup(result.key).unwrap().expect("a record");

    assert_eq!(record.output_bytes, result.hash);
    assert_eq!(record.output_canonical, result.canonical);
}

#[test]
fn an_input_is_recorded_by_content_not_by_name() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);

    let report = bench.run(&[node]);
    let key = report.result(node).unwrap().key;
    let record = bench.prov.lookup(key).unwrap().expect("a record");

    assert_eq!(record.inputs.len(), 1);
    let recorded = &record.inputs[0];
    assert_eq!(recorded.param, "in");
    // Both hashes: the bytes say which file, the canonical hash says what a
    // downstream key was actually built from.
    assert_eq!(
        recorded.bytes,
        bench.cache.content_hash(&bench.path("in.txt")).unwrap()
    );
    assert_eq!(
        recorded.canonical,
        bench
            .cache
            .semantic_hash(DataKind::Reference, &bench.path("in.txt"))
            .unwrap()
    );
}

#[test]
fn parameters_are_recorded_as_they_were_hashed() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "copy",
        "cat {in} > {out}",
        &[("in", input)],
        &[("min_freq", "0.01")],
        &[],
    );

    let report = bench.run(&[node]);
    let key = report.result(node).unwrap().key;
    let record = bench.prov.lookup(key).unwrap().expect("a record");

    assert_eq!(
        record.parameters,
        vec![("min_freq".to_owned(), "0.01".to_owned())]
    );
}

#[test]
fn a_tool_is_recorded_with_the_whole_version_it_printed() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "copy",
        "cat {in} > {out}",
        &[("in", input)],
        &[],
        &["python3"],
    );

    let report = bench.run(&[node]);
    let key = report.result(node).unwrap().key;
    let record = bench.prov.lookup(key).unwrap().expect("a record");

    assert_eq!(record.tools.len(), 1);
    assert_eq!(record.tools[0].0, "python3");
    assert!(
        record.tools[0].1.contains("Python"),
        "the version is what the tool printed: {:?}",
        record.tools[0].1
    );
}

// ------------------------------------------------------ the reference --

#[test]
fn a_reference_is_pinned_by_accession_and_checksum() {
    let mut bench = Bench::new();
    let reference = bench.reference("HXB2.fasta", HXB2);
    let node = bench.step("copy", "cat {ref} > {out}", &[("ref", reference)]);

    let report = bench.run(&[node]);
    let key = report.result(node).unwrap().key;
    let record = bench.prov.lookup(key).unwrap().expect("a record");

    assert_eq!(record.references.len(), 1);
    let pinned = &record.references[0];
    // The accession is read from the defline, not declared: it is the contig
    // name, which for a reference deposited under one is the accession with
    // its version.
    assert_eq!(pinned.accession.as_deref(), Some("K03455.1"));
    assert_eq!(pinned.build.as_deref(), Some("HXB2"));
    assert_eq!(pinned.length, 14);
    assert_eq!(pinned.contigs, 1);
    // The MD5 the SAM specification defines for @SQ M5, over the uppercased
    // sequence with whitespace removed.
    assert_eq!(
        pinned.checksum.as_deref(),
        Some("7cd4b12a021ac61104e680b721b76271"),
        "the contig checksum the loader computed"
    );
    assert!(!pinned.fingerprint.is_empty());
}

#[test]
fn a_contig_name_that_is_not_an_accession_is_not_called_one() {
    let mut bench = Bench::new();
    let reference = bench.reference("chr.fasta", ">chr1 some assembly\nACGT\n");
    let node = bench.step("copy", "cat {ref} > {out}", &[("ref", reference)]);

    let report = bench.run(&[node]);
    let key = report.result(node).unwrap().key;
    let record = bench.prov.lookup(key).unwrap().expect("a record");

    let pinned = &record.references[0];
    assert_eq!(
        pinned.accession, None,
        "`chr1` is a sequence name, not an accession"
    );
    // What cannot be named can still be pinned: the fingerprint is always
    // there, so two references that describe the same still differ here.
    assert!(!pinned.fingerprint.is_empty());
}

// ------------------------------------------------------- the lifetime --

#[test]
fn a_cached_step_still_knows_how_it_was_produced() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);

    let first = bench.run(&[node]);
    let key = first.result(node).unwrap().key;
    let when_it_ran = bench.prov.lookup(key).unwrap().expect("a record");

    // The second run serves it from the cache and runs no command at all.
    let second = bench.run(&[node]);
    assert_eq!(second.result(node).unwrap().key, key);

    let still = bench.prov.lookup(key).unwrap().expect("still a record");
    assert_eq!(still.command, when_it_ran.command);
    assert_eq!(still.output_canonical, when_it_ran.output_canonical);
}

#[test]
fn provenance_outlives_the_cache() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);

    let report = bench.run(&[node]);
    let key = report.result(node).unwrap().key;
    assert!(bench.prov.lookup(key).unwrap().is_some());

    // The most brutal version of a key-format bump: the cache's whole
    // database goes. Provenance is a separate file with a separate lifetime,
    // so the answer to "how was this produced?" is not collateral damage.
    fs::remove_file(bench.root.join("cache.sqlite")).unwrap();
    let _reopened = Cache::open(&bench.root).unwrap();

    let reopened = Prov::open(&bench.root).unwrap();
    let record = reopened.lookup(key).unwrap().expect("a record");
    assert_eq!(record.step, "copy");
    assert!(record.command.is_some());
}

#[test]
fn a_step_that_never_ran_has_no_record() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);

    // The key a run would compute, without the run.
    let report = bench.run(&[node]);
    let key = report.result(node).unwrap().key;

    let elsewhere = TempDir::new().unwrap();
    let empty = Prov::open(elsewhere.path()).unwrap();
    assert!(
        empty.lookup(key).unwrap().is_none(),
        "an empty store knows nothing, and says so rather than guessing"
    );
}

// ------------------------------------------------- the methods section --

/// The methods section for a run that has just happened.
fn methods_of(bench: &Bench, roots: &[NodeId], report: &RunReport) -> prov::Methods {
    prov::methods(&bench.graph, roots, &bench.tasks, report, &bench.prov).unwrap()
}

#[test]
fn the_paragraph_names_the_tool_version_that_ran() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "copy",
        "cat {in} > {out}",
        &[("in", input)],
        &[],
        &["python3"],
    );

    let report = bench.run(&[node]);
    let text = methods_of(&bench, &[node], &report).text();

    assert!(text.contains("python3"), "{text}");
    assert!(text.contains("Python 3"), "the version that ran: {text}");
}

#[test]
fn the_paragraph_names_the_parameters_as_they_were_set() {
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
    let text = methods_of(&bench, &[node], &report).text();

    assert!(text.contains("min_freq = 0.01"), "{text}");
}

#[test]
fn the_paragraph_pins_the_reference_by_accession() {
    let mut bench = Bench::new();
    let reference = bench.reference("HXB2.fasta", HXB2);
    let node = bench.step("copy", "cat {ref} > {out}", &[("ref", reference)]);

    let report = bench.run(&[node]);
    let text = methods_of(&bench, &[node], &report).text();

    assert!(text.contains("K03455.1"), "{text}");
    assert!(text.contains("14 bp"), "{text}");
    assert!(text.contains("MD5 7cd4b12a"), "{text}");
    assert!(text.contains("the HXB2 reference"), "{text}");
}

#[test]
fn the_paragraph_shows_the_command_as_it_was_written() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);

    let report = bench.run(&[node]);
    let text = methods_of(&bench, &[node], &report).text();

    // The template, not the filled command: a filled one names a staging
    // file inside the cache, which is true and of no use in a paper.
    assert!(text.contains("cat {in} > {out}"), "{text}");
}

#[test]
fn a_known_tool_is_cited_and_an_unknown_one_is_named_as_uncited() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "copy",
        "cat {in} > {out}",
        &[("in", input)],
        &[],
        &["python3"],
    );

    let report = bench.run(&[node]);
    let text = methods_of(&bench, &[node], &report).text();

    assert!(
        text.contains("No reference is recorded for python3"),
        "an uncited tool is named rather than quietly dropped: {text}"
    );
}

#[test]
fn a_step_that_declares_its_own_reference_is_cited_by_it() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step_with(
        "copy",
        "cat {in} > {out}",
        &[("in", input)],
        &[],
        &["python3"],
    );
    bench.cite(node, &["Van Rossum G (1995) Python reference manual."]);

    let report = bench.run(&[node]);
    let text = methods_of(&bench, &[node], &report).text();

    assert!(text.contains("Van Rossum"), "{text}");
    assert!(
        !text.contains("No reference is recorded"),
        "a step that said what to cite has no gap: {text}"
    );
}

#[test]
fn nodes_of_one_step_are_one_sentence() {
    let mut bench = Bench::new();
    let first = bench.input("a.txt", "a\n");
    let second = bench.input("b.txt", "b\n");
    let one = bench.step("copy", "cat {in} > {out}", &[("in", first)]);
    let two = bench.step("copy", "cat {in} > {out}", &[("in", second)]);

    let report = bench.run(&[one, two]);
    let methods = methods_of(&bench, &[one, two], &report);

    assert_eq!(methods.groups.len(), 1, "{:?}", methods.groups);
    assert_eq!(methods.groups[0].nodes.len(), 2);
    assert_eq!(methods.summary(), "2 steps, all accounted for.");
}

#[test]
fn nodes_of_one_step_run_differently_get_a_sentence_each() {
    let mut bench = Bench::new();
    let first = bench.input("a.txt", "a\n");
    let second = bench.input("b.txt", "b\n");
    let one = bench.step_with(
        "call",
        "cat {in} > {out}",
        &[("in", first)],
        &[("min_freq", "0.01")],
        &[],
    );
    let two = bench.step_with(
        "call",
        "cat {in} > {out}",
        &[("in", second)],
        &[("min_freq", "0.5")],
        &[],
    );

    let report = bench.run(&[one, two]);
    let methods = methods_of(&bench, &[one, two], &report);

    // Where two nodes of a step really were run differently, the difference
    // is the thing worth reporting, so neither sentence speaks for both.
    assert_eq!(methods.groups.len(), 2, "{:?}", methods.groups);
    let text = methods.text();
    assert!(text.contains("min_freq = 0.01"), "{text}");
    assert!(text.contains("min_freq = 0.5"), "{text}");
}

#[test]
fn a_step_with_no_record_is_reported_as_a_gap() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);

    // A cache populated before provenance was kept: the output is there and
    // the record is not.
    let elsewhere = TempDir::new().unwrap();
    let empty = Prov::open(elsewhere.path()).unwrap();
    let methods = prov::methods(&bench.graph, &[node], &bench.tasks, &report, &empty).unwrap();

    assert_eq!(methods.groups.len(), 0);
    assert_eq!(methods.gaps.len(), 1);
    assert_eq!(methods.gaps[0].step, "copy");

    let text = methods.text();
    assert!(text.contains("recorded no provenance"), "{text}");
    assert!(
        text.contains("ligase run --force"),
        "a gap says how to fill it: {text}"
    );
    assert_eq!(methods.summary(), "1 step: 0 accounted for, 1 not.");
}

// ------------------------------------------------------- the bundle --

/// Write a bundle for a run, into a fresh directory.
fn bundle_of(bench: &Bench, roots: &[NodeId], report: &RunReport, into: &Path) -> bundle::Summary {
    let methods = methods_of(bench, roots, report);
    bundle::write(
        into,
        &bundle::Request {
            graph: &bench.graph,
            roots,
            report,
            prov: &bench.prov,
            methods: &methods,
            include: &[],
        },
    )
    .unwrap()
}

#[test]
fn a_bundle_holds_what_it_claims_to() {
    let mut bench = Bench::new();
    let reference = bench.reference("HXB2.fasta", HXB2);
    let node = bench.step("copy", "cat {ref} > {out}", &[("ref", reference)]);
    let report = bench.run(&[node]);

    let into = TempDir::new().unwrap();
    let summary = bundle_of(&bench, &[node], &report, into.path());

    for name in [
        "METHODS.md",
        "README.md",
        "results.tsv",
        "commands.tsv",
        "inputs.tsv",
        "references.tsv",
        "environment.tsv",
        "MANIFEST",
        "steps/copy.py",
    ] {
        assert!(
            into.path().join(name).is_file(),
            "the bundle should hold {name}; it holds {:?}",
            summary.files
        );
    }
    assert_eq!(summary.steps, 1);
    assert_eq!(summary.gaps, 0);

    // The reference is pinned where a reader will look for it.
    let references = fs::read_to_string(into.path().join("references.tsv")).unwrap();
    assert!(references.contains("K03455.1"), "{references}");
    assert!(
        references.contains("7cd4b12a021ac61104e680b721b76271"),
        "{references}"
    );

    // And the command that ran is there verbatim, which is the thing a
    // cache hit can no longer prove (D22).
    let commands = fs::read_to_string(into.path().join("commands.tsv")).unwrap();
    assert!(commands.contains("cat "), "{commands}");
    assert!(commands.contains("HXB2.fasta"), "{commands}");
}

#[test]
fn the_manifest_hashes_every_file_in_the_bundle() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);

    let into = TempDir::new().unwrap();
    let summary = bundle_of(&bench, &[node], &report, into.path());

    let manifest = fs::read_to_string(into.path().join("MANIFEST")).unwrap();
    let mut lines = manifest.lines();
    assert_eq!(
        lines.next(),
        Some("# blake3"),
        "the manifest names its hash"
    );

    let mut listed = 0;
    for line in lines {
        let (hash, name) = line.split_once("  ").expect("hash then name");
        let path = into.path().join(name);
        assert!(path.is_file(), "{name} is listed and absent");
        assert_eq!(
            hash,
            ContentHash::of_file(&path).unwrap().to_hex(),
            "{name} does not match its manifest line"
        );
        listed += 1;
    }
    // Everything but the manifest itself, which cannot hash itself.
    assert_eq!(listed, summary.files.len());
}

#[test]
fn a_bundle_verifies_against_the_run_it_came_from() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);

    let into = TempDir::new().unwrap();
    bundle_of(&bench, &[node], &report, into.path());

    let again = bench.run(&[node]);
    let checked = bundle::verify(into.path(), &bench.graph, &[node], &again).unwrap();

    assert!(checked.reproduced(), "{}", checked.text());
    assert_eq!(checked.verdict(), bundle::Match::Identical);
    assert_eq!(checked.summary(), "1 result: 1 identical.");
}

#[test]
fn a_bundle_verifies_on_a_clean_cache() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);

    let into = TempDir::new().unwrap();
    bundle_of(&bench, &[node], &report, into.path());

    // What a clean machine is, to this engine: an empty cache, nothing
    // computed, nothing remembered. The step runs again from nothing.
    let elsewhere = TempDir::new().unwrap();
    let cache = Cache::open(&elsewhere.path().join(".ligase")).unwrap();
    let prov = Prov::open(&elsewhere.path().join(".ligase")).unwrap();
    let fresh = exec::run(
        &bench.graph,
        &[node],
        &bench.tasks,
        &Stores {
            cache: &cache,
            prov: &prov,
        },
        &Options::default(),
        &Refusing,
    );
    assert_eq!(fresh.counts().ran, 1, "a clean cache computes it again");

    let checked = bundle::verify(into.path(), &bench.graph, &[node], &fresh).unwrap();
    assert!(checked.reproduced(), "{}", checked.text());
}

#[test]
fn a_result_that_moved_is_a_finding() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);

    let into = TempDir::new().unwrap();
    bundle_of(&bench, &[node], &report, into.path());

    // The step now produces something else.
    bench.retemplate(node, "echo something-else > {out}");
    let changed = bench.run(&[node]);

    let checked = bundle::verify(into.path(), &bench.graph, &[node], &changed).unwrap();

    assert!(!checked.reproduced());
    assert_eq!(checked.verdict(), bundle::Match::Differs);
    let text = checked.text();
    assert!(text.contains("differs"), "{text}");
    assert!(text.contains("expected"), "{text}");
}

#[test]
fn the_same_result_from_a_different_key_is_reproduced() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);

    let into = TempDir::new().unwrap();
    bundle_of(&bench, &[node], &report, into.path());

    // A comment edited, a tool upgraded, a different machine: the key moves
    // and the result does not. This is the outcome a reviewer on another
    // machine actually gets, and the one a bundle exists to report.
    bench.edit(node, "def copy(): ...  # same thing, said differently");
    let again = bench.run(&[node]);

    let checked = bundle::verify(into.path(), &bench.graph, &[node], &again).unwrap();

    assert_eq!(checked.verdict(), bundle::Match::Reproduced);
    assert!(checked.reproduced(), "{}", checked.text());
    assert!(!checked.checks[0].same_key, "the key moved");
    let text = checked.text();
    assert!(text.contains("the result is the same"), "{text}");
    assert!(text.contains("environment moved"), "{text}");
}

#[test]
fn a_step_the_pipeline_no_longer_produces_is_missing() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let first = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let second = bench.step("twice", "cat {in} {in} > {out}", &[("in", first)]);
    let report = bench.run(&[second]);

    let into = TempDir::new().unwrap();
    bundle_of(&bench, &[second], &report, into.path());

    // Only the first step is asked for now, so the bundle's second row has
    // nothing to compare against.
    let narrower = bench.run(&[first]);
    let checked = bundle::verify(into.path(), &bench.graph, &[first], &narrower).unwrap();

    assert_eq!(checked.verdict(), bundle::Match::Missing);
    let missing: Vec<&str> = checked
        .checks
        .iter()
        .filter(|check| check.verdict == bundle::Match::Missing)
        .map(|check| check.step.as_str())
        .collect();
    assert_eq!(missing, vec!["twice"]);
}

#[test]
fn an_input_that_is_not_the_one_recorded_is_reported() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);

    let into = TempDir::new().unwrap();
    bundle_of(&bench, &[node], &report, into.path());

    // The same filename, different content: exactly what a content hash is
    // for, and what a bundle cannot fetch for itself (D8).
    fs::write(bench.path("in.txt"), "something else\n").unwrap();
    let again = bench.run(&[node]);

    let checked = bundle::verify(into.path(), &bench.graph, &[node], &again).unwrap();

    assert_eq!(checked.inputs.len(), 1);
    assert_eq!(checked.inputs[0].presence, bundle::Presence::Changed);
    let text = checked.text();
    assert!(text.contains("changed"), "{text}");
    assert!(
        text.contains("A changed input explains a changed output"),
        "a differing result is read alongside its inputs: {text}"
    );
}

#[test]
fn a_bundle_carries_the_files_it_was_given() {
    let mut bench = Bench::new();
    let input = bench.input("in.txt", "one\n");
    let node = bench.step("copy", "cat {in} > {out}", &[("in", input)]);
    let report = bench.run(&[node]);

    let pipeline = bench.path("pipeline.py");
    fs::write(&pipeline, "pipe = lg.Pipeline(...)\n").unwrap();

    let into = TempDir::new().unwrap();
    let methods = methods_of(&bench, &[node], &report);
    bundle::write(
        into.path(),
        &bundle::Request {
            graph: &bench.graph,
            roots: &[node],
            report: &report,
            prov: &bench.prov,
            methods: &methods,
            include: &[pipeline],
        },
    )
    .unwrap();

    let carried = into.path().join("pipeline").join("pipeline.py");
    assert!(carried.is_file());
    assert_eq!(
        fs::read_to_string(carried).unwrap(),
        "pipe = lg.Pipeline(...)\n"
    );
}

#[test]
fn a_parameter_that_only_names_the_sample_does_not_split_a_step() {
    let mut bench = Bench::new();
    let first = bench.sample("SRR000001", "ACGT");
    let second = bench.sample("SRR000002", "ACGG");
    // Each node is told which sample it is for, as the demo's `align` is so
    // that `samtools addreplacerg` can write a read group.
    let one = bench.step_with(
        "align",
        "cat {in} > {out}",
        &[("in", first)],
        &[("sample_id", "'SRR000001'"), ("threads", "4")],
        &[],
    );
    let two = bench.step_with(
        "align",
        "cat {in} > {out}",
        &[("in", second)],
        &[("sample_id", "'SRR000002'"), ("threads", "4")],
        &[],
    );

    let report = bench.run(&[one, two]);
    let methods = methods_of(&bench, &[one, two], &report);

    // One sentence, not two: being for a different sample is what the
    // samples list already says.
    assert_eq!(methods.groups.len(), 1, "{:?}", methods.groups);
    assert_eq!(methods.groups[0].per_sample, vec!["sample_id".to_owned()]);

    let text = methods.text();
    assert!(text.contains("threads = 4"), "{text}");
    assert!(
        text.contains("sample_id naming the sample"),
        "the parameter is named, not pinned to one value: {text}"
    );
    assert!(
        !text.contains("SRR000001'"),
        "no one sample's value stands for all of them: {text}"
    );
}

#[test]
fn a_parameter_that_differs_for_another_reason_still_splits_a_step() {
    let mut bench = Bench::new();
    let first = bench.sample("SRR000001", "ACGT");
    let second = bench.sample("SRR000002", "ACGG");
    let one = bench.step_with(
        "call",
        "cat {in} > {out}",
        &[("in", first)],
        &[("sample_id", "'SRR000001'"), ("min_freq", "0.01")],
        &[],
    );
    let two = bench.step_with(
        "call",
        "cat {in} > {out}",
        &[("in", second)],
        &[("sample_id", "'SRR000002'"), ("min_freq", "0.5")],
        &[],
    );

    let report = bench.run(&[one, two]);
    let methods = methods_of(&bench, &[one, two], &report);

    // The threshold is a real difference, so it is still reported as one.
    assert_eq!(methods.groups.len(), 2, "{:?}", methods.groups);
    let text = methods.text();
    assert!(text.contains("min_freq = 0.01"), "{text}");
    assert!(text.contains("min_freq = 0.5"), "{text}");
}
