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
//! # What is deliberately not here
//!
//! Provenance (M5). Nothing here records *why* a result is what it is.
//! Running a step twice to find out whether it is reproducible at all is
//! [`crate::audit`], which borrows this module's [`produce`] rather than
//! repeating it.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::cache::{Cache, Stored};
use crate::graph::{Graph, NodeId, StepSpec};
use crate::hash::ContentHash;
use crate::key::{Ingredients, StepKey};
use crate::types::{DataKind, Dataset};

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
    /// # Errors
    ///
    /// Whatever the callback raised, as a message for the user.
    fn call(&self, id: u64, out: &Path, inputs: &[(String, Vec<PathBuf>)]) -> Result<(), String>;
}

/// A host that has no callbacks, for a pipeline of shell steps.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoCallbacks;

impl Host for NoCallbacks {
    fn call(&self, id: u64, _out: &Path, _inputs: &[(String, Vec<PathBuf>)]) -> Result<(), String> {
        Err(format!(
            "this step's body is a callback ({id}), and nothing here can run one"
        ))
    }
}

/// The version an external tool reports.
///
/// The whole of `<tool> --version`, because the parts a tool prints below
/// the first line — the htslib a samtools was built against, say — change
/// results too.
///
/// # Errors
///
/// If the tool is not installed or prints nothing. Hashing an absent tool
/// as "no version" would let two machines with different tools agree on a
/// key, which is exactly the failure this is here to prevent.
pub fn tool_version(tool: &str) -> Result<String, String> {
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
    cache: &Cache,
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
        cache,
        host,
        force: options.force,
        tasks: tasks.iter().map(|task| (task.node, task)).collect(),
        versions: versions(graph, &nodes, tasks),
        state: Mutex::new(State::new(graph, &nodes)),
        woken: Condvar::new(),
    };

    let jobs = options.jobs.max(1).min(nodes.len());
    match rayon::ThreadPoolBuilder::new().num_threads(jobs).build() {
        Ok(pool) => pool.scope(|scope| {
            for _ in 0..jobs {
                scope.spawn(|_| runner.work());
            }
        }),
        // A pool we cannot build is not a reason to refuse to run.
        Err(_) => runner.work(),
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
) -> Result<Duration, String> {
    let started = Instant::now();
    let produced = match recipe {
        Recipe::Shell(template) => {
            let command = fill_template(template, inputs, parameters, out)?;
            shell(&command)
        }
        Recipe::Callback(id) => host.call(*id, out, inputs),
    };
    let duration = started.elapsed();

    produced?;
    if !out.is_file() {
        return Err(format!(
            "the step finished without writing its output to {}",
            out.display()
        ));
    }
    Ok(duration)
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
    ready: Vec<NodeId>,
    remaining: HashMap<NodeId, usize>,
    dependents: HashMap<NodeId, Vec<NodeId>>,
    produced: HashMap<NodeId, Produced>,
    results: Vec<StepResult>,
    failures: Vec<StepFailure>,
    running: HashSet<StepKey>,
    active: usize,
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
            active: 0,
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
    host: &'a dyn Host,
    force: bool,
    tasks: HashMap<NodeId, &'a Task>,
    versions: HashMap<String, Result<String, String>>,
    state: Mutex<State>,
    woken: Condvar,
}

impl Runner<'_> {
    /// Take ready steps until there are none left and nobody is still busy.
    fn work(&self) {
        while let Some(node) = self.take() {
            let done = self.execute(node);

            let Ok(mut state) = self.state.lock() else {
                return;
            };
            state.active -= 1;
            match done {
                Ok(result) => {
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
                Err(failure) => state.failures.push(failure),
            }
            drop(state);
            self.woken.notify_all();
        }
    }

    /// The next ready step, or `None` once the run is over.
    fn take(&self) -> Option<NodeId> {
        let mut state = self.state.lock().ok()?;
        loop {
            if let Some(node) = state.ready.pop() {
                state.active += 1;
                return Some(node);
            }
            if state.active == 0 {
                // Nothing to do and nobody who could produce more.
                self.woken.notify_all();
                return None;
            }
            state = self.woken.wait(state).ok()?;
        }
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

    fn execute(&self, node: NodeId) -> Result<StepResult, StepFailure> {
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

    fn attempt(&self, node: NodeId) -> Result<StepResult, String> {
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
            return self.cached_result(node, spec, key, &entry.output);
        }

        // Two nodes can share a key -- the same step over identical data --
        // and running both would be the same work twice. The second waits.
        let _claim = self.claim(key)?;
        if !self.force
            && let Some(entry) = self.cache.lookup(key).map_err(|error| error.to_string())?
        {
            return self.cached_result(node, spec, key, &entry.output);
        }

        let staging = self.cache.staging(key, output_suffix(spec.output));
        let _ = std::fs::remove_file(&staging);

        let duration = produce(&task.recipe, &inputs, &task.parameters, &staging, self.host)
            .inspect_err(|_| {
                let _ = std::fs::remove_file(&staging);
            })?;

        let stored = self
            .cache
            .store(&staging)
            .map_err(|error| error.to_string())?;
        let milliseconds = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        self.cache
            .record(
                key,
                &spec.name,
                &stored,
                &self.tool_summary(task),
                milliseconds,
            )
            .map_err(|error| error.to_string())?;

        let canonical = self
            .cache
            .semantic_hash(spec.output, &stored.path)
            .map_err(|error| error.to_string())?;

        Ok(StepResult {
            node,
            step: spec.name.clone(),
            outcome: Outcome::Ran,
            key,
            hash: stored.hash,
            canonical,
            output: stored.path,
            duration,
        })
    }

    /// Hold this key for the length of the step, so a twin waits.
    fn claim(&self, key: StepKey) -> Result<Claim<'_>, String> {
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        while state.running.contains(&key) {
            // Another worker is producing exactly this; wait for it rather
            // than race it. It cannot be waiting on us -- it is running, not
            // waiting -- so this always ends. The worker stays counted as
            // active throughout, or the run would look finished while it is
            // not.
            state = self.woken.wait(state).map_err(|_| poisoned())?;
        }
        state.running.insert(key);
        Ok(Claim { runner: self, key })
    }

    fn cached_result(
        &self,
        node: NodeId,
        spec: &StepSpec,
        key: StepKey,
        output: &Stored,
    ) -> Result<StepResult, String> {
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
        let canonical = self
            .cache
            .semantic_hash(spec.output, &output.path)
            .map_err(|error| error.to_string())?;

        Ok(StepResult {
            node,
            step: spec.name.clone(),
            outcome,
            key,
            hash: output.hash,
            canonical,
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

/// A key held for the length of one step.
struct Claim<'a> {
    runner: &'a Runner<'a>,
    key: StepKey,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.runner.state.lock() {
            state.running.remove(&self.key);
        }
        self.runner.woken.notify_all();
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
