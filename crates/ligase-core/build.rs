//! What build this is, asked of git at compile time.
//!
//! `CARGO_PKG_VERSION` is read from the manifest, and the manifest is bumped
//! by a release commit — so for every commit between one release and the
//! next, a source build reports the number of the release it *followed*.
//! That string is not only printed: it is the engine's half of a `hivdb`
//! step's tool version, so it reaches the cache key, and a branch build that
//! answers differently from the published release shared a key with it. One
//! consumer measured exactly that (#45): a build of the commit carrying the
//! scored-codon floor reported `read by ligase 0.3.0` while answering
//! differently from the published 0.3.0.
//!
//! So this script asks git what the source actually is and hands the answer
//! to [`ligase::build`], which composes the version string. Nothing here
//! interprets it: the composition is a pure function in the library, where it
//! can be tested without a repository.
//!
//! # What it cannot know
//!
//! Three limits, written down rather than discovered later.
//!
//! * **No git, no answer.** A build from an sdist or from crates.io has no
//!   repository to ask, and falls back to the manifest version. For a release
//!   that is correct; for a snapshot tarball of a post-release commit it is
//!   the old behaviour, and `LIGASE_GIT_DESCRIBE` is how a packager says so.
//! * **The marker can go stale.** Cargo reruns a build script when a file it
//!   was told to watch changes, so `HEAD` and the current branch's ref are
//!   watched. A commit that moves neither — and the tree becoming dirty — can
//!   leave the marker a commit behind until the crate is rebuilt for another
//!   reason.
//! * **It is not a hash of the binary.** Two builds of the same commit with
//!   different features or toolchains report the same string, which is the
//!   third option #45 lists and this is not.

use std::path::Path;
use std::process::Command;

/// Where git keeps the ref `HEAD` names, relative to the git directory.
const PACKED_REFS: &str = "packed-refs";

fn main() {
    // A packager building outside a checkout can state the description
    // instead, and gets the same treatment as if git had answered.
    println!("cargo::rerun-if-env-changed=LIGASE_GIT_DESCRIBE");
    let described = match std::env::var("LIGASE_GIT_DESCRIBE") {
        Ok(stated) if !stated.trim().is_empty() => Some(stated),
        _ => {
            watch_the_checkout();
            describe()
        }
    };

    if let Some(described) = described {
        println!("cargo::rustc-env=LIGASE_GIT_DESCRIBE={}", described.trim());
    }
}

/// What `git describe` says this source is, or `None` if nothing answered.
///
/// `--dirty=.dirty` rather than the default `-dirty`, so that the marker
/// composes into a local version part by joining on `.` — and `--always` so
/// that a shallow clone with no tags still names its commit.
fn describe() -> Option<String> {
    let output = Command::new("git")
        .args([
            "describe",
            "--tags",
            "--always",
            "--dirty=.dirty",
            "--match",
            "v[0-9]*",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let described = String::from_utf8(output.stdout).ok()?;
    let described = described.trim();
    (!described.is_empty()).then(|| described.to_owned())
}

/// Ask Cargo to rerun this script when the checkout moves.
///
/// `HEAD` covers a checkout or a branch switch, the ref it names covers a
/// commit on the current branch, and `packed-refs` covers the tagging that
/// turns this build into a release build. A worktree's `HEAD` is in its own
/// git directory while its refs are in the main one, so both are asked for
/// by name rather than assumed to be `.git`.
fn watch_the_checkout() {
    let Some(git_dir) = git_path(&["--git-dir"]) else {
        return;
    };
    let git_dir = Path::new(&git_dir);
    watch(&git_dir.join("HEAD"));
    watch(&git_dir.join(PACKED_REFS));

    if let Some(common) = git_path(&["--git-common-dir"]) {
        let common = Path::new(&common);
        if common != git_dir {
            watch(&common.join(PACKED_REFS));
        }
        if let Some(reference) = git_path(&["--symbolic-full-name", "HEAD"]) {
            watch(&common.join(&reference));
        }
    }
}

/// One path `git rev-parse` resolves, if git answers with one.
fn git_path(arguments: &[&str]) -> Option<String> {
    let mut command = Command::new("git");
    command.arg("rev-parse");
    command.args(arguments);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    (!path.is_empty()).then(|| path.to_owned())
}

/// Watch one file, if it is there.
///
/// A path that does not exist is watched by some Cargo versions and ignored
/// by others, and an absent `packed-refs` is the ordinary case in a fresh
/// clone — so the check is here rather than left to Cargo.
fn watch(path: &Path) {
    if path.exists() {
        println!("cargo::rerun-if-changed={}", path.display());
    }
}
