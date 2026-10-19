//! Running a step twice, and saying whether it is reproducible.
//!
//! `CLAUDE.md` asks for an audit mode, and `docs/golden-example.md` gives
//! it a command: `ligase audit <step>`. The question it answers is not
//! "does this pipeline work" but "if I ran this again, would I get the same
//! answer" — which is the question a reviewer asks, and the one nothing
//! else here can answer, because a cache is in the business of *not*
//! running things twice.
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
    /// The two runs wrote the same bytes.
    Deterministic,
    /// The two runs wrote different bytes that mean the same thing.
    ///
    /// The ordinary verdict for a real tool, and not a problem: a `@PG`
    /// line or a timestamp moved, and no result did.
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
                }
                Verdict::NotReproducible => {
                    lines.push(format!(
                        "    the two runs mean different things   {} / {}",
                        short(step.first_canonical),
                        short(step.second_canonical)
                    ));
                }
            }
        }

        for failure in &self.failures {
            lines.push(format!("{:<16} `{}`", "failed", failure.step));
            for line in failure.message.lines() {
                lines.push(format!("    {line}"));
            }
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

    let verdict = if first == second {
        Verdict::Deterministic
    } else if first_canonical == second_canonical {
        Verdict::Reproducible
    } else {
        Verdict::NotReproducible
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
