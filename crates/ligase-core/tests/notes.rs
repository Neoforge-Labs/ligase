//! D26: a sample identifier that is a file, noticed from end to end.
//!
//! `bcftools` names a VCF's sample column after its input BAM's path when
//! the BAM carries no `@RG SM`, and a Ligase run hands it a file inside the
//! content-addressed store. A sample set is a dimension, so the identifier
//! is hashed as the file writes it and the hash of such an output depends on
//! where the cache is — which is what made M5's bundle verification report
//! `differs` on every sample with byte-identical variant records.
//!
//! The rule itself is unit-tested in `canon::note`, and the canonical forms
//! in `tests/canon.rs`. These tests are about the other half: that what was
//! noticed reaches the places a user looks — the run report, the methods
//! section, the bundle and its verification — and that it still does when
//! nothing ran, because a cache hit does not reopen the file.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use ligase::cache::Cache;
use ligase::dim::Dimensions;
use ligase::exec::{self, Host, Options, Outcome, Recipe, RunReport, Stores, Task};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::note::Note;
use ligase::prov::{self, Prov, bundle};
use ligase::types::{DataKind, Dataset};
use tempfile::TempDir;

// ------------------------------------------------------------ scaffold --

/// The path `bcftools` wrote into the `#CHROM` line when this was found.
const STORE_PATH: &str = "/Users/kelyn/ligase/.ligase/store/62/7db9fa0cbac.bam";

/// A VCF of one call, with the sample column the caller asks for.
fn vcf(sample: &str) -> String {
    format!(
        "##fileformat=VCFv4.3\n\
         ##contig=<ID=K03455.1,length=20>\n\
         #CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\t{sample}\n\
         K03455.1\t5\t.\tA\tG\t30\tPASS\t.\tGT\t1\n"
    )
}

/// A workspace holding a cache, a provenance store and a one-step graph
/// whose step produces variant calls.
struct Bench {
    dir: TempDir,
    root: PathBuf,
    cache: Cache,
    prov: Prov,
    graph: Graph,
    tasks: Vec<Task>,
}

impl Bench {
    /// A pipeline of one step, `call`, which gzips a VCF of `sample` into
    /// its output.
    ///
    /// Gzipped because an output's name decides its format and a step
    /// producing variants is given `.vcf.gz`, so a plain file there would
    /// not parse as one — and a file that does not parse hashes as its
    /// bytes and notices nothing (D21).
    fn new(sample: &str) -> (Self, NodeId) {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join(".ligase");
        let cache = Cache::open(&root).unwrap();
        let prov = Prov::open(&root).unwrap();
        let mut graph = Graph::new();

        let path = dir.path().join("calls.vcf");
        fs::write(&path, vcf(sample)).unwrap();
        let input = graph.add_input(
            "calls",
            Dataset::new(DataKind::Variants, Some(path), Dimensions::default()),
        );

        let node = graph
            .add_step(StepSpec {
                name: "call".to_owned(),
                output: DataKind::Variants,
                coordinates: None,
                arguments: vec![Argument {
                    param: "calls".to_owned(),
                    declared: DataKind::Variants,
                    node: input,
                }],
            })
            .unwrap();

        let tasks = vec![Task {
            node,
            code: "def call(): ...".to_owned(),
            recipe: Recipe::Shell("gzip -c {calls} > {out}".to_owned()),
            tools: Vec::new(),
            parameters: Vec::new(),
            citations: Vec::new(),
        }];

        (
            Self {
                dir,
                root,
                cache,
                prov,
                graph,
                tasks,
            },
            node,
        )
    }

    fn run(&self, roots: &[NodeId]) -> RunReport {
        self.run_with(roots, &Refusing)
    }

    fn run_with(&self, roots: &[NodeId], host: &dyn Host) -> RunReport {
        exec::run(
            &self.graph,
            roots,
            &self.tasks,
            &Stores {
                cache: &self.cache,
                prov: &self.prov,
            },
            &Options::default(),
            host,
        )
    }

    /// The same pipeline, with its one step's work done in a body.
    ///
    /// The body writes something that is not a VCF, so canonicalisation
    /// falls back to the bytes and notices nothing (D21) — which leaves
    /// whatever the report says about this step coming from the body alone.
    fn by_callback(mut self) -> Self {
        for task in &mut self.tasks {
            task.recipe = Recipe::Callback(0);
        }
        self
    }

