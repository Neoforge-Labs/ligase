//! Interpreting variants as drug resistance.
//!
//! Everything above this module is about keeping data straight. This is
//! the one module that says what the data *means*: it takes variant calls
//! against HXB2, works out which amino acids each codon of protease,
//! reverse transcriptase, integrase and capsid holds, and scores them
//! against a vendored Stanford HIVdb algorithm.
//!
//! # The three parts
//!
//! - [`ruleset`] reads a vendored algorithm. The file is derived from
//!   Stanford's ASI2 XML, which is committed beside it and pinned by
//!   checksum; see `crates/ligase-core/rulesets/README.md`.
//! - [`codon`] turns a reference and a VCF into the amino acids present at
//!   each position of each protein. This is where mixtures and coverage
//!   are decided.
//! - [`score`] evaluates those amino acids against the algorithm.
//!
//! # Why nothing here reaches the network
//!
//! Stanford runs a web service that does this interpretation. Calling it
//! would make a result depend on the day it was asked for, which no cache
//! key can see and no bundle can reproduce. The algorithm is vendored
//! instead, version and date and checksum and all, so a run from 2026
//! and a rerun in 2030 agree or explain themselves.

pub mod codon;
pub mod frame;
pub mod report;
pub mod ruleset;
pub mod score;

pub use codon::{Gene, Reading, read};
pub use ruleset::{Acids, Ruleset};
pub use score::{Assessment, Observed, Protein, interpret};

use std::fmt;
use std::path::Path;

use crate::load::LoadError;
use crate::note::Note;

/// Why a report could not be written.
///
/// Two variants rather than one string because the two mean different
/// things to a caller: a frame mismatch is the reference being in the wrong
/// coordinate system, which a Python caller catches as a
/// `ReferenceFrameError` and fixes by putting the sequence back where its
/// numbers mean what they say. Everything else is a file, an algorithm name
/// or a score out of range.
#[derive(Clone, Debug)]
pub enum ReportError {
    /// The reference is not the frame [`codon::GENES`] is stated in.
    Frame(String),
    /// Anything else, already phrased for a user.
    Other(String),
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frame(detail) | Self::Other(detail) => f.write_str(detail),
        }
    }
}

impl std::error::Error for ReportError {}

/// Interpret one sample's calls and write the report.
///
/// This is the whole of what a `resistance` step does: read the
/// reference and the calls into amino acids, score them against a
/// vendored algorithm, and write the result.
///
/// # Arguments
///
/// - `reference` — the FASTA the variants were called against. Codons
///   cannot be named without it: a VCF holds only the positions that
///   differ, and two of the three bases of a codon are usually not among
///   them.
/// - `variants` — the calls.
/// - `alignment` — the BAM they were called from, for coverage. Without
///   it the reading notes that it is treating every uncalled position as
///   reference, and the caller is expected to hand that note back to the
///   engine (D32).
/// - `algorithm` — which vendored algorithm to use, e.g. `HIVDB_9.8`.
/// - `mixture` — the fraction an allele must reach to count as present.
/// - `depth` — the depth a position must reach to count as read.
/// - `gene_coverage` — the share of a gene's codons that must be read for
///   the gene to be assessed rather than reported as not assessed.
///
/// # Returns
///
/// What the reading noticed about itself: the hedges with a clinical
/// direction that the report it is about cannot state. A caller that drops
/// these has made the assumption silent, which is the one thing it must
/// not be (D29).
///
/// # Errors
///
/// If a file cannot be read, the reference cannot be the frame the gene
/// windows are stated in, the algorithm is not one this engine ships, or a
/// score falls outside the algorithm's own ranges.
#[expect(
    clippy::too_many_arguments,
    reason = "each is a separately documented judgement that belongs in the cache key; bundling them would hide them from the Python signature this mirrors"
)]
pub fn write_report(
    reference: &Path,
    variants: &Path,
    alignment: Option<&Path>,
    algorithm: &str,
    mixture: f64,
    depth: u32,
    gene_coverage: f64,
    out: &Path,
) -> Result<Vec<Note>, ReportError> {
    let ruleset = Ruleset::vendored(algorithm).map_err(ReportError::Other)?;
    let reading = read(
        reference,
        variants,
        alignment,
        mixture,
        depth,
        gene_coverage,
    )
    .map_err(|error| match &error {
        LoadError::Frame { .. } => ReportError::Frame(error.to_string()),
        _ => ReportError::Other(error.to_string()),
    })?;
    let assessments = interpret(&ruleset, &reading.observed).map_err(ReportError::Other)?;
    std::fs::write(out, report::render(&ruleset, &reading, &assessments))
        .map_err(|error| ReportError::Other(format!("{}: {error}", out.display())))?;
    Ok(reading.notes)
}
