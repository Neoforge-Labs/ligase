//! Loading a gene or protein list, and reading the space its names live in.
//!
//! **The silent failure this catches.** A list of gene symbols joined to a
//! table of Ensembl IDs overlaps in nothing, and nothing says so: the merge
//! succeeds, the overlap is empty or small, and the result is a plausible
//! answer to a question nobody asked. The same list scored against a
//! curated truth set that spells its genes differently gives a precision of
//! zero that reads as a failed method. And a list whose symbols a
//! spreadsheet has turned into dates — `SEPT7` into `2-Sep`, `MARCH1` into
//! `1-Mar` — is missing exactly those genes from every join downstream,
//! which is a published-literature problem rather than a hypothetical one.
//!
//! So the space is a dimension, and this loader fills it in.
//!
//! The reading itself lives in [`super::named_list`], because a ranked list
//! is read the same way; what is here is the kind, and the three sentences
//! this format uses to say why it cannot supply a dimension on its own.

use std::path::Path;

use super::named_list::{Reasons, named_list};
use super::{Declared, LoadError};
use crate::types::{DataKind, Dataset};

/// What a gene list cannot tell the engine about itself.
const REASONS: Reasons = Reasons {
    organism: "a gene list does not record which organism its genes belong to",
    subject: "a gene list does not record what it is a list of",
    release: "a gene list does not record which release named its entities",
};

/// Load a gene or protein list from a TSV with a header.
///
/// `id_column` names the column holding the identifiers, defaulting to
/// `id`. Every other column is an attribute and is carried without being
/// read: what a list says about its genes is the user's business, and the
/// dimension is about the names.
///
/// The rows are a **set**: this loader reads the order they arrive in and
/// records none of it, and the canonical form discards it too. A list whose
/// order means something is a [`DataKind::RankedList`]
/// ([`super::ranked_list`]).
///
/// # Errors
///
/// [`LoadError::Io`] if the file cannot be read, and [`LoadError::Format`]
/// if it has no header row, has no column of the name asked for, or holds a
/// row the header cannot account for.
pub fn entity_set(
    path: &Path,
    id_column: Option<&str>,
    declared: &Declared,
) -> Result<Dataset, LoadError> {
    named_list(path, id_column, declared, DataKind::EntitySet, &REASONS)
}
