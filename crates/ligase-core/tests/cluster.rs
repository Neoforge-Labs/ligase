//! M18: several workers running one pipeline between them, over one store.
//!
//! The store here is a `file://` directory, which is not a stand-in for a
//! bucket but the shared-filesystem deployment in its own right: most HPC
//! sites have Lustre or GPFS and no object store, and `Backend::Local`
//! serves them with no feature and no dependency. `tests/s3.rs` runs the
//! same claims against a real S3 server, where the writes are concurrent
//! HTTP requests rather than renames.
//!
//! Each worker gets its own cache directory and its own index, which is
//! D43's rule unchanged, and the store is the only thing they share. That
//! is the whole architecture: no coordinator, no lock, and a lease that is
//! advisory.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ligase::cache::Cache;
use ligase::dim::Dimensions;
use ligase::exec::{self, Host, Options, Outcome, Recipe, RunReport, Stores, Task};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::key::{Ingredients, StepKey};
use ligase::note::Note;
use ligase::prov::Prov;
use ligase::store::FsStore;
use ligase::store::lease::{self, Claimed, Lease};
use ligase::types::{DataKind, Dataset};
use ligase::worker::{Lock, Worker};
use tempfile::TempDir;

// ------------------------------------------------------------ scaffold --

/// A lab: one shared store, and as many workers as a test wants.
struct Lab {
    dir: TempDir,
}

impl Lab {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("shared")).unwrap();
        Self { dir }
    }

    /// The URL every worker in this lab is pointed at.
    fn url(&self) -> String {
        format!("file://{}", self.dir.path().join("shared").display())
    }

    /// The shared store, opened directly, for a test that wants to read it
    /// without going through a worker.
    fn store(&self) -> FsStore {
        FsStore::open(&self.dir.path().join("shared")).unwrap()
    }

    /// A worker: its own cache, its own index, the lab's store.
    fn worker(&self, name: &str) -> Bench {
        Bench::new(self, name)
    }
}

/// One worker's workspace, and the graph it is about to run.
///
/// Two benches in one lab build the *same* graph over the *same* input
/// contents, which is what gives them the same step keys: a key is a hash
/// of code, parameters, input canonical hashes and tool versions, and
/// nothing about where a cache lives or what a file is called.
struct Bench {
    root: PathBuf,
    name: String,
    cache: Cache,
    prov: Prov,
    graph: Graph,
    tasks: Vec<Task>,
}

