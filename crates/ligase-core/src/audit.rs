//! Running a step twice, and saying whether it is reproducible.
//!
//! `CLAUDE.md` asks for an audit mode, and
//! `web/content/project/golden-example.mdx` gives it a command: `ligase audit
//! <step>`. The question it answers is not "does this pipeline work" but "if
//! I ran this again, would I get the same answer" — which is the question a
//! reviewer asks, and the one nothing else here can answer, because a cache
//! is in the business of *not* running things twice.
//!
//! # Why there are three verdicts and not two
//!
//! Byte equality is the wrong test, and the ordinary case proves it: every
//! tool in the reference pipeline records its own command line, and
//! `bcftools` writes the time of day beside it. Such a step is not
//! irreproducible — it computes the same calls every time — so reporting it
//! as a failure would make the audit useless on real tools, and reporting it
//! as a pass would hide the thing it exists to find.
//!
//! So the bytes and the meaning are compared separately
//! ([`crate::canon`]): [`Verdict::Deterministic`] when even the bytes
//! match, [`Verdict::Reproducible`] when only the meaning does, and
//! [`Verdict::NotReproducible`] when it does not.
//!
//! # Why a stochastic step is never deterministic
//!
//! A verdict here is a claim about the **step**, not about the pair of files
//! in front of it. The question is "if I ran this again, would I get the
//! same answer", and a step that declares itself stochastic (D40) has
//! already answered part of it: it draws on randomness, and the only reason
//! two runs of it agree is that its seed was held still.
//!
//! Such a step reaches byte equality whenever its seed really is what the
//! tool draws on — which is the *success* case, and the case where byte
//! equality alone would have reported [`Verdict::Deterministic`]. That word
//! would be false. A reader given it concludes there is no randomness in the
//! step; move the seed and the bytes move, so the conclusion is wrong, and
//! an engine stating something false about data is the one thing this
//! project exists not to do.
//!
//! So [`Verdict::Deterministic`] requires byte equality *and* a step that
//! declared nothing. A declaration adds no verdict; it removes one from
//! reach, and [`Verdict::Reproducible`] is where a stochastic step
//! saturates.
//!
//! This stays on the same scale rather than becoming its own type, which is
//! where it differs from [`RemoteVerdict`]: a remote's verdict is about
//! where bytes came from, a question beside this one, while "this step draws
//! on randomness" is a statement about whether a step repeats — the very
//! question a [`Verdict`] answers. A second enum would make a reader combine
//! two words to learn one fact, and would leave `Deterministic` free to go
//! on being wrong.
//!
//! Note that [`crate::prov::bundle::Match`] needs none of this and is
//! correct as it stands. `Match::Identical` compares one run against a
//! record — a question about two artefacts, to which "identical" is a true
//! answer however the step arrived at it. The two enums differ in subject,
//! not in strictness.
//!
//! What the saturation must not become is a relabel. The ordinary wording
//! for [`Verdict::Reproducible`] is "the bytes differ; what they mean does
//! not", and that is false of a byte-identical seeded step — so [`AuditReport::text`]
//! says what actually happened, and says the seed it happened under.
//!
//! # What an audit does to the cache
//!
//! Nothing. Its two runs write into scratch files and are thrown away:
//! recording either would put a result in the cache that nobody asked for,
//! from a step that has just been shown to wobble. The pipeline is run once
//! beforehand, normally and through the cache, so that the step being
//! audited has its inputs — that run is an ordinary run and records what an
//! ordinary run would.
//!
//! The audited nodes are taken in turn rather than at once. A duration is
//! part of what an audit reports, and two runs of an aligner fighting each
//! other for cores would produce timings that say more about the machine
//! than about the step.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::cache::Cache;
use crate::canon;
use crate::check::Scope;
use crate::exec::{self, Host, Options, StepFailure, Stores, Task, output_suffix};
use crate::graph::{Graph, NodeId};
use crate::hash::ContentHash;
use crate::prov::day;
use crate::types::{Dataset, Origin, RemoteVerdict};

