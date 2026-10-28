//! M14: what a default build is *not* allowed to contain.
//!
//! D6 says the default install is zero-config, and D43 says the remote
//! store sits behind a cargo feature so that a user who wants none of it
//! acquires no dependency on it. That is a claim about the dependency
//! graph, so it is checked against the dependency graph.
//!
//! A sentence in the docs would say the same thing and would not notice
//! the day someone adds `remote-store` to `default` — which is exactly how
//! an install story is lost: not by a decision, by a convenience.
//!
//! The whole file is about the build *without* the feature, so it compiles
//! to nothing when the feature is on. CI runs both.
#![cfg(not(feature = "remote-store"))]
#![allow(clippy::unwrap_used, clippy::expect_used)]

/// Everything the remote tier drags in, named individually so that a
/// failure says which one arrived rather than only that something did.
///
/// `aws-lc-sys` is the one that matters most: it has a non-optional
/// build-dependency on the `cmake` crate, so its presence in a default
/// build would mean `pip install ligase-bio` had started needing cmake.
const FORBIDDEN: [&str; 6] = [
    "object_store",
    "aws-lc-sys",
    "aws-lc-rs",
    "tokio",
    "reqwest",
    "hyper",
];

#[test]
fn the_default_feature_set_pulls_no_object_store() {
    let output = std::process::Command::new(env!("CARGO"))
        .args([
            "tree",
            "--package",
            "ligase",
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--no-dedupe",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let tree = String::from_utf8_lossy(&output.stdout);
    let arrived: Vec<&str> = FORBIDDEN
        .iter()
        .copied()
        .filter(|crate_name| {
            tree.lines()
                .any(|line| line.split_whitespace().next() == Some(crate_name))
        })
        .collect();

    assert!(
        arrived.is_empty(),
        "a default build of ligase now pulls {}. The remote store is behind \
         the `remote-store` feature precisely so that it does not: \
         `aws-lc-sys` needs cmake at build time, and a wheel that needs \
         cmake does not build on a machine without it (D6, D11, D43).",
        arrived.join(", ")
    );
}

#[test]
fn a_bucket_url_in_a_default_build_says_so() {
    // The done-when clause. A user whose URL is right and whose build is
    // wrong must be sent to the build, by name.
    let dir = tempfile::tempdir().unwrap();
    let error = ligase::store::TieredStore::open(dir.path(), Some("s3://lab-cache"))
        .expect_err("a default build cannot open a bucket")
        .to_string();

    assert!(error.contains("remote-store"), "{error}");
    assert!(error.contains("ligase-bio[remote-store]"), "{error}");
    assert!(
        error.contains("file://"),
        "{error} does not say what this build *can* do"
    );
}

#[test]
fn cluster_execution_needs_no_feature_and_no_new_dependency() {
    // M18's done-when. Workers coordinate through published rows and
    // `std::time`, so a `file://` store on a shared filesystem -- which is
    // most of HPC -- runs a cluster in a default build. If this ever needs
    // the feature, the install story has quietly acquired cmake.
    let dir = tempfile::tempdir().unwrap();
    let shared = dir.path().join("shared");
    std::fs::create_dir_all(&shared).unwrap();

    let cache = ligase::cache::Cache::open_with(
        &dir.path().join(".ligase"),
        Some(&format!("file://{}", shared.display())),
    )
    .expect("a default build can share a store on a filesystem");

    let key = ligase::key::Ingredients {
        step: "align".to_owned(),
        code: "def align(): ...".to_owned(),
        recipe: "sh:true".to_owned(),
        output: ligase::types::DataKind::Alignment,
        parameters: Vec::new(),
        inputs: Vec::new(),
        tools: Vec::new(),
    }
    .key();

    let worker = ligase::worker::Worker::named("node01");
    let shared = cache
        .objects()
        .shared()
        .expect("a file:// store is the tier every worker sees");
    assert!(matches!(
        ligase::store::lease::acquire(shared, key, &worker, 9999, 0).unwrap(),
        ligase::store::lease::Claimed::Acquired { .. }
    ));
}
