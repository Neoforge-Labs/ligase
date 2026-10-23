//! A gene list: a header row, and an unordered set of rows under it.
//!
//! The header is kept as a field, because which columns a list carries is
//! part of what it says — a list of symbols with a fold change is not the
//! same claim as the same symbols with a p-value. The rows below it are
//! recorded rather than written in order, because this is a *set*: the
//! order the rows came off whatever produced them is not a fact about the
//! genes.
//!
//! # Why this is not `canon::report`
//!
//! A resistance report is also a header and a set of rows, and the two
//! functions do the same work today. They are kept apart because they are
//! about to diverge and because they answer to different owners: a report's
//! canonical form is Ligase's own choice about Ligase's own format, while an
//! entity set's has to stay a claim about what a gene list *means* as the
//! kind grows a ranked sibling ([`DataKind::RankedList`] is specified to
//! canonicalise in order, which is the opposite rule over the same bytes).
//! Sharing one function now would make that divergence an edit to a
//! function two kinds depend on.
//!
//! What they share is the reader, which is the part where disagreeing would
//! be a bug: `load::table` decides what a row is, once, for this, for the
//! loader, and for `map_identifiers`.
//!
//! # What this does not check
//!
//! Which column holds the identifiers. It is not told: a canonical form is
//! computed from a path and a kind, and the identifier column is a loader
//! argument. So a gene list whose header has no `id` column still hashes as
//! a canonical form here — the loader is what refuses it, and it refuses it
//! before anything is hashed.
//!
//! A row the header cannot account for means this is not the table it
//! claims to be, so it is rejected — which, by the rule in [`super::hash`],
//! makes the file hash as its own bytes rather than as the canonical form of
//! something it is not.

use std::path::Path;

use super::Digest;
use crate::load::{self, Table};

/// Hash a gene list's canonical form.
pub(super) fn canonicalise(path: &Path, digest: &mut Digest) -> Result<(), String> {
    let reader = load::open(path).map_err(|error| error.to_string())?;
    let mut table = Table::read(reader).map_err(|error| error.to_string())?;

    digest.text("columns", table.header().join("\t").as_str());

    while let Some(row) = table.next_row().map_err(|error| error.to_string())? {
        digest.record(&[row.line.as_bytes()]);
    }
    Ok(())
}