/// Which nodes an audit is about.
///
/// A step's name and, where one written step has become one node per
/// sample, which of those to look at.
#[derive(Clone, Copy, Debug)]
pub struct Subject<'a> {
    /// The step's name, as written.
    pub step: &'a str,
    /// The one sample to narrow to, or `None` for every node of the step.
    pub sample: Option<&'a str>,
}

/// What two runs of a step showed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    /// The two runs wrote the same bytes, and the step declared no seed.
    ///
    /// Never the verdict for a step that declared itself stochastic,
    /// however its bytes fall — see the module note. Both halves are the
    /// condition, not just the first.
    Deterministic,
    /// The two runs agreed on the result.
    ///
    /// The ordinary verdict for a real tool, and not a problem: a `@PG`
    /// line or a timestamp moved, and no result did.
    ///
    /// Also where a step that declared itself stochastic saturates, whether
    /// its bytes moved or not. For such a step this is the *best* available
    /// verdict rather than a middling one — it says the seed held, which is
    /// the whole of what a seeded tool can promise.
    Reproducible,
    /// The two runs did not agree on the result.
    NotReproducible,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Deterministic => "deterministic",
            Self::Reproducible => "reproducible",
            Self::NotReproducible => "not reproducible",
        })
    }
}

/// What two runs of one step showed, in detail.
#[derive(Clone, Debug)]
pub struct StepAudit {
    /// The node audited.
    pub node: NodeId,
    /// The step's name.
    pub step: String,
    /// How to name this node, where its step's name is shared.
    pub scope: Option<String>,
    /// The verdict.
    pub verdict: Verdict,
    /// The first run's bytes.
    pub first: ContentHash,
    /// The second run's bytes.
    pub second: ContentHash,
    /// What the first run's output means.
    pub first_canonical: ContentHash,
    /// What the second run's output means.
    pub second_canonical: ContentHash,
    /// How long the first run took.
    pub first_duration: Duration,
    /// How long the second run took.
    pub second_duration: Duration,
    /// The seed this step declared, and the value it was held at.
    ///
    /// `None` for an ordinary step. Both halves, because the parameter's
    /// name alone is not what a reader needs: a verdict reached under
    /// `seed = 42` was reached under *that* seed, and reporting the name
    /// without the value would leave the one number that makes the run
    /// repeatable off the report.
    pub seed: Option<(String, String)>,
}

impl StepAudit {
    /// How this step reads in a report: `` `align` (sample SRR8525904) ``.
    #[must_use]
    pub fn describe(&self) -> String {
        match &self.scope {
            Some(scope) => format!("`{}` ({scope})", self.step),
            None => format!("`{}`", self.step),
        }
    }
}

/// What an audit found.
#[derive(Clone, Debug, Default)]
pub struct AuditReport {
    /// Every node audited, in graph order.
    pub audited: Vec<StepAudit>,
    /// Every node that could not be audited because it failed.
    pub failures: Vec<StepFailure>,
    /// Every remote the audited steps rest on, with no duplicates.
    ///
    /// Reported beside the step verdicts rather than folded into
    /// [`Self::verdict`], because the two are not on one scale: a remote's
    /// verdict says who else could obtain this input, and a step's says
    /// whether the same input twice gives the same answer. A pipeline whose
    /// every step is deterministic can still rest on a query nobody else
    /// can repeat, and that is worth reading rather than averaging away.
    pub remotes: Vec<Origin>,
}

