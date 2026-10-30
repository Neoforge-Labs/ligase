//! Binding a published number to the computation behind it.
//!
//! [`super::methods`] runs one way: from a result back to how it was
//! produced. This module runs the other way, from a sentence in a manuscript
//! to the step, the key and the output hash that stand behind it — which is
//! the direction a reader of the paper cares about and the one the engine has
//! never been able to answer.
//!
//! # Why the engine owes this and not something more
//!
//! Three decisions already make one promise, which is that *the record names
//! what it cannot account for*. D24 names an uncited tool rather than
//! omitting it. D26 names a field that cannot be dropped. [`super::Gap`]
//! names a step whose production is unknown rather than printing a paragraph
//! with a silent hole in it.
//!
//! So `ligase methods` will tell you that `fastp` has no reference, that a
//! sample identifier in your VCF is really a file name, and that one step's
//! provenance is missing. It has never been able to tell you that the number
//! in your abstract is not a number this pipeline produces, because nothing
//! told it what that number was. A [`Claim`] is how it is told, and this
//! module is the fourth instance of that promise rather than a new one.
//!
//! # Why every part of the declaration is checked
//!
//! M13's finding, recorded in `milestones.mdx`: *a declaration nothing can
//! check is a comment.* A `Claims` section that reprinted the user's own
//! sentences under a heading would be worse than no section at all — it would
//! make an unchecked paragraph look checked, which is the failure D24 refuses
//! in the reference list. So a claim's step, its sample, the parameter or
//! tool it restates and the value it gives them are each compared against
//! what the run recorded, and only the *text* is taken on trust. A claim that
//! cannot survive those checks is named rather than printed.
//!
//! # What a claim binds to, which is D45
//!
//! The step **key** and the canonical hash of the output, never the occasion.
//! The key is already the identity of a computation — the same code,
//! parameters, inputs and tool versions — which is what D22 spent a milestone
//! restoring to provenance. So a step served from the cache backs its claim
//! completely, and what the binding adds is *when that computation happened*:
//! the record's own date and platform, which may be long before this run.
//!
//! The alternative, binding only what this run recomputed, would make
//! `ligase run --force` the only honest way to write up a paper. A claim
//! mechanism that did not believe the cache would be arguing with the engine
//! it lives in.
//!
//! A claim also never reaches a cache key, for a reason that needs no
//! balancing: a claim in the key would mean editing a sentence in a
//! manuscript reruns the aligner.

use std::fmt;

use super::methods::{Group, Methods};
use crate::hash::ContentHash;
use crate::key::StepKey;

/// A number or a phrase a paper states, and where it came from.
///
/// The text and the value are separate on purpose, and the separation is the
/// sharpest edge in the design. A paper writes **1%**; the pipeline holds
/// **0.01**. Comparing the sentence against the parameter would be a guess
/// about arithmetic in English, so the sentence is what a reader greps for
/// and [`Restated`] is what the engine compares.
///
/// `Eq` is deliberately absent, here and on every type that can hold a
/// [`Restated::Value`]: a claim can now carry an `f64`, which has no total
/// equality. Nothing wanted it — these types are compared for sameness and
/// never used as a key.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Claim {
    /// The sentence as the paper states it. Never checked, and cannot be.
    pub text: String,
    /// The step that produced it, by name.
    pub step: String,
    /// The sample it is about, where it is about one.
    ///
    /// `None` means the claim is about the step across every sample it ran
    /// on, which is right for "every sample was interpreted" and wrong for a
    /// count from one of them. There is no inference: this field is how a
    /// user says which they mean.
    pub scope: Option<String>,
    /// The value it restates, where it restates one.
    pub restates: Option<Restated>,
}

