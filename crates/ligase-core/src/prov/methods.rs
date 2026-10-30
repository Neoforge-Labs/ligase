//! Turning provenance records into the paragraph a paper needs.
//!
//! `web/content/project/milestones.mdx` says what M5 is signed off on: a
//! methods paragraph with tool versions, parameters, reference accession and
//! citations. Each of those four is already recorded by the time this runs
//! ([`super::Record`]); this module's whole job is to say them in order, in
//! sentences, without adding anything that was not recorded.
//!
//! # Why the prose is generated rather than written
//!
//! Every sentence here follows a fixed form. That is a deliberate limit: a
//! generator that tried to write fluently would have to decide what to
//! leave out, and the one thing a methods section must not do is leave
//! something out. A reader of these paragraphs will have to tidy the
//! wording. They will not have to check whether a version is real.
//!
//! # Why steps are grouped
//!
//! One written step becomes one node per sample, and a paragraph that said
//! the same sentence four times with a different accession in it would be
//! unreadable. So nodes are grouped by everything that could make their
//! sentences differ — the step, its parameters, its tools and its command —
//! and each group gets one sentence naming how many nodes it covers. Where
//! two nodes of a step genuinely were run differently, they land in
//! different groups and get a sentence each, because then the difference is
//! the thing worth reporting.
//!
//! # What a gap is
//!
//! A step whose output is in the cache from before provenance was kept has
//! no record, and nothing can reconstruct one: the command that produced it
//! is not in the hash (D22) and not in the object. Such a step is reported
//! as a gap, by name, with the way to fill it. The alternative — printing a
//! paragraph that silently omits a step — is the failure this whole
//! milestone exists to prevent.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::claim::{self, Claim, ClaimCheck, ClaimVerdict, Located, Restated, Unbacked};
use super::{Prov, ProvError, Record, ReferenceRecord, cite, day};
use crate::canon::Note;
use crate::exec::{RunReport, Task};
use crate::graph::{Graph, NodeId};
use crate::hash::ContentHash;
use crate::key::StepKey;
use crate::types::{Origin, RemoteVerdict};

/// Stands in for a space that must not be broken across lines.
///
/// `min_freq = 0.01` is one thing to read, and a wrap between the `=` and
/// the value makes a reader hunt for it. It is not whitespace, so the
/// wrapper treats such a phrase as one word, and it becomes a space again
/// on the way out.
const KNIT: char = '\u{1}';

/// How wide the generated prose is wrapped.
///
/// Narrow enough to paste into a manuscript or a terminal without
/// reflowing.
const WIDTH: usize = 76;

/// One or more nodes that were run the same way, and how they were run.
#[derive(Clone, Debug)]
pub struct Group {
    /// The step's name, as written.
    pub step: String,
    /// The nodes this covers, in graph order.
    pub nodes: Vec<NodeId>,
    /// What tells those nodes apart, e.g. the sample identifiers.
    pub scopes: Vec<String>,
    /// What kind of thing that is, e.g. `sample`.
    ///
    /// Taken from the graph rather than assumed: under per-sample fan-out
    /// it is the sample, but two steps of one name over different files of
    /// the same sample are told apart by the file, and calling those
    /// samples would send a reader to the wrong data (D15).
    pub scope_kind: Option<&'static str>,
    /// How one of them was produced. They were produced the same way.
    pub record: Record,
    /// The command as written in the pipeline, placeholders and all.
    ///
    /// Taken from the step rather than from the record, because it is the
    /// readable form: a filled command names a staging file inside the
    /// cache, which is true and of no use in a paper. The filled one is in
    /// the record, and the bundle prints it.
    pub template: Option<String>,
    /// References the step declared for itself.
    pub declared: Vec<String>,
    /// The parameter this step declared as its seed, for a stochastic step.
    ///
    /// Taken from the task rather than from the record, for the reason
    /// [`crate::exec::Task::citations`] gives: a record that stored the
    /// declaration would go stale the moment the decorator line was edited,
    /// and a methods paragraph takes it from the pipeline in front of it.
    /// The seed's *value* does come from the record, because that is what
    /// the step was actually run with.
    pub seed: Option<String>,
    /// Parameters whose value is just the sample's own identifier.
    ///
    /// A step may have to be *told* which sample it is working on — the
    /// reference pipeline passes one so that `samtools addreplacerg` can
    /// write a read group. Such a parameter is not a difference in how the
    /// step was run, so it does not split a group, and printing
    /// `sample_id = 'SRR8525904'` beside a sentence that already names four
    /// samples would say nothing and imply the sentence was about one.
    pub per_sample: Vec<String>,
}

/// A step that cannot be accounted for.
#[derive(Clone, Debug)]
pub struct Gap {
    /// The step's name.
    pub step: String,
    /// What tells its nodes apart.
    pub scopes: Vec<String>,
    /// What kind of thing that is, e.g. `sample`.
    pub scope_kind: Option<&'static str>,
    /// How many nodes are unaccounted for.
    pub nodes: usize,
}

/// Something noticed about one step's output.
///
/// Kept beside the groups rather than inside one, because a group is a set
/// of nodes *run the same way* and a note is about one node's output: four
/// samples run identically can be named after four different files.
#[derive(Clone, Debug)]
pub struct Noticed {
    /// The step's name.
    pub step: String,
    /// What tells this node from its siblings, e.g. the sample identifier.
    pub scope: Option<String>,
    /// What kind of thing that is, e.g. `sample`.
    pub scope_kind: Option<&'static str>,
    /// What was noticed.
    pub note: Note,
}

