//! M0: the core crate reports its own version, and that version is the one
//! Cargo built it with.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[test]
fn version_matches_cargo_manifest() {
    assert_eq!(ligase::version(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn version_is_not_empty() {
    assert!(!ligase::version().is_empty());
}