/// A value a claim restates, which the engine compares against the run.
///
/// A sum type rather than three optional fields, so a claim that restates
/// both a parameter and a tool version **cannot be built**. The Python
/// surface is flat — `parameter=`, `tool=`, `value=` — because that is how a
/// person writes one, and refusing the incoherent combinations is the one
/// job its `__post_init__` has.
#[derive(Clone, Debug, PartialEq)]
pub enum Restated {
    /// A parameter, and the value the paper gives it.
    Parameter {
        /// The parameter's name, as the step declares it.
        name: String,
        /// What the paper says it was.
        value: String,
    },
    /// A tool, and the version the paper gives it.
    Tool {
        /// The tool's name, as `tool=` names it.
        name: String,
        /// What the paper says its version was.
        version: String,
    },
    /// A number, where in the output it lives, and to what precision the
    /// paper states it.
    ///
    /// The third kind, and the first the engine cannot answer from the
    /// record alone: a parameter and a tool version are things the run
    /// *recorded*, and this is a number in the bytes the run *produced*.
    /// So it is the only restatement that reads an artifact, and it reads
    /// exactly where it was told and nowhere else (D61).
    Value {
        /// What the paper says the number is.
        value: f64,
        /// Where in the output it lives.
        at: Located,
        /// How far the recorded number may be from the stated one.
        ///
        /// The consumer's, declared, and never defaulted: a claim to three
        /// decimals and a claim to machine precision are different claims,
        /// and a default would invent a precision nobody stated.
        tolerance: f64,
    },
}

/// Where in a step's output a number lives.
///
/// An enum with one variant, so that a second way of locating a number is
/// a variant rather than a redesign — and so that the engine is never
/// asked to go looking. It reads the place it was given, in the one format
/// it already knows how to read, and a claim that cannot say where its
/// number is stays step-only (which is a legitimate state, not an error).
#[derive(Clone, Debug, PartialEq)]
pub enum Located {
    /// A cell of a TSV with a header: a column by name, a row by position.
    ///
    /// Row 0 is the first row *after* the header, because that is the first
    /// row of data and a claim is about data. A ranking's top row is
    /// therefore row 0, which is also the row a ranking's own rank 1 is.
    Cell {
        /// The column's name, as the output's header spells it.
        column: String,
        /// The row's position among the data rows, counting from zero.
        row: usize,
    },
}

impl Located {
    /// The number at this location, or why there is not one.
    ///
    /// # Errors
    ///
    /// A phrase for [`Unbacked::NoSuchCell`], naming what was there
    /// instead: the columns the file does have, how many rows it holds, or
    /// the text that is not a number. Each is a different mistake with a
    /// different fix, and a claim that merely said "could not read it"
    /// would send a reader looking for all three.
    pub fn read(&self, output: &std::path::Path) -> Result<f64, String> {
        let Self::Cell { column, row } = self;
        let reader = crate::load::open(output)
            .map_err(|error| format!("its output could not be read: {error}"))?;
        let mut table = crate::load::Table::read(reader)
            .map_err(|detail| format!("its output is not a table with a header: {detail}"))?;
        let at = table.column(column).ok_or_else(|| {
            format!(
                "its output has no column named `{column}`; its columns are {}",
                table.header().join(", ")
            )
        })?;

        let mut seen = 0usize;
        while let Some(line) = table
            .next_row()
            .map_err(|detail| format!("its output could not be read past row {seen}: {detail}"))?
        {
            if seen == *row {
                let field = line.field(at).unwrap_or_default().trim();
                return field.parse::<f64>().map_err(|_| {
                    format!(
                        "its output holds `{field}` at column `{column}` row {row}, which is \
                         not a number"
                    )
                });
            }
            seen += 1;
        }
        Err(format!(
            "its output has {seen} rows, so there is no row {row}"
        ))
    }
}

/// One computation a claim rests on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound {
    /// The sample it was for, where anything tells the nodes apart.
    pub scope: Option<String>,
    /// The step key: the identity of the computation (D45).
    pub key: StepKey,
    /// The canonical hash of what it produced.
    pub canonical: ContentHash,
    /// Where that output is, so a located claim can read its number (D61).
    pub output: std::path::PathBuf,
    /// When the computation happened, which is not when this run was.
    pub recorded_at: i64,
    /// The platform it happened on.
    pub platform: String,
}