/// One result, and the computation that produced it.
///
/// Not a duplicate of [`Group`]: a group is a set of nodes *run the same
/// way* and keeps one representative [`Record`], discarding the other nodes'
/// keys. A claim binds to a computation (D45), so the keys are what it needs,
/// and this is where they are kept.
#[derive(Clone, Debug)]
pub struct Produced {
    /// The step's name, as written.
    pub step: String,
    /// What tells this node from its siblings, e.g. the sample identifier.
    pub scope: Option<String>,
    /// The step key: the identity of the computation.
    pub key: StepKey,
    /// The canonical hash of what it produced.
    pub canonical: ContentHash,
    /// Where that output is, in the content-addressed store.
    ///
    /// Carried so that a claim naming a place in the output can read it
    /// (D61). It is the only thing in a write-up that depends on an
    /// artifact still being on disk, which is why nothing else uses it:
    /// everything above is from the record.
    pub output: PathBuf,
    /// When the computation happened, which is not when this run was.
    pub recorded_at: i64,
    /// The platform it happened on.
    pub platform: String,
}

/// Everything needed to write a methods section.
#[derive(Clone, Debug, Default)]
pub struct Methods {
    /// Each distinct way a step was run, in graph order.
    pub groups: Vec<Group>,
    /// Steps nothing is known about.
    pub gaps: Vec<Gap>,
    /// What canonicalising the outputs noticed, in graph order (D26).
    pub noticed: Vec<Noticed>,
    /// Every result this write-up accounts for, in graph order.
    pub produced: Vec<Produced>,
    /// Each claim the pipeline declared, and what became of it (D45).
    ///
    /// Empty until [`Methods::bind`] is called, and empty afterwards for a
    /// pipeline that declared none — which is why nothing about an existing
    /// write-up changes.
    pub claims: Vec<ClaimCheck>,
    /// Every version of Ligase that produced any of the results, sorted.
    ///
    /// One entry for every run on one machine, which is why this was a
    /// `String` until M18. A distributed run is the first time it is not:
    /// a worker on an older wheel is a different engine, and a methods
    /// section that named one of two would be making a claim nobody
    /// checked (D55).
    pub engines: Vec<String>,
    /// Every platform any of the results were produced on, sorted.
    ///
    /// As [`engines`](Self::engines), and for the commoner reason: a mixed
    /// cluster, or an ARM workstation beside x86 nodes. A record has
    /// always carried its own platform; before M18 they were all the same
    /// because one machine ran everything.
    pub platforms: Vec<String>,
}

impl Methods {
    /// Check each of these claims against this write-up (D45).
    ///
    /// An operation over the finished write-up rather than an argument to
    /// [`methods`], because every check a claim needs is against this value:
    /// the steps it accounts for, the samples they ran on, the parameters and
    /// tools they recorded, and the keys in [`Self::produced`]. Nothing about
    /// building a methods section depends on the claims, so nothing about
    /// building one changes.
    ///
    /// Calling it replaces whatever was bound before, so binding an empty
    /// list is how a pipeline says it declares none.
    pub fn bind(&mut self, claims: &[Claim]) {
        self.claims = claims
            .iter()
            .map(|claim| claim::check(self, claim))
            .collect();
    }

    /// The claims this write-up cannot stand behind.
    ///
    /// Contradicted ones included: a number the run positively disagrees
    /// with is not a number the write-up stands behind.
    #[must_use]
    pub fn unbacked(&self) -> Vec<&ClaimCheck> {
        self.claims
            .iter()
            .filter(|check| !check.verdict.is_bound())
            .collect()
    }

    /// Every tool named by any step, with the version it ran at.
    #[must_use]
    pub fn tools(&self) -> Vec<(String, String)> {
        let mut seen: Vec<(String, String)> = Vec::new();
        for group in &self.groups {
            for (tool, version) in &group.record.tools {
                let named = (tool.clone(), version.clone());
                if !seen.contains(&named) {
                    seen.push(named);
                }
            }
        }
        seen.sort();
        seen
    }

    /// Every reference any step was given, with no duplicates.
    #[must_use]
    pub fn references(&self) -> Vec<ReferenceRecord> {
        let mut seen: Vec<ReferenceRecord> = Vec::new();
        for group in &self.groups {
            for reference in &group.record.references {
                if !seen
                    .iter()
                    .any(|held| held.fingerprint == reference.fingerprint)
                {
                    seen.push(reference.clone());
                }
            }
        }
        seen
    }

    /// Every remote any step was given, with no duplicates, in the order
    /// they were first seen.
    ///
    /// De-duplicated by locator: a reference fetched once and handed to
    /// three steps is one fetch, and a paragraph that said so three times
    /// would be describing the pipeline rather than the work.
    #[must_use]
    pub fn remotes(&self) -> Vec<Origin> {
        let mut seen: Vec<Origin> = Vec::new();
        for group in &self.groups {
            for remote in &group.record.remotes {
                if !seen.iter().any(|held| held.locator == remote.locator) {
                    seen.push(remote.clone());
                }
            }
        }
        seen
    }

