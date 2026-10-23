//! Running a checked graph, once.
//!
//! The rule is one sentence: a step runs when something that could change
//! its result has changed, and not otherwise. Everything here follows from
//! it.
//!
//! A step's key is `hash(function code, parameters, input content hashes,
//! tool versions)` ([`crate::key`]). If the key is in the cache, the step
//! does not run. If it is not, the step runs and its output is stored under
//! the hash of its bytes ([`crate::cache`]). Because a downstream key is
//! built from its inputs' *content*, a step that reruns and produces the
//! same result leaves every key below it unchanged — which is early cutoff,
//! falling out of content addressing rather than being bolted on.
//!
//! "The same result" is the canonical form of the output, not its bytes
//! ([`crate::canon`], D4). With bytes it could not have worked past the
//! first real tool: `minimap2` writes its command line into `@PG CL`, that
//! command line holds the output path, and the output path is the staging
//! file named after the step's own key — so a step's bytes depended on its
//! own key, and any change at all cascaded to the bottom of the pipeline.
//!
//! # Why the body is not here
//!
//! A step body is Python. Running it inside the scheduler would put the GIL
//! in the hot path of a parallel run, so it is not: the body is called once,
//! before anything executes, and what it returns is a [`Recipe`] — a command
//! to render, or a callback for the one case that genuinely has to run
//! Python. The core then owns the whole run and the GIL is only taken for
//! those callbacks.
//!
//! # Why no worker ever waits
//!
//! A step is run by a rayon job of its own, and a job that finishes starts
//! the steps it released before it ends. Nothing loops looking for work and
//! nothing blocks waiting for it, so a worker with nothing of its own to do
//! is idle *as far as rayon is concerned* — and can therefore be given work
//! that a step queued on the pool from inside itself.
//!
//! That last part is the whole reason (D27). Hashing a step's inputs
//! happens inside the step, and BLAKE3 hashes a large file across the pool
//! it is called from (D3), so a step queues pool work as a matter of course.
//! A scheduler that parked its spare workers on a condition variable of its
//! own left that work with nobody able to take it, and a run could stop for
//! good — which is what happened under heavy load in M5.
//!
//! A worker can still block on something rayon cannot see: a step's own
//! child process, or the GIL in a callback. What it must never block on is
//! something that needs the pool to make progress, because that is the
//! shape a deadlock needs.
//!
//! # What a run records besides its own answer
//!
//! A step that runs writes a provenance record ([`crate::prov`]) naming the
//! command that ran, with its placeholders filled in. This is the one moment
//! that command exists: a cache hit cannot reconstruct it, because M4 drops
//! the `@PG` line that used to hold it (D22). A step served from the cache
//! writes nothing — the record from the run that did the work is still
//! there, which is the point of keeping it outside the cache.
//!
//! # What is deliberately not here
//!
//! Turning those records into prose. A methods paragraph and a bundle are
//! [`crate::prov`]'s business, built from what this module recorded and from
//! the graph, after a run rather than during one.
//!
//! Running a step twice to find out whether it is reproducible at all is
//! [`crate::audit`], which borrows this module's [`produce`] rather than
//! repeating it.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rayon::Scope;

use crate::cache::{Cache, Entry, Stored};
use crate::canon::Note;
use crate::graph::{Graph, NodeId, StepSpec};
use crate::hash::ContentHash;
use crate::key::{Ingredients, StepKey};
use crate::prov::{self, InputRecord, Prov, Record, day};
use crate::types::{DataKind, Dataset, Origin, RemoteVerdict};

/// How a step's output is produced.
///
/// Both are decided before the run starts, by asking the step's body once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recipe {
    /// A shell command, with `{placeholders}` for the step's inputs,
    /// parameters and `{out}`.
    Shell(String),
    /// A callback the host runs, identified by a number the host chose.
    Callback(u64),
}

impl Recipe {
    /// How this recipe enters the cache key.
    fn describe(&self) -> String {
        match self {
            Self::Shell(command) => format!("sh:{command}"),
            // A callback's identity is its function's code, which is
            // already an ingredient; the number is a position in this run's
            // table and means nothing between runs.
            Self::Callback(_) => "callback".to_owned(),
        }
    }
}

/// What the host knows about one step that the graph does not.
#[derive(Clone, Debug)]
pub struct Task {
    /// The node this is the recipe for.
    pub node: NodeId,
    /// The step function's source, as written.
    pub code: String,
    /// How to produce the output.
    pub recipe: Recipe,
    /// External tools whose versions join the key.
    pub tools: Vec<String>,
    /// Plain parameters, as `(name, value)`.
    pub parameters: Vec<(String, String)>,
    /// References the step asks to be cited by, as written.
    ///
    /// Deliberately **not** a cache-key ingredient: a citation cannot
    /// change a result, so adding one must not rerun a pipeline. For the
    /// same reason it is not stored in a provenance record either, which
    /// would go stale the moment it was edited — a methods section takes it
    /// from the pipeline in front of it ([`crate::prov::methods`]).
    pub citations: Vec<String>,
}

/// What happened to one step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It ran.
    Ran,
    /// Its key was already in the cache, and nothing above it ran.
    Cached,
    /// Its key was already in the cache although something above it ran:
    /// that step produced the same output as before, so there was nothing
    /// to redo here. This is early cutoff.
    Skipped,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ran => "ran",
            Self::Cached => "cached",
            Self::Skipped => "skipped",
        })
    }
}

/// What one step produced.
#[derive(Clone, Debug)]
pub struct StepResult {
    /// The node it was.
    pub node: NodeId,
    /// The step's name.
    pub step: String,
    /// Whether it ran, was cached, or was skipped.
    pub outcome: Outcome,
    /// Its cache key.
    pub key: StepKey,
    /// The hash of its output's bytes, which is its name in the store.
    pub hash: ContentHash,
    /// The hash of what its output *means* ([`crate::canon`]).
    ///
    /// This is what the steps below it were keyed on, so two runs that
    /// agree here cut off everything downstream however far their bytes
    /// diverged.
    pub canonical: ContentHash,
    /// What canonicalising the output noticed ([`Note`]).
    ///
    /// Empty for almost every step. A note is not a failure: the step ran
    /// and its result stands. It is reported because the fix is in the
    /// pipeline and a user cannot make it without being told (D26).
    pub notes: Vec<Note>,
    /// Where the output is.
    pub output: PathBuf,
    /// How long it took, when it ran.
    pub duration: Duration,
}

