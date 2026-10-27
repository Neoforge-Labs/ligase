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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// When the computation happened, which is not when this run was.
    pub recorded_at: i64,
    /// The platform it happened on.
    pub platform: String,
}

/// What became of one claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimVerdict {
    /// Every check passed: the number rests on a computation this write-up
    /// accounts for.
    Bound,
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
            Self::Unbacked(_) => "unbacked",
            Self::Contradicted { .. } => "contradicted",
        }
    }

    /// Whether the write-up can stand behind the claim.
    #[must_use]
    pub fn is_bound(&self) -> bool {
        matches!(self, Self::Bound)
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
}

/// One claim, and what the run had to say about it.
#[derive(Clone, Debug, PartialEq, Eq)]
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
/// Textual, and never numeric. Two things make equality the wrong test and
/// both are in the citation table's own tools: `samtools --version` prints
/// `samtools 1.24`, so the tool's name has to come off first, and
/// `minimap2 --version` prints `2.31-r1287`, so a paper that says `2.31` is
/// right and an equality test would call it wrong.
///
/// A prefix must end at a component boundary. Without that rule `2.4` would
/// claim `2.41`, which is a wrong version reported as a correct one — the
/// one outcome this whole mechanism exists to prevent.
#[must_use]
pub fn version_states(tool: &str, recorded: &str, stated: &str) -> bool {
    let first = recorded.lines().next().unwrap_or(recorded).trim();
    let held = match first
        .get(..tool.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(tool))
    {
        true => first[tool.len()..].trim_start(),
        false => first,
    };
    let stated = stated.trim();

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