    /// The sentence a methods section wants about what was fetched.
    ///
    /// `None` where nothing was. The wording says *what*, *from where* and
    /// *when*, because for a remote input those are the three things a
    /// reader cannot work out from anything else — and the digest, because
    /// a date alone does not identify bytes.
    #[must_use]
    pub fn fetched_sentence(&self) -> Option<String> {
        let remotes = self.remotes();
        if remotes.is_empty() {
            return None;
        }

        // Two claims, kept apart, because conflating them is what D39
        // exists to prevent. A pinned remote is reproducible from the
        // *source*: anyone can obtain it and check it against the digest.
        // A recorded query is reproducible from the *record*, because its
        // source promises nothing -- so what a reader needs told is that
        // the responses travel in the bundle.
        let (pinned, recorded): (Vec<&Origin>, Vec<&Origin>) = remotes
            .iter()
            .partition(|remote| remote.verdict == RemoteVerdict::Pinned);

        let mut sentences = Vec::new();
        if !pinned.is_empty() {
            sentences.push(format!(
                "Input data was retrieved from {}. Each is pinned by the \
                 digest of its canonical form, so a rerun anywhere verifies \
                 the same bytes before any step runs.",
                join_names(&described(&pinned))
            ));
        }
        if !recorded.is_empty() {
            sentences.push(format!(
                "Responses were recorded from {}. A live source promises \
                 nothing about what it will answer next, so the responses \
                 themselves are included in the reproducibility bundle and \
                 the pipeline replays from them with no network access.",
                join_names(&described(&recorded))
            ));
        }
        Some(sentences.join(" "))
    }

    /// The reference list, and the tools that have no reference.
    ///
    /// The second half is the point: a tool with nothing to cite is named,
    /// so a short reference list cannot be mistaken for a complete one
    /// (D24).
    ///
    /// The engine itself is always in the first half and never in the
    /// second. It ran — the paragraph's opening sentence says so, with its
    /// version — so a reference list that left it out would be missing one
    /// of the pieces of software the sentence above it credits. And naming
    /// it as uncited was worse than an omission: `ligase` is the tool of the
    /// built-in conversions, which take no decorator, so the gap came with
    /// advice that pointed nowhere (D58).
    #[must_use]
    pub fn citations(&self) -> (Vec<String>, Vec<String>) {
        let mut cited: BTreeSet<String> = BTreeSet::new();
        let mut uncited: BTreeSet<String> = BTreeSet::new();

        cited.insert(cite::engine(&self.engines));

        for group in &self.groups {
            for text in &group.declared {
                // A consumer who added a Ligase reference by hand -- which
                // 0.3.0's own message told them to do -- is citing the same
                // work as the self-citation above. One work, one entry, and
                // the engine's own is the one kept: it names the build that
                // ran rather than whichever version the string was written
                // against (D63).
                if cite::is_engine_reference(text) {
                    continue;
                }
                cited.insert(text.clone());
            }
            for (tool, _) in &group.record.tools {
                // Already cited above, for the whole run rather than per
                // step: two steps running the engine do not make two
                // references, and a step that declared its own does not
                // make the engine's go away.
                if tool == cite::ENGINE {
                    continue;
                }
                match cite::of(tool) {
                    Some(citation) => {
                        cited.insert(citation.render());
                    }
                    // A step that declared its own references has said what
                    // it wanted cited, so its tools are not gaps.
                    None if group.declared.is_empty() => {
                        uncited.insert(tool.clone());
                    }
                    None => {}
                }
            }
        }

        (cited.into_iter().collect(), uncited.into_iter().collect())
    }

    /// The methods paragraph, wrapped.
    #[must_use]
    pub fn paragraph(&self) -> String {
        let mut paragraphs = vec![wrap(&format!(
            "Analyses were run with Ligase {} on {}.",
            english(&self.engines),
            english(&self.platforms)
        ))];

        if let Some(fetched) = self.fetched_sentence() {
            paragraphs.push(wrap(&fetched));
        }

        for group in &self.groups {
            paragraphs.push(wrap(&group.sentence()));
        }

        for gap in &self.gaps {
            paragraphs.push(wrap(&format!(
                "`{}` ({}) is in the cache from a run that recorded no \
                 provenance, so how it was produced is not known here. \
                 `ligase run --force` runs it again and records it.",
                gap.step,
                counted(gap.nodes, &gap.scopes, gap.scope_kind),
            )));
        }

        paragraphs.join("\n\n")
    }

    /// The whole thing, as the command line prints it.
    ///
    /// Worded here rather than in Python so that `ligase methods`, a script
    /// calling `pipe.methods()` and the bundle all read the same, the way
    /// the check, run and audit reports already do.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = self.paragraph();
        let (cited, uncited) = self.citations();

        if !cited.is_empty() {
            out.push_str("\n\nReferences\n\n");
            out.push_str(
                &cited
                    .iter()
                    .map(|reference| wrap_hanging(reference))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }

        if !uncited.is_empty() {
            out.push_str("\n\n");
            out.push_str(&wrap(&format!(
                "No reference is recorded for {}. Add one with \
                 `@lg.step(cite=...)`, or cite it by hand.",
                join_names(&uncited)
            )));
        }

        // The claims, after the reference list and the uncited admission,
        // because all three are remarks about the write-up rather than
        // sentences about the science -- and because an unbacked claim is
        // the same shape of complaint as an uncited tool (D24).
        //
        // Nothing at all for a pipeline that declared no claims: a heading
        // over an empty section is a promise nobody made.
        if !self.claims.is_empty() {
            out.push_str("\n\nClaims\n\n");
            out.push_str(
                &self
                    .claims
                    .iter()
                    .map(|check| wrap_hanging(&claim_sentence(check)))
                    .collect::<Vec<_>>()
                    .join("\n\n"),
            );
        }

        // Last, and outside the paragraph a reader would paste into a
        // manuscript: this is a remark about the pipeline rather than a
        // sentence about the science.
        for notice in self.notices() {
            out.push_str("\n\n");
            out.push_str(&notice);
        }

        out
    }

