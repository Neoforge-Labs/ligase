//! Writing a resistance report.
//!
//! The format is Ligase's own, and it is modelled deliberately closely on
//! the report Stanford's own service produces: one row per drug, with the
//! score, the level, the wording and the mutations that got it there. That
//! is not imitation for its own sake. M6 is signed off by comparing this
//! file against published Stanford reports for the same samples, and a
//! comparison is only as good as the thing it lines up against.
//!
//! Two columns exist that Stanford's does not, and both are here because
//! this engine knows something a sequence-based service cannot:
//!
//! - `unread` names the positions a drug's rules mention that the data
//!   never covered. Stanford is handed a consensus sequence and has no way
//!   to know which of its positions were guessed; this engine was handed
//!   an alignment and does.
//! - `algorithm` names the rules, their version, their date and the digest
//!   of their bytes, so a report says what it was interpreted by without
//!   anyone having to remember.

use std::fmt::Write as _;

use super::codon::Reading;
use super::ruleset::Ruleset;
use super::score::Assessment;

/// The columns a report carries, in order.
///
/// The header is part of what a report says, so the canonicaliser keeps it
/// as a row and sorts only what is under it
/// (`crate::canon::report`). Changing this list changes every report's
/// canonical hash, which is correct: a report with different columns is a
/// different report.
pub const COLUMNS: &[&str] = &[
    "gene",
    "drug_class",
    "drug",
    "drug_name",
    "score",
    "level",
    "interpretation",
    "mutations",
    "unread",
    "algorithm",
];

/// Render a report.
///
/// Rows come out ordered by drug class and then by drug, which is how a
/// clinician reads one. The canonical form sorts them anyway, so this
/// ordering is for the person rather than for the hash.
#[must_use]
pub fn render(ruleset: &Ruleset, reading: &Reading, assessments: &[Assessment]) -> String {
    let mut out = String::new();
    out.push_str(&COLUMNS.join("\t"));
    out.push('\n');

    let algorithm = ruleset.describe();
    for assessment in assessments {
        let mutations: Vec<String> = assessment
            .mutations
            .iter()
            .map(|(position, acids)| {
                let reference = reading
                    .reference
                    .get(&(assessment.gene.clone(), *position))
                    .copied()
                    .unwrap_or('X');
                // `M184V`, and `K219KE` for a mixture: the reference
                // amino acid, the position, then every amino acid
                // present -- the reference one first where it is still
                // there, the rest alphabetically. That is how Stanford
                // writes them, so the two reports can be diffed without
                // translating between notations.
                let present = acids.to_string();
                let rest: String = present.chars().filter(|&a| a != reference).collect();
                let ordered = if present.contains(reference) {
                    format!("{reference}{rest}")
                } else {
                    rest
                };
                format!("{reference}{position}{ordered}")
            })
            .collect();

        let unread = assessment
            .unread
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");

        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            assessment.gene,
            assessment.class,
            assessment.drug,
            assessment.name,
            assessment
                .score
                .map_or_else(String::new, |score| score.to_string()),
            assessment
                .level
                .map_or_else(String::new, |level| level.to_string()),
            assessment.interpretation,
            mutations.join(","),
            unread,
            algorithm,
        );
    }
    out
}
