//! A molecule table: its representation, its header, and an unordered set of
//! rows under them.
//!
//! The ordinary rule, not the ranked sibling's exception. The order compounds
//! came off whatever assembled the table is not a fact about the chemistry,
//! so each row is passed to [`Digest::record`](super::Digest::record) and
//! sorted with the rest. For the kind where the order *is* the answer, see
//! [`molecule_ranking`](super::molecule_ranking).
//!
//! # Why the representation is hashed, and why that is not optional
//!
//! A step key is hash(function code, parameters, input content hashes, tool
//! versions). **Dimensions are not in it.** So two tables holding a
//! byte-identical InChIKey column and differing only in what was done to
//! produce it would have the same content hash, and the cache would serve
//! one when the pipeline asked for the other — in silence, which is the one
//! failure semantic hashing exists to be incapable of.
//!
//! Twelve of thirty ordinary drugs change key under a different
//! standardisation, so this is not a corner. Everywhere else in this module
//! a field is hashed because it is part of what a file says; here it is
//! hashed because leaving it out would be a correctness bug.
//!
//! # Nothing is dropped
//!
//! The other leading `#` lines are hashed too, in order, as somebody's own
//! words about their own file. A canonicaliser that dropped them would make
//! two files that differ compare equal, and there is no volatile set to
//! appeal to here: unlike a served JSON document, a table nobody fetched
//! carries no serving system's signature.
//!
//! # Why this checks the identity column where `entity_set` does not
//!
//! [`entity_set`](super::entity_set) deliberately does not check which
//! column holds the identifiers, because it is not told: a canonical form is
//! computed from a path and a kind, and the identifier column is a loader
//! argument. Here the column's name is fixed by the kind — `inchikey`, or
//! `smiles` — so this *can* check, and therefore must. Without the check any
//! one-column text file parses as a molecule table and hashes as the
//! canonical form of something it is not, which is the opposite of the
//! conservative direction [`super::hash`] takes everywhere else. A JSON
//! document is one such file: every line is a row of one field.

use std::io::BufRead;
use std::path::Path;

use super::Digest;
use crate::dim::Identity;
use crate::load::{self, REPRESENTATION_LINE, Table, spelled_on_a_line};

/// A molecule table being read, and the comment lines above it.
///
/// A named type because the two canonical forms share the reader and clippy
/// is right that the tuple is a mouthful.
type Opened = (Table<Box<dyn BufRead>>, Vec<String>);

/// Hash a molecule table's canonical form, rows unordered.
pub(super) fn canonicalise(path: &Path, digest: &mut Digest) -> Result<(), String> {
    let (mut table, comments) = read(path)?;
    fields(digest, &comments, &table);

    while let Some(row) = table.next_row().map_err(|error| error.to_string())? {
        digest.record(&[row.line.as_bytes()]);
    }
    Ok(())
}

/// Open a molecule table and read past its comment block.
///
/// Shared with [`molecule_ranking`](super::molecule_ranking), because the
/// two kinds must agree about where the header starts and what the
/// representation says. They differ in one call, and this is not it.
pub(super) fn read(path: &Path) -> Result<Opened, String> {
    let reader = load::open(path).map_err(|error| error.to_string())?;
    let (table, comments) = Table::read_after_comments(reader)?;
    if table.column(Identity::InChIKey.spelling()).is_none()
        && table.column(Identity::Smiles.spelling()).is_none()
    {
        return Err(format!(
            "no `{}` or `{}` column, so this is not a molecule table",
            Identity::InChIKey.spelling(),
            Identity::Smiles.spelling()
        ));
    }
    Ok((table, comments))
}

/// The parts of a molecule table that are fields rather than records.
///
/// The representation first and by itself, so that a reader of the hash
/// stream sees the dimension being hashed rather than finding it inside a
/// block of comments.
pub(super) fn fields(
    digest: &mut Digest,
    comments: &[String],
    table: &Table<impl std::io::BufRead>,
) {
    digest.text("representation", spelled_on_a_line(comments).unwrap_or(""));
    let rest: Vec<&str> = comments
        .iter()
        .filter(|line| !line.starts_with(REPRESENTATION_LINE))
        .map(String::as_str)
        .collect();
    digest.text("comments", &rest.join("\n"));
    digest.text("columns", table.header().join("\t").as_str());
}