    /// What was noticed about the outputs, as paragraphs (D26).
    ///
    /// Gathered by step and by kind of note, with the identifiers **named**
    /// rather than counted, for the reason the check and run reports name
    /// their samples (D15): a reader who has to go and look up which
    /// outputs are affected has a shorter report rather than a better one.
    #[must_use]
    pub fn notices(&self) -> Vec<String> {
        let mut gathered: Vec<Vec<&Noticed>> = Vec::new();
        for item in &self.noticed {
            match gathered.iter_mut().find(|seen| {
                seen.first().is_some_and(|first| {
                    first.step == item.step && first.note.code() == item.note.code()
                })
            }) {
                Some(seen) => seen.push(item),
                None => gathered.push(vec![item]),
            }
        }

        gathered
            .iter()
            .filter_map(|seen| {
                let (first, rest) = seen.split_first()?;
                let mut scopes: Vec<String> =
                    seen.iter().filter_map(|item| item.scope.clone()).collect();
                scopes.sort();
                scopes.dedup();

                let mut sentence = format!(
                    "`{}` ran on {}, where {}",
                    first.step,
                    counted(seen.len(), &scopes, first.scope_kind),
                    first.note,
                );
                if !rest.is_empty() {
                    let others: Vec<String> = rest
                        .iter()
                        .map(|item| format!("`{}`", item.note.value()))
                        .collect();
                    sentence.push_str(&format!(" — and the same for {}", join_names(&others)));
                }
                sentence.push_str(&format!(
                    ". {} {}",
                    first.note.consequence(),
                    first.note.advice()
                ));
                Some(wrap(&sentence))
            })
            .collect()
    }

    /// One line summarising what was accounted for.
    #[must_use]
    pub fn summary(&self) -> String {
        let accounted: usize = self.groups.iter().map(|group| group.nodes.len()).sum();
        let missing: usize = self.gaps.iter().map(|gap| gap.nodes).sum();
        let total = accounted + missing;
        let noun = if total == 1 { "step" } else { "steps" };

        let mut line = if missing == 0 {
            format!("{total} {noun}, all accounted for.")
        } else {
            format!("{total} {noun}: {accounted} accounted for, {missing} not.")
        };

        // A second sentence only where there are claims, so a pipeline that
        // declares none reads exactly as it read before this milestone.
        if !self.claims.is_empty() {
            let claims = self.claims.len();
            let unbacked = self.unbacked().len();
            let noun = if claims == 1 { "claim" } else { "claims" };
            line.push(' ');
            if unbacked == 0 {
                line.push_str(&format!("{claims} {noun}, all bound."));
            } else {
                line.push_str(&format!(
                    "{claims} {noun}: {} bound, {unbacked} not.",
                    claims - unbacked
                ));
            }
        }
        line
    }
}

impl Group {
    /// This group's sentence.
    fn sentence(&self) -> String {
        let mut sentence = format!(
            "`{}` ran on {}",
            self.step,
            counted(self.nodes.len(), &self.scopes, self.scope_kind)
        );

        match self.record.tools.as_slice() {
            [] => {}
            tools => {
                let named: Vec<String> = tools
                    .iter()
                    .map(|(tool, version)| describe_tool(tool, version))
                    .collect();
                sentence.push_str(&format!(" with {}", join_names(&named)));
            }
        }

        match (&self.template, &self.record.command) {
            (Some(template), _) => sentence.push_str(&format!(", as `{template}`")),
            // No template and no command is a step whose body is Python:
            // its code is the answer, and the bundle carries it.
            (None, None) => sentence.push_str(", by a function in the pipeline itself"),
            (None, Some(command)) => sentence.push_str(&format!(", as `{command}`")),
        }

        // The seed is pulled out of the settings and given its own clause.
        // Printed among them it reads as `seed = 42` beside
        // `min_freq = 0.01`, which states the number and says nothing about
        // what it is for: a reader cannot tell the step is stochastic, or
        // that this is what makes it repeat. D40 asks the paragraph to state
        // the seed, and a methods section states a seed by calling it one.
        let seeded = self.seed.as_ref().and_then(|parameter| {
            self.record
                .parameters
                .iter()
                .find(|(name, _)| name == parameter)
                .map(|(name, value)| format!("{name}{KNIT}={KNIT}{value}"))
        });

        let settings: Vec<String> = self
            .record
            .parameters
            .iter()
            .filter(|(name, _)| !self.per_sample.contains(name))
            .filter(|(name, _)| Some(name) != self.seed.as_ref())
            .map(|(name, value)| format!("{name}{KNIT}={KNIT}{value}"))
            .collect();
        if !settings.is_empty() {
            sentence.push_str(&format!(", with {}", join_names(&settings)));
        }
        if !self.per_sample.is_empty() {
            let named = join_names(&self.per_sample);
            let verb = if self.per_sample.len() == 1 {
                "naming"
            } else {
                "each naming"
            };
            let joined = if settings.is_empty() {
                "with"
            } else {
                "and with"
            };
            sentence.push_str(&format!(", {joined} {named} {verb} the sample"));
        }

        // Last of the clauses about how it was run, so that the sentence
        // ends on the number a reader has to copy to repeat it.
        if let Some(seeded) = seeded {
            sentence.push_str(&format!(", seeded with {seeded}"));
        }

        for reference in &self.record.references {
            sentence.push_str(&format!(", against {}", describe_reference(reference)));
        }

        sentence.push('.');
        sentence
    }
}

