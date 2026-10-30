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
//!         organism: Some(vec!["HIV-1".into()]),
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

pub mod audit;
pub mod cache;
pub mod canon;
pub mod check;
pub mod convert;
pub mod dim;
pub mod exec;
pub mod graph;
pub mod hash;
pub mod hivdb;
pub mod inspect;
pub mod key;
pub mod load;
pub mod note;
pub mod prov;
pub mod remote;
pub mod store;
pub mod types;
pub mod worker;

/// The version of the core engine, as compiled.
///
/// This is the single source of truth for the version reported by the
/// Python package, so a wheel can never report a version it was not
/// built from.
///
/// The *release* number, which is what a wheel is published under and what
/// `ligase-bio>=0.4.0` resolves against. What this build is — which between
/// releases is not the same question — is [`build`].
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// What this build is: the release version, and the commit where it is not
/// one.
///
/// `0.4.0` from a release build, `0.4.0+13.g0c5980e` from thirteen commits
/// after it, and `.dirty` on the end from a modified tree.
///
/// # Why this exists beside [`version`]
///
/// Because the manifest's number is bumped by a release commit, so every
/// commit between one release and the next reports the number of the release
/// it followed — and that string is not only printed. It is the engine's
/// half of a `hivdb` step's tool version (D20), so it reaches the cache key,
/// and a branch build that answers differently from the published release
/// shared a key with it: the store serves one engine's answer to the other,
/// with a provenance record asserting the version that did not produce it.
/// A consumer measured that on a build of the commit carrying the
/// scored-codon floor, which reported `read by ligase 0.3.0` while answering
/// differently from the published 0.3.0.
///
/// So this is what the engine reports **about itself as software that ran**:
/// the rule fingerprint, `tool="ligase"`, the provenance record and the
/// self-citation. [`version`] stays the release number, because a wheel's
/// metadata and a dependency floor are about releases and must not acquire a
/// build marker.
///
/// The marker is a local version part — after a `+` — so the whole string
/// remains a version a methods paragraph can print and a reader can resolve.
///
/// A build with no repository to ask, from an sdist or from crates.io, is
/// the release version: see `build.rs` for what that cannot know.
#[must_use]
pub fn build() -> &'static str {
    static BUILD: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    BUILD
        .get_or_init(|| build_version(version(), option_env!("LIGASE_GIT_DESCRIBE")))
        .as_str()
}

/// How a build's description composes into a version string.
///
/// Separated from the build script so that it can be tested without a
/// repository: every case below is one a release or a working tree really
/// produces, and none of them is reachable from a test that has to `git tag`
/// to get there.
fn build_version(manifest: &str, described: Option<&str>) -> String {
    /// What the build script asks `git describe` to mark a modified tree
    /// with, chosen so that it joins onto the rest with a `.`.
    const DIRTY: &str = ".dirty";

    let Some(described) = described.map(str::trim).filter(|text| !text.is_empty()) else {
        return manifest.to_owned();
    };
    let (described, dirty) = match described.strip_suffix(DIRTY) {
        Some(clean) => (clean, true),
        None => (described, false),
    };

    // At the tag whose name this release carries, which is the one case
    // where the manifest's number is the whole truth. The `v` is optional
    // because the tag convention is the repository's rather than this
    // function's.
    let released = described == manifest || described.strip_prefix('v') == Some(manifest);

    let mut marker = Vec::new();
    if !released {
        // `v0.4.0-13-g0c5980e` is thirteen commits past `v0.4.0`; a bare
        // `0c5980e` is what `--always` falls back to where no tag is in
        // reach. Both end in the commit, which is the part that makes the
        // string differ whenever the source does.
        match described.rsplit_once("-g") {
            Some((before, commit)) => match before.rsplit_once('-') {
                Some((_, distance)) => marker.push(format!("{distance}.g{commit}")),
                None => marker.push(format!("g{commit}")),
            },
            None => marker.push(format!("g{described}")),
        }
    }
    if dirty {
        marker.push("dirty".to_owned());
    }

    if marker.is_empty() {
        manifest.to_owned()
    } else {
        format!("{manifest}+{}", marker.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_at_the_release_tag_is_the_release() {
        assert_eq!(build_version("0.4.0", Some("v0.4.0")), "0.4.0");
        assert_eq!(build_version("0.4.0", Some("0.4.0")), "0.4.0");
    }

    #[test]
    fn a_build_after_a_release_names_the_commit_it_is() {
        assert_eq!(
            build_version("0.4.0", Some("v0.4.0-13-g0c5980e")),
            "0.4.0+13.g0c5980e"
        );
    }

    #[test]
    fn a_release_commit_that_is_not_tagged_yet_is_not_a_release() {
        // The commit that bumps the manifest to 0.5.0 sits one past
        // `v0.4.0` and is tagged afterwards. Until it is, a build of it is
        // not the published 0.5.0 and does not say it is.
        assert_eq!(
            build_version("0.5.0", Some("v0.4.0-1-g8d94323")),
            "0.5.0+1.g8d94323"
        );
    }

    #[test]
    fn a_modified_tree_cannot_claim_a_release() {
        assert_eq!(build_version("0.4.0", Some("v0.4.0.dirty")), "0.4.0+dirty");
        assert_eq!(
            build_version("0.4.0", Some("v0.4.0-13-g0c5980e.dirty")),
            "0.4.0+13.g0c5980e.dirty"
        );
    }

    #[test]
    fn a_checkout_with_no_tag_in_reach_still_names_its_commit() {
        assert_eq!(build_version("0.4.0", Some("0c5980e")), "0.4.0+g0c5980e");
    }

    #[test]
    fn a_build_with_nothing_to_ask_is_the_manifest_version() {
        // An sdist or a crates.io build: no repository, so no marker. For a
        // release that is the truth, and `build.rs` says what it cannot
        // know.
        assert_eq!(build_version("0.4.0", None), "0.4.0");
        assert_eq!(build_version("0.4.0", Some("  ")), "0.4.0");
    }

    #[test]
    fn the_marker_is_a_local_version_part() {
        // So that the string stays printable in a methods paragraph and
        // valid where a version is expected: everything the engine adds
        // goes after a single `+`.
        for described in ["v0.4.0-13-g0c5980e", "v0.4.0.dirty", "0c5980e"] {
            let composed = build_version("0.4.0", Some(described));
            assert!(composed.starts_with("0.4.0+"), "{composed}");
            assert_eq!(composed.matches('+').count(), 1, "{composed}");
            assert!(!composed.contains(char::is_whitespace), "{composed}");
        }
    }
}
