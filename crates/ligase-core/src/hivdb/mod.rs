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
pub mod ruleset;
pub mod score;

pub use codon::{Gene, Reading, read};
pub use ruleset::{Acids, Ruleset};
pub use score::{Assessment, Observed, Protein, interpret};
