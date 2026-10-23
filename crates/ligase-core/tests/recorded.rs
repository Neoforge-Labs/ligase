//! M10b: recording a query, replaying it, and refreshing it.
//!
//! A `RecordedQuery` promises the same bytes forever *from the record*,
//! because its source promises nothing. That needs exactly one thing — the
//! canonicalised response body goes into the content-addressed store, not
//! merely its digest — after which the body is an ordinary input with a
//! content hash and everything downstream is deterministic by construction
//! (D39).
//!
//! Everything here runs against a real socket on localhost, so "with the
//! server down" is a closed port rather than a stubbed error.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::mock_http::{MockServer, Reply};
use ligase::cache::Cache;
use ligase::remote::Locator;
use ligase::remote::fetch::{Network, Offline, Policy, Transport};
use ligase::remote::recorded::{self, Drift, MAX_RECORDED_BYTES, Mode, QueryError, Served};
use ligase::remote::request::{Auth, CanonicalRequest, Request};
use ligase::store::BlobStore;
use ligase::types::{DataKind, RemoteVerdict};
use tempfile::TempDir;

/// A gene list, as a TSV query endpoint would answer with one.
const GENES: &[u8] = b"gene\tscore\nTP53\t0.91\nBRCA1\t0.72\n";

/// The same endpoint after the source changed its mind.
const GENES_CHANGED: &[u8] = b"gene\tscore\nTP53\t0.91\nBRCA1\t0.55\n";

fn setup() -> (TempDir, Cache, MockServer) {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open(dir.path()).unwrap();
    (dir, cache, MockServer::start())
}

fn quick() -> Policy {
    Policy {
        attempts: 1,
        timeout: std::time::Duration::from_secs(5),
        ..Policy::default()
    }
}

/// Resolve a query at `url`, in `mode`.
fn resolve(
    url: &str,
    mode: Mode,
    cache: &Cache,
    transport: &dyn Transport,
) -> Result<recorded::ResolvedQuery, QueryError> {
    let locator = Locator::parse(url).unwrap();
    recorded::resolve(&locator, DataKind::EntitySet, None, mode, cache, transport)
}

/// Delete one object from the store, as a user clearing space would.
///
/// Objects are stored read-only, so making one writable is the only way to
/// remove it -- which is the point of the two tests that call this: a
/// record whose body has gone is a row that cannot be served.
#[expect(
    clippy::permissions_set_readonly_false,
    reason = "undoing the store's own read-only bit is the whole of what this does"
)]
fn empty_the_store_of(cache: &Cache, key: &ligase::store::ObjectKey) {
    let object = cache.objects().path_of(key);
    let mut permissions = std::fs::metadata(&object).unwrap().permissions();
    permissions.set_readonly(false);
    std::fs::set_permissions(&object, permissions).unwrap();
    std::fs::remove_file(&object).unwrap();
}

// --------------------------------------------------- record, then replay --

#[test]
fn replay_is_the_default_where_a_record_exists_and_record_where_it_does_not() {
    let (_dir, cache, server) = setup();
    server.on("/genes", GENES);
    let network = Network::new(quick());
    let url = server.url("/genes");

    let first = resolve(&url, Mode::Auto, &cache, &network).unwrap();
    assert_eq!(first.served, Served::Recorded);
    assert_eq!(server.hits("/genes"), 1);

    let second = resolve(&url, Mode::Auto, &cache, &network).unwrap();
    assert!(matches!(second.served, Served::Replayed { .. }));
    // The whole claim: the second run asked the source nothing.
    assert_eq!(server.hits("/genes"), 1);
    assert_eq!(first.digest, second.digest);
}

#[test]
fn a_record_is_replayed_with_the_server_down_and_no_transport_at_all() {
    let (_dir, cache, mut server) = setup();
    server.on("/genes", GENES);
    let url = server.url("/genes");

    let recorded_run = resolve(&url, Mode::Auto, &cache, &Network::new(quick())).unwrap();
    server.shutdown();

    // `Offline` cannot fetch anything, so a result here can only have come
    // out of the record.
    let replayed = resolve(&url, Mode::Auto, &cache, &Offline).unwrap();

    assert!(matches!(replayed.served, Served::Replayed { .. }));
    assert_eq!(replayed.digest, recorded_run.digest);
    assert_eq!(replayed.record.body, recorded_run.record.body);
}