/// Build a methods section from a run that has just happened.
///
/// The run is where the keys come from: a node's key is what a provenance
/// record is filed under, and computing it is exactly what a run does. So
/// `methods` reads rather than recomputes, and a pipeline that has just been
/// run entirely from cache is as good a subject as one that recomputed
/// everything — which is the property D22 asked provenance to restore.
///
/// # Errors
///
/// If a provenance record cannot be read.
pub fn methods(
    graph: &Graph,
    roots: &[NodeId],
    tasks: &[Task],
    report: &RunReport,
    prov: &Prov,
) -> Result<Methods, ProvError> {
    let scopes = graph.scopes(roots);
    let scope_of = |node: NodeId| scopes.get(&node).map(|scope| scope.value.clone());
    let kind_of = |node: NodeId| scopes.get(&node).map(|scope| scope.kind);
    let template_of = |node: NodeId| {
        tasks
            .iter()
            .find(|task| task.node == node)
            .and_then(|task| match &task.recipe {
                crate::exec::Recipe::Shell(command) => Some(command.clone()),
                crate::exec::Recipe::Callback(_) => None,
            })
    };
    let declared_of = |node: NodeId| {
        tasks
            .iter()
            .find(|task| task.node == node)
            .map(|task| task.citations.clone())
            .unwrap_or_default()
    };
    let seed_of = |node: NodeId| {
        tasks
            .iter()
            .find(|task| task.node == node)
            .and_then(|task| task.seed.clone())
    };

    let mut groups: Vec<Group> = Vec::new();
    let mut gaps: Vec<Gap> = Vec::new();
    let mut noticed: Vec<Noticed> = Vec::new();
    let mut produced: Vec<Produced> = Vec::new();

    // Graph order, so the paragraph reads in the order the pipeline runs.
    for result in &report.results {
        let node = result.node;
        let Some(record) = prov.lookup(result.key)? else {
            match gaps.iter_mut().find(|gap| gap.step == result.step) {
                Some(gap) => {
                    gap.nodes += 1;
                    gap.scopes.extend(scope_of(node));
                }
                None => gaps.push(Gap {
                    step: result.step.clone(),
                    scopes: scope_of(node).into_iter().collect(),
                    scope_kind: kind_of(node),
                    nodes: 1,
                }),
            }
            continue;
        };

        for note in &record.output_notes {
            noticed.push(Noticed {
                step: record.step.clone(),
                scope: scope_of(node),
                scope_kind: kind_of(node),
                note: note.clone(),
            });
        }

        // One row per result rather than per group, because this is what a
        // claim binds to and a group keeps only its first node's key.
        produced.push(Produced {
            step: record.step.clone(),
            scope: scope_of(node),
            key: result.key,
            canonical: record.output_canonical,
            output: result.output.clone(),
            recorded_at: record.recorded_at,
            platform: record.platform.clone(),
        });

        let template = template_of(node);
        let seed = seed_of(node);
        let per_sample = restating(&record, scope_of(node).as_deref(), seed.as_deref());
        let signature = signature(&record, template.as_deref(), &per_sample, seed.as_deref());

        match groups
            .iter_mut()
            .find(|group| group.step == record.step && signature == signature_of(group))
        {
            Some(group) => {
                group.nodes.push(node);
                group.scopes.extend(scope_of(node));
            }
            None => groups.push(Group {
                step: record.step.clone(),
                nodes: vec![node],
                scopes: scope_of(node).into_iter().collect(),
                scope_kind: kind_of(node),
                record,
                template,
                declared: declared_of(node),
                per_sample,
                seed,
            }),
        }
    }

    // Every distinct one, not the first group's. Taking the first was
    // right for as long as one machine ran everything, and a distributed
    // run is exactly the case it was wrong for: the records each carry
    // their own, and a worker's record arrives through the store with
    // whatever machine wrote it (D55).
    //
    // Falls back to this build and this machine when there are no groups
    // at all, as it always did: a run of nothing still has to say
    // something true.
    let mut engines: Vec<String> = groups
        .iter()
        .map(|group| group.record.engine.clone())
        .collect();
    let mut platforms: Vec<String> = groups
        .iter()
        .map(|group| group.record.platform.clone())
        .collect();
    if engines.is_empty() {
        engines.push(crate::version().to_owned());
        platforms.push(super::platform());
    }
    for list in [&mut engines, &mut platforms] {
        list.sort();
        list.dedup();
    }

    for group in &mut groups {
        group.scopes.sort();
        group.scopes.dedup();
    }
    for gap in &mut gaps {
        gap.scopes.sort();
        gap.scopes.dedup();
    }

    Ok(Methods {
        groups,
        gaps,
        noticed,
        produced,
        // Filled by `Methods::bind`, which the caller holding the claims
        // calls. Nothing about building a write-up depends on them.
        claims: Vec::new(),
        engines,
        platforms,
    })
}

/// `a`, `a and b`, `a, b and c` — for a sentence rather than a list.
///
/// A methods paragraph is read aloud in a seminar, so the separator is a
/// word and not a comma at the end.
fn english(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [front @ .., last] => format!("{} and {last}", front.join(", ")),
    }
}

/// Everything that could make two nodes' sentences differ.
///
/// A parameter that only restates which sample the node is for is left out:
/// that is what tells the nodes of a group apart, not what distinguishes one
/// group from another.
///
/// The seed declaration is in here because it changes the sentence — a
/// seeded step says *seeded with*, where an unseeded one with the same
/// parameter says *with*. Two written steps of one name, one stochastic and
/// one not, is a thin case; this is one `format!` and keeps the sentence
/// above true.
fn signature(
    record: &Record,
    template: Option<&str>,
    per_sample: &[String],
    seed: Option<&str>,
) -> String {
    let mut parts = vec![
        record.step.clone(),
        template.unwrap_or("").to_owned(),
        format!("seed={}", seed.unwrap_or("")),
    ];
    for (name, value) in &record.parameters {
        if per_sample.contains(name) {
            parts.push(format!("{name}=<sample>"));
            continue;
        }
        parts.push(format!("{name}={value}"));
    }
    for (tool, version) in &record.tools {
        parts.push(format!("{tool}@{version}"));
    }
    for reference in &record.references {
        parts.push(reference.fingerprint.clone());
    }
    parts.join("\u{1f}")
}