impl AuditReport {
    /// The worst verdict any node got.
    ///
    /// The worst rather than the commonest: one irreproducible sample in a
    /// cohort is an irreproducible step, and a summary that averaged them
    /// would bury exactly what a user is auditing for.
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        self.audited
            .iter()
            .map(|step| step.verdict)
            .max()
            .unwrap_or(Verdict::Deterministic)
    }

    /// One line summarising the audit.
    #[must_use]
    pub fn summary(&self) -> String {
        let count = |wanted: Verdict| {
            self.audited
                .iter()
                .filter(|step| step.verdict == wanted)
                .count()
        };
        let mut parts = Vec::new();
        for verdict in [
            Verdict::Deterministic,
            Verdict::Reproducible,
            Verdict::NotReproducible,
        ] {
            let found = count(verdict);
            if found > 0 {
                parts.push(format!("{found} {verdict}"));
            }
        }
        if !self.failures.is_empty() {
            parts.push(format!("{} failed", self.failures.len()));
        }

        let places = self.audited.len() + self.failures.len();
        let noun = if places == 1 { "step" } else { "steps" };
        if parts.is_empty() {
            return format!("{places} {noun}.");
        }
        format!("{places} {noun}: {}.", parts.join(", "))
    }

    /// The whole report, as the command line prints it.
    ///
    /// Worded here so that `ligase audit` and a script read the same, the
    /// way a check report and a run report already are.
    #[must_use]
    pub fn text(&self) -> String {
        let mut lines = Vec::new();

        for step in &self.audited {
            lines.push(format!(
                "{:<16} {}",
                step.verdict.to_string(),
                step.describe()
            ));
            match step.verdict {
                Verdict::Deterministic => {
                    lines.push(format!(
                        "    the same bytes both times   {}",
                        short(step.first)
                    ));
                    // And the canonical digest, which is the one a claim
                    // cites. Without it these two commands named different
                    // digests of the same result and a reader given one
                    // could not reconcile it with the other -- which is the
                    // irreconcilability D59 set out to remove, in a new
                    // form. The bytes digest stays first: it is the
                    // artifact's name in the store.
                    lines.push(format!(
                        "    canonical                   {}",
                        short(step.first_canonical)
                    ));
                }
                // A seeded step whose bytes matched. Saying "the bytes
                // differ" of it would be false, and leaving it at "the same
                // bytes both times" would read as the line above — so the
                // report says what the equality rests on, which is the one
                // thing a reader of `reproducible` here needs to know.
                Verdict::Reproducible if step.first == step.second => {
                    lines.push(format!(
                        "    the same bytes both times   {}",
                        short(step.first)
                    ));
                    lines.push(format!(
                        "    canonical                   {}",
                        short(step.first_canonical)
                    ));
                    if let Some((name, value)) = &step.seed {
                        lines.push(format!(
                            "    because `{name}` was held at {value}, not because the step \
                             cannot wobble"
                        ));
                    }
                }
                Verdict::Reproducible => {
                    lines.push(format!(
                        "    the bytes differ            {} / {}",
                        short(step.first),
                        short(step.second)
                    ));
                    lines.push(format!(
                        "    what they mean does not     {}",
                        short(step.first_canonical)
                    ));
                    if let Some((name, value)) = &step.seed {
                        lines.push(format!("    seeded by `{name}` = {value}"));
                    }
                }
                Verdict::NotReproducible => {
                    lines.push(format!(
                        "    the two runs mean different things   {} / {}",
                        short(step.first_canonical),
                        short(step.second_canonical)
                    ));
                    // The most useful line in the milestone. A step that
                    // names a seed and still disagrees with itself has a
                    // declaration that is false — the parameter is not what
                    // the tool draws on — and filing that under ordinary
                    // irreproducibility would waste the one thing the
                    // declaration bought.
                    if let Some((name, value)) = &step.seed {
                        lines.push(format!(
                            "    `{name}` was held at {value} and the result moved anyway: the \
                             seed does not control this tool"
                        ));
                    }
                }
            }
        }

        for failure in &self.failures {
            lines.push(format!("{:<16} `{}`", "failed", failure.step));
            for line in failure.message.lines() {
                lines.push(format!("    {line}"));
            }
        }

        for remote in &self.remotes {
            lines.push(format!(
                "{:<16} `{}`",
                remote.verdict.to_string(),
                remote.locator
            ));
            // What the date *means* differs by verdict, so the word in
            // front of it does too. A replayed query was not fetched on
            // this run, and saying "fetched" of it would be the one thing
            // this report exists not to do.
            let what = match remote.verdict {
                RemoteVerdict::Pinned => "fetched",
                RemoteVerdict::Replayed => "from record of",
                RemoteVerdict::Live => "answered",
            };
            lines.push(format!(
                "    {what} {}            {}",
                day(remote.fetched_at),
                short(remote.digest)
            ));
        }

        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(self.summary());
        lines.join("\n")
    }
}