impl Bench {
    fn new(lab: &Lab, name: &str) -> Self {
        let root = lab.dir.path().join(name);
        fs::create_dir_all(&root).unwrap();
        let ligase = root.join(".ligase");
        Self {
            cache: Cache::open_with(&ligase, Some(&lab.url())).unwrap(),
            prov: Prov::open(&ligase).unwrap(),
            root,
            name: name.to_owned(),
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
        self.root.join(name)
    }

    fn input(&mut self, name: &str, contents: &str) -> NodeId {
        let path = self.path(name);
        fs::write(&path, contents).unwrap();
        let dataset = Dataset::new(DataKind::Reference, Some(path), Dimensions::default());
        self.graph.add_input(name, dataset)
    }

    fn step(&mut self, name: &str, command: &str, input: NodeId) -> NodeId {
        self.shell_of(name, command, input, DataKind::Alignment)
    }

    fn shell_of(&mut self, name: &str, command: &str, input: NodeId, output: DataKind) -> NodeId {
        let node = self
            .graph
            .add_step(StepSpec {
                name: name.to_owned(),
                output,
                coordinates: None,
                conversion: None,
                arguments: vec![Argument {
                    param: "in".to_owned(),
                    declared: DataKind::Reference,
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
            seed: None,
        });
        node
    }

    /// Run as a worker, with a lease of `lease` seconds.
    fn work(&self, roots: &[NodeId], lease: Duration) -> RunReport {
        self.run_with(
            roots,
            &Options {
                worker: Some(Worker::named(&self.name)),
                lease,
                ..Options::default()
            },
        )
    }

    /// Run as a worker with `jobs` slots of its own.
    ///
    /// One slot is the realistic cluster shape -- a Slurm array task is
    /// usually sized to one step -- and it is also what makes a division
    /// observable: a worker with as many slots as the graph is wide claims
    /// the whole graph in one go, quite correctly, and the other worker
    /// finds nothing left. Which worker gets which step is deliberately
    /// not promised by anything here.
    fn work_with(&self, roots: &[NodeId], jobs: usize, lease: Duration) -> RunReport {
        self.run_with(
            roots,
            &Options {
                worker: Some(Worker::named(&self.name)),
                jobs,
                lease,
                ..Options::default()
            },
        )
    }

    /// Run as a worker that computes its steps whatever the store says.
    ///
    /// `--force` is how a worker ends up computing a key another machine
    /// has already answered without waiting for a lease to expire, so it
    /// is how the two divergence cases are reached deterministically
    /// rather than by racing two threads and hoping.
    fn rework(&self, roots: &[NodeId], lease: Duration) -> RunReport {
        self.run_with(
            roots,
            &Options {
                worker: Some(Worker::named(&self.name)),
                lease,
                force: true,
                ..Options::default()
            },
        )
    }

    /// Run as an ordinary run — the collector of the design's §2.4.
    fn collect(&self, roots: &[NodeId]) -> RunReport {
        self.run_with(roots, &Options::default())
    }

    fn run_with(&self, roots: &[NodeId], options: &Options) -> RunReport {
        exec::run(
            &self.graph,
            roots,
            &self.tasks,
            &self.stores(),
            options,
            &Refusing,
        )
    }
}

/// A host with no callbacks: every step here is a shell step.
struct Refusing;

impl Host for Refusing {
    fn call(
        &self,
        id: u64,
        _out: &Path,
        _in: &[(String, Vec<PathBuf>)],
    ) -> Result<Vec<Note>, String> {
        Err(format!("no callback {id} in this test"))
    }
}

/// A command that copies its input and records that it really ran here.
///
/// The marker's path is **derived from `{out}`** rather than written into
/// the command, and that is the whole point: a step key hashes the recipe,
/// so a command naming this worker's own directory would be a *different
/// step* on every worker, the keys would never meet, and a test of what
/// two workers do about one key would quietly be testing nothing. (Which
/// is what the first draft of this file did.)
///
/// `{out}` is `<root>/<worker>/.ligase/tmp/<key>`, so two levels up from
/// its directory is the worker's own root — the same template everywhere,
/// a different file on each machine. Exactly what a real pipeline's
/// `$TMPDIR` or `--threads` does, and the reason D50 has to be argued
/// rather than assumed.
fn marker() -> &'static str {
    "echo ran >> \"$(dirname {out})/../../marker\""
}

/// A command that copies its input, recording that it ran.
fn copy() -> String {
    format!("cat {{in}} > {{out}}; {}", marker())
}

/// The same, taking long enough that another worker is certain to probe
/// its lease while it is held.
fn slow_copy() -> String {
    format!("sleep 0.4; cat {{in}} > {{out}}; {}", marker())
}

/// How many commands this worker actually ran.
fn executions(bench: &Bench) -> usize {
    fs::read_to_string(bench.path("marker")).map_or(0, |log| log.lines().count())
}

/// Four independent steps over one input, identical on every worker.
fn fan_out(bench: &mut Bench) -> Vec<NodeId> {
    let reads = bench.input("reads.txt", "ACGT\n");
    let command = slow_copy();
    ["a", "b", "c", "d"]
        .iter()
        .map(|tag| bench.shell_of(tag, &command, reads, DataKind::Alignment))
        .collect()
}

fn outcomes(report: &RunReport, wanted: Outcome) -> BTreeSet<String> {
    report
        .results
        .iter()
        .filter(|result| result.outcome == wanted)
        .map(|result| result.step.clone())
        .collect()
}

fn a_key(step: &str) -> StepKey {
    Ingredients {
        step: step.to_owned(),
        code: format!("def {step}(): ..."),
        recipe: "sh:true".to_owned(),
        output: DataKind::Alignment,
        parameters: Vec::new(),
        inputs: Vec::new(),
        tools: Vec::new(),
    }
    .key()
}

// -------------------------------------------------------------- leases --

#[test]
fn a_lease_nobody_has_taken_is_free() {
    let lab = Lab::new();
    let store = lab.store();

    assert_eq!(
        lease::probe(&store, a_key("align"), 1000).unwrap(),
        Lease::Free { generation: 0 }
    );
}

#[test]
fn a_lease_another_worker_holds_is_not_free() {
    let lab = Lab::new();
    let store = lab.store();
    let key = a_key("align");

    let taken = lease::acquire(&store, key, &Worker::named("node01"), 2000, 1000).unwrap();
    assert_eq!(taken, Claimed::Acquired { generation: 0 });

    // A second worker, probing at a time before the deadline.
    match lease::probe(&store, key, 1500).unwrap() {
        Lease::Held {
            worker, deadline, ..
        } => {
            assert_eq!(worker, "node01");
            assert_eq!(deadline, 2000);
        }
        other => panic!("a held lease read as {other:?}"),
    }
}

#[test]
fn a_lease_whose_deadline_has_passed_is_free_again() {
    let lab = Lab::new();
    let store = lab.store();
    let key = a_key("align");

    lease::acquire(&store, key, &Worker::named("node01"), 2000, 1000).unwrap();

    // Past the deadline the lease is free, and the next generation is the
    // one a stealing worker takes -- generation 0 cannot be rewritten,
    // because objects are write-once.
    assert_eq!(
        lease::probe(&store, key, 2001).unwrap(),
        Lease::Free { generation: 1 }
    );
}

#[test]
fn two_workers_racing_for_one_lease_leave_exactly_one_holder() {
    // Both probe, both see nothing, both publish. Write-once means one
    // row wins, and the read-back is what tells the loser it lost -- a
    // check the store can honour, unlike a compare-and-swap, which it
    // cannot.
    let lab = Lab::new();
    let store = lab.store();
    let key = a_key("align");

    let first = lease::acquire(&store, key, &Worker::named("node01"), 2000, 1000).unwrap();
    let second = lease::acquire(&store, key, &Worker::named("node02"), 2000, 1000).unwrap();

    assert_eq!(first, Claimed::Acquired { generation: 0 });
    match second {
        Claimed::Held { worker, .. } => assert_eq!(worker, "node01"),
        other => panic!("two workers both hold one lease: {other:?}"),
    }
}

#[test]
fn a_relinquished_lease_is_free_before_its_deadline() {
    // A worker whose step failed gives the key back at once, so another
    // worker retries it now rather than idling to the deadline. A release
    // is just a lease nobody holds, which is why it needs no second
    // object class.
    let lab = Lab::new();
    let store = lab.store();
    let key = a_key("align");

    let Claimed::Acquired { generation } =
        lease::acquire(&store, key, &Worker::named("node01"), 9999, 1000).unwrap()
    else {
        panic!("the first worker did not get the lease");
    };
    lease::relinquish(&store, key, generation, 1100).unwrap();

    assert_eq!(
        lease::probe(&store, key, 1101).unwrap(),
        Lease::Free { generation: 2 }
    );
    assert!(matches!(
        lease::acquire(&store, key, &Worker::named("node02"), 9999, 1101).unwrap(),
        Claimed::Acquired { .. }
    ));
}

#[test]
fn a_key_claimed_and_given_up_too_often_is_free_to_anyone() {
    // Exhausting the generations degrades to the lock-free correctness of
    // the design's §4 rather than to a stall: everyone computes, the store
    // deduplicates, and the answer is the same.
    let lab = Lab::new();
    let store = lab.store();
    let key = a_key("align");

    for generation in 0..lease::MAX_GENERATIONS {
        lease::acquire(&store, key, &Worker::named("node01"), 9999, 1000).unwrap();
        lease::relinquish(&store, key, generation, 1000).unwrap();
    }

    assert!(matches!(
        lease::probe(&store, key, 1000).unwrap(),
        Lease::Free { .. }
    ));
}

// ------------------------------------------------- a worker's identity --

#[test]
fn a_worker_names_itself_in_its_report() {
    let lab = Lab::new();
    let mut bench = lab.worker("node01");
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.step("align", "cat {in} > {out}", reads);

    let report = bench.work(&[align], Duration::from_secs(60));

    assert!(report.failures.is_empty(), "{report:?}");
    let text = exec::render(&bench.graph, &[align], &report);
    assert!(
        text.contains("node01"),
        "a worker's report does not name it:\n{text}"
    );
}

#[test]
fn a_worker_with_no_name_still_has_an_identity_of_its_own() {
    // Two unnamed workers must not look like one, or the read-back that
    // settles a lease race cannot tell them apart.
    assert_ne!(Worker::unnamed().instance(), Worker::unnamed().instance());
}

// ------------------------------------------- dividing one graph, lock-free --

#[test]
fn two_workers_divide_one_graph_between_them() {
    // Four independent steps, two workers, one store. Each step is slow
    // enough that the other worker is certain to probe its lease while it
    // is held, so a division is the lease working rather than a race going
    // one way.
    let lab = Lab::new();
    let (mut one, mut two) = (lab.worker("node01"), lab.worker("node02"));
    let (first, second) = (fan_out(&mut one), fan_out(&mut two));

    // One slot each, which is how an array task is usually sized.
    let (left, right) = std::thread::scope(|scope| {
        let a = scope.spawn(|| one.work_with(&first, 1, Duration::from_secs(60)));
        let b = scope.spawn(|| two.work_with(&second, 1, Duration::from_secs(60)));
        (a.join().unwrap(), b.join().unwrap())
    });

    assert!(left.failures.is_empty(), "{left:?}");
    assert!(right.failures.is_empty(), "{right:?}");

    // Every step finished for both workers: each sees the whole graph,
    // whether it computed a step or took it from the store.
    assert_eq!(left.results.len(), 4, "{left:?}");
    assert_eq!(right.results.len(), 4, "{right:?}");

    // And between them they ran each step exactly once. This is the
    // milestone in one assertion: two machines, no coordinator, four
    // steps, four executions.
    assert_eq!(
        executions(&one) + executions(&two),
        4,
        "the two workers ran {} and {} commands between them, not 4",
        executions(&one),
        executions(&two)
    );
    // Each did some of it, so "divided" is the right word. With one slot
    // apiece this is deterministic; with a slot per step it would not be,
    // and asserting it then would be asserting a race.
    assert!(
        executions(&one) > 0 && executions(&two) > 0,
        "one worker did all of it: {} and {}",
        executions(&one),
        executions(&two)
    );
}

#[test]
fn neither_worker_computes_what_the_other_computed() {
    let lab = Lab::new();
    let (mut one, mut two) = (lab.worker("node01"), lab.worker("node02"));
    let (first, second) = (fan_out(&mut one), fan_out(&mut two));

    let (left, right) = std::thread::scope(|scope| {
        let a = scope.spawn(|| one.work(&first, Duration::from_secs(60)));
        let b = scope.spawn(|| two.work(&second, Duration::from_secs(60)));
        (a.join().unwrap(), b.join().unwrap())
    });

    let ran_here = outcomes(&left, Outcome::Ran);
    let ran_there = outcomes(&right, Outcome::Ran);

    assert!(
        ran_here.is_disjoint(&ran_there),
        "both workers computed {:?}",
        ran_here.intersection(&ran_there).collect::<Vec<_>>()
    );
    let between: BTreeSet<_> = ran_here.union(&ran_there).cloned().collect();
    assert_eq!(between.len(), 4, "the graph was not covered: {between:?}");
}

#[test]
fn a_worker_whose_only_work_is_leased_waits_and_takes_it_from_the_store() {
    // One step, two workers. The second cannot compute it and must not: it
    // waits, and the answer arrives through the store.
    let lab = Lab::new();
    let (mut one, mut two) = (lab.worker("node01"), lab.worker("node02"));

    let only_step = |bench: &mut Bench| {
        let reads = bench.input("reads.txt", "ACGT\n");
        let command = format!("sleep 0.8; cat {{in}} > {{out}}; {}", marker());
        bench.shell_of("align", &command, reads, DataKind::Alignment)
    };
    let first = only_step(&mut one);
    let second = only_step(&mut two);

    let (left, right) = std::thread::scope(|scope| {
        let a = scope.spawn(|| one.work(&[first], Duration::from_secs(60)));
        // Started a moment later, so the lease is certainly held.
        let b = scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(200));
            two.work(&[second], Duration::from_secs(60))
        });
        (a.join().unwrap(), b.join().unwrap())
    });

    assert_eq!(left.result(first).unwrap().outcome, Outcome::Ran);
    assert_eq!(right.result(second).unwrap().outcome, Outcome::Cached);
    assert_eq!(executions(&one), 1);
    assert_eq!(executions(&two), 0, "the second worker duplicated the work");

    // And it says so, rather than reporting an ordinary cache hit: "four
    // came from the cache" and "four were computed by somebody else while
    // I waited" are different news.
    assert_eq!(right.waited, 1, "{right:?}");
    let text = exec::render(&two.graph, &[second], &right);
    assert!(
        text.contains("waited for other workers"),
        "the report does not say it waited:\n{text}"
    );
}