/// What became of one claim.
#[derive(Clone, Debug, PartialEq)]
pub enum ClaimVerdict {
    /// Every check passed: the number rests on a computation this write-up
    /// accounts for.
    Bound,
    /// Bound, **and** the number was read out of the output and agrees.
    ///
    /// Its own verdict rather than a kind of [`Self::Bound`], because it is
    /// the stronger statement and the difference is the whole of what a
    /// located claim buys: bound says the sentence rests on a computation
    /// this write-up accounts for, and verified says the number in the
    /// sentence is the number in the bytes (D61).
    Verified {
        /// The number the output holds, as read.
        recorded: f64,
    },
    /// Nothing in the run stands behind it.
    Unbacked(Unbacked),
    /// Something stands behind it and disagrees with it.
    ///
    /// Its own verdict rather than a kind of unbacked, because it is the
    /// stronger statement: the claim *is* bound, to a computation that
    /// contradicts the paper.
    Contradicted {
        /// What the paper says.
        stated: String,
        /// What the run recorded.
        recorded: String,
    },
}

impl ClaimVerdict {
    /// The one word a table prints for this verdict.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Bound => "bound",
            Self::Verified { .. } => "verified",
            Self::Unbacked(_) => "unbacked",
            Self::Contradicted { .. } => "contradicted",
        }
    }

    /// Whether the write-up can stand behind the claim.
    #[must_use]
    pub fn is_bound(&self) -> bool {
        matches!(self, Self::Bound | Self::Verified { .. })
    }

    /// Whether the number was read out of the output and agreed.
    #[must_use]
    pub const fn is_verified(&self) -> bool {
        matches!(self, Self::Verified { .. })
    }
}

impl fmt::Display for ClaimVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Why nothing in the run stands behind a claim.
///
/// Five reasons rather than one flag, because each names a different mistake
/// and each has a different fix. A step renamed, a sample dropped, a
/// parameter renamed and a tool swapped are four ordinary edits, and a
/// complaint that merely said "unbacked" would send a reader looking for all
/// four.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unbacked {
    /// No step of that name is in this write-up.
    NoSuchStep {
        /// The steps it does account for.
        produced: Vec<String>,
    },
    /// The step is there and is a [`super::Gap`]: its output is in the cache
    /// from a run that recorded no provenance, so a claim on it rests on
    /// nothing.
    NoRecord,
    /// The step ran on no such sample.
    NoSuchScope {
        /// The samples it did run on.
        ran_on: Vec<String>,
    },
    /// The step takes no parameter of that name.
    NoSuchParameter {
        /// The parameters it does take.
        takes: Vec<String>,
    },
    /// The step ran no tool of that name.
    NoSuchTool {
        /// The tools it did run.
        ran: Vec<String>,
    },
    /// The claim named a place in the output, and there is no number there.
    ///
    /// One variant for several mistakes, because they are one mistake from
    /// the claim's side — the location is wrong — and the `detail` is where
    /// the difference lives: the columns the output does have, how many
    /// rows it holds, or the text that is not a number. It is phrased where
    /// it is discovered, by [`Located::read`], for the same reason a
    /// loader's unknown-dimension reason is.
    NoSuchCell {
        /// What was there instead, as a phrase a sentence can carry.
        detail: String,
    },
}

/// One claim, and what the run had to say about it.
#[derive(Clone, Debug, PartialEq)]
pub struct ClaimCheck {
    /// The claim as it was declared.
    pub claim: Claim,
    /// What became of it.
    pub verdict: ClaimVerdict,
    /// The computations it rests on, in graph order.
    ///
    /// Empty for every [`Unbacked`] verdict. One row for a claim naming a
    /// sample, and one per sample for a claim that names none — a claim is
    /// bound to computations, and a [`Group`] keeps one representative
    /// record and discards the rest of its keys.
    pub bound: Vec<Bound>,
}

