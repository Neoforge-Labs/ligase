//! M10: getting the bytes, and what happens when that goes wrong.
//!
//! Everything here runs against a real socket on localhost
//! (`common::mock_http`), because the failures worth testing are the ones a
//! stub cannot produce: a server that is down, a redirect chain, a status
//! that should be retried against one that should not.
//!
//! The client is deliberately dull. What it must get right is not speed but
//! restraint: never retry something that will not change, never follow a
//! redirect forever, never buffer a multi-gigabyte run into memory, and
//! never report success for bytes it did not finish reading.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;
use std::time::Duration;

use common::mock_http::{MockServer, Reply};
use ligase::remote::fetch::{FetchError, Network, Policy, Transport};
use ligase::remote::request::Request;
use tempfile::TempDir;

/// Fast retries, so a test that exercises them does not sit waiting.
fn quick() -> Policy {
    Policy {
        attempts: 3,
        backoff: Duration::from_millis(1),
        ..Policy::default()
    }
}

fn staging(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(name)
}

// ---------------------------------------------------------- the happy path --

#[test]
fn a_two_hundred_is_streamed_to_the_staged_path() {
    let server = MockServer::start();
    server.on("/HXB2.fasta", b">K03455.1 HIV-1\nACGTACGT\n");
    let dir = tempfile::tempdir().unwrap();
    let into = staging(&dir, "staged.fasta");

    let fetched = Network::new(quick())
        .fetch(&Request::get(&server.url("/HXB2.fasta")), &into)
        .unwrap();

    assert_eq!(fetched.status, 200);
    assert_eq!(fetched.bytes, 25);
    assert_eq!(
        fs::read(&into).unwrap(),
        b">K03455.1 HIV-1\nACGTACGT\n".to_vec()
    );
}

#[test]
fn a_fetch_is_one_request_and_no_more() {
    let server = MockServer::start();
    server.on("/x", b"once");
    let dir = tempfile::tempdir().unwrap();

    Network::new(quick())
        .fetch(&Request::get(&server.url("/x")), &staging(&dir, "x"))
        .unwrap();

    assert_eq!(server.hits("/x"), 1);
}

// ------------------------------------------------------------- statuses --

#[test]
fn a_four_oh_four_names_the_url_and_the_status() {
    let server = MockServer::start();
    let dir = tempfile::tempdir().unwrap();
    let url = server.url("/missing.fasta");

    let error = Network::new(quick())
        .fetch(&Request::get(&url), &staging(&dir, "x"))
        .unwrap_err();

    let message = error.to_string();
    assert!(matches!(error, FetchError::Status { .. }), "{message}");
    assert!(message.contains("404"), "{message}");
    assert!(
        message.contains(&url),
        "the URL is the actionable half of the message: {message}"
    );
}

#[test]
fn nothing_is_retried_on_a_four_hundred() {
    let server = MockServer::start();
    server.replies("/gone", vec![Reply::status(404)]);
    let dir = tempfile::tempdir().unwrap();

    let _ = Network::new(quick()).fetch(&Request::get(&server.url("/gone")), &staging(&dir, "x"));

    // A 404 will not become a 200 on a second ask. Retrying one wastes time
    // and, against a rate-limited API, spends a budget the next real
    // request needs.
    assert_eq!(server.hits("/gone"), 1);
}

#[test]
fn a_five_hundred_is_retried_and_a_later_success_is_taken() {
    let server = MockServer::start();
    server.replies(
        "/flaky",
        vec![
            Reply::status(503),
            Reply::status(503),
            Reply::ok(b"at last"),
        ],
    );
    let dir = tempfile::tempdir().unwrap();
    let into = staging(&dir, "flaky");

    Network::new(quick())
        .fetch(&Request::get(&server.url("/flaky")), &into)
        .unwrap();

    assert_eq!(server.hits("/flaky"), 3);
    assert_eq!(fs::read(&into).unwrap(), b"at last".to_vec());
}

#[test]
fn a_rate_limit_is_retried() {
    let server = MockServer::start();
    server.replies("/busy", vec![Reply::status(429), Reply::ok(b"ok now")]);
    let dir = tempfile::tempdir().unwrap();

    Network::new(quick())
        .fetch(&Request::get(&server.url("/busy")), &staging(&dir, "busy"))
        .unwrap();

    assert_eq!(server.hits("/busy"), 2);
}

#[test]
fn a_server_that_never_recovers_fails_after_the_attempts_are_spent() {
    let server = MockServer::start();
    server.replies("/down", vec![Reply::status(500)]);
    let dir = tempfile::tempdir().unwrap();

    let error = Network::new(quick())
        .fetch(&Request::get(&server.url("/down")), &staging(&dir, "x"))
        .unwrap_err();

    assert!(matches!(error, FetchError::Status { .. }));
    assert_eq!(server.hits("/down"), 3, "three attempts, then the error");
}