#[test]
fn a_lost_worker_cannot_stall_a_run() {
    // A lease whose holder is never coming back, with a deadline already
    // long past. An idle worker takes the step anyway.
    //
    // Expiry does not have to be *right*, and that is the whole reason a
    // lost worker needs no recovery protocol, no fencing and no
    // coordinator to notice it: stealing is safe by D50, so the mechanism
    // that decides when to steal is allowed to be approximate.
    let lab = Lab::new();
    let store = lab.store();

    let mut bench = lab.worker("node01");
    let reads = bench.input("reads.txt", "ACGT\n");
    let align = bench.shell_of("align", &copy(), reads, DataKind::Alignment);

    // Claim this step's key for a worker that has died, using the key the
    // worker itself computes -- taken from a dry run on a *different*
    // store so that nothing is recorded in this one.
    let key = key_of(&lab, "align", &copy());
    lease::acquire(&store, key, &Worker::named("ghost"), 1, 0).unwrap();
    assert!(matches!(
        lease::probe(&store, key, now()).unwrap(),
        Lease::Free { .. }
    ));

    let report = bench.work(&[align], Duration::from_secs(60));

    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(report.result(align).unwrap().outcome, Outcome::Ran);
    assert_eq!(
        report.result(align).unwrap().key,
        key,
        "the test's key is stale"
    );
    assert_eq!(
        executions(&bench),
        1,
        "the run stalled on a dead worker's lease"
    );
}