    /// The same pipeline against an empty cache, as a clean machine is.
    fn clean(&self) -> (Cache, Prov) {
        let root = self.dir.path().join(".clean");
        (Cache::open(&root).unwrap(), Prov::open(&root).unwrap())
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

/// A body that writes its output and says what its work rested on.
struct Noticing;

impl Host for Noticing {
    fn call(
        &self,
        _id: u64,
        out: &Path,
        _inputs: &[(String, Vec<PathBuf>)],
    ) -> Result<Vec<Note>, String> {
        fs::write(out, "whatever a body writes\n").map_err(|error| error.to_string())?;
        Ok(vec![Note::ReadingWithoutCoverage])
    }
}

// ---------------------------------------------------- what a run says --

#[test]
fn a_run_whose_output_names_a_file_says_so() {
    let (bench, node) = Bench::new(STORE_PATH);
    let report = bench.run(&[node]);

    let result = report.result(node).expect("the step ran");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(
        result.notes,
        vec![Note::SampleIsAFile {
            sample: STORE_PATH.to_owned()
        }],
    );

    let text = exec::render(&bench.graph, &[node], &report);
    assert!(text.contains("noticed"), "{text}");
    assert!(text.contains(STORE_PATH), "{text}");
    assert!(text.contains("addreplacerg"), "{text}");
}

#[test]
fn a_run_whose_output_names_a_sample_says_nothing() {
    // The common case has to stay silent: a report that remarks on every
    // run is a report a user stops reading.
    let (bench, node) = Bench::new("SRR8525904");
    let report = bench.run(&[node]);

    assert!(report.result(node).unwrap().notes.is_empty());
    let text = exec::render(&bench.graph, &[node], &report);
    assert!(!text.contains("noticed"), "{text}");
}

#[test]
fn a_step_served_from_the_cache_still_says_it() {
    // The one that justifies storing notes rather than recomputing them: a
    // cache hit does not reopen the file, and a warning that stopped
    // appearing on the second run would be worse than none at all.
    let (bench, node) = Bench::new(STORE_PATH);
    assert_eq!(
        bench.run(&[node]).result(node).unwrap().outcome,
        Outcome::Ran
    );

    let again = bench.run(&[node]);
    let result = again.result(node).unwrap();
    assert_eq!(result.outcome, Outcome::Cached);
    assert_eq!(
        result.notes,
        vec![Note::SampleIsAFile {
            sample: STORE_PATH.to_owned()
        }],
    );
}

// ----------------------------------------- what provenance keeps of it --

#[test]
fn provenance_records_what_was_noticed() {
    let (bench, node) = Bench::new(STORE_PATH);
    let report = bench.run(&[node]);
    let key = report.result(node).unwrap().key;

    let record = bench.prov.lookup(key).unwrap().expect("a record");
    assert_eq!(
        record.output_notes,
        vec![Note::SampleIsAFile {
            sample: STORE_PATH.to_owned()
        }],
    );
}

#[test]
fn a_note_survives_the_cache_being_thrown_away() {
    // Provenance has its own lifetime (D23): the question "what was wrong
    // with this output?" outlives the question "has this been computed?".
    let (bench, node) = Bench::new(STORE_PATH);
    let report = bench.run(&[node]);
    let key = report.result(node).unwrap().key;

    fs::remove_file(bench.root.join("cache.sqlite")).unwrap();
    let record = bench.prov.lookup(key).unwrap().expect("a record");
    assert_eq!(record.output_notes.len(), 1);
}

// ------------------------------------------------- what a write-up says --

#[test]
fn the_methods_section_says_an_identifier_is_a_file() {
    let (bench, node) = Bench::new(STORE_PATH);
    let report = bench.run(&[node]);
    let methods = prov::methods(&bench.graph, &[node], &bench.tasks, &report, &bench.prov).unwrap();

    assert_eq!(methods.notices().len(), 1);
    let text = methods.text();
    assert!(text.contains(STORE_PATH), "{text}");
    assert!(text.contains("not a sample"), "{text}");
    // Outside the paragraph a reader would paste into a manuscript: the
    // sentences about the science come first.
    let paragraph = methods.paragraph();
    assert!(!paragraph.contains(STORE_PATH), "{paragraph}");
}

#[test]
fn a_write_up_with_nothing_wrong_says_nothing() {
    let (bench, node) = Bench::new("SRR8525904");
    let report = bench.run(&[node]);
    let methods = prov::methods(&bench.graph, &[node], &bench.tasks, &report, &bench.prov).unwrap();

    assert!(methods.notices().is_empty());
    assert!(!methods.text().contains("not a sample"));
}

// ----------------------------------------------------- what a bundle says --

#[test]
fn a_bundle_records_what_was_noticed_and_warns_in_its_readme() {
    let (bench, node) = Bench::new(STORE_PATH);
    let report = bench.run(&[node]);
    let methods = prov::methods(&bench.graph, &[node], &bench.tasks, &report, &bench.prov).unwrap();

    let into = bench.dir.path().join("bundle");
    let summary = bundle::write(
        &into,
        &bundle::Request {
            graph: &bench.graph,
            roots: &[node],
            report: &report,
            prov: &bench.prov,
            methods: &methods,
            include: &[],
        },
    )
    .unwrap();
    assert!(summary.files.iter().any(|name| name == "notes.tsv"));

    let notes = fs::read_to_string(into.join("notes.tsv")).unwrap();
    assert!(notes.contains("sample-is-a-file"), "{notes}");
    assert!(notes.contains(STORE_PATH), "{notes}");

    let readme = fs::read_to_string(into.join("README.md")).unwrap();
    assert!(readme.contains("What was noticed"), "{readme}");
}

#[test]
fn a_bundle_with_nothing_wrong_still_has_the_file_and_no_warning() {
    // The table is always written, so a reader never has to wonder whether
    // an absent file means "nothing to report" or "an older bundle".
    let (bench, node) = Bench::new("SRR8525904");
    let report = bench.run(&[node]);
    let methods = prov::methods(&bench.graph, &[node], &bench.tasks, &report, &bench.prov).unwrap();

    let into = bench.dir.path().join("bundle");
    bundle::write(
        &into,
        &bundle::Request {
            graph: &bench.graph,
            roots: &[node],
            report: &report,
            prov: &bench.prov,
            methods: &methods,
            include: &[],
        },
    )
    .unwrap();

    let notes = fs::read_to_string(into.join("notes.tsv")).unwrap();
    assert_eq!(notes.lines().count(), 1, "the header, and nothing under it");
    let readme = fs::read_to_string(into.join("README.md")).unwrap();
    assert!(!readme.contains("What was noticed"), "{readme}");
}

#[test]
fn verifying_a_bundle_reports_an_output_that_will_not_travel() {
    // The point of saying it here. A canonical hash that depends on a file
    // path does not travel, so a `differs` for that reason differs for a
    // reason that has nothing to do with the science — and a reader needs
    // to be told which of the two they are looking at.
    let (bench, node) = Bench::new(STORE_PATH);
    let report = bench.run(&[node]);
    let methods = prov::methods(&bench.graph, &[node], &bench.tasks, &report, &bench.prov).unwrap();

    let into = bench.dir.path().join("bundle");
    bundle::write(
        &into,
        &bundle::Request {
            graph: &bench.graph,
            roots: &[node],
            report: &report,
            prov: &bench.prov,
            methods: &methods,
            include: &[],
        },
    )
    .unwrap();

    // A clean machine, which to this engine is an empty cache.
    let (cache, prov) = bench.clean();
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

    let verification = bundle::verify(&into, &bench.graph, &[node], &fresh).unwrap();
    assert_eq!(verification.noticed.len(), 1);
    let text = verification.text();
    assert!(text.contains("where a sample identifier belongs"), "{text}");
    assert!(text.contains(STORE_PATH), "{text}");
}

#[test]
fn verifying_an_ordinary_bundle_says_nothing_about_notes() {
    let (bench, node) = Bench::new("SRR8525904");
    let report = bench.run(&[node]);
    let methods = prov::methods(&bench.graph, &[node], &bench.tasks, &report, &bench.prov).unwrap();

    let into = bench.dir.path().join("bundle");
    bundle::write(
        &into,
        &bundle::Request {
            graph: &bench.graph,
            roots: &[node],
            report: &report,
            prov: &bench.prov,
            methods: &methods,
            include: &[],
        },
    )
    .unwrap();

    let (cache, prov) = bench.clean();
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

    let verification = bundle::verify(&into, &bench.graph, &[node], &fresh).unwrap();
    assert!(verification.noticed.is_empty());
    assert!(!verification.text().contains("sample identifier belongs"));
}

// -------------------------------------------- what a step says itself --

#[test]
fn a_run_says_what_a_body_noticed_about_its_own_work() {
    // The other direction (D32). Nothing in the file says the work was
    // done without coverage — two reports, one fully covered and one with
    // none, can come out byte for byte the same — so the only thing that
    // can say it is the body, and the only thing that can carry it is the
    // engine.
    let (bench, node) = Bench::new("SRR8525904");
    let bench = bench.by_callback();
    let report = bench.run_with(&[node], &Noticing);

    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let result = report.result(node).expect("the step ran");
    assert_eq!(result.notes, vec![Note::ReadingWithoutCoverage]);

    let text = exec::render(&bench.graph, &[node], &report);
    assert!(text.contains("noticed"), "{text}");
    assert!(text.contains("coverage is unknown"), "{text}");
    assert!(text.contains("coverage=aln"), "{text}");
}

#[test]
fn a_body_served_from_the_cache_still_says_what_it_noticed() {
    // The one that justifies storing a body's notes against its key. A
    // cache hit does not rerun the body, so there is nothing to ask: the
    // hedge either was remembered or has gone silent, and a hedge that
    // appears only on the first run is worse than none at all.
    let (bench, node) = Bench::new("SRR8525904");
    let bench = bench.by_callback();
    assert_eq!(
        bench
            .run_with(&[node], &Noticing)
            .result(node)
            .unwrap()
            .outcome,
        Outcome::Ran
    );

    // A host that would fail if it were asked, so a note that survives
    // here survived without the body being run again.
    let again = bench.run_with(&[node], &Refusing);
    let result = again.result(node).unwrap();
    assert_eq!(result.outcome, Outcome::Cached);
    assert_eq!(result.notes, vec![Note::ReadingWithoutCoverage]);

    let text = exec::render(&bench.graph, &[node], &again);
    assert!(text.contains("coverage is unknown"), "{text}");
}

#[test]
fn provenance_and_a_bundle_keep_what_a_body_noticed() {
    let (bench, node) = Bench::new("SRR8525904");
    let bench = bench.by_callback();
    let report = bench.run_with(&[node], &Noticing);
    let key = report.result(node).unwrap().key;

    let record = bench.prov.lookup(key).unwrap().expect("a record");
    assert_eq!(record.output_notes, vec![Note::ReadingWithoutCoverage]);

    let methods = prov::methods(&bench.graph, &[node], &bench.tasks, &report, &bench.prov).unwrap();
    assert_eq!(methods.notices().len(), 1);
    // Wrapped to a paragraph, so read as one line to look for a sentence.
    let said = methods
        .text()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(said.contains("no alignment was supplied"), "{said}");
    assert!(
        said.contains("every score in this report a floor"),
        "{said}"
    );

    let into = bench.dir.path().join("bundle");
    bundle::write(
        &into,
        &bundle::Request {
            graph: &bench.graph,
            roots: &[node],
            report: &report,
            prov: &bench.prov,
            methods: &methods,
            include: &[],
        },
    )
    .unwrap();

    let notes = fs::read_to_string(into.join("notes.tsv")).unwrap();
    assert!(notes.contains("reading-without-coverage"), "{notes}");
    let readme = fs::read_to_string(into.join("README.md")).unwrap();
    assert!(readme.contains("What was noticed"), "{readme}");
}

#[test]
fn a_hedge_is_not_an_excuse_for_a_hash_that_moved() {
    // A sample identifier that is a file explains a `differs` with no
    // science behind it. A reading made without coverage does not: it is
    // the same on every machine. Offering it as an explanation would send
    // a reader after the wrong thing.
    let (bench, node) = Bench::new("SRR8525904");
    let bench = bench.by_callback();
    let report = bench.run_with(&[node], &Noticing);
    let methods = prov::methods(&bench.graph, &[node], &bench.tasks, &report, &bench.prov).unwrap();

    let into = bench.dir.path().join("bundle");
    bundle::write(
        &into,
        &bundle::Request {
            graph: &bench.graph,
            roots: &[node],
            report: &report,
            prov: &bench.prov,
            methods: &methods,
            include: &[],
        },
    )
    .unwrap();

    let verification = bundle::verify(&into, &bench.graph, &[node], &report).unwrap();
    assert!(
        verification.noticed.is_empty(),
        "{:?}",
        verification.noticed
    );
    assert!(!verification.text().contains("coverage is unknown"));
}
