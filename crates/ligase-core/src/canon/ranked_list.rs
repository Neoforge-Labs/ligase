//! A ranked list: a header row, and the rows under it **in order**.
//!
//! # This module does not sort, and that is deliberate
//!
//! Read this before changing anything below. Every other canonicaliser in
//! [`super`] sorts its records — alignments, variants, intervals, reads,
//! report rows and gene lists all do — because the order records came off
//! whatever produced them is not a fact about the biology. A ranking is the
//! one kind where it is a fact about the biology, and the only one.
//!
//! Rank is the row position. A ranking of twelve genes and the same twelve
//! genes in a different order are two different answers to the question
//! "which of these matters most?", and the answer is the output. So the
//! order is hashed, and a reordered ranking has a different canonical hash
//! ([D41](https://ligase.neoforgelabs.tech/project/decisions/)). Sorting
//! here would make two different answers agree, which would hand a cache
//! the wrong one, and it would do it silently — exactly the failure the
//! engine exists to refuse.
//!
//! This is why [`Digest::ordered_record`] exists beside
//! [`Digest::record`](super::Digest::record): the two take the same
//! arguments and mean opposite things, so the choice is made by name at the
//! call site rather than being remembered.
//!
//! # Why this is not `canon::entity_set`
//!
//! Because the two differ in exactly the one way this module is about, and
//! sharing a function would mean expressing that difference as a flag
//! somewhere. [`load::ranked_list`](crate::load::ranked_list) *does* share
//! the gene list's reader, because reading names and the space they live in
//! is the same work; the canonical form is the one place the kinds are not
//! the same, so it is the one place they have separate code.
//!
//! # What this does not check
//!
//! Which column holds the identifiers, and which holds the rank. It is not
//! told: a canonical form is computed from a path and a kind, and the
//! identifier column is a loader argument. A rank column, where a file has
//! one, is hashed as the ordinary attribute column it is — so a file that
//! numbers its rows disagrees with one that does not, which is right,
//! because the numbers are part of what it says.
//!
//! A `score` column is hashed exactly, with no tolerance
//! ([`super::tolerance`]): a tolerance nobody has measured would be a guess
//! baked into every cache key, and audit mode has not measured this one.
//!
//! A row the header cannot account for means this is not the table it
//! claims to be, so it is rejected — which, by the rule in [`super::hash`],
//! makes the file hash as its own bytes rather than as the canonical form of
//! something it is not.

use std::path::Path;

use super::Digest;
use crate::load::{self, Table};

/// Hash a ranked list's canonical form, order included.
pub(super) fn canonicalise(path: &Path, digest: &mut Digest) -> Result<(), String> {
    let reader = load::open(path).map_err(|error| error.to_string())?;
    let mut table = Table::read(reader).map_err(|error| error.to_string())?;

    digest.text("columns", table.header().join("\t").as_str());

    while let Some(row) = table.next_row().map_err(|error| error.to_string())? {
        // `ordered_record`, never `record`. See the module comment.
        digest.ordered_record(&[row.line.as_bytes()]);
    }
    Ok(())
}