/// The key a one-step graph over `ACGT` has, computed the way the engine
/// computes it.
///
/// Worked out on a throwaway lab so that nothing it touches is in the lab
/// under test — a cache is the thing these tests are about, and warming it
/// here would answer the question before it was asked.
fn key_of(lab: &Lab, step: &str, command: &str) -> StepKey {
    let probe = lab.worker("key-probe");
    let path = probe.path("reads.txt");
    fs::write(&path, "ACGT\n").unwrap();
    let hash = probe
        .cache
        .semantic_hash(DataKind::Reference, &path)
        .unwrap();
    Ingredients {
        step: step.to_owned(),
        code: format!("def {step}(): ..."),
        recipe: format!("sh:{command}"),
        output: DataKind::Alignment,
        parameters: Vec::new(),
        inputs: vec![(
            "in".to_owned(),
            ligase::hash::ContentHash::of_hashes(&[hash]),
        )],
        tools: Vec::new(),
    }
    .key()
}

fn now() -> i64 {
    ligase::prov::now()
}

#[test]
fn a_failed_step_is_given_back_rather_than_held_to_its_deadline() {
    // A worker whose step failed gives the key back at once, so another
    // worker retries it now rather than idling until the deadline. For a
    // node-local failure -- a full scratch disk, a missing module, a bad
    // GPU -- a retry elsewhere is what the user wants.
    //
    // The lease is an hour, so a worker that waited for it would be here
    // for an hour. Measuring the wait rather than only the outcome is
    // deliberate: a regression here should be a red test and not a suite
    // that never returns.
    let lab = Lab::new();
    let failing = "exit 3";

    let mut one = lab.worker("node01");
    let reads = one.input("reads.txt", "ACGT\n");
    let bad = one.shell_of("align", failing, reads, DataKind::Alignment);
    let report = one.work(&[bad], Duration::from_secs(3600));
    assert_eq!(report.failures.len(), 1, "{report:?}");

    let store = lab.store();
    assert!(
        matches!(
            lease::probe(&store, key_of(&lab, "align", failing), now()).unwrap(),
            Lease::Free { .. }
        ),
        "a failed step kept its lease, so the next worker waits an hour"
    );

    // And the next worker really does get to it.
    let mut two = lab.worker("node02");
    let reads = two.input("reads.txt", "ACGT\n");
    let also = two.shell_of("align", failing, reads, DataKind::Alignment);
    let started = std::time::Instant::now();
    let retried = two.work(&[also], Duration::from_secs(3600));

    assert_eq!(retried.failures.len(), 1, "{retried:?}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the second worker waited {:?} for a lease that was given back",
        started.elapsed()
    );
}

