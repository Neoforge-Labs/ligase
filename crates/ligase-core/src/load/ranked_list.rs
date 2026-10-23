//! Loading a ranked list, where the row order is the result.
//!
//! **The silent failure this catches.** Everything
//! [`super::entity_set`](super::entity_set) catches — a list of symbols
//! joined to a table of Ensembl IDs, a truth set that spells its genes
//! differently, symbols a spreadsheet turned into dates — plus one more
//! that only a ranking can have: a rank-ordered result compared against one
//! in a different namespace or from a different release. Two rankings of
//! "the same" genes are a concordance measurement, and if the two were
//! named by different HGNC releases or in different spaces then the
//! measurement is of the naming rather than of the biology, and it comes
//! out low and plausible.
//!
//! What this loader does **not** record is the rank. Rank is the row
//! position, so recording it would be recording the file: the order is kept
//! where order is kept, which is the canonical form
//! (`canon::ranked_list`, D41). A file that also *numbers* its rows carries
//! that number as an ordinary attribute column, like any other.
//!
//! The reading itself lives in [`super::named_list`], shared with the gene
//! list, because reading names and the space they live in is the same work
//! for both kinds. The difference between them is one thing and lives in
//! one place.

use std::path::Path;

use super::named_list::{Reasons, named_list};
use super::{Declared, LoadError};
use crate::types::{DataKind, Dataset};

/// What a ranked list cannot tell the engine about itself.
const REASONS: Reasons = Reasons {
    organism: "a ranked list does not record which organism its entities belong to",
    subject: "a ranked list does not record what it is a ranking of",
    release: "a ranked list does not record which release named its entities",
};

/// Load a ranked list from a TSV with a header, highest rank first.
///
/// `id_column` names the column holding the identifiers, defaulting to
/// `id`. Every other column is an attribute and is carried without being
/// read — a `score`, a `rank`, an adjusted p-value — and a `score` is
/// compared **exactly**, because no tolerance has been measured for one
/// (see [`canon::tolerance`](crate::canon::tolerance)).
///
/// The rows are in order, and the order is part of what the file says: two
/// files holding these rows in different orders are two different rankings
/// and have different canonical hashes. A list whose order means nothing is
/// an [`DataKind::EntitySet`] ([`super::entity_set`]).
///
/// # Errors
///
/// [`LoadError::Io`] if the file cannot be read, and [`LoadError::Format`]
/// if it has no header row, has no column of the name asked for, or holds a
/// row the header cannot account for.
pub fn ranked_list(
    path: &Path,
    id_column: Option<&str>,
    declared: &Declared,
) -> Result<Dataset, LoadError> {
    named_list(path, id_column, declared, DataKind::RankedList, &REASONS)
}