/// Why one step did not produce anything.
#[derive(Clone, Debug)]
pub struct StepFailure {
    /// The node it was.
    pub node: NodeId,
    /// The step's name.
    pub step: String,
    /// What went wrong, in the tool's own words where there are any.
    pub message: String,
}

/// What a run did.
///
/// A step whose input failed is in neither list: one cause, one failure.
#[derive(Clone, Debug, Default)]
pub struct RunReport {
    /// Every step that produced an output, in the order they finished.
    pub results: Vec<StepResult>,
    /// Every step that did not.
    pub failures: Vec<StepFailure>,
}

impl RunReport {
    /// What one node produced, if it produced anything.
    #[must_use]
    pub fn result(&self, node: NodeId) -> Option<&StepResult> {
        self.results.iter().find(|result| result.node == node)
    }

    /// How many steps had each outcome.
    #[must_use]
    pub fn counts(&self) -> Counts {
        let count = |wanted: Outcome| {
            self.results
                .iter()
                .filter(|result| result.outcome == wanted)
                .count()
        };
        Counts {
            ran: count(Outcome::Ran),
            cached: count(Outcome::Cached),
            skipped: count(Outcome::Skipped),
            failed: self.failures.len(),
        }
    }
}

/// How a run came out, in numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Steps that ran.
    pub ran: usize,
    /// Steps served from the cache.
    pub cached: usize,
    /// Steps skipped because an upstream rerun changed nothing.
    pub skipped: usize,
    /// Steps that failed.
    pub failed: usize,
}

/// Where a run reads what is already known, and writes down what it did.
///
/// Two stores with different lifetimes, handed over together because every
/// run needs both: the cache decides whether a step has to run at all
/// ([`crate::cache`]), and provenance records how it ran when it does
/// ([`crate::prov`]). They are separate because a cache is meant to be
/// thrown away and a record of how a result was produced is not.
#[derive(Clone, Copy, Debug)]
pub struct Stores<'a> {
    /// What has already been computed, and what it produced.
    pub cache: &'a Cache,
    /// How each result was produced.
    pub prov: &'a Prov,
}

/// How to run.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// How many steps may run at once.
    pub jobs: usize,
    /// Run every step, cache or no cache.
    pub force: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            jobs: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
            force: false,
        }
    }
}

/// Whatever can run the callbacks the core cannot.
///
/// A shell step is run by the core. A step whose body is Python has to go
/// back to Python, and this is the only door through which it does.
pub trait Host: Sync {
    /// Produce the output at `out`, from inputs given by parameter name.
    ///
    /// Returns what the body noticed about its own work, which is almost
    /// always nothing. A body is the only thing that can see an assumption
    /// its work rested on — a resistance reading made without coverage
    /// leaves no trace of it in the report it writes — so this is the door
    /// such a finding comes back through (D32). A note never fails a step:
    /// the work was done and its result stands.
    ///
    /// # Errors
    ///
    /// Whatever the callback raised, as a message for the user.
    fn call(
        &self,
        id: u64,
        out: &Path,
        inputs: &[(String, Vec<PathBuf>)],
    ) -> Result<Vec<Note>, String>;
}

/// A host that has no callbacks, for a pipeline of shell steps.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoCallbacks;

impl Host for NoCallbacks {
    fn call(
        &self,
        id: u64,
        _out: &Path,
        _inputs: &[(String, Vec<PathBuf>)],
    ) -> Result<Vec<Note>, String> {
        Err(format!(
            "this step's body is a callback ({id}), and nothing here can run one"
        ))
    }
}

/// A dependency whose version is part of a result but which is not a
/// program to run.
///
/// `tool=` means "something whose version changes the answer". Almost
/// always that is an external program, probed with `--version`. Two are
/// not, and both are compiled into this engine with no process to run and
/// no `--version` to print, while changing answers exactly as a tool does.
///
/// `hivdb` is a body of versioned knowledge — the resistance rules.
/// `ligase` is the engine's own work: a conversion such as
/// `map_identifiers` is code in this crate, so what a step declaring it
/// depends on is the crate. Nothing else would put that in a cache key: a
/// step body that only calls into the engine hashes the same however the
/// engine changes underneath it.
///
/// Naming them here rather than special-casing them inside the probe keeps
/// the exceptions visible, and keeps them to one place.
const BUILT_IN: &[&str] = &["hivdb", "ligase"];

/// The version a step's dependency reports.
///
/// For a built-in, what the engine carries. For everything else, the
/// whole of `<tool> --version`, because the parts a tool prints below the
/// first line — the htslib a samtools was built against, say — change
/// results too.
///
/// # Errors
///
/// If the tool is not installed or prints nothing. Hashing an absent tool
/// as "no version" would let two machines with different tools agree on a
/// key, which is exactly the failure this is here to prevent.
pub fn tool_version(tool: &str) -> Result<String, String> {
    match tool {
        "hivdb" => return crate::hivdb::ruleset::fingerprint(),
        "ligase" => return Ok(format!("ligase {}", env!("CARGO_PKG_VERSION"))),
        _ => {}
    }
    debug_assert!(
        !BUILT_IN.contains(&tool),
        "a built-in tool with no version of its own"
    );

    let output = Command::new(tool)
        .arg("--version")
        .output()
        .map_err(|error| format!("`{tool}` could not be run: {error}"))?;

    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if text.is_empty() {
        text = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    }
    if text.is_empty() {
        return Err(format!("`{tool} --version` printed nothing"));
    }
    Ok(text)
}

