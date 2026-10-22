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
//! # Why a declaration is recorded rather than checked
//!
//! When the user declares `identifier_namespace`, the identifiers are not
//! consulted at all. That looks like a missed chance to catch a wrong
//! declaration, and it is deliberate: the catalogue
//! ([`KnownNamespace`](crate::dim::KnownNamespace)) knows seven spaces and
//! there are more than seven, so a curated list's own spelling is a
//! namespace the engine has never heard of and must still accept. Checking
//! a declaration against the catalogue would make the dimension unusable
//! for exactly the in-house truth sets that most need it. A declaration is
//! a user telling the engine something it cannot read; `map_identifiers` is
//! where a wrong one surfaces.
//!
//! Inference is therefore the fallback, not the authority, and it declines
//! in every case it cannot be sure of — see
//! [`KnownNamespace::infer`](crate::dim::KnownNamespace::infer).

use std::path::Path;

use super::{Declared, LoadError, Row, Table, open};
use crate::dim::{Dim, Dimensions, IdentifierNamespace, KnownNamespace, Source};
use crate::note::Note;
use crate::types::{DataKind, Dataset};

/// The column the identifiers are in when the caller does not say.
const DEFAULT_ID_COLUMN: &str = "id";

/// Why a namespace is unknown when nothing declared one and the identifiers
/// did not settle it.
const NOT_INFERRED: &str = "not declared, and the identifiers do not all match one \
                            namespace's pattern -- declare identifier_namespace";

/// Load a gene or protein list from a TSV with a header.
///
/// `id_column` names the column holding the identifiers, defaulting to
/// `id`. Every other column is an attribute and is carried without being
/// read: what a list says about its genes is the user's business, and the
/// dimension is about the names.
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
    let wanted = id_column.unwrap_or(DEFAULT_ID_COLUMN);
    let reader = open(path)?;
    let mut table = Table::read(reader).map_err(|detail| LoadError::format(path, detail))?;

    let at = table.column(wanted).ok_or_else(|| {
        LoadError::format(
            path,
            format!(
                "no column named `{wanted}` to read identifiers from; this file's columns are {}",
                table.header().join(", ")
            ),
        )
    })?;

    let mut identifiers: Vec<String> = Vec::new();
    let mut notes: Vec<Note> = Vec::new();
    while let Some(row) = table
        .next_row()
        .map_err(|detail| LoadError::format(path, detail))?
    {
        let identifier = read_identifier(&row, at);
        if let Some(note) = Note::about_identifier(identifier)
            && !notes.contains(&note)
        {
            notes.push(note);
        }
        identifiers.push(identifier.to_owned());
    }

    let dimensions = Dimensions {
        organism: super::organism(
            declared,
            "a gene list does not record which organism its genes belong to",
        ),
        identifier_namespace: namespace(path, declared, &identifiers),
        subject: super::subject(declared, "a gene list does not record what it is a list of"),
        release: super::release(
            declared,
            "a gene list does not record which release named its entities",
        ),
        ..Dimensions::default()
    };

    Ok(Dataset::new(DataKind::EntitySet, Some(path.to_path_buf()), dimensions).with_notes(notes))
}

/// One row's identifier, trimmed.
///
/// A row the header accounts for always has the column, so the fallback is
/// unreachable in practice; it is an empty identifier rather than a panic
/// because an empty one is simply a name no namespace claims, which is the
/// honest answer and stops inference as any other unreadable name would.
fn read_identifier(row: &Row, at: usize) -> &str {
    row.field(at).unwrap_or_default()
}

/// The namespace dimension: declared if stated, else read off the names.
fn namespace(path: &Path, declared: &Declared, identifiers: &[String]) -> Dim<IdentifierNamespace> {
    if let Some(label) = declared
        .identifier_namespace
        .as_deref()
        .map(IdentifierNamespace::new)
        && !label.is_empty()
    {
        return Dim::known(label, Source::Declared);
    }

    KnownNamespace::infer(identifiers.iter().map(String::as_str)).map_or_else(
        || Dim::unknown(NOT_INFERRED),
        |found| {
            Dim::known(
                IdentifierNamespace::new(found.name()),
                Source::Inferred {
                    path: path.to_path_buf(),
                    detail: "the identifiers' own pattern",
                },
            )
        },
    )
}
