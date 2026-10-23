//! What a response means, as against what its bytes are.
//!
//! A recorded query's body goes into the content-addressed store, so it is
//! canonicalised first, exactly as any other input is: two answers meaning
//! the same thing must hash the same, or a cache would miss on a server that
//! reordered its JSON and a drift report would cry wolf every week (D39).
//!
//! # Keys are sorted, and arrays are not
//!
//! Order is meaningless between the keys of a JSON object and meaningful
//! between the elements of an array, so the keys are sorted at every depth
//! and the arrays are left exactly as they came. That is D14's rule — sort
//! what has no order, keep what has — applied to one more format, and D41's
//! reason for a ranked list keeping its order is the same reason an array
//! keeps its own.
//!
//! No number is reformatted. A response carrying a twenty-digit identifier
//! has to be stored with that identifier intact, so numbers are kept as the
//! text the server sent rather than round-tripped through a double. The
//! consequence is that `1` and `1.0` canonicalise differently, which is the
//! honest answer: the engine does not claim an equality it has not
//! verified.
//!
//! # Volatile fields, and the one place it is safe to drop them
//!
//! A `requestId` or a `generatedAt` is the server's account of *how it
//! served this answer*. That is a signature rather than part of the answer,
//! and dropping a tool's signature before hashing is exactly what D22
//! settled for `@PG` lines.
//!
//! They are dropped **only from the top level of a JSON object**, and that
//! line is the whole of the care here. A `timestamp` key at the top of a
//! response envelope is the server describing its own work; the same key
//! inside a data record is what the record *says*, and dropping it
//! recursively would silently delete a time series. D26 draws that line —
//! a field that is part of what the data is about cannot be dropped — and
//! the top level is the one place the two cases can be told apart.
//!
//! Even there a dropped key might have been the user's own data, so every
//! drop is a [`Note`]: the hash stands and the engine says what it saw.
//!
//! # A body that is not JSON
//!
//! Falls back to its bytes, which needs no rule of its own because D21
//! already says canonicalisation falls back to them. A query answered with
//! TSV or plain text is recorded as it arrived.
//!
//! Note the layering: this produces the bytes that go **into** the store,
//! and the stored body is then read as its declared kind by the ordinary
//! loader. A query declared `EntitySet` over a TSV API canonicalises to its
//! bytes here and is parsed as a TSV there.

use serde_json::{Map, Value};

use crate::canon::Form;
use crate::note::Note;

/// The response-body keys a server writes about itself.
///
/// Matched against a key lowercased with `-` and `_` removed, so
/// `requestId`, `request_id`, `request-id` and `REQUESTID` are one name.
///
/// Closed and listed here, on the reasoning the four locator schemes rest
/// on: a set extensible by configuration would be a promise the engine
/// could not keep, and the failure would be a silently deleted field rather
/// than an error.
const VOLATILE: [&str; 10] = [
    "requestid",
    "traceid",
    "correlationid",
    "timestamp",
    "generatedat",
    "servedat",
    "retrievedat",
    "took",
    "elapsed",
    "querytime",
];

/// A response body reduced to what it means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalBody {
    /// The bytes to store and to hash.
    pub bytes: Vec<u8>,
    /// Whether they are a canonical form or the original bytes standing in
    /// for one.
    pub form: Form,
    /// What canonicalising it noticed: one note per field dropped.
    pub notes: Vec<Note>,
}

/// Reduce a response body to what it means.
///
/// JSON is parsed, its keys sorted at every depth and the volatile keys
/// dropped from its top level. Anything that is not JSON is returned as it
/// arrived ([`Form::Raw`]).
///
/// `locator` is only for the notes, so that a run making several queries
/// can say which of them dropped a field.
#[must_use]
pub fn canonicalise(locator: &str, body: &[u8]) -> CanonicalBody {
    let Ok(parsed) = serde_json::from_slice::<Value>(body) else {
        return CanonicalBody {
            bytes: body.to_vec(),
            form: Form::Raw,
            notes: Vec::new(),
        };
    };

    let mut notes = Vec::new();
    let stripped = match parsed {
        // Only an object at the root has an envelope to hold a server's
        // signature. An array at the root is a list of answers, and a
        // `took` inside one of them is that answer's own field.
        Value::Object(fields) => Value::Object(without_volatile(locator, fields, &mut notes)),
        other => other,
    };

    // `serde_json`'s map is a `BTreeMap` unless `preserve_order` is on, so
    // re-serialising is what sorts the keys -- at every depth, and without
    // a traversal of our own to get wrong.
    let Ok(bytes) = serde_json::to_vec(&stripped) else {
        // Serialising a value that was just parsed cannot fail. Saying so
        // costs a branch; a panic in a canonicaliser would cost a run.
        return CanonicalBody {
            bytes: body.to_vec(),
            form: Form::Raw,
            notes: Vec::new(),
        };
    };

    CanonicalBody {
        bytes,
        form: Form::Canonical,
        notes,
    }
}

/// The top-level fields, without the ones a server writes about itself.
fn without_volatile(
    locator: &str,
    fields: Map<String, Value>,
    notes: &mut Vec<Note>,
) -> Map<String, Value> {
    let mut kept = Map::new();
    for (name, value) in fields {
        if VOLATILE.contains(&normalise(&name).as_str()) {
            notes.push(Note::QueryFieldDropped {
                locator: locator.to_owned(),
                field: name,
            });
            continue;
        }
        kept.insert(name, value);
    }
    kept
}

/// A key as [`VOLATILE`] spells it: lower-cased, with `-` and `_` removed.
///
/// So that one name written four ways by four APIs is one name, without an
/// entry per spelling.
fn normalise(name: &str) -> String {
    name.chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}