/// The file extension an output of this kind is given.
///
/// Tools read the format off the name, so this is not cosmetic: a BAM step
/// writing to a name ending `.sam` gets SAM.
#[must_use]
pub const fn output_suffix(kind: DataKind) -> &'static str {
    match kind {
        DataKind::Reads => ".fastq.gz",
        DataKind::Reference => ".fasta",
        DataKind::Alignment => ".bam",
        DataKind::Variants => ".vcf.gz",
        DataKind::Intervals => ".bed",
        DataKind::ResistanceReport => ".tsv",
        DataKind::EntitySet | DataKind::RankedList => ".tsv",
    }
}

/// Run every step these outputs depend on.
///
/// Independent steps run at once, up to [`Options::jobs`]. A step whose
/// input failed does not run and is not reported: one cause, one failure.
#[must_use]
pub fn run(
    graph: &Graph,
    roots: &[NodeId],
    tasks: &[Task],
    stores: &Stores<'_>,
    options: &Options,
    host: &dyn Host,
) -> RunReport {
    let nodes: Vec<NodeId> = graph
        .reachable_nodes(roots)
        .into_iter()
        .filter(|node| graph.step(*node).is_some())
        .collect();
    if nodes.is_empty() {
        return RunReport::default();
    }

    let runner = Runner {
        graph,
        cache: stores.cache,
        prov: stores.prov,
        host,
        force: options.force,
        tasks: tasks.iter().map(|task| (task.node, task)).collect(),
        versions: versions(graph, &nodes, tasks),
        state: Mutex::new(State::new(graph, &nodes)),
    };

    let jobs = options.jobs.max(1).min(nodes.len());
    match rayon::ThreadPoolBuilder::new().num_threads(jobs).build() {
        Ok(pool) => pool.scope(|scope| runner.start(scope)),
        // A pool we cannot build is not a reason to refuse to run.
        Err(_) => runner.alone(),
    }

    runner.finish()
}

/// Each of a step's arguments' files, by the parameter they fill.
///
/// `produced` answers where an upstream step put its output; an argument it
/// has no answer for is a loaded dataset, and its own files are used. Shared
/// with [`crate::audit`], so that a step being audited is handed exactly
/// what a run would have handed it.
///
/// # Errors
///
/// If an argument is bound to nothing, or to data with no file behind it.
pub fn inputs_of(
    graph: &Graph,
    node: NodeId,
    produced: &dyn Fn(NodeId) -> Option<PathBuf>,
) -> Result<Vec<(String, Vec<PathBuf>)>, String> {
    let spec = graph
        .step(node)
        .ok_or_else(|| "this node is not a step".to_owned())?;

    spec.arguments
        .iter()
        .map(|argument| {
            let files = if let Some(path) = produced(argument.node) {
                vec![path]
            } else {
                let dataset = graph
                    .dataset(argument.node)
                    .ok_or_else(|| format!("`{}` is bound to nothing", argument.param))?;
                let files = dataset.files();
                if files.is_empty() {
                    return Err(format!(
                        "`{}` is bound to data with no file behind it",
                        argument.param
                    ));
                }
                files
            };
            Ok((argument.param.clone(), files))
        })
        .collect()
}

/// What running a step took, and what it ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    /// How long it took.
    pub duration: Duration,
    /// The command that ran, with its placeholders filled in.
    ///
    /// `None` for a callback, which has no command. This is what a
    /// provenance record keeps: it exists only here, because the field a
    /// tool writes it into is dropped before hashing (D22).
    pub command: Option<String>,
    /// What the body said it noticed about its own work ([`Note`]).
    ///
    /// Always empty for a shell step: a command's assumptions are not
    /// something the engine can be told about.
    pub notes: Vec<Note>,
}

/// Produce one step's output at `out`, from inputs already in place.
///
/// The cache is not consulted and nothing is recorded: this is the part of
/// running a step that is just running it, which is why
/// [`crate::audit`] can do it twice.
///
/// # Errors
///
/// Whatever the command or the callback said, or a note that the step
/// finished without writing anything.
pub fn produce(
    recipe: &Recipe,
    inputs: &[(String, Vec<PathBuf>)],
    parameters: &[(String, String)],
    out: &Path,
    host: &dyn Host,
) -> Result<Execution, String> {
    let started = Instant::now();
    let mut ran = None;
    let produced = match recipe {
        Recipe::Shell(template) => {
            let command = fill_template(template, inputs, parameters, out)?;
            let outcome = shell(&command).map(|()| Vec::new());
            ran = Some(command);
            outcome
        }
        Recipe::Callback(id) => host.call(*id, out, inputs),
    };
    let duration = started.elapsed();

    let notes = produced?;
    if !out.is_file() {
        return Err(format!(
            "the step finished without writing its output to {}",
            out.display()
        ));
    }
    Ok(Execution {
        duration,
        command: ran,
        notes,
    })
}

/// What a step said about its work, then what reading its output found.
///
/// One list, because every surface that prints notes prints them together
/// and a reader has no use for the distinction. In this order because a
/// note about how a result was arrived at is the one to read first. Nothing
/// is said twice: the two kinds cannot collide today, but a note repeated
/// in a report is a report that looks broken.
fn noticed(said: Vec<Note>, found: Vec<Note>) -> Vec<Note> {
    let mut notes = said;
    for note in found {
        if !notes.contains(&note) {
            notes.push(note);
        }
    }
    notes
}

/// Every tool named by any of these steps, probed once.
fn versions(
    graph: &Graph,
    nodes: &[NodeId],
    tasks: &[Task],
) -> HashMap<String, Result<String, String>> {
    let wanted: BTreeSet<&str> = tasks
        .iter()
        .filter(|task| nodes.contains(&task.node) && graph.step(task.node).is_some())
        .flat_map(|task| task.tools.iter().map(String::as_str))
        .collect();

    wanted
        .into_iter()
        .map(|tool| (tool.to_owned(), tool_version(tool)))
        .collect()
}

// ------------------------------------------------------------ the loop --

/// What a finished step left for the steps below it.
#[derive(Clone, Debug)]
struct Produced {
    output: Stored,
    /// Whether this step, or anything above it, ran in this run. The step
    /// below uses it to tell a cache hit from early cutoff.
    ran_above: bool,
}

