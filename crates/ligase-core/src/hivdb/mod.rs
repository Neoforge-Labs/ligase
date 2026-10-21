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
pub mod report;
pub mod ruleset;
pub mod score;

pub use codon::{Gene, Reading, read};
pub use ruleset::{Acids, Ruleset};
pub use score::{Assessment, Observed, Protein, interpret};

use std::path::Path;

use crate::load::LoadError;
use crate::note::Note;

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
/// If a file cannot be read, the algorithm is not one this engine ships,
/// or a score falls outside the algorithm's own ranges.
pub fn write_report(
    reference: &Path,
    variants: &Path,
    alignment: Option<&Path>,
    algorithm: &str,
    mixture: f64,
    depth: u32,
    out: &Path,
) -> Result<Vec<Note>, String> {
    let ruleset = Ruleset::vendored(algorithm)?;
    let reading = read(reference, variants, alignment, mixture, depth)
        .map_err(|error: LoadError| error.to_string())?;
    let assessments = interpret(&ruleset, &reading.observed)?;
    std::fs::write(out, report::render(&ruleset, &reading, &assessments))
        .map_err(|error| format!("{}: {error}", out.display()))?;
    Ok(reading.notes)
}