/// Check one claim against a write-up.
///
/// The checks are in the order a reader would make them, and each failure
/// stops there: a claim naming a step that does not exist has nothing to say
/// about a parameter.
pub(super) fn check(methods: &Methods, claim: &Claim) -> ClaimCheck {
    let unbacked = |why: Unbacked| ClaimCheck {
        claim: claim.clone(),
        verdict: ClaimVerdict::Unbacked(why),
        bound: Vec::new(),
    };

    // 1. The step is one this write-up accounts for, as a group or as a gap.
    let in_groups = methods.produced.iter().any(|row| row.step == claim.step);
    let in_gaps = methods.gaps.iter().any(|gap| gap.step == claim.step);
    if !in_groups && !in_gaps {
        return unbacked(Unbacked::NoSuchStep {
            produced: accounted_steps(methods),
        });
    }

    // 2. The sample, where one is named, is one that step ran on.
    let ran_on = scopes_of(methods, &claim.step);
    if let Some(scope) = &claim.scope
        && !ran_on.contains(scope)
    {
        return unbacked(Unbacked::NoSuchScope { ran_on });
    }

    // 3. Something is known about how it was produced. Reached when every
    //    node this claim selects is a gap, which is a claim resting on
    //    nothing however firmly the step exists.
    let bound: Vec<Bound> = methods
        .produced
        .iter()
        .filter(|row| row.step == claim.step)
        .filter(|row| match (&claim.scope, &row.scope) {
            (None, _) => true,
            (Some(wanted), Some(held)) => wanted == held,
            (Some(_), None) => false,
        })
        .map(|row| Bound {
            scope: row.scope.clone(),
            key: row.key,
            canonical: row.canonical,
            output: row.output.clone(),
            recorded_at: row.recorded_at,
            platform: row.platform.clone(),
        })
        .collect();
    if bound.is_empty() {
        return unbacked(Unbacked::NoRecord);
    }

    // 4 and 5. The value it restates exists, and agrees.
    let Some(restates) = &claim.restates else {
        return ClaimCheck {
            claim: claim.clone(),
            verdict: ClaimVerdict::Bound,
            bound,
        };
    };

    let groups = groups_for(methods, claim);
    let verdict = match restates {
        Restated::Parameter { name, value } => {
            compare(&held_parameters(&groups, name), value, || {
                Unbacked::NoSuchParameter {
                    takes: names(&groups, |group| {
                        group
                            .record
                            .parameters
                            .iter()
                            .map(|(held, _)| held.clone())
                            .collect()
                    }),
                }
            })
        }
        // The only restatement that reads an artifact. Every bound row is
        // checked rather than the first: a claim naming no sample is about
        // the step across every sample it ran on, and one sample whose
        // number disagrees refutes the sentence as surely as all of them
        // would.
        Restated::Value {
            value,
            at,
            tolerance,
        } => match read_value(&bound, at) {
            Err(detail) => ClaimVerdict::Unbacked(Unbacked::NoSuchCell { detail }),
            Ok(found) => match found
                .iter()
                .find(|recorded| (*recorded - value).abs() > *tolerance)
            {
                Some(recorded) => ClaimVerdict::Contradicted {
                    stated: format!("{value}"),
                    recorded: format!("{recorded}"),
                },
                // Every reading agreed, so the number in the sentence is
                // the number in the bytes. The first is reported because a
                // claim over one sample has exactly one.
                None => ClaimVerdict::Verified {
                    recorded: found.first().copied().unwrap_or(*value),
                },
            },
        },
        Restated::Tool { name, version } => {
            // A tool's version is compared by `version_states` rather than by
            // equality, because `samtools --version` prints its own name and
            // `minimap2 --version` prints a build suffix. The disagreement
            // reported is still the whole recorded string: what the engine
            // holds, not what it trimmed.
            let held = held_tools(&groups, name);
            match held
                .iter()
                .find(|recorded| !version_states(name, recorded, version))
            {
                Some(recorded) => ClaimVerdict::Contradicted {
                    stated: version.clone(),
                    recorded: (*recorded).clone(),
                },
                None if held.is_empty() => ClaimVerdict::Unbacked(Unbacked::NoSuchTool {
                    ran: names(&groups, |group| {
                        group
                            .record
                            .tools
                            .iter()
                            .map(|(held, _)| held.clone())
                            .collect()
                    }),
                }),
                None => ClaimVerdict::Bound,
            }
        }
    };

    // An unbacked verdict rests on nothing, whatever the step did.
    let bound = match &verdict {
        ClaimVerdict::Unbacked(_) => Vec::new(),
        _ => bound,
    };
    ClaimCheck {
        claim: claim.clone(),
        verdict,
        bound,
    }
}