#[test]
fn a_recorded_body_is_the_canonical_response_and_is_in_the_store() {
    let (_dir, cache, server) = setup();
    server.on("/genes", GENES);

    let resolved = resolve(
        &server.url("/genes"),
        Mode::Auto,
        &cache,
        &Network::new(quick()),
    )
    .unwrap();

    // Not merely the digest: the body itself, verifiable against its name.
    let held = cache.objects().checked_get(&resolved.record.body).unwrap();
    assert_eq!(held.as_deref(), Some(GENES));
    assert!(resolved.path.is_file());
}

#[test]
fn the_run_that_makes_a_record_is_live_and_every_later_one_is_replayed() {
    let (_dir, cache, server) = setup();
    server.on("/genes", GENES);
    let network = Network::new(quick());
    let url = server.url("/genes");

    let first = resolve(&url, Mode::Auto, &cache, &network).unwrap();
    let second = resolve(&url, Mode::Auto, &cache, &network).unwrap();

    assert_eq!(first.origin().verdict, RemoteVerdict::Live);
    assert_eq!(second.origin().verdict, RemoteVerdict::Replayed);
}

// ----------------------------------------------------------- strict replay --

#[test]
fn replay_mode_with_no_record_is_an_error_naming_the_locator() {
    let (_dir, cache, server) = setup();
    server.on("/genes", GENES);
    let url = server.url("/genes");

    let refused = resolve(&url, Mode::Replay, &cache, &Network::new(quick()));

    let Err(QueryError::NoRecord { locator, .. }) = refused else {
        panic!("expected a refusal, got {refused:?}");
    };
    assert_eq!(locator, url);
    // Never a silent live fetch: a half-replayed run is the
    // plausible-but-wrong result this engine exists to refuse.
    assert_eq!(server.hits("/genes"), 0);
}

#[test]
fn replay_mode_says_what_to_do_about_a_query_it_has_no_record_of() {
    let (_dir, cache, server) = setup();
    server.on("/genes", GENES);

    let refused = resolve(&server.url("/genes"), Mode::Replay, &cache, &Offline);

    let message = refused.unwrap_err().to_string();
    assert!(message.contains("--replay"), "{message}");
    assert!(message.contains("no record"), "{message}");
}

#[test]
fn a_record_whose_body_has_left_the_store_is_refetched_under_auto() {
    let (_dir, cache, server) = setup();
    server.on("/genes", GENES);
    let network = Network::new(quick());
    let url = server.url("/genes");

    let first = resolve(&url, Mode::Auto, &cache, &network).unwrap();
    // A store emptied by hand, which is the same situation a cold cache is
    // in: one fetch, never a wrong answer.
    empty_the_store_of(&cache, &first.record.body);

    let again = resolve(&url, Mode::Auto, &cache, &network).unwrap();

    assert_eq!(again.served, Served::Recorded);
    assert_eq!(server.hits("/genes"), 2);
    assert_eq!(again.digest, first.digest);
}

#[test]
fn a_record_whose_body_has_left_the_store_is_an_error_under_replay() {
    let (_dir, cache, server) = setup();
    server.on("/genes", GENES);
    let network = Network::new(quick());
    let url = server.url("/genes");

    let first = resolve(&url, Mode::Auto, &cache, &network).unwrap();
    empty_the_store_of(&cache, &first.record.body);

    let refused = resolve(&url, Mode::Replay, &cache, &network);

    assert!(matches!(refused, Err(QueryError::NoRecord { .. })));
    assert_eq!(server.hits("/genes"), 1);
}

// --------------------------------------------------- what is not recorded --

#[test]
fn a_five_hundred_in_record_mode_writes_no_record() {
    let (_dir, cache, server) = setup();
    server.replies("/flaky", vec![Reply::status(500)]);
    let url = server.url("/flaky");

    let failed = resolve(&url, Mode::Auto, &cache, &Network::new(quick()));

    assert!(matches!(failed, Err(QueryError::Fetch { .. })));
    // A record of a failure would replay the failure forever as though it
    // were an answer.
    let request = CanonicalRequest::of(&Request::get(&url)).hash();
    assert!(cache.query_record(request).unwrap().is_none());
}

