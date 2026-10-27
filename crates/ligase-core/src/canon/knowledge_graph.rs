//! A knowledge graph: nodes and edges as sets, with the serving system's
//! own bookkeeping dropped.
//!
//! The ordinary rule, not the ranked list's exception. The order a graph's
//! nodes and edges came off whatever assembled them is not a fact about the
//! biology, so each is passed to [`Digest::record`](super::Digest::record)
//! and sorted with everything else. Two graphs holding the same nodes and
//! edges in different orders are the same graph and hash the same.
//!
//! The same goes one level down. A node's other `ids` and the `sources` on a
//! node or an edge are *sets*, so they are sorted before they are hashed:
//! `["disgenet", "open-targets"]` and `["open-targets", "disgenet"]` are one
//! answer, and a graph re-serialised by a different library must not become
//! a different graph. What is *not* dropped is which sources those are — a
//! source added upstream is a different graph, and a cache that served the
//! old answer for it would be serving the wrong one, which is the whole of
//! why `source_set` is a dimension.
//!
//! # What is dropped, and why only at the top
//!
//! The keys a serving system writes about itself — `generatedAt`,
//! `requestId`, `took` and the rest of
//! [`response`](crate::remote::response)'s closed set — are dropped from the
//! **top level** and nowhere else. Dropping them at all is
//! [D22](https://ligase.neoforgelabs.tech/project/decisions/): a
//! `generatedAt` says how the graph was served, which is a signature rather
//! than part of the answer. Dropping them only at the top level is D26:
//! inside a node or an edge the same key is what that record *says*, and a
//! graph of clinical trials carrying a `timestamp` on every node would be
//! quietly gutted by a recursive filter. The top level is the one place the
//! two cases can be told apart, and each drop emits a [`Note`] in case the
//! key was the user's own data after all.
//!
//! # What this does not check
//!
//! That every edge endpoint names a node. A canonical form is a claim about
//! what a file says, not a validation of it, and a graph assembled in shards
//! legitimately holds edges into nodes another shard carries. An edge
//! pointing nowhere is a fact about the graph and is hashed as one.

use std::path::Path;

use serde_json::{Map, Value};

use super::Digest;
use crate::load::{self, GraphReader};
use crate::note::Note;
use crate::remote::response;

/// The byte that separates one name from the next inside a hashed set.
///
/// ASCII unit separator: no identifier, URL or accession holds one, so two
/// different sets cannot join into one string.
const SEPARATOR: char = '\u{1f}';

/// Hash a knowledge graph's canonical form.
pub(super) fn canonicalise(path: &Path, digest: &mut Digest) -> Result<(), String> {
    let reader = load::open(path).map_err(|error| error.to_string())?;
    let graph = GraphReader::read(reader)?;

    digest.text("sources", &set(&graph.sources));
    for (name, value) in &graph.rest {
        if response::is_volatile(name) {
            digest.note(Note::GraphFieldDropped {
                field: name.clone(),
            });
            continue;
        }
        digest.text(name, &json(value));
    }

    // `node` and `edge` lead each record as a discriminator. The two go
    // into one sorted list of records, so without it a node and an edge
    // whose parts happened to line up would be one record.
    for node in &graph.nodes {
        let (ids, sources, rest) = (set(&node.ids), set(&node.sources), fields(&node.rest));
        digest.record(&[
            b"node",
            node.id.as_bytes(),
            ids.as_bytes(),
            sources.as_bytes(),
            rest.as_bytes(),
        ]);
    }
    for edge in &graph.edges {
        let (sources, rest) = (set(&edge.sources), fields(&edge.rest));
        digest.record(&[
            b"edge",
            edge.from.as_bytes(),
            edge.to.as_bytes(),
            edge.relation.as_bytes(),
            sources.as_bytes(),
            rest.as_bytes(),
        ]);
    }
    Ok(())
}

/// A set of names as one hashable string: sorted, deduplicated, separated.
///
/// Sorted because these are sets. Deduplicated **exactly**, never folding
/// case, so that the canonical form and
/// [`SourceSet`](crate::dim::SourceSet) agree on what two names are: a hash
/// that folded `A.json` into `a.json` would make two graphs the dimension
/// checker refuses to join agree on their content, which is the two halves
/// of the engine contradicting each other.
fn set(names: &[String]) -> String {
    let mut sorted: Vec<&str> = names.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.join(&SEPARATOR.to_string())
}

/// A node's or an edge's carried keys, as the bytes to hash.
fn fields(carried: &Map<String, Value>) -> String {
    serde_json::to_string(carried).unwrap_or_default()
}

/// One JSON value, as the bytes to hash.
///
/// `serde_json`'s map is a `BTreeMap` with `preserve_order` off, so
/// re-serialising sorts the keys at every depth without a traversal of our
/// own to get wrong — the same mechanism, and the same reason, as
/// [`response`](crate::remote::response).
///
/// Serialising a value that was just parsed cannot fail. The fallback is an
/// empty string rather than a panic, for the reason `response` gives: a
/// panic in a canonicaliser would cost a run.
fn json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