struct State {
    /// The steps that can start now. Nothing waits on this queue: a worker
    /// that finds it empty has nothing to wait for, because every node that
    /// will ever land here is put there by a step that is still running
    /// (D27).
    ready: Vec<NodeId>,
    remaining: HashMap<NodeId, usize>,
    dependents: HashMap<NodeId, Vec<NodeId>>,
    produced: HashMap<NodeId, Produced>,
    results: Vec<StepResult>,
    failures: Vec<StepFailure>,
    running: HashSet<StepKey>,
    /// Nodes waiting for a key that another node is already producing,
    /// under that key. They go back onto `ready` when it is released.
    parked: HashMap<StepKey, Vec<NodeId>>,
}

impl State {
    fn new(graph: &Graph, nodes: &[NodeId]) -> Self {
        let wanted: HashSet<NodeId> = nodes.iter().copied().collect();
        let mut remaining = HashMap::new();
        let mut dependents: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        for node in nodes {
            let upstream: BTreeSet<NodeId> = graph
                .step(*node)
                .map(|spec| {
                    spec.arguments
                        .iter()
                        .map(|argument| argument.node)
                        .filter(|from| wanted.contains(from))
                        .collect()
                })
                .unwrap_or_default();
            remaining.insert(*node, upstream.len());
            for from in upstream {
                dependents.entry(from).or_default().push(*node);
            }
        }

        let ready = remaining
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(node, _)| *node)
            .collect();

        Self {
            ready,
            remaining,
            dependents,
            produced: HashMap::new(),
            results: Vec::new(),
            failures: Vec::new(),
            running: HashSet::new(),
            parked: HashMap::new(),
        }
    }

    /// Release the steps that were waiting on this one.
    fn release(&mut self, node: NodeId) {
        let Some(waiting) = self.dependents.get(&node).cloned() else {
            return;
        };
        for next in waiting {
            if let Some(count) = self.remaining.get_mut(&next) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.ready.push(next);
                }
            }
        }
    }
}

struct Runner<'a> {
    graph: &'a Graph,
    cache: &'a Cache,
    prov: &'a Prov,
    host: &'a dyn Host,
    force: bool,
    tasks: HashMap<NodeId, &'a Task>,
    versions: HashMap<String, Result<String, String>>,
    state: Mutex<State>,
}