#[test]
fn a_body_over_thirty_two_mebibytes_is_refused_saying_pin_this_as_a_release_instead() {
    let (_dir, cache, server) = setup();
    let huge = vec![b'x'; usize::try_from(MAX_RECORDED_BYTES).unwrap() + 1];
    server.on("/huge", &huge);

    let refused = resolve(
        &server.url("/huge"),
        Mode::Auto,
        &cache,
        &Network::new(Policy {
            max_bytes: MAX_RECORDED_BYTES,
            ..quick()
        }),
    );

    let message = refused.unwrap_err().to_string();
    assert!(
        message.contains("pin this as a release instead"),
        "the refusal must say what to do instead: {message}"
    );
}

const ERROR_PAGE: &[u8] = b"<html><body>Error: no such dataset</body></html>";

#[test]
fn a_body_that_will_not_read_as_its_kind_is_refused_rather_than_recorded() {
    // D39's "a 200 is not a promise about the body", which a query needs as
    // much as a pinned remote: an API asked for something it does not have
    // answers with an error page and a 200.
    let (_dir, cache, server) = setup();
    server.on("/hxb2", ERROR_PAGE);
    let url = server.url("/hxb2");
    let locator = Locator::parse(&url).unwrap();

    let refused = recorded::resolve(
        &locator,
        DataKind::Reference,
        None,
        Mode::Auto,
        &cache,
        &Network::new(quick()),
    );

    assert!(matches!(refused, Err(QueryError::Unreadable { .. })));
    let request = CanonicalRequest::of(&Request::get(&url)).hash();
    assert!(cache.query_record(request).unwrap().is_none());
}

#[test]
fn an_error_page_passes_the_kind_check_for_an_entity_set_and_this_is_why() {
    // Not a hole this milestone opened and not one it can close: a
    // single-column TSV *is* a valid gene list, so an HTML page is a gene
    // list of one implausible gene, and the same is true of
    // `lg.entity_set` on a local file and of `lg.remote`. The guard is only
    // ever as strong as the kind's parser.
    //
    // Pinned here rather than left to be discovered, so that the day the
    // entity-set loader learns to refuse an identifier with a `<` in it,
    // this test is what says so.
    let (_dir, cache, server) = setup();
    server.on("/genes", ERROR_PAGE);

    let recorded_anyway = resolve(
        &server.url("/genes"),
        Mode::Auto,
        &cache,
        &Network::new(quick()),
    );

    assert!(recorded_anyway.is_ok(), "{recorded_anyway:?}");
}

// ------------------------------------------------------------- pagination --

#[test]
fn paginated_calls_are_records_keyed_by_request_not_by_sequence() {
    let (_dir, cache, server) = setup();
    server.on("/genes?page=1", b"gene\tscore\nTP53\t0.91\n");
    server.on("/genes?page=2", b"gene\tscore\nBRCA1\t0.72\n");
    let network = Network::new(quick());
    let one = server.url("/genes?page=1");
    let two = server.url("/genes?page=2");

    // Recorded in one order ...
    let first = resolve(&one, Mode::Auto, &cache, &network).unwrap();
    let second = resolve(&two, Mode::Auto, &cache, &network).unwrap();

    // ... and replayed in the other.
    let replayed_two = resolve(&two, Mode::Auto, &cache, &Offline).unwrap();
    let replayed_one = resolve(&one, Mode::Auto, &cache, &Offline).unwrap();

    assert_eq!(replayed_two.digest, second.digest);
    assert_eq!(replayed_one.digest, first.digest);
    assert_ne!(first.digest, second.digest);
    assert_eq!(server.hits("/genes?page=1"), 1);
    assert_eq!(server.hits("/genes?page=2"), 1);
}

#[test]
fn two_urls_differing_only_in_parameter_order_replay_one_record() {
    let (_dir, cache, server) = setup();
    server.on("/genes?a=1&b=2", GENES);
    let network = Network::new(quick());

    resolve(&server.url("/genes?a=1&b=2"), Mode::Auto, &cache, &network).unwrap();
    // The same question, asked with its parameters the other way round.
    let replayed = resolve(&server.url("/genes?b=2&a=1"), Mode::Auto, &cache, &Offline).unwrap();

    assert!(matches!(replayed.served, Served::Replayed { .. }));
}

