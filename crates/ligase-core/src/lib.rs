//! Core engine for Ligase: biological types, dimension checking, graph,
//! cache, execution and provenance.
//!
//! This crate holds the correctness-critical logic. It knows nothing about
//! Python; the bindings live in the `ligase-py` crate.

/// The version of the core engine, as compiled.
///
/// This is the single source of truth for the version reported by the
/// Python package, so a wheel can never report a version it was not
/// built from.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