#[test]
fn a_partly_complete_graph_completes_without_recomputing_what_landed() {
    let lab = Lab::new();

    // One worker gets through the first step and stops.
    let mut one = lab.worker("node01");
    let reads = one.input("reads.txt", "ACGT\n");
    let first = one.shell_of("align", &copy(), reads, DataKind::Alignment);
    let report = one.work(&[first], Duration::from_secs(60));
    assert_eq!(report.result(first).unwrap().outcome, Outcome::Ran);

    // A second worker, with nothing of its own, takes that and does the
    // rest. This is resumability and it needed nothing built: the entries
    // that landed are in the store, so a later run takes them from cache.
    let mut two = lab.worker("node02");
    let reads = two.input("reads.txt", "ACGT\n");
    let align = two.shell_of("align", &copy(), reads, DataKind::Alignment);
    let call = two.shell_of("call", &copy(), align, DataKind::Alignment);
    let report = two.work(&[call], Duration::from_secs(60));

    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(report.result(align).unwrap().outcome, Outcome::Cached);
    assert_eq!(report.result(call).unwrap().outcome, Outcome::Ran);
    assert_eq!(
        executions(&two),
        1,
        "the second worker recomputed what had already landed"
    );
}

// ---------------------------------------------------------- divergence --

/// A command that writes the same sequence under a description taken from
/// the machine it ran on.
///
/// FASTA canonicalisation drops the description after a contig's name, so
/// two workers running this produce **different bytes** and the **same
/// meaning** — which is the ordinary case, and the case the whole design
/// rests on. `minimap2` writing its command line into `@PG` is this, and
/// it is why every downstream key is built on the canonical form.
fn same_meaning() -> String {
    "printf '>chr1 made at %s\\nACGT\\n' \"$(dirname {out})\" > {out}".to_owned()
}