// ----------------------------------------------------------------- refresh --

#[test]
fn a_refresh_with_no_record_reports_a_first_record_not_drift() {
    let (_dir, cache, server) = setup();
    server.on("/genes", GENES);

    let refreshed = resolve(
        &server.url("/genes"),
        Mode::Refresh,
        &cache,
        &Network::new(quick()),
    )
    .unwrap();

    // Nothing to compare against is not a change, and a refresh that
    // reported every query as changed would teach you to ignore it.
    assert_eq!(
        refreshed.served,
        Served::Refreshed {
            drift: Drift::First
        }
    );
}

#[test]
fn a_refresh_against_a_changed_response_reports_the_drift_naming_both_records() {
    let (_dir, cache, server) = setup();
    server.replies("/genes", vec![Reply::ok(GENES), Reply::ok(GENES_CHANGED)]);
    let network = Network::new(quick());
    let url = server.url("/genes");

    let first = resolve(&url, Mode::Auto, &cache, &network).unwrap();
    let refreshed = resolve(&url, Mode::Refresh, &cache, &network).unwrap();

    let Served::Refreshed {
        drift:
            Drift::Changed {
                was,
                now,
                previous,
                revision,
                ..
            },
    } = refreshed.served
    else {
        panic!("expected drift, got {:?}", refreshed.served);
    };
    assert_eq!(was, first.record.body.hash);
    assert_eq!(now, refreshed.record.body.hash);
    assert_ne!(was, now);
    assert_eq!(previous, 1);
    assert_eq!(revision, 2);
}

#[test]
fn a_refresh_writes_a_new_dated_record_beside_the_old_one() {
    let (_dir, cache, server) = setup();
    server.replies("/genes", vec![Reply::ok(GENES), Reply::ok(GENES_CHANGED)]);
    let network = Network::new(quick());
    let url = server.url("/genes");

    resolve(&url, Mode::Auto, &cache, &network).unwrap();
    resolve(&url, Mode::Refresh, &cache, &network).unwrap();

    let request = CanonicalRequest::of(&Request::get(&url)).hash();
    let held = cache.query_records(request).unwrap();

    // Beside, not instead of: both revisions are there, each with its own
    // date, and nothing was overwritten.
    assert_eq!(held.len(), 2);
    assert_eq!(held[0].revision, 1);
    assert_eq!(held[1].revision, 2);
    assert_ne!(held[0].body.hash, held[1].body.hash);
}

#[test]
fn an_earlier_run_is_still_replayable_after_a_refresh() {
    let (_dir, cache, server) = setup();
    server.replies("/genes", vec![Reply::ok(GENES), Reply::ok(GENES_CHANGED)]);
    let network = Network::new(quick());
    let url = server.url("/genes");

    let first = resolve(&url, Mode::Auto, &cache, &network).unwrap();
    resolve(&url, Mode::Refresh, &cache, &network).unwrap();

    // The old body is still in the store under its own name, so the run
    // that recorded it can be replayed exactly as it ran. A surveillance
    // pipeline refreshing weekly is a series of runs, each replayable.
    let old = cache.objects().checked_get(&first.record.body).unwrap();
    assert_eq!(old.as_deref(), Some(GENES));
}

#[test]
fn a_refresh_that_finds_the_same_answer_records_nothing_new() {
    let (_dir, cache, server) = setup();
    server.replies("/genes", vec![Reply::ok(GENES), Reply::ok(GENES)]);
    let network = Network::new(quick());
    let url = server.url("/genes");

    resolve(&url, Mode::Auto, &cache, &network).unwrap();
    let refreshed = resolve(&url, Mode::Refresh, &cache, &network).unwrap();

    assert_eq!(
        refreshed.served,
        Served::Refreshed {
            drift: Drift::Unchanged
        }
    );
    // A second revision of identical bytes would add a row and no replay:
    // the record that exists already is the record of those bytes.
    let request = CanonicalRequest::of(&Request::get(&url)).hash();
    assert_eq!(cache.query_records(request).unwrap().len(), 1);
}

