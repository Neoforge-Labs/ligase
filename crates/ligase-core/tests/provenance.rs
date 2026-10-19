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
use ligase::load::{self, Declared};
use ligase::prov::Prov;
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

    /// A FASTA input loaded the way `lg.reference` loads one, so its contigs
    /// and their checksums are inferred rather than invented.
    fn reference(&mut self, name: &str, contents: &str) -> NodeId {
        let path = self.path(name);
        fs::write(&path, contents).unwrap();
        let dataset = load::reference(
            &path,
            &Declared {
                organism: Some("HIV-1".into()),
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
        });
        node
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
    ) -> Result<(), String> {
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