/// The number at `at` in each computation's output.
///
/// # Errors
///
/// The first reading that fails, with the phrase [`Located::read`] gave it.
/// Stopping at the first is right for a wrong *location*, which is what
/// this can report: a column name that is wrong is wrong for every sample,
/// and a claim that listed the same complaint once per sample would bury
/// it.
fn read_value(bound: &[Bound], at: &Located) -> Result<Vec<f64>, String> {
    bound.iter().map(|row| at.read(&row.output)).collect()
}

/// Compare a stated value against every value the run holds for it.
///
/// One disagreement is enough: two groups of a step run with different
/// thresholds cannot both be the one a single sentence describes.
fn compare(held: &[String], stated: &str, missing: impl FnOnce() -> Unbacked) -> ClaimVerdict {
    if held.is_empty() {
        return ClaimVerdict::Unbacked(missing());
    }
    match held.iter().find(|recorded| recorded.as_str() != stated) {
        Some(recorded) => ClaimVerdict::Contradicted {
            stated: stated.to_owned(),
            recorded: recorded.clone(),
        },
        None => ClaimVerdict::Bound,
    }
}

/// Every value this step recorded for a parameter, unquoted.
///
/// Unquoted because a Python string arrives rendered — `'pol'` — and a user
/// writes `pol`. One layer of rendering quotes is not a disagreement, which
/// is the same allowance [`super::methods`] already makes when it works out
/// which parameters merely restate the sample.
fn held_parameters(groups: &[&Group], name: &str) -> Vec<String> {
    groups
        .iter()
        .filter_map(|group| {
            group
                .record
                .parameters
                .iter()
                .find(|(held, _)| held == name)
                .map(|(_, value)| super::methods::unquote(value).to_owned())
        })
        .collect()
}

/// Every version this step recorded for a tool, whole.
fn held_tools(groups: &[&Group], name: &str) -> Vec<String> {
    groups
        .iter()
        .filter_map(|group| {
            group
                .record
                .tools
                .iter()
                .find(|(held, _)| held == name)
                .map(|(_, version)| version.clone())
        })
        .collect()
}

/// The names a step had to offer, sorted and without duplicates.
fn names(groups: &[&Group], of: impl Fn(&Group) -> Vec<String>) -> Vec<String> {
    let mut held: Vec<String> = groups.iter().flat_map(|group| of(group)).collect();
    held.sort();
    held.dedup();
    held
}

/// The groups a claim is about: its step, narrowed to its sample where it
/// names one.
fn groups_for<'a>(methods: &'a Methods, claim: &Claim) -> Vec<&'a Group> {
    methods
        .groups
        .iter()
        .filter(|group| group.step == claim.step)
        .filter(|group| match &claim.scope {
            None => true,
            Some(scope) => group.scopes.contains(scope),
        })
        .collect()
}

/// Every step this write-up accounts for, gaps included, without duplicates.
///
/// Gaps included because a claim on a gapped step must be told that the step
/// exists and its record does not, which is a different fix from a renamed
/// step.
fn accounted_steps(methods: &Methods) -> Vec<String> {
    let mut steps: Vec<String> = methods
        .groups
        .iter()
        .map(|group| group.step.clone())
        .chain(methods.gaps.iter().map(|gap| gap.step.clone()))
        .collect();
    steps.sort();
    steps.dedup();
    steps
}

/// Every sample one step ran on, gaps included, without duplicates.
fn scopes_of(methods: &Methods, step: &str) -> Vec<String> {
    let mut scopes: Vec<String> = methods
        .groups
        .iter()
        .filter(|group| group.step == step)
        .flat_map(|group| group.scopes.clone())
        .chain(
            methods
                .gaps
                .iter()
                .filter(|gap| gap.step == step)
                .flat_map(|gap| gap.scopes.clone()),
        )
        .collect();
    scopes.sort();
    scopes.dedup();
    scopes
}