/// A command whose **sequence** depends on the machine it ran on.
///
/// The same template, so the same step key; a different result, so a
/// different canonical form. This is a seeded stochastic tool whose thread
/// count differs between two cluster nodes, in the smallest form that
/// reproduces it — and it is contrived only in being fast.
fn different_meaning() -> String {
    "printf '>chr1\\n%s\\n' \"$(echo {out} | tr -dc 'A-Za-z0-9')\" > {out}".to_owned()
}

#[test]
fn two_workers_whose_bytes_differ_but_whose_meaning_agrees_say_nothing() {
    // The ordinary case. A reference described differently is the same
    // reference, so the two workers agree about everything a downstream
    // key is built on, and there is nothing to report.
    let lab = Lab::new();
    let command = same_meaning();

    let mut one = lab.worker("node01");
    let reads = one.input("reads.txt", "ACGT\n");
    let here = one.shell_of("assemble", &command, reads, DataKind::Reference);
    let left = one.work(&[here], Duration::from_secs(60));
    assert!(left.failures.is_empty(), "{left:?}");

    let mut two = lab.worker("node02");
    let reads = two.input("reads.txt", "ACGT\n");
    let there = two.shell_of("assemble", &command, reads, DataKind::Reference);
    let right = two.rework(&[there], Duration::from_secs(60));
    assert!(right.failures.is_empty(), "{right:?}");

    // Same key, so they really were racing for one answer.
    assert_eq!(
        left.result(here).unwrap().key,
        right.result(there).unwrap().key
    );

    // The two commands really did write different bytes: the shared store
    // holds both objects. Without this the rest of the test would pass
    // just as well on a step that is bit-for-bit reproducible, and would
    // be asserting nothing.
    assert_eq!(
        objects_in(&lab, "fasta"),
        2,
        "the two workers produced identical bytes, so this test proves nothing"
    );

    // And nothing to say about it, because the only thing anything
    // downstream is keyed on agrees.
    assert!(left.diverged.is_empty(), "{:?}", left.diverged);
    assert!(right.diverged.is_empty(), "{:?}", right.diverged);
    assert_eq!(
        left.result(here).unwrap().canonical,
        right.result(there).unwrap().canonical,
        "canonicalisation is what makes lock-free distribution safe, and it did not"
    );

    // Both workers name **one** object, which is the adoption rule doing
    // its job: the store's answer is in force everywhere, so no two
    // machines can hold different outputs for one key even when they
    // computed different bytes for it.
    assert_eq!(
        left.result(here).unwrap().hash,
        right.result(there).unwrap().hash,
        "each worker kept its own bytes, so the run disagrees with itself"
    );
}