fn signature_of(group: &Group) -> String {
    signature(
        &group.record,
        group.template.as_deref(),
        &group.per_sample,
        group.seed.as_deref(),
    )
}

/// The parameters whose value is just this node's own sample identifier.
///
/// Compared with the quotes a host may have rendered the value with, since a
/// Python string arrives as `'SRR8525904'`.
///
/// The declared seed is never one of them, whatever its value happens to
/// equal. This rule was written when every parameter was a setting, and its
/// catch-all is too wide now that one parameter is obliged to appear: a seed
/// caught by it would vanish from the paragraph, and D40 requires the
/// paragraph to state the seed. The exemption is principled rather than
/// defensive — a seed never *merely restates* the scope, which is the only
/// thing this function is for — so where the two rules meet, D40 wins.
fn restating(record: &Record, scope: Option<&str>, seed: Option<&str>) -> Vec<String> {
    let Some(scope) = scope else {
        return Vec::new();
    };
    record
        .parameters
        .iter()
        .filter(|(name, _)| Some(name.as_str()) != seed)
        .filter(|(_, value)| unquote(value) == scope)
        .map(|(name, _)| name.clone())
        .collect()
}

impl ClaimCheck {
    /// What the write-up says about this claim, unwrapped.
    ///
    /// Implemented here rather than beside the type because the wording is
    /// the write-up's: it shares `counted`, `join_names` and `describe_tool`
    /// with every other sentence in the report, and a second place that
    /// phrased a verdict would be a second place to phrase it differently.
    #[must_use]
    pub fn sentence(&self) -> String {
        claim_sentence(self)
    }
}

/// What a write-up says about one claim.
///
/// The claim's own sentence first and in quotes, because that is what a
/// reader greps the write-up for: they have the manuscript open beside it.
fn claim_sentence(check: &ClaimCheck) -> String {
    let claim = &check.claim;
    let step = &claim.step;

    match &check.verdict {
        ClaimVerdict::Bound | ClaimVerdict::Verified { .. } => {
            let mut sentence = format!("\"{}\" rests on `{step}`", claim.text);
            match check.bound.as_slice() {
                // One computation, so its key belongs in the sentence.
                [only] => {
                    if let Some(scope) = &only.scope {
                        sentence.push_str(&format!(" ({scope})"));
                    }
                    sentence.push_str(&restated_clause(check));
                    sentence.push_str(&format!(
                        ", output {}, recorded {} on {}.",
                        digest(only.canonical),
                        day(only.recorded_at),
                        only.platform
                    ));
                }
                bound => {
                    let scopes: Vec<String> =
                        bound.iter().filter_map(|row| row.scope.clone()).collect();
                    sentence.push_str(&format!(
                        " on {}",
                        counted(bound.len(), &scopes, Some("sample"))
                    ));
                    sentence.push_str(&restated_clause(check));
                    // Every digest, because naming beats counting (D15) and
                    // a reviewer checking one sample needs that sample's.
                    let digests: Vec<String> = bound
                        .iter()
                        .map(|row| match &row.scope {
                            Some(scope) => format!("{scope} at {}", digest(row.canonical)),
                            None => digest(row.canonical),
                        })
                        .collect();
                    sentence.push_str(&format!(". Outputs: {}.", digests.join(", ")));
                }
            }
            sentence
        }
        // A located claim is the one disagreement this write-up *can*
        // adjudicate, so it says so. For a parameter or a tool version the
        // engine knows only that the two differ: the sentence may be
        // describing a run that happened, and the pipeline may have been
        // edited since. For a number read out of the artifact the claim is
        // bound to, there is nothing left to be uncertain about.
        ClaimVerdict::Contradicted { stated, recorded } => {
            if let Some(Restated::Value { at, tolerance, .. }) = &claim.restates {
                let Located::Cell { column, row } = at;
                return format!(
                    "\"{}\" states {column}{KNIT}={KNIT}{stated}, and `{step}`'s output \
                     holds {recorded} at column `{column}` row {row} — further from it than \
                     the {tolerance} this claim allows. The number in the sentence is not \
                     the number in the bytes.",
                    claim.text
                );
            }
            let (what, held) = match &claim.restates {
                Some(Restated::Parameter { name, .. }) => (
                    format!("{name}{KNIT}={KNIT}{stated}"),
                    format!("{name}{KNIT}={KNIT}{recorded}"),
                ),
                Some(Restated::Tool { name, .. }) => {
                    (format!("{name} {stated}"), describe_tool(name, recorded))
                }
                // Unreachable: nothing is compared without a restatement,
                // and a value is handled above.
                Some(Restated::Value { .. }) | None => (stated.clone(), recorded.clone()),
            };
            format!(
                "\"{}\" states {what}, and `{step}` ran with {held}. One of \
                 the two is wrong, and this write-up cannot tell you which.",
                claim.text
            )
        }
        ClaimVerdict::Unbacked(why) => {
            format!(
                "\"{}\" has nothing behind it: {}",
                claim.text,
                why_of(claim, why)
            )
        }
    }
}

