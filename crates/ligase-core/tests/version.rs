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

#[test]
fn a_build_identifies_itself_beyond_the_release_it_followed() {
    // #45: `CARGO_PKG_VERSION` is the *last released* number for every
    // commit between one release and the next, so a source build of a
    // post-release commit claimed a version whose published behaviour
    // differs from its own -- and the claim reached the cache key, where
    // two engines that disagree about a result shared one.
    let build = ligase::build();

    assert!(build.starts_with(ligase::version()), "{build}");
    assert!(!build.contains(char::is_whitespace), "{build}");
    // Either this is a release build, or the marker is a local version
    // part, which keeps the string a version a methods paragraph can print.
    assert!(
        build == ligase::version() || build[ligase::version().len()..].starts_with('+'),
        "{build}"
    );
}

#[test]
fn the_rule_fingerprint_names_the_build_and_not_the_release() {
    // The failure #45 measured: a branch build answering differently from
    // the published release reported `read by ligase 0.3.0`, so a shared
    // store served one engine's answer to the other.
    let fingerprint = ligase::hivdb::ruleset::fingerprint().expect("the rules parse");

    assert!(
        fingerprint.ends_with(&format!("read by ligase {}", ligase::build())),
        "{fingerprint}"
    );
}

#[test]
fn the_engine_as_a_tool_reports_the_build_it_is() {
    // `tool="ligase"` is what the built-in conversions declare, and a tool
    // version is hashed into a step key.
    assert_eq!(
        ligase::exec::tool_version("ligase").expect("the engine knows its own version"),
        format!("ligase {}", ligase::build())
    );
}