/// Run one step twice and report whether it is reproducible.
///
/// The pipeline is run first, normally, so that the audited step has its
/// inputs; then each node of `step` is run twice more, into scratch files,
/// and the two outputs are compared both as bytes and as meaning.
///
/// `sample` narrows the audit to the one node a report would label with it.
///
/// # Errors
///
/// If no step of that name is in the pipeline, or no node of it is for that
/// sample. Both are a mistake in the command rather than a finding about
/// the pipeline, so neither comes back as a verdict.
pub fn audit(
    graph: &Graph,
    roots: &[NodeId],
    tasks: &[Task],
    stores: &Stores<'_>,
    options: &Options,
    host: &dyn Host,
    subject: &Subject<'_>,
) -> Result<AuditReport, String> {
    let step = subject.step;
    let scopes = graph.scopes(roots);
    let mut chosen: Vec<NodeId> = graph
        .reachable_nodes(roots)
        .into_iter()
        .filter(|node| graph.step(*node).is_some_and(|spec| spec.name == step))
        .collect();

    if chosen.is_empty() {
        return Err(unknown_step(graph, roots, step));
    }

    if let Some(wanted) = subject.sample {
        let every = chosen.clone();
        chosen.retain(|node| scopes.get(node).is_some_and(|scope| scope.value == wanted));
        if chosen.is_empty() {
            return Err(unknown_sample(&scopes, &every, step, wanted));
        }
    }

    // An ordinary run, so that the audited steps have their inputs. It uses
    // the cache and records what it computes, because it *is* an ordinary
    // run; only the two audit runs below are set aside.
    let run = exec::run(graph, roots, tasks, stores, options, host);
    let produced: Vec<(NodeId, PathBuf)> = run
        .results
        .iter()
        .map(|result| (result.node, result.output.clone()))
        .collect();

    let mut report = AuditReport::default();

    // Every remote the audited steps rest on, de-duplicated by locator: a
    // reference fetched once and handed to three steps is one fetch.
    for node in graph.reachable_nodes(roots) {
        if let Some(origin) = graph.dataset(node).and_then(Dataset::origin)
            && !report
                .remotes
                .iter()
                .any(|held| held.locator == origin.locator)
        {
            report.remotes.push(origin.clone());
        }
    }

    for node in chosen {
        let name = graph
            .step(node)
            .map_or_else(|| step.to_owned(), |spec| spec.name.clone());
        let scope = scopes.get(&node).map(Scope::describe);

        // A step that could not run in the preliminary run has nothing to
        // audit, and the run has already said why.
        if let Some(failure) = run.failures.iter().find(|failure| failure.node == node) {
            report.failures.push(failure.clone());
            continue;
        }

        match twice(graph, node, tasks, stores.cache, host, &produced) {
            Ok(mut audited) => {
                audited.step = name;
                audited.scope = scope;
                report.audited.push(audited);
            }
            Err(message) => report.failures.push(StepFailure {
                node,
                step: name,
                message,
            }),
        }
    }

    report.audited.sort_by_key(|step| step.node.index());
    report.failures.sort_by_key(|failure| failure.node.index());
    Ok(report)
}

