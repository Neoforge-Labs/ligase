//! Reading a list of named entities, and the space its names live in.
//!
//! Two kinds are loaded from here: [`DataKind::EntitySet`], the gene or
//! protein list, and [`DataKind::RankedList`], the same list with its rows
//! in rank order. They share this module because reading one is reading the
//! other: a TSV with a header, an identifier column named by the caller,
//! every other column carried without being read, and four dimensions —
//! `organism`, `identifier_namespace`, `subject` and `release` — of which
//! the loader can usually work out one.
//!
//! The two kinds differ in their **canonical form** and in nothing else
//! (D41), so that is the one place they have separate code:
//! [`canon::entity_set`](crate::canon) discards row order and
//! `canon::ranked_list` keeps it. Sharing the reader is what keeps the
//! difference in one place; copying it would have copied the part that is
//! subtle, which is the next section.
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
pub(super) const DEFAULT_ID_COLUMN: &str = "id";

/// Why a namespace is unknown when nothing declared one and the identifiers
/// did not settle it.
const NOT_INFERRED: &str = "not declared, and the identifiers do not all match one \
                            namespace's pattern -- declare identifier_namespace";

/// The same, for a file holding an identifier a spreadsheet turned into a
/// date.
///
/// A separate reason because the advice differs, and the difference matters
/// more than it looks. "Declare identifier_namespace" is sound advice for a
/// file whose identifiers are merely unrecognised, and it is the **worst**
/// advice for this one: declare it and the file is accepted from then on
/// with `SEPT7` missing from every join, which is the corruption made
/// permanent. The reason a dimension is unknown is what a user reads and
/// acts on, so it has to arrive with the cause rather than beside it in a
/// note nobody thought to look at.
const DATES_AMONG_THEM: &str = "not declared, and some identifiers are dates rather than \
                                names -- see this dataset's notes, and re-export the file \
                                with the identifier column typed as text";

/// What a kind says about itself when it cannot supply a dimension.
///
/// The three sentences are the only thing that differs between the two
/// kinds loaded here, and they differ because each is advice a user reads:
/// "a gene list does not record which organism its genes belong to" tells
/// someone what to do, and a message that named the wrong kind of file
/// would be a loader describing something the user is not holding. So they
/// stay the caller's, as [`super::organism`] and its siblings already have
/// them be.
pub(super) struct Reasons {
    /// Why the organism is unknown when nothing declared it.
    pub organism: &'static str,
    /// Why the subject is unknown when nothing declared it.
    pub subject: &'static str,
    /// Why the release is unknown when nothing declared it.
    pub release: &'static str,
}

/// Load a list of named entities as `kind`, from a TSV with a header.
///
/// `id_column` names the column holding the identifiers, defaulting to
/// [`DEFAULT_ID_COLUMN`]. Every other column is an attribute and is carried
/// without being read: what a list says about its entities is the user's
/// business, and the dimension is about the names.
///
/// # Errors
///
/// [`LoadError::Io`] if the file cannot be read, and [`LoadError::Format`]
/// if it has no header row, has no column of the name asked for, or holds a
/// row the header cannot account for.
pub(super) fn named_list(
    path: &Path,
    id_column: Option<&str>,
    declared: &Declared,
    kind: DataKind,
    reasons: &Reasons,
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
        organism: super::organism(declared, reasons.organism),
        identifier_namespace: namespace(
            path,
            declared,
            &identifiers,
            notes
                .iter()
                .any(|note| matches!(note, Note::IdentifierIsADate { .. })),
        ),
        subject: super::subject(declared, reasons.subject),
        release: super::release(declared, reasons.release),
        ..Dimensions::default()
    };

    Ok(Dataset::new(kind, Some(path.to_path_buf()), dimensions).with_notes(notes))
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
fn namespace(
    path: &Path,
    declared: &Declared,
    identifiers: &[String],
    dated: bool,
) -> Dim<IdentifierNamespace> {
    if let Some(label) = declared
        .identifier_namespace
        .as_deref()
        .map(IdentifierNamespace::new)
        && !label.is_empty()
    {
        return Dim::known(label, Source::Declared);
    }

    KnownNamespace::infer(identifiers.iter().map(String::as_str)).map_or_else(
        || {
            Dim::unknown(if dated {
                DATES_AMONG_THEM
            } else {
                NOT_INFERRED
            })
        },
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