/// How many objects with this suffix the shared store holds.
fn objects_in(lab: &Lab, suffix: &str) -> usize {
    fn walk(at: &Path, suffix: &str, found: &mut usize) {
        let Ok(entries) = fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, suffix, found);
            } else if path.extension().is_some_and(|had| had == suffix) {
                *found += 1;
            }
        }
    }
    let mut found = 0;
    walk(&lab.dir.path().join("shared"), suffix, &mut found);
    found
}

#[test]
fn two_workers_whose_outputs_mean_different_things_are_reported() {
    // The case the argument does not cover. The run that loses the publish
    // adopts the store's answer -- so the two workers do not each believe
    // their own -- and says so.
    let lab = Lab::new();
    let command = different_meaning();

    let mut one = lab.worker("node01");
    let reads = one.input("reads.txt", "ACGT\n");
    let here = one.shell_of("sample", &command, reads, DataKind::Reference);
    let left = one.work(&[here], Duration::from_secs(60));
    assert!(left.failures.is_empty(), "{left:?}");
    assert!(
        left.diverged.is_empty(),
        "the first worker had nobody to disagree with"
    );

    let mut two = lab.worker("node02");
    let reads = two.input("reads.txt", "ACGT\n");
    let there = two.shell_of("sample", &command, reads, DataKind::Reference);
    let right = two.rework(&[there], Duration::from_secs(60));

    assert!(right.failures.is_empty(), "{right:?}");
    assert_eq!(
        right.result(there).unwrap().key,
        left.result(here).unwrap().key
    );
    assert_eq!(
        right.diverged.len(),
        1,
        "two workers disagreed about a result and nothing said so: {right:?}"
    );
    let divergence = &right.diverged[0];
    assert_eq!(divergence.step, "sample");
    assert_ne!(divergence.ours, divergence.theirs);

    // The store's answer is the one in force, on both workers, so nothing
    // below this step disagrees between machines.
    assert_eq!(
        right.result(there).unwrap().canonical,
        left.result(here).unwrap().canonical,
        "the loser kept its own answer instead of adopting the published one"
    );

    // Said in words, not only in a struct, and it names the step and
    // points at the tool that answers "which of the two reasons is it?".
    let text = exec::render(&two.graph, &[there], &right);
    assert!(text.contains("diverged"), "{text}");
    assert!(text.contains("sample"), "{text}");
    assert!(text.contains("ligase audit"), "{text}");
}