/// Run one node twice into scratch files and compare what came out.
fn twice(
    graph: &Graph,
    node: NodeId,
    tasks: &[Task],
    cache: &Cache,
    host: &dyn Host,
    produced: &[(NodeId, PathBuf)],
) -> Result<StepAudit, String> {
    let spec = graph
        .step(node)
        .ok_or_else(|| "this node is not a step".to_owned())?;
    let task = tasks
        .iter()
        .find(|task| task.node == node)
        .ok_or_else(|| "no recipe was recorded for this step".to_owned())?;

    let inputs = exec::inputs_of(graph, node, &|from| {
        produced
            .iter()
            .find(|(at, _)| *at == from)
            .map(|(_, path)| path.clone())
    })?;

    let suffix = output_suffix(spec.output);
    let mut runs = Vec::with_capacity(2);
    for attempt in 1..=2 {
        let out = cache.scratch(&format!("audit-{}-{attempt}{suffix}", node.index()));
        let _ = std::fs::remove_file(&out);

        let outcome = exec::produce(&task.recipe, &inputs, &task.parameters, &out, host).and_then(
            |produced| {
                let bytes = ContentHash::of_file(&out).map_err(|error| error.to_string())?;
                let meaning = canon::hash(spec.output, &out, bytes)
                    .map_err(|error| error.to_string())?
                    .hash;
                Ok((bytes, meaning, produced.duration))
            },
        );

        // Scratch files go whatever happened: an audit leaves no trace.
        let _ = std::fs::remove_file(&out);
        runs.push(outcome?);
    }

    let (first, first_canonical, first_duration) = runs[0];
    let (second, second_canonical, second_duration) = runs[1];

    // The seed this step declared, with the value it was held at. Both are
    // on the task: the name because the step declared it, the value because
    // a seed is an ordinary parameter besides.
    let seed = task.seed.as_ref().and_then(|parameter| {
        task.parameters
            .iter()
            .find(|(name, _)| name == parameter)
            .map(|(name, value)| (name.clone(), value.clone()))
    });

    let verdict = if first_canonical != second_canonical {
        Verdict::NotReproducible
    } else if first == second && task.seed.is_none() {
        // Both halves. A step that declared a seed is byte-identical
        // *because the seed was held still*, and calling that deterministic
        // would tell a reader there is no randomness in it — see the module
        // note. Do not simplify this to byte equality.
        //
        // The test is `task.seed`, the declaration, and not `seed`, the
        // pair found for it above. They agree for anything the decorator
        // built, since it refuses a seed naming no parameter — but a
        // verdict is the wrong place to depend on that. If the two ever
        // part company, a declared step must come out `Reproducible` with
        // no value to show, never `Deterministic`.
        Verdict::Deterministic
    } else {
        Verdict::Reproducible
    };

    Ok(StepAudit {
        node,
        step: spec.name.clone(),
        scope: None,
        verdict,
        first,
        second,
        first_canonical,
        second_canonical,
        first_duration,
        second_duration,
        seed,
    })
}

/// A step nothing is named after, with the names that are.
fn unknown_step(graph: &Graph, roots: &[NodeId], step: &str) -> String {
    let mut names: Vec<&str> = graph.step_names(roots);
    names.sort_unstable();
    names.dedup();
    format!(
        "no step named `{step}` in this pipeline; it has {}",
        names
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// A sample no node of this step is for, with the ones that are.
///
/// Only the nodes of *this* step are listed. The samples of some other step
/// are not an answer to "which ones are there?" here, and offering them
/// would send a user to try one that cannot work either.
fn unknown_sample(
    scopes: &HashMap<NodeId, Scope>,
    nodes: &[NodeId],
    step: &str,
    sample: &str,
) -> String {
    let mut values: Vec<&str> = nodes
        .iter()
        .filter_map(|node| scopes.get(node))
        .map(|scope| scope.value.as_str())
        .collect();
    values.sort_unstable();
    values.dedup();

    if values.is_empty() {
        return format!(
            "no node of `{step}` is for `{sample}`; this pipeline has only one `{step}`, so \
             there is nothing to narrow"
        );
    }
    format!(
        "no node of `{step}` is for `{sample}`; there are {}",
        values.join(", ")
    )
}

/// A hash, short enough to read and long enough to tell apart.
fn short(hash: ContentHash) -> String {
    hash.to_hex()[..12].to_owned()
}