/// Whether a version a paper states is the version a tool printed.
///
/// Textual, and never numeric. Equality is the wrong test three times over,
/// and each case is a tool this project actually runs:
///
/// - `samtools --version` prints `samtools 1.24`, so a tool's own name has
///   to come off.
/// - `python3 --version` prints `Python 3.15.0` — a name that is *not* the
///   name it was invoked under — so anything before the first digit has to
///   come off too.
/// - `minimap2 --version` prints `2.31-r1287`, so a paper that says `2.31`
///   is right and equality would call it wrong.
///
/// So the stated version is tried against the whole first line and against
/// each way of trimming a leading name off it, and one match is enough.
/// Generosity here cannot produce a false agreement, because of the rule
/// that does the work: **a prefix must end at a component boundary.** Without
/// it `2.4` would claim `2.41`, which is a wrong version reported as a
/// correct one — the one outcome this whole mechanism exists to prevent.
///
/// A version line with no digit in it is left whole. `bwa` prints
/// `[main] unrecognized command '--version'`, which D42 found the engine
/// recording as a version; a claim against that is wrong rather than
/// accidentally right.
#[must_use]
pub fn version_states(tool: &str, recorded: &str, stated: &str) -> bool {
    let first = recorded.lines().next().unwrap_or(recorded).trim();
    let stated = stated.trim();
    if stated.is_empty() {
        return false;
    }

    let mut candidates = vec![first];
    // The tool's own name, where it printed it.
    if let Some(head) = first.get(..tool.len())
        && head.eq_ignore_ascii_case(tool)
    {
        candidates.push(first[tool.len()..].trim_start());
    }
    // Or some other name. Everything before the *first* digit, so what is
    // dropped can hold no digit by construction.
    if let Some(at) = first.find(|character: char| character.is_ascii_digit())
        && at > 0
    {
        candidates.push(first[at..].trim_start());
    }

    candidates.iter().any(|held| states(held, stated))
}

/// Whether one version string states another, allowing a build suffix.
fn states(held: &str, stated: &str) -> bool {
    if held == stated {
        return true;
    }
    held.strip_prefix(stated)
        .and_then(|rest| rest.chars().next())
        .is_some_and(|next| !next.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_matched_past_the_tools_own_name() {
        assert!(version_states("samtools", "samtools 1.24", "1.24"));
        assert!(version_states("SAMTOOLS", "samtools 1.24", "1.24"));
        assert!(version_states("minimap2", "2.31-r1287", "2.31"));
    }

    #[test]
    fn a_version_is_matched_past_a_name_that_is_not_the_tools() {
        // `python3 --version` prints `Python`, not `python3`, and this is
        // what a real run of the test suite met.
        assert!(version_states("python3", "Python 3.15.0", "3.15"));
        assert!(version_states("python3", "Python 3.15.0", "3.15.0"));
        assert!(!version_states("python3", "Python 3.15.0", "2.7"));
        assert!(
            !version_states("python3", "Python 3.15.0", "3.1"),
            "3.1 must not claim 3.15"
        );
    }

    #[test]
    fn a_whole_version_line_states_itself() {
        // A paper may well quote the line as printed.
        assert!(version_states("samtools", "samtools 1.24", "samtools 1.24"));
        assert!(!version_states("samtools", "samtools 1.24", "samtools 1.2"));
    }

    #[test]
    fn a_version_stated_as_nothing_states_nothing() {
        assert!(!version_states("samtools", "samtools 1.24", ""));
        assert!(!version_states("samtools", "samtools 1.24", "   "));
    }

    #[test]
    fn only_a_tools_first_line_is_its_version() {
        assert!(version_states(
            "samtools",
            "samtools 1.24\nUsing htslib 1.24\n",
            "1.24"
        ));
    }

    #[test]
    fn a_prefix_must_end_at_a_component_boundary() {
        assert!(
            !version_states("minimap2", "2.41-r1287", "2.4"),
            "2.4 must not claim 2.41"
        );
        assert!(!version_states("samtools", "samtools 1.24", "1.2"));
        assert!(!version_states("samtools", "samtools 1.24", "1.21"));
    }

    #[test]
    fn a_tool_that_prints_nothing_recognisable_is_compared_whole() {
        // `bwa` prints an unrecognised-command banner, which D42 found the
        // engine recording as a version. A claim against it is wrong rather
        // than accidentally right.
        assert!(!version_states(
            "bwa",
            "[main] unrecognized command '--version'",
            "0.7.18"
        ));
    }
}