/// The clause naming the value a claim restates, as the run recorded it.
///
/// Takes the whole check rather than the claim, because one of the three
/// restatements reports a **reading** rather than a restatement: a verified
/// claim's clause gives the number that was in the bytes, and that number
/// is on the verdict.
fn restated_clause(check: &ClaimCheck) -> String {
    let claim = &check.claim;
    match &claim.restates {
        Some(Restated::Parameter { name, value }) => {
            format!(", where {name}{KNIT}={KNIT}{value} as this claim states")
        }
        Some(Restated::Tool { name, version }) => {
            format!(", which ran {name} {version} as this claim states")
        }
        // Named place, read number, declared tolerance: all three, because
        // a reader checking this by hand needs to know which cell was
        // opened, and the tolerance is what makes the agreement a fact
        // rather than a rounding that happened to land (D61).
        Some(Restated::Value { at, tolerance, .. }) => {
            let Located::Cell { column, row } = at;
            let read = match &check.verdict {
                ClaimVerdict::Verified { recorded } => format!("{recorded}"),
                // Unreachable: a located claim that reached this clause was
                // verified. The stated value rather than a panic.
                _ => match &claim.restates {
                    Some(Restated::Value { value, .. }) => format!("{value}"),
                    _ => String::new(),
                },
            };
            format!(
                ", whose output gives {column}{KNIT}={KNIT}{read} at row {row}, within the \
                 {tolerance} this claim allows"
            )
        }
        None => String::new(),
    }
}

/// Why nothing stands behind a claim, and what to do about it.
///
/// Each reason names what there *was*, for the reason the check and run
/// reports name their samples (D15): a reader who has to go and find out
/// which steps exist has a shorter report rather than a better one. And each
/// names what the claim asked for, so the two can be read side by side
/// without going back to the pipeline file.
fn why_of(claim: &Claim, why: &Unbacked) -> String {
    let step = &claim.step;
    match why {
        Unbacked::NoSuchStep { produced } => format!(
            "nothing in this pipeline produces `{step}`. It produces {}. \
             Name the step that produces this number, or drop the claim.",
            join_names(&quoted(produced))
        ),
        Unbacked::NoRecord => format!(
            "`{step}` is in the cache from a run that recorded no provenance, \
             so nothing is known about how this number was produced. \
             `ligase run --force` runs it again and records it."
        ),
        Unbacked::NoSuchScope { ran_on } => {
            let wanted = claim.scope.as_deref().unwrap_or("");
            match ran_on.as_slice() {
                // A step with one node has no scope at all: the graph gives
                // one only where something tells siblings apart (D15). So
                // the answer is not "it ran on nothing" but "there is
                // nothing here to name", and a claim on such a step should
                // drop its sample rather than correct it.
                [] => format!(
                    "`{step}` ran once and nothing tells its nodes apart, so \
                     there is no sample called `{wanted}` or anything else. \
                     Drop the sample from this claim."
                ),
                ran_on => format!(
                    "`{step}` ran on no sample called `{wanted}`. It ran on {}.",
                    join_names(&quoted(ran_on))
                ),
            }
        }
        Unbacked::NoSuchParameter { takes } => {
            let wanted = match &claim.restates {
                Some(Restated::Parameter { name, .. }) => format!(" `{name}`"),
                _ => String::new(),
            };
            match takes.as_slice() {
                [] => format!("`{step}` takes no parameters at all."),
                takes => format!(
                    "`{step}` takes no parameter{wanted}. It takes {}.",
                    join_names(&quoted(takes))
                ),
            }
        }
        Unbacked::NoSuchTool { ran } => {
            let wanted = match &claim.restates {
                Some(Restated::Tool { name, .. }) => format!(" called `{name}`"),
                _ => String::new(),
            };
            match ran.as_slice() {
                [] => format!("`{step}` ran no external tool at all."),
                ran => format!(
                    "`{step}` ran no tool{wanted}. It ran {}.",
                    join_names(&quoted(ran))
                ),
            }
        }
        // The claim named a place and there is no number there. The detail
        // is phrased where it was discovered, by `Located::read`, because
        // what was there instead -- the columns the output does have, how
        // many rows it holds, the text that is not a number -- is known
        // only at the point of reading.
        //
        // Not a contradiction: an unreadable location says nothing about
        // whether the sentence is true, so the write-up reports that it
        // could not check rather than that the claim is wrong.
        Unbacked::NoSuchCell { detail } => format!(
            "`{step}` is in this pipeline and {detail}. Point the claim at a \
             cell that holds the number, or drop `at` and let the claim be \
             bound without being verified."
        ),
    }
}

/// Names in backticks, as a sentence about code writes them.
fn quoted(names: &[String]) -> Vec<String> {
    names.iter().map(|name| format!("`{name}`")).collect()
}

/// The citable identifier of a result: the digest of its canonical form.
///
/// Twelve characters, as `audit` already prints a content hash, and the
/// **canonical** digest rather than the raw bytes' — that is what `audit`
/// compares two runs by and what a bundle verifies against, so it is the
/// one a reader can check on another machine (D4, D59).
fn digest(hash: ContentHash) -> String {
    hash.to_hex().chars().take(12).collect()
}

