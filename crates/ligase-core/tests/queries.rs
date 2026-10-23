//! The canonical request and the canonical response (D39, M10b).
//!
//! A record's identity is the canonical *request*, so that twenty paginated
//! calls are twenty records keyed by what was asked rather than by the order
//! it was asked in. Its contents are the canonical *response*, so that two
//! answers meaning the same thing are one answer.
#![allow(clippy::unwrap_used)]

use ligase::canon::Form;
use ligase::note::Note;
use ligase::remote::request::{Auth, CanonicalRequest, Method, Request};
use ligase::remote::response;

// ------------------------------------------------- the canonical request --

#[test]
fn two_urls_differing_only_in_parameter_order_are_one_request() {
    let one = CanonicalRequest::of(&Request::get("https://api.test/v1/x?b=2&a=1"));
    let other = CanonicalRequest::of(&Request::get("https://api.test/v1/x?a=1&b=2"));

    assert_eq!(one.hash(), other.hash());
}

#[test]
fn two_urls_differing_in_a_parameter_value_are_two_requests() {
    let one = CanonicalRequest::of(&Request::get("https://api.test/v1/x?page=1"));
    let other = CanonicalRequest::of(&Request::get("https://api.test/v1/x?page=2"));

    assert_ne!(one.hash(), other.hash());
}

#[test]
fn a_repeated_parameter_keeps_both_of_its_values() {
    let both = CanonicalRequest::of(&Request::get("https://api.test/x?id=a&id=b"));
    let one = CanonicalRequest::of(&Request::get("https://api.test/x?id=a"));

    assert_ne!(both.hash(), one.hash());
    assert_eq!(both.url(), "https://api.test/x?id=a&id=b");
}

#[test]
fn the_url_that_is_sent_is_the_one_the_user_wrote_not_the_sorted_one() {
    // A server may care about parameter order, and the user wrote what they
    // wrote. Sorting is for deciding which record this is, never for
    // deciding what to ask.
    let written = "https://api.test/v1/x?b=2&a=1";
    let request = Request::get(written);

    assert_eq!(request.url, written);
    assert_eq!(
        CanonicalRequest::of(&request).url(),
        "https://api.test/v1/x?a=1&b=2"
    );
}

#[test]
fn a_fragment_is_not_part_of_a_request_because_it_is_never_sent() {
    let with = CanonicalRequest::of(&Request::get("https://api.test/x?a=1#section"));
    let without = CanonicalRequest::of(&Request::get("https://api.test/x?a=1"));

    assert_eq!(with.hash(), without.hash());
}

#[test]
fn a_post_and_a_get_to_one_url_are_two_requests() {
    // The transport cannot issue a POST yet. Hashing the method anyway is
    // what stops a later one from changing the identity of a record written
    // today.
    let get = CanonicalRequest::of(&Request::get("https://api.test/graphql"));
    let post = CanonicalRequest::of(&Request::post("https://api.test/graphql", b"{}"));

    assert_ne!(get.hash(), post.hash());
    assert_eq!(get.method(), Method::Get);
    assert_eq!(post.method(), Method::Post);
}

#[test]
fn two_posts_differing_only_in_body_are_two_requests() {
    let one = CanonicalRequest::of(&Request::post("https://api.test/graphql", b"{\"q\":1}"));
    let other = CanonicalRequest::of(&Request::post("https://api.test/graphql", b"{\"q\":2}"));

    assert_ne!(one.hash(), other.hash());
}

#[test]
fn an_auth_token_is_no_part_of_which_request_this_is() {
    // Two runs of one pipeline, one of them authenticated, must replay each
    // other's record: a token is how you are let in, not what you asked
    // for.
    let bare = Request::get("https://api.test/x");
    let authenticated =
        Request::get("https://api.test/x").with_auth(Some(Auth::bearer("sk-secret")));

    assert_eq!(
        CanonicalRequest::of(&bare).hash(),
        CanonicalRequest::of(&authenticated).hash()
    );
}

#[test]
fn a_token_is_not_printed_by_the_debug_that_an_error_message_would_use() {
    // `Fetched` has no field for a header, but a `Request` must carry the
    // token in order to send it -- so the one place it could still escape
    // is a message that formats the request.
    let request = Request::get("https://api.test/x").with_auth(Some(Auth::bearer("sk-secret")));

    let shown = format!("{request:?}");
    assert!(!shown.contains("sk-secret"), "the token is in {shown}");
    assert!(
        shown.contains("redacted"),
        "nor is it said to be hidden: {shown}"
    );
}

// ------------------------------------------------ the canonical response --