impl Runner<'_> {
    /// Start every step that can start, on the pool.
    ///
    /// The run is over when the scope these are spawned into is empty,
    /// which is when the graph has drained: each step starts the steps it
    /// releases before its own job ends, so the scope cannot empty while
    /// anything can still start.
    fn start<'scope>(&'scope self, scope: &Scope<'scope>) {
        for node in self.released() {
            self.spawn(scope, node);
        }
    }

    /// Run one step on the pool, and whatever it releases after it.
    ///
    /// A worker is never held waiting for the next step. It runs one and
    /// hands the scheduler back, so rayon knows it is idle and can give it
    /// work queued from inside another step — which is what hashing a
    /// step's inputs does (D27).
    fn spawn<'scope>(&'scope self, scope: &Scope<'scope>, node: NodeId) {
        scope.spawn(move |scope| {
            for next in self.run_one(node) {
                self.spawn(scope, next);
            }
        });
    }

    /// Run the whole graph on this thread, one step at a time.
    ///
    /// Only reached when a thread pool cannot be built at all. One thread
    /// means no two steps are ever in flight, so no key is ever claimed
    /// twice and nothing is ever parked.
    fn alone(&self) {
        let mut ready = self.released();
        while let Some(node) = ready.pop() {
            ready.extend(self.run_one(node));
        }
    }

    /// Run one step, write down what happened, and say what can start now.
    ///
    /// The steps returned are the ones whose last input just arrived, plus
    /// any twin that was waiting for this step's key. A step that found its
    /// own key already being produced returns nothing: it is parked on that
    /// key, and the step holding it will hand it back.
    fn run_one(&self, node: NodeId) -> Vec<NodeId> {
        let done = self.execute(node);

        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        match done {
            Ok(Attempt::Done(result)) => {
                let ran_above = result.outcome == Outcome::Ran || self.ran_above(&state, node);
                state.produced.insert(
                    node,
                    Produced {
                        output: Stored {
                            hash: result.hash,
                            path: result.output.clone(),
                        },
                        ran_above,
                    },
                );
                state.results.push(result);
                state.release(node);
            }
            // Nothing happened yet, and nothing was released. This node is
            // on `parked` under its key, and comes back when it is free.
            Ok(Attempt::Parked) => {}
            Err(failure) => state.failures.push(failure),
        }
        std::mem::take(&mut state.ready)
    }

    /// Everything on the ready queue, emptying it.
    fn released(&self) -> Vec<NodeId> {
        self.state
            .lock()
            .map_or_else(|_| Vec::new(), |mut state| std::mem::take(&mut state.ready))
    }

    fn ran_above(&self, state: &State, node: NodeId) -> bool {
        self.graph.step(node).is_some_and(|spec| {
            spec.arguments.iter().any(|argument| {
                state
                    .produced
                    .get(&argument.node)
                    .is_some_and(|produced| produced.ran_above)
            })
        })
    }

    fn execute(&self, node: NodeId) -> Result<Attempt, StepFailure> {
        let name = self
            .graph
            .step(node)
            .map_or_else(|| "?".to_owned(), |spec| spec.name.clone());
        self.attempt(node).map_err(|message| StepFailure {
            node,
            step: name,
            message,
        })
    }

    fn attempt(&self, node: NodeId) -> Result<Attempt, String> {
        let spec = self
            .graph
            .step(node)
            .ok_or_else(|| "this node is not a step".to_owned())?;
        let task = *self.tasks.get(&node).ok_or_else(|| {
            "no recipe was recorded for this step, so there is nothing to run".to_owned()
        })?;

        let inputs = self.inputs(node)?;
        let key = self.key(spec, task, &inputs)?;

        if !self.force
            && let Some(entry) = self.cache.lookup(key).map_err(|error| error.to_string())?
        {
            return Ok(Attempt::Done(self.cached_result(node, spec, key, &entry)?));
        }

        // Two nodes can share a key -- the same step over identical data --
        // and running both would be the same work twice. The second parks.
        let Some(_claim) = self.claim(key, node)? else {
            return Ok(Attempt::Parked);
        };
        if !self.force
            && let Some(entry) = self.cache.lookup(key).map_err(|error| error.to_string())?
        {
            return Ok(Attempt::Done(self.cached_result(node, spec, key, &entry)?));
        }

        let staging = self.cache.staging(key, output_suffix(spec.output));
        let _ = std::fs::remove_file(&staging);

        let produced = produce(&task.recipe, &inputs, &task.parameters, &staging, self.host)
            .inspect_err(|_| {
                let _ = std::fs::remove_file(&staging);
            })?;

        let stored = self
            .cache
            .store(&staging)
            .map_err(|error| error.to_string())?;
        let milliseconds = u64::try_from(produced.duration.as_millis()).unwrap_or(u64::MAX);
        self.cache
            .record(
                key,
                &spec.name,
                &stored,
                &self.tool_summary(task),
                milliseconds,
                &produced.notes,
            )
            .map_err(|error| error.to_string())?;

        let meaning = self
            .cache
            .semantic(spec.output, &stored.path)
            .map_err(|error| error.to_string())?;
        let canonical = meaning.hash;

        // What the step said about its own work, then what reading its
        // output found. Both are notes about the same result and every
        // surface prints them together; the body's come first because they
        // are about how the result was arrived at.
        let notes = noticed(produced.notes, meaning.notes);

        // Recorded here and nowhere else: this is the only moment the filled
        // command exists, and a step served from the cache will need it.
        self.remember(&Made {
            spec,
            task,
            key,
            inputs: &inputs,
            command: produced.command,
            bytes: stored.hash,
            canonical,
            notes: &notes,
            duration_ms: milliseconds,
        })?;

        Ok(Attempt::Done(StepResult {
            node,
            step: spec.name.clone(),
            outcome: Outcome::Ran,
            key,
            hash: stored.hash,
            canonical,
            notes,
            output: stored.path,
            duration: produced.duration,
        }))
    }

    /// Hold this key for the length of the step, so a twin waits.
    ///
    /// `None` when another node is already producing exactly this. That
    /// node is parked on the key rather than blocked on it: a worker left
    /// waiting here is a worker rayon counts as busy, and so a worker that
    /// cannot help the step it is waiting for (D27). It comes back when the
    /// claim is dropped, by which time the key is in the cache and it is a
    /// cache hit.
    fn claim(&self, key: StepKey, node: NodeId) -> Result<Option<Claim<'_>>, String> {
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if state.running.contains(&key) {
            state.parked.entry(key).or_default().push(node);
            return Ok(None);
        }
        state.running.insert(key);
        Ok(Some(Claim { runner: self, key }))
    }

    fn cached_result(
        &self,
        node: NodeId,
        spec: &StepSpec,
        key: StepKey,
        entry: &Entry,
    ) -> Result<StepResult, String> {
        let output = &entry.output;
        let outcome = {
            let upstream_ran = self
                .state
                .lock()
                .is_ok_and(|state| self.ran_above(&state, node));
            if upstream_ran {
                Outcome::Skipped
            } else {
                Outcome::Cached
            }
        };
        // Remembered with the hash, so a step served from the cache still
        // reports what was seen when its output was first read.
        let meaning = self
            .cache
            .semantic(spec.output, &output.path)
            .map_err(|error| error.to_string())?;

        Ok(StepResult {
            node,
            step: spec.name.clone(),
            outcome,
            key,
            hash: output.hash,
            canonical: meaning.hash,
            // Both kinds remembered rather than recomputed: a cache hit
            // neither reruns the body nor reopens the file, and a note that
            // stopped appearing on the second run would be worse than none.
            notes: noticed(entry.notes.clone(), meaning.notes),
            output: output.path.clone(),
            duration: Duration::ZERO,
        })
    }

    /// Each argument's files, by the parameter they fill.
    fn inputs(&self, node: NodeId) -> Result<Vec<(String, Vec<PathBuf>)>, String> {
        let state = self.state.lock().map_err(|_| poisoned())?;
        inputs_of(self.graph, node, &|from| {
            state
                .produced
                .get(&from)
                .map(|produced| produced.output.path.clone())
        })
    }

    fn key(
        &self,
        spec: &StepSpec,
        task: &Task,
        inputs: &[(String, Vec<PathBuf>)],
    ) -> Result<StepKey, String> {
        let mut hashed = Vec::with_capacity(inputs.len());
        // The arguments and the inputs are in the same order: the inputs
        // were built from the arguments.
        for (argument, (param, files)) in spec.arguments.iter().zip(inputs) {
            let kind = self
                .graph
                .dataset(argument.node)
                .map_or(argument.declared, Dataset::kind);
            let mut each = Vec::with_capacity(files.len());
            for file in files {
                each.push(
                    self.cache
                        // What a file means, not what bytes it holds (D4).
                        .semantic_hash(kind, file)
                        .map_err(|error| format!("`{param}`: {error}"))?,
                );
            }
            hashed.push((param.clone(), ContentHash::of_hashes(&each)));
        }

        let mut tools = Vec::with_capacity(task.tools.len());
        for tool in &task.tools {
            let version = self
                .versions
                .get(tool)
                .cloned()
                .unwrap_or_else(|| tool_version(tool))?;
            tools.push((tool.clone(), version));
        }

        Ok(Ingredients {
            step: spec.name.clone(),
            code: task.code.clone(),
            recipe: task.recipe.describe(),
            output: spec.output,
            parameters: task.parameters.clone(),
            inputs: hashed,
            tools,
        }
        .key())
    }

    /// Write down how this step produced what it just produced.
    ///
    /// Every hash here is already known to the cache — the inputs were
    /// hashed to build the key moments ago — so this costs lookups rather
    /// than reads.
    fn remember(&self, made: &Made<'_>) -> Result<(), String> {
        let mut inputs = Vec::new();
        let mut references = Vec::new();
        let mut remotes = Vec::new();

        // The arguments and the inputs are in the same order: the inputs
        // were built from the arguments.
        for (argument, (param, files)) in made.spec.arguments.iter().zip(made.inputs) {
            let dataset = self.graph.dataset(argument.node);
            let kind = dataset.map_or(argument.declared, Dataset::kind);

            for file in files {
                let bytes = self
                    .cache
                    .content_hash(file)
                    .map_err(|error| format!("`{param}`: {error}"))?;
                let canonical = self
                    .cache
                    .semantic_hash(kind, file)
                    .map_err(|error| format!("`{param}`: {error}"))?;
                inputs.push(InputRecord {
                    param: param.clone(),
                    kind,
                    path: file.clone(),
                    bytes,
                    canonical,
                });
            }

            // Where an input was fetched from, if it was fetched. A remote
            // the pipeline declared once but passed to three steps is
            // recorded against each of them, because a record is about one
            // computation; the methods report is what de-duplicates.
            if let Some(dataset) = dataset
                && let Some(origin) = dataset.origin()
            {
                remotes.push(origin.clone());
            }

            // A reference is pinned by what its own file says: the accession
            // in its defline and the checksum computed from its sequence.
            if kind == DataKind::Reference
                && let Some(dataset) = dataset
                && let Some(identity) = dataset.dimensions().reference.value()
                && let Some(path) = dataset.path()
            {
                references.push(prov::pin(param, path, identity));
            }
        }

        let mut tools = Vec::with_capacity(made.task.tools.len());
        for tool in &made.task.tools {
            if let Some(Ok(version)) = self.versions.get(tool) {
                tools.push((tool.clone(), version.clone()));
            }
        }

        self.prov
            .record(&Record {
                key: made.key,
                step: made.spec.name.clone(),
                code: made.task.code.clone(),
                command: made.command.clone(),
                output_kind: made.spec.output,
                output_bytes: made.bytes,
                output_canonical: made.canonical,
                output_notes: made.notes.to_vec(),
                parameters: made.task.parameters.clone(),
                inputs,
                references,
                remotes,
                tools,
                engine: crate::version().to_owned(),
                platform: prov::platform(),
                recorded_at: prov::now(),
                duration_ms: made.duration_ms,
            })
            .map_err(|error| error.to_string())
    }

    fn tool_summary(&self, task: &Task) -> String {
        task.tools
            .iter()
            .map(|tool| match self.versions.get(tool) {
                Some(Ok(version)) => format!("{tool} {}", first_line(version)),
                _ => tool.clone(),
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn finish(self) -> RunReport {
        let Ok(state) = self.state.into_inner() else {
            return RunReport::default();
        };
        let mut results = state.results;
        let mut failures = state.failures;
        // Workers finish in whatever order the machine allows; a report that
        // changed between two identical runs would be a strange thing for a
        // reproducibility tool to print.
        results.sort_by_key(|result| result.node.index());
        failures.sort_by_key(|failure| failure.node.index());
        RunReport { results, failures }
    }
}

/// What one turn at a step came to.
enum Attempt {
    /// The step is finished, by running or straight from the cache.
    Done(StepResult),
    /// Another node is producing this step's key, so this one is parked on
    /// it and will be started again once that key is free.
    Parked,
}

/// What a step just produced, as a provenance record wants it.
struct Made<'a> {
    spec: &'a StepSpec,
    task: &'a Task,
    key: StepKey,
    inputs: &'a [(String, Vec<PathBuf>)],
    command: Option<String>,
    bytes: ContentHash,
    canonical: ContentHash,
    notes: &'a [Note],
    duration_ms: u64,
}

/// A key held for the length of one step.
struct Claim<'a> {
    runner: &'a Runner<'a>,
    key: StepKey,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.runner.state.lock() {
            state.running.remove(&self.key);
            // Whoever was waiting for this key can go now. They are put
            // back on the ready queue rather than woken, because the step
            // that just finished is the one that starts them -- which is
            // why the queue never needs waiting on.
            if let Some(waiting) = state.parked.remove(&self.key) {
                state.ready.extend(waiting);
            }
        }
    }
}