/// A rendered value with one layer of surrounding quotes removed.
pub(super) fn unquote(value: &str) -> &str {
    for quote in ['\'', '"'] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

/// A tool and its version, without saying the tool's name twice.
///
/// `samtools --version` prints `samtools 1.24`, while `minimap2 --version`
/// prints `2.31-r1287` and nothing else. Only the first line is used: the
/// rest is the libraries it was built against, which belong in the bundle
/// rather than in a sentence.
fn describe_tool(tool: &str, version: &str) -> String {
    let first = version.lines().next().unwrap_or(version).trim();
    if first.to_lowercase().starts_with(&tool.to_lowercase()) {
        return first.to_owned();
    }
    format!("{tool} {first}")
}

/// A reference, as firmly as it can be named.
///
/// The accession where the file gives one, the build label where the user
/// gave one, and the fingerprint where neither exists — so this never
/// returns nothing, and never claims an accession it did not read.
fn describe_reference(reference: &ReferenceRecord) -> String {
    // What it is called, which may be nothing.
    let name = match &reference.build {
        Some(build) => format!("the {build} reference"),
        None => "the reference".to_owned(),
    };

    // What it is, in descending order of how firmly it says so. The
    // fingerprint is only reached for when there is no accession, because
    // an accession is the better answer and both would be noise.
    let mut pinned = Vec::new();
    match &reference.accession {
        Some(accession) => pinned.push(accession.clone()),
        None => pinned.push(format!("fingerprint {}", reference.fingerprint)),
    }
    if reference.contigs > 1 {
        pinned.push(format!("{} sequences", reference.contigs));
    }
    pinned.push(format!("{} bp", reference.length));
    if let Some(checksum) = &reference.checksum {
        pinned.push(format!("MD5 {checksum}"));
    }

    format!("{name} ({})", pinned.join(", "))
}

/// `4 samples (a, b, c, d)`, or `one input` when nothing tells them apart.
fn counted(nodes: usize, scopes: &[String], kind: Option<&str>) -> String {
    let Some(kind) = kind.filter(|_| !scopes.is_empty()) else {
        return match nodes {
            1 => "one input".to_owned(),
            many => format!("{many} inputs"),
        };
    };
    let noun = if scopes.len() == 1 {
        kind.to_owned()
    } else {
        format!("{kind}s")
    };
    format!("{} {noun} ({})", scopes.len(), noun_list(scopes))
}

/// The scope values, as a list a sentence can hold.
fn noun_list(scopes: &[String]) -> String {
    scopes.join(", ")
}

/// `a`, `a and b`, `a, b and c`.
fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Wrap at [`WIDTH`], breaking only between words.
fn wrap(text: &str) -> String {
    fold(text, WIDTH, "")
}

/// Wrap at [`WIDTH`], indenting every line after the first.
fn wrap_hanging(text: &str) -> String {
    fold(text, WIDTH, "    ")
}

fn fold(text: &str, width: usize, indent: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();

    for word in text.split_whitespace() {
        let margin = if lines.is_empty() { 0 } else { indent.len() };
        // A knitted phrase is one word here and several on the page, so its
        // width is what it will occupy once the marks become spaces.
        if !line.is_empty() && margin + line.len() + 1 + word.len() > width {
            lines.push(line);
            line = String::new();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }

    lines
        .iter()
        .enumerate()
        .map(|(at, line)| {
            if at == 0 {
                line.clone()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .replace(KNIT, " ")
}

/// Each remote as a methods sentence names it: what, from where, and when.
///
/// The digest as well as the date, because a date alone does not identify
/// bytes — two people fetching "on 20 October" may not have the same thing.
fn described(remotes: &[&Origin]) -> Vec<String> {
    remotes
        .iter()
        .map(|remote| {
            format!(
                "`{}` on {} (BLAKE3 {})",
                remote.locator,
                day(remote.fetched_at),
                remote.digest.to_hex()
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_that_prints_its_own_name_does_not_get_it_twice() {
        assert_eq!(describe_tool("samtools", "samtools 1.24"), "samtools 1.24");
        assert_eq!(
            describe_tool("minimap2", "2.31-r1287"),
            "minimap2 2.31-r1287"
        );
    }

    #[test]
    fn only_a_tools_first_line_reaches_a_sentence() {
        assert_eq!(
            describe_tool("samtools", "samtools 1.24\nUsing htslib 1.24\n"),
            "samtools 1.24"
        );
    }

    #[test]
    fn names_are_joined_as_english_joins_them() {
        let one = ["a".to_owned()];
        let two = ["a".to_owned(), "b".to_owned()];
        let three = ["a".to_owned(), "b".to_owned(), "c".to_owned()];
        assert_eq!(join_names(&one), "a");
        assert_eq!(join_names(&two), "a and b");
        assert_eq!(join_names(&three), "a, b and c");
    }

    #[test]
    fn a_setting_is_never_broken_across_lines() {
        let settings: Vec<String> = (0..9)
            .map(|n| format!("parameter_number_{n}{KNIT}={KNIT}0.0{n}"))
            .collect();
        let text = wrap(&format!("`align` ran with {}.", join_names(&settings)));
        for line in text.lines() {
            assert!(line.len() <= WIDTH, "{line:?}");
        }
        for n in 0..9 {
            assert!(
                text.contains(&format!("parameter_number_{n} = 0.0{n}")),
                "a setting was split: {text}"
            );
        }
    }

    #[test]
    fn wrapping_breaks_between_words_only() {
        let text = "one two three four five six seven eight nine ten eleven \
                    twelve thirteen fourteen fifteen sixteen seventeen";
        for line in wrap(text).lines() {
            assert!(line.len() <= WIDTH, "{line:?} is {} long", line.len());
        }
        assert_eq!(
            wrap(text).split_whitespace().collect::<Vec<_>>(),
            text.split_whitespace().collect::<Vec<_>>(),
            "wrapping changes where the lines break and nothing else"
        );
    }
}
