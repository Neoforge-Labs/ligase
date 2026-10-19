//! Turning provenance records into the paragraph a paper needs.
//!
//! `docs/milestones.md` says what M5 is signed off on: a methods paragraph
//! with tool versions, parameters, reference accession and citations. Each
//! of those four is already recorded by the time this runs
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

use super::{Prov, ProvError, Record, ReferenceRecord, cite};
use crate::exec::{RunReport, Task};
use crate::graph::{Graph, NodeId};

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

/// Everything needed to write a methods section.
#[derive(Clone, Debug, Default)]
pub struct Methods {
    /// Each distinct way a step was run, in graph order.
    pub groups: Vec<Group>,
    /// Steps nothing is known about.
    pub gaps: Vec<Gap>,
    /// The version of Ligase that produced the results.
    pub engine: String,
    /// The platform they were produced on.
    pub platform: String,
}

impl Methods {
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

    /// The reference list, and the tools that have no reference.
    ///
    /// The second half is the point: a tool with nothing to cite is named,
    /// so a short reference list cannot be mistaken for a complete one
    /// (D24).
    #[must_use]
    pub fn citations(&self) -> (Vec<String>, Vec<String>) {
        let mut cited: BTreeSet<String> = BTreeSet::new();
        let mut uncited: BTreeSet<String> = BTreeSet::new();

        for group in &self.groups {
            for text in &group.declared {
                cited.insert(text.clone());
            }
            for (tool, _) in &group.record.tools {
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
            self.engine, self.platform
        ))];

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

        out
    }

    /// One line summarising what was accounted for.
    #[must_use]
    pub fn summary(&self) -> String {
        let accounted: usize = self.groups.iter().map(|group| group.nodes.len()).sum();
        let missing: usize = self.gaps.iter().map(|gap| gap.nodes).sum();
        let total = accounted + missing;
        let noun = if total == 1 { "step" } else { "steps" };

        if missing == 0 {
            return format!("{total} {noun}, all accounted for.");
        }
        format!("{total} {noun}: {accounted} accounted for, {missing} not.")
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

        let settings: Vec<String> = self
            .record
            .parameters
            .iter()
            .filter(|(name, _)| !self.per_sample.contains(name))
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

    let mut groups: Vec<Group> = Vec::new();
    let mut gaps: Vec<Gap> = Vec::new();

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

        let template = template_of(node);
        let per_sample = restating(&record, scope_of(node).as_deref());
        let signature = signature(&record, template.as_deref(), &per_sample);

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
            }),
        }
    }

    let (engine, platform) = groups.first().map_or_else(
        || (crate::version().to_owned(), super::platform()),
        |group| (group.record.engine.clone(), group.record.platform.clone()),
    );

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
        engine,
        platform,
    })
}

/// Everything that could make two nodes' sentences differ.
///
/// A parameter that only restates which sample the node is for is left out:
/// that is what tells the nodes of a group apart, not what distinguishes one
/// group from another.
fn signature(record: &Record, template: Option<&str>, per_sample: &[String]) -> String {
    let mut parts = vec![record.step.clone(), template.unwrap_or("").to_owned()];
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
    signature(&group.record, group.template.as_deref(), &group.per_sample)
}

/// The parameters whose value is just this node's own sample identifier.
///
/// Compared with the quotes a host may have rendered the value with, since a
/// Python string arrives as `'SRR8525904'`.
fn restating(record: &Record, scope: Option<&str>) -> Vec<String> {
    let Some(scope) = scope else {
        return Vec::new();
    };
    record
        .parameters
        .iter()
        .filter(|(_, value)| unquote(value) == scope)
        .map(|(name, _)| name.clone())
        .collect()
}

/// A rendered value with one layer of surrounding quotes removed.
fn unquote(value: &str) -> &str {
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