// ------------------------------------------------------------ redirects --

#[test]
fn a_redirect_is_followed_and_the_final_url_is_what_is_recorded() {
    let server = MockServer::start();
    server.replies("/old", vec![Reply::redirect(&server.url("/new"))]);
    server.on("/new", b"moved here");
    let dir = tempfile::tempdir().unwrap();
    let into = staging(&dir, "x");

    let fetched = Network::new(quick())
        .fetch(&Request::get(&server.url("/old")), &into)
        .unwrap();

    assert_eq!(fs::read(&into).unwrap(), b"moved here".to_vec());
    // The locator is what the user asked for; the final URL is what
    // answered. Provenance wants both, and they are different facts.
    assert_eq!(fetched.url, server.url("/new"));
}

#[test]
fn a_redirect_loop_is_refused_rather_than_followed_forever() {
    let server = MockServer::start();
    server.replies("/a", vec![Reply::redirect(&server.url("/b"))]);
    server.replies("/b", vec![Reply::redirect(&server.url("/a"))]);
    let dir = tempfile::tempdir().unwrap();

    let error = Network::new(quick())
        .fetch(&Request::get(&server.url("/a")), &staging(&dir, "x"))
        .unwrap_err();

    assert!(
        matches!(
            error,
            FetchError::Unreachable { .. } | FetchError::Status { .. }
        ),
        "a loop must end in an error rather than a hang: {error}"
    );
}

// ------------------------------------------------------- server is down --

#[test]
fn a_server_that_is_down_is_an_error_not_a_panic() {
    let mut server = MockServer::start();
    let url = server.url("/x");
    server.shutdown();
    let dir = tempfile::tempdir().unwrap();

    let error = Network::new(Policy {
        attempts: 1,
        backoff: Duration::from_millis(1),
        ..Policy::default()
    })
    .fetch(&Request::get(&url), &staging(&dir, "x"))
    .unwrap_err();

    assert!(
        matches!(error, FetchError::Unreachable { .. }),
        "expected unreachable, got {error}"
    );
    assert!(error.to_string().contains(&url));
}

#[test]
fn a_failed_fetch_leaves_no_staged_file_behind() {
    let server = MockServer::start();
    let dir = tempfile::tempdir().unwrap();
    let into = staging(&dir, "never-arrived.fasta");

    let _ = Network::new(quick()).fetch(&Request::get(&server.url("/missing")), &into);

    assert!(
        !into.exists(),
        "a half-fetch must not look like a whole file to whatever runs next"
    );
}

// ----------------------------------------------------------- size limit --

#[test]
fn a_body_over_the_limit_is_refused_naming_the_limit() {
    let server = MockServer::start();
    server.on("/big", vec![b'A'; 4096]);
    let dir = tempfile::tempdir().unwrap();

    let error = Network::new(Policy {
        max_bytes: 1024,
        ..quick()
    })
    .fetch(&Request::get(&server.url("/big")), &staging(&dir, "big"))
    .unwrap_err();

    let message = error.to_string();
    assert!(matches!(error, FetchError::TooLarge { .. }), "{message}");
    assert!(message.contains("1024"), "{message}");
}

#[test]
fn a_body_at_exactly_the_limit_is_accepted() {
    let server = MockServer::start();
    server.on("/edge", vec![b'A'; 1024]);
    let dir = tempfile::tempdir().unwrap();

    let fetched = Network::new(Policy {
        max_bytes: 1024,
        ..quick()
    })
    .fetch(&Request::get(&server.url("/edge")), &staging(&dir, "edge"))
    .unwrap();

    assert_eq!(fetched.bytes, 1024);
}

// -------------------------------------------------- no trace of a secret --

#[test]
fn the_transport_has_nowhere_to_put_a_request_header() {
    // Not a filter but a shape: `Fetched` has no field for request headers,
    // so an `Authorization` token has nowhere to be recorded. The assertion
    // is over the debug rendering of the whole value, which is what would
    // leak if a field were added without thinking.
    let server = MockServer::start();
    server.on("/private", b"secret-free body");
    let dir = tempfile::tempdir().unwrap();

    let fetched = Network::new(quick())
        .fetch(&Request::get(&server.url("/private")), &staging(&dir, "x"))
        .unwrap();

    let rendered = format!("{fetched:?}");
    assert!(
        !rendered.to_lowercase().contains("authorization"),
        "{rendered}"
    );
    assert!(!rendered.to_lowercase().contains("header"), "{rendered}");
}
