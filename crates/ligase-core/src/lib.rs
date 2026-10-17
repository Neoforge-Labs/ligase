//! Core engine for Ligase: biological types, dimension checking, graph,
//! cache, execution and provenance.
//!
//! This crate holds the correctness-critical logic. It knows nothing about
//! Python; the bindings live in the `ligase-py` crate.
//!
//! # The idea
//!
//! Biological data carries *dimensions* — organism, reference build,
//! coordinate system, sample set — and the engine refuses to connect steps
//! whose dimensions disagree, the way a physics engine refuses to add metres
//! to seconds.
//!
//! ```no_run
//! use ligase::check::{Binding, check_step};
//! use ligase::load::{self, Declared};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let hxb2 = load::reference(
//!     "refs/HXB2.fasta".as_ref(),
//!     &Declared {
//!         organism: Some("HIV-1".into()),
//!         build: Some("HXB2".into()),
//!         ..Declared::default()
//!     },
//! )?;
//! let aligned = load::alignment("out/sample.bam".as_ref(), &Declared::default())?;
//!
//! for error in check_step(
//!     "call",
//!     &[
//!         Binding { name: "aln".into(), dataset: aligned },
//!         Binding { name: "ref".into(), dataset: hxb2 },
//!     ],
//! ) {
//!     eprintln!("{error}");
//! }
//! # Ok(())
//! # }
//! ```

pub mod cache;
pub mod check;
pub mod dim;
pub mod exec;
pub mod graph;
pub mod hash;
pub mod key;
pub mod load;
pub mod types;

/// The version of the core engine, as compiled.
///
/// This is the single source of truth for the version reported by the
/// Python package, so a wheel can never report a version it was not
/// built from.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