fn poisoned() -> String {
    "the run was left locked by a panicking step".to_owned()
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text).trim()
}

// --------------------------------------------------------- the command --

/// Run a command through the shell, as written.
fn shell(command: &str) -> Result<(), String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .map_err(|error| format!("the command could not be started: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    let code = output
        .status
        .code()
        .map_or_else(|| "a signal".to_owned(), |code| code.to_string());
    let mut message = format!("the command failed with exit status {code}");
    for stream in [&output.stderr, &output.stdout] {
        let text = String::from_utf8_lossy(stream);
        let text = text.trim();
        if !text.is_empty() {
            message.push('\n');
            message.push_str(text);
        }
    }
    Err(message)
}

/// Fill a command's placeholders.
///
/// `{name}` is an input or a parameter, `{name.r1}` and `{name.r2}` the
/// mates of a paired input, and `{out}` the file the step must write. A
/// literal brace is written `{{` or `}}`, which is what an `awk '{{print}}'`
/// needs; without the escape there is no way to tell an awk program from a
/// placeholder, and a silently mistyped placeholder is worse than a rule.
///
/// Every substituted value becomes exactly one shell word, whatever is in
/// it: a path with a space in it is a path, not two arguments.
fn fill_template(
    template: &str,
    inputs: &[(String, Vec<PathBuf>)],
    parameters: &[(String, String)],
    out: &Path,
) -> Result<String, String> {
    let mut command = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(at) = rest.find(['{', '}']) {
        command.push_str(&rest[..at]);
        let after = &rest[at..];
        let brace = &after[..1];
        let inside = &after[1..];

        // A doubled brace is a literal one.
        if inside.starts_with(brace) {
            command.push_str(brace);
            rest = &inside[1..];
            continue;
        }
        if brace == "}" {
            return Err(unbalanced(template));
        }

        let Some(end) = inside.find('}') else {
            return Err(unbalanced(template));
        };
        let name = &inside[..end];
        if !is_name(name) {
            return Err(unbalanced(template));
        }
        command.push_str(&quote(&resolve(name, inputs, parameters, out)?));
        rest = &inside[end + 1..];
    }

    command.push_str(rest);
    Ok(command)
}

/// Whether what sits between braces is meant to be a placeholder at all.
///
/// An `awk '{print $1}'` is not a mistyped placeholder, so it is not
/// reported as one -- it is reported as a brace that needs escaping, which
/// is the thing the user has to change either way.
fn is_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

fn unbalanced(template: &str) -> String {
    format!(
        "this command has a `{{` that is not a placeholder; write `{{{{` and `}}}}` for \
         literal braces, as an `awk '{{{{print $1}}}}'` needs: {template}"
    )
}

fn resolve(
    name: &str,
    inputs: &[(String, Vec<PathBuf>)],
    parameters: &[(String, String)],
    out: &Path,
) -> Result<String, String> {
    if name == "out" {
        return Ok(out.to_string_lossy().into_owned());
    }

    let (base, field) = match name.split_once('.') {
        Some((base, field)) => (base, Some(field)),
        None => (name, None),
    };

    if let Some((_, files)) = inputs.iter().find(|(param, _)| param == base) {
        let first = || {
            files
                .first()
                .map(|path| path.to_string_lossy().into_owned())
                .ok_or_else(|| format!("`{{{name}}}`: `{base}` has no file behind it"))
        };
        return match field {
            None | Some("r1") => first(),
            Some("r2") => files
                .get(1)
                .map(|path| path.to_string_lossy().into_owned())
                .ok_or_else(|| {
                    format!(
                        "`{{{name}}}`: `{base}` is a single-end sample, so it has no second mate"
                    )
                }),
            Some(other) => Err(format!(
                "`{{{name}}}`: an input has `r1` and `r2`, not `{other}`"
            )),
        };
    }

    if field.is_none()
        && let Some((_, value)) = parameters.iter().find(|(param, _)| param == base)
    {
        return Ok(value.clone());
    }

    let mut known: Vec<&str> = inputs.iter().map(|(param, _)| param.as_str()).collect();
    known.extend(parameters.iter().map(|(param, _)| param.as_str()));
    known.push("out");
    known.sort_unstable();
    Err(format!(
        "`{{{name}}}` is not a parameter of this step; it has {}",
        known.join(", ")
    ))
}

/// One shell word, whatever is in it.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

// ---------------------------------------------------------- report --

/// One line summarising a run.
///
/// Counts places rather than names, for the same reason a check summary
/// does (D15): one written step is many steps under fan-out.
#[must_use]
pub fn summarise(report: &RunReport) -> String {
    let counts = report.counts();
    let steps = counts.ran + counts.cached + counts.skipped + counts.failed;
    if steps == 0 {
        return "nothing to run.".to_owned();
    }

    let mut parts = Vec::new();
    if counts.ran > 0 {
        parts.push(format!("{} ran", counts.ran));
    }
    if counts.cached > 0 {
        parts.push(format!("{} from cache", counts.cached));
    }
    if counts.skipped > 0 {
        parts.push(format!("{} skipped", counts.skipped));
    }
    if counts.failed > 0 {
        parts.push(format!("{} failed", counts.failed));
    }

    format!("{steps} {}: {}.", plural(steps, "step"), parts.join(", "))
}

/// The whole run as the command line prints it.
///
/// A step that ran gets its own line, with what it cost. Steps that did not
/// are grouped by what happened to them and their samples are *named*
/// rather than counted, because a user who has to expand something to find
/// out which samples were cached has a shorter report rather than a better
/// one — the same rule the check report follows (D15).
#[must_use]
pub fn render(graph: &Graph, roots: &[NodeId], report: &RunReport) -> String {
    let scopes = graph.scopes(roots);
    let mut lines = Vec::new();

    // Steps that ran, in graph order, each with its own cost. The costs
    // line up, because reading down a column is how a user finds the step
    // that took the night.
    let ran: Vec<(String, String)> = report
        .results
        .iter()
        .filter(|result| result.outcome == Outcome::Ran)
        .map(|result| {
            (
                format!(
                    "{}{}",
                    quoted(&result.step),
                    scope_suffix(scopes.get(&result.node))
                ),
                duration(result.duration),
            )
        })
        .collect();
    let widest = ran.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
    for (label, cost) in &ran {
        lines.push(format!("ran       {label:<widest$}   {cost}"));
    }

    // Steps that did not, gathered by name and outcome.
    for wanted in [Outcome::Cached, Outcome::Skipped] {
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for result in report
            .results
            .iter()
            .filter(|result| result.outcome == wanted)
        {
            let samples = scopes
                .get(&result.node)
                .map(|scope| scope.value.clone())
                .into_iter()
                .collect::<Vec<String>>();
            match groups.iter_mut().find(|(step, _)| *step == result.step) {
                Some((_, seen)) => seen.extend(samples),
                None => groups.push((result.step.clone(), samples)),
            }
        }

        for (step, samples) in groups {
            let label = match wanted {
                Outcome::Skipped => "skipped  ",
                _ => "cached   ",
            };
            lines.push(match samples.as_slice() {
                [] => format!("{label} {}", quoted(&step)),
                [one] => format!("{label} {} ({one})", quoted(&step)),
                many => format!("{label} {} for {}", quoted(&step), many.join(", ")),
            });
        }
    }

    // What the outputs' canonical forms noticed. Not failures: every one of
    // these steps ran and its result stands. They are here because the fix
    // is in the pipeline and a user cannot make it without being told
    // (D26), and before the failures because a failure is the thing to read
    // first.
    let noted: Vec<(&StepResult, &Note)> = report
        .results
        .iter()
        .flat_map(|result| result.notes.iter().map(move |note| (result, note)))
        .collect();
    if !noted.is_empty() {
        lines.push(String::new());
        for (result, note) in &noted {
            lines.push(format!(
                "noticed   {}{}: {note}",
                quoted(&result.step),
                scope_suffix(scopes.get(&result.node))
            ));
        }
        // Said once per kind, not once per step: four samples named after
        // four files have one thing wrong with them (D15).
        let mut given: Vec<&str> = Vec::new();
        for (_, note) in &noted {
            if !given.contains(&note.code()) {
                given.push(note.code());
                lines.push(format!("          {}", note.advice()));
            }
        }
    }

    // Where this run's inputs came from, which for a query is the whole
    // question: "it worked" and "it worked without asking anybody" are
    // different statements, and D39 asks that the report always say which
    // of the two this was.
    let mut remotes: Vec<(&Origin, &[Note])> = Vec::new();
    for node in graph.reachable_nodes(roots) {
        if let Some(dataset) = graph.dataset(node)
            && let Some(origin) = dataset.origin()
            && !remotes
                .iter()
                .any(|(held, _)| held.locator == origin.locator)
        {
            remotes.push((origin, dataset.notes()));
        }
    }
    if !remotes.is_empty() {
        lines.push(String::new());
        for (origin, notes) in remotes {
            let what = match origin.verdict {
                RemoteVerdict::Pinned => format!("pinned, fetched {}", day(origin.fetched_at)),
                RemoteVerdict::Replayed => {
                    format!("replayed from record of {}", day(origin.fetched_at))
                }
                RemoteVerdict::Live => format!("answered live on {}", day(origin.fetched_at)),
            };
            lines.push(format!("input     {} -- {what}", quoted(&origin.locator)));
            // What resolving it noticed: drift a refresh found, a field
            // dropped from a recorded response, a source that rewrote a
            // pinned file. Printed here rather than in the `noticed` block
            // above, which is about what a *step* saw in its own output.
            for note in notes {
                lines.push(format!("          {note}"));
            }
        }
    }

    for failure in &report.failures {
        lines.push(String::new());
        lines.push(format!(
            "{}{} failed:",
            quoted(&failure.step),
            scope_suffix(scopes.get(&failure.node))
        ));
        for line in failure.message.lines() {
            lines.push(format!("  {line}"));
        }
    }

    if !lines.is_empty() {
        lines.push(String::new());
    }
    lines.push(summarise(report));
    lines.join("\n")
}

/// How a step whose name is ambiguous is told apart, where it is.
fn scope_suffix(scope: Option<&crate::check::Scope>) -> String {
    scope.map_or_else(String::new, |scope| {
        format!(" ({} {})", scope.kind, scope.value)
    })
}

fn quoted(step: &str) -> String {
    format!("`{step}`")
}

/// A duration as a reader wants it, not as a float.
fn duration(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs_f64();
    if seconds < 1.0 {
        return format!("{}ms", elapsed.as_millis());
    }
    if seconds < 60.0 {
        return format!("{seconds:.1}s");
    }
    let whole = elapsed.as_secs();
    format!("{}m {}s", whole / 60, whole % 60)
}

fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        word.to_owned()
    } else {
        format!("{word}s")
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn inputs() -> Vec<(String, Vec<PathBuf>)> {
        vec![
            ("ref".into(), vec![PathBuf::from("/refs/HXB2.fasta")]),
            (
                "reads".into(),
                vec![PathBuf::from("/data/s_1.fq"), PathBuf::from("/data/s_2.fq")],
            ),
        ]
    }

    fn parameters() -> Vec<(String, String)> {
        vec![("min_freq".into(), "0.01".into())]
    }

    fn fill(template: &str) -> Result<String, String> {
        fill_template(
            template,
            &inputs(),
            &parameters(),
            Path::new("/tmp/out.bam"),
        )
    }

    #[test]
    fn an_input_a_mate_a_parameter_and_the_output_are_filled_in() {
        assert_eq!(
            fill("minimap2 -ax sr {ref} {reads.r1} {reads.r2} -f {min_freq} -o {out}").unwrap(),
            "minimap2 -ax sr '/refs/HXB2.fasta' '/data/s_1.fq' '/data/s_2.fq' \
             -f '0.01' -o '/tmp/out.bam'"
        );
    }

    #[test]
    fn an_input_without_a_mate_named_is_its_first_file() {
        assert_eq!(fill("cat {reads}").unwrap(), "cat '/data/s_1.fq'");
    }

    #[test]
    fn a_doubled_brace_is_a_literal_one() {
        assert_eq!(
            fill("awk '{{print $1}}' {ref}").unwrap(),
            "awk '{print $1}' '/refs/HXB2.fasta'"
        );
    }

    #[test]
    fn an_unknown_placeholder_lists_what_the_step_does_have() {
        let error = fill("cat {refrence}").unwrap_err();
        assert!(error.contains("refrence"), "{error}");
        assert!(error.contains("min_freq"), "{error}");
        assert!(error.contains("ref"), "{error}");
    }

    #[test]
    fn an_unmatched_brace_says_how_to_write_a_literal_one() {
        assert!(fill("awk '{print $1}' {ref}").unwrap_err().contains("{{"));
        assert!(fill("cat {ref").unwrap_err().contains("{{"));
    }

    #[test]
    fn a_single_end_sample_asked_for_a_second_mate_says_so() {
        let single = vec![("reads".to_owned(), vec![PathBuf::from("/data/s.fq")])];
        let error =
            fill_template("cat {reads.r2}", &single, &[], Path::new("/tmp/out")).unwrap_err();
        assert!(error.contains("single-end"), "{error}");
    }

    #[test]
    fn a_value_with_a_space_in_it_stays_one_word() {
        let spaced = vec![("ref".to_owned(), vec![PathBuf::from("/my refs/HXB2.fa")])];
        assert_eq!(
            fill_template("cat {ref}", &spaced, &[], Path::new("/tmp/out")).unwrap(),
            "cat '/my refs/HXB2.fa'"
        );
    }

    #[test]
    fn a_quote_in_a_value_cannot_end_the_quoting() {
        let nasty = vec![("ref".to_owned(), vec![PathBuf::from("/refs/it's.fa")])];
        assert_eq!(
            fill_template("cat {ref}", &nasty, &[], Path::new("/tmp/out")).unwrap(),
            r"cat '/refs/it'\''s.fa'"
        );
    }

    #[test]
    fn a_callback_enters_the_key_as_itself_rather_than_as_its_number() {
        assert_eq!(
            Recipe::Callback(1).describe(),
            Recipe::Callback(2).describe()
        );
        assert_ne!(
            Recipe::Shell("a".into()).describe(),
            Recipe::Shell("b".into()).describe()
        );
    }
}