#[test]
fn two_responses_that_mean_the_same_thing_hash_the_same() {
    let one = response::canonicalise("https://api.test/x", br#"{"b":2,"a":1}"#);
    let other = response::canonicalise("https://api.test/x", br#"{ "a": 1, "b": 2 }"#);

    assert_eq!(one.form, Form::Canonical);
    assert_eq!(one.bytes, other.bytes);
}

#[test]
fn nested_keys_are_sorted_too_because_order_is_nowhere_meaningful_in_json() {
    let one = response::canonicalise("https://api.test/x", br#"{"o":{"z":1,"y":2}}"#);
    let other = response::canonicalise("https://api.test/x", br#"{"o":{"y":2,"z":1}}"#);

    assert_eq!(one.bytes, other.bytes);
}

#[test]
fn an_array_keeps_its_order_because_there_order_is_the_answer() {
    let one = response::canonicalise("https://api.test/x", br#"{"a":[1,2]}"#);
    let other = response::canonicalise("https://api.test/x", br#"{"a":[2,1]}"#);

    assert_ne!(one.bytes, other.bytes);
}

#[test]
fn a_server_timestamp_is_not_part_of_what_a_response_means() {
    let monday = response::canonicalise(
        "https://api.test/x",
        br#"{"generatedAt":"2026-10-20T09:00:00Z","hits":3}"#,
    );
    let tuesday = response::canonicalise(
        "https://api.test/x",
        br#"{"generatedAt":"2026-10-21T17:42:11Z","hits":3}"#,
    );

    assert_eq!(monday.bytes, tuesday.bytes);
    assert_eq!(
        String::from_utf8(monday.bytes.clone()).unwrap(),
        r#"{"hits":3}"#
    );
}

#[test]
fn a_request_id_is_dropped_however_it_is_spelled() {
    for spelling in ["requestId", "request_id", "request-id", "REQUESTID"] {
        let body = format!(r#"{{"{spelling}":"abc","hits":1}}"#);
        let canonical = response::canonicalise("https://api.test/x", body.as_bytes());

        assert_eq!(
            String::from_utf8(canonical.bytes).unwrap(),
            r#"{"hits":1}"#,
            "`{spelling}` survived"
        );
    }
}

#[test]
fn a_timestamp_inside_a_record_is_data_and_is_kept() {
    // The line D26 draws. At the top of an envelope a timestamp is the
    // server's account of its own work; inside a record it is what the
    // record says, and dropping it would delete a time series.
    let canonical = response::canonicalise(
        "https://api.test/x",
        br#"{"rows":[{"timestamp":"2026-10-20","value":7}]}"#,
    );

    let shown = String::from_utf8(canonical.bytes).unwrap();
    assert!(
        shown.contains("timestamp"),
        "the row's own timestamp went: {shown}"
    );
}

#[test]
fn a_dropped_field_is_noticed() {
    let canonical = response::canonicalise("https://api.test/x", br#"{"took":12,"hits":1}"#);

    assert_eq!(
        canonical.notes,
        vec![Note::QueryFieldDropped {
            locator: "https://api.test/x".to_owned(),
            field: "took".to_owned(),
        }]
    );
}

#[test]
fn a_response_with_nothing_volatile_in_it_notices_nothing() {
    let canonical = response::canonicalise("https://api.test/x", br#"{"hits":1}"#);

    assert!(canonical.notes.is_empty());
}

#[test]
fn a_body_that_is_not_json_falls_back_to_its_bytes() {
    // D21, which already says canonicalisation falls back to the bytes, so
    // a TSV or a plain-text answer needs no special case here.
    let body = b"gene\tscore\nTP53\t0.9\n";
    let canonical = response::canonicalise("https://api.test/x", body);

    assert_eq!(canonical.form, Form::Raw);
    assert_eq!(canonical.bytes, body);
    assert!(canonical.notes.is_empty());
}

#[test]
fn a_number_too_long_for_a_double_is_not_rounded() {
    // A canonicaliser must never silently change a number: a twenty-digit
    // identifier that arrived exact has to be stored exact.
    let canonical =
        response::canonicalise("https://api.test/x", br#"{"id":12345678901234567890123}"#);

    assert_eq!(
        String::from_utf8(canonical.bytes).unwrap(),
        r#"{"id":12345678901234567890123}"#
    );
}

#[test]
fn a_whole_number_and_the_same_number_written_with_a_point_are_not_claimed_to_be_equal() {
    let plain = response::canonicalise("https://api.test/x", br#"{"n":1}"#);
    let pointed = response::canonicalise("https://api.test/x", br#"{"n":1.0}"#);

    assert_ne!(plain.bytes, pointed.bytes);
}

#[test]
fn a_json_array_at_the_root_is_canonicalised_without_anything_being_dropped() {
    // Nothing to drop: there is no envelope to hold a server's signature,
    // and a `took` inside an element of the array is that element's data.
    let canonical = response::canonicalise("https://api.test/x", br#"[{"took":1},{"b":2,"a":1}]"#);

    assert_eq!(canonical.form, Form::Canonical);
    assert_eq!(
        String::from_utf8(canonical.bytes).unwrap(),
        r#"[{"took":1},{"a":1,"b":2}]"#
    );
    assert!(canonical.notes.is_empty());
}

#[test]
fn an_empty_body_is_its_own_bytes_rather_than_a_parse_failure() {
    let canonical = response::canonicalise("https://api.test/x", b"");

    assert_eq!(canonical.form, Form::Raw);
    assert!(canonical.bytes.is_empty());
}