// ------------------------------------------------- one cache directory --

#[test]
fn a_second_worker_in_one_cache_directory_is_refused() {
    // Two workers sharing a `cache.sqlite` across machines is the failure
    // D43 names, and a Slurm array defaulting to one $HOME walks into it.
    let lab = Lab::new();
    let root = lab.dir.path().join("shared-cache");
    fs::create_dir_all(&root).unwrap();

    let held = Lock::take(&root, &Worker::named("node01")).unwrap();
    let error = Lock::take(&root, &Worker::named("node02"))
        .expect_err("two workers took one cache directory");

    let message = error.to_string();
    assert!(
        message.contains("node01"),
        "the refusal does not name the worker holding it: {message}"
    );
    drop(held);

    // And it is given back on a clean exit.
    assert!(Lock::take(&root, &Worker::named("node02")).is_ok());
}

#[test]
fn a_workers_cache_directory_is_its_own_by_default() {
    // The right thing is the default rather than merely the documented
    // thing: two array tasks pointed at one `.ligase` get one directory
    // each without being told to ask for it.
    let root = Path::new("/tmp/ligase-nowhere/.ligase");

    let first = Lock::directory(root, &Worker::named("node01"));
    let second = Lock::directory(root, &Worker::named("node02"));

    assert_ne!(first, second);
    assert!(first.starts_with(root), "{first:?}");
    assert!(first.ends_with("node01"), "{first:?}");
}

// ---------------------------------------- writing up a distributed run --

#[test]
fn a_run_that_computed_nothing_can_still_write_it_up() {
    // The collector of §2.4. Every step was computed elsewhere, so every
    // record was adopted rather than written -- which is the whole of what
    // D43 fixed, used deliberately.
    let lab = Lab::new();

    let mut worker = lab.worker("node01");
    let reads = worker.input("reads.txt", "ACGT\n");
    let align = worker.step("align", "cat {in} > {out}", reads);
    let call = worker.step("call", "cat {in} {in} > {out}", align);
    let done = worker.work(&[call], Duration::from_secs(60));
    assert_eq!(outcomes(&done, Outcome::Ran).len(), 2, "{done:?}");

    let mut collector = lab.worker("desk");
    let reads = collector.input("reads.txt", "ACGT\n");
    let align = collector.step("align", "cat {in} > {out}", reads);
    let call = collector.step("call", "cat {in} {in} > {out}", align);
    let report = collector.collect(&[call]);

    assert!(report.failures.is_empty(), "{report:?}");
    assert_eq!(report.counts().ran, 0, "the collector computed something");
    assert_eq!(report.counts().cached, 2, "{report:?}");

    // And it holds a record for every step, which is what `ligase methods`
    // needs: a run that completes and cannot be written up is what M14
    // found and D43 closed.
    for result in &report.results {
        assert!(
            collector.prov.lookup(result.key).unwrap().is_some(),
            "no record for `{}`, so this run cannot be written up",
            result.step
        );
    }
}