#[test]
fn a_refresh_inside_the_same_second_still_writes_beside() {
    // The reason the key is `(request, revision)` rather than
    // `(request, recorded_at)`: a clock is the wrong thing to rest "never
    // overwrite" on, and two refreshes in one second is the case that shows
    // it.
    let (_dir, cache, server) = setup();
    server.replies(
        "/genes",
        vec![
            Reply::ok(GENES),
            Reply::ok(GENES_CHANGED),
            Reply::ok(b"gene\tscore\nTP53\t0.10\n" as &[u8]),
        ],
    );
    let network = Network::new(quick());
    let url = server.url("/genes");

    resolve(&url, Mode::Auto, &cache, &network).unwrap();
    resolve(&url, Mode::Refresh, &cache, &network).unwrap();
    resolve(&url, Mode::Refresh, &cache, &network).unwrap();

    let request = CanonicalRequest::of(&Request::get(&url)).hash();
    let held = cache.query_records(request).unwrap();
    assert_eq!(held.len(), 3);
    assert_eq!(
        held.iter()
            .map(|record| record.revision)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn a_refresh_serves_what_it_just_fetched_rather_than_the_old_record() {
    let (_dir, cache, server) = setup();
    server.replies("/genes", vec![Reply::ok(GENES), Reply::ok(GENES_CHANGED)]);
    let network = Network::new(quick());
    let url = server.url("/genes");

    let first = resolve(&url, Mode::Auto, &cache, &network).unwrap();
    let refreshed = resolve(&url, Mode::Refresh, &cache, &network).unwrap();

    assert_ne!(refreshed.digest, first.digest);
    assert_eq!(refreshed.origin().verdict, RemoteVerdict::Live);
}

// ------------------------------------------------------- no trace of a token --

#[test]
fn a_request_made_with_an_auth_header_stores_no_trace_of_it() {
    const TOKEN: &str = "sk-live-do-not-record-me";
    let (_dir, cache, server) = setup();
    server.on("/private", GENES);
    let url = server.url("/private");
    let locator = Locator::parse(&url).unwrap();

    let resolved = recorded::resolve(
        &locator,
        DataKind::EntitySet,
        Some(Auth::bearer(TOKEN)),
        Mode::Auto,
        &cache,
        &Network::new(quick()),
    )
    .unwrap();

    // It really was sent: a test that proved nothing was recorded without
    // proving anything was sent would pass on a request that never carried
    // a token at all.
    let sent = server.received().last().cloned().unwrap();
    assert_eq!(
        sent.header("authorization").map(str::to_owned),
        Some(format!("Bearer {TOKEN}"))
    );

    // Asserted over the whole record and over the whole of what the store
    // holds, rather than over a list of header names, so that a field added
    // later cannot reintroduce the leak quietly.
    let rendered = format!("{:?}", resolved.record);
    assert!(!rendered.contains(TOKEN), "the record holds it: {rendered}");
    assert!(!rendered.to_lowercase().contains("authorization"));

    let body = cache
        .objects()
        .checked_get(&resolved.record.body)
        .unwrap()
        .unwrap();
    assert!(!String::from_utf8_lossy(&body).contains(TOKEN));
}

#[test]
fn an_authenticated_run_and_a_bare_one_replay_the_same_record() {
    // A token is how you are let in, not what you asked for, so it is no
    // part of which record this is.
    let (_dir, cache, server) = setup();
    server.on("/private", GENES);
    let url = server.url("/private");
    let locator = Locator::parse(&url).unwrap();

    recorded::resolve(
        &locator,
        DataKind::EntitySet,
        Some(Auth::bearer("sk-live-secret")),
        Mode::Auto,
        &cache,
        &Network::new(quick()),
    )
    .unwrap();

    // No token, no network: the reviewer's situation.
    let replayed = recorded::resolve(
        &locator,
        DataKind::EntitySet,
        None,
        Mode::Auto,
        &cache,
        &Offline,
    )
    .unwrap();

    assert!(matches!(replayed.served, Served::Replayed { .. }));
}
