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
//! A resistance report is also a header and sorted rows, and the two
//! functions look alike. They are kept apart because they are strict about
//! different things: a report has no particular column it must have, while
//! an entity set is a set *of* something and a file whose header names no
//! identifier column is not one. Folding them together would mean the
//! stricter check applying to reports or the looser one to entity sets, and
//! the second is the direction that lets a wrong file through
//! ([`super`]'s note on parsers being deliberately strict).
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
