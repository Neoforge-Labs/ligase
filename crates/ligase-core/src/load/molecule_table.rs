//! Reading a table of molecules, and what its names were arrived at by.
//!
//! Two kinds are loaded from here: [`DataKind::MoleculeSet`], the compound
//! library or hit list, and [`DataKind::MoleculeRanking`], the same table
//! with its rows in rank order. They share this module because reading one
//! is reading the other, exactly as the two named lists do: a TSV with a
//! header, an identity column, every other column carried without being
//! read, and four dimensions — `organism`, `representation`, `subject` and
//! `release` — of which the file can speak to one.
//!
//! The two kinds differ in their **canonical form** and in nothing else
//! (D41, D48), so that is the one place they have separate code.
//!
//! # No chemistry is read here, or anywhere in the core
//!
//! This loader does not parse a structure, perceive a ring or compute a key.
//! It reads a column of strings and a line of prose. The engine core knows
//! no chemistry at all (D49): computing an InChIKey needs RDKit, RDKit is a
//! Python package, and a core that linked an InChI library would end the
//! promise that `pip install ligase-bio` needs no toolchain beyond Rust —
//! which is the promise `tests/default_build.rs` exists to defend.
//!
//! What the engine does instead is refuse to connect two sets whose
//! representations disagree, which needs no chemistry and is the whole of
//! what the dimension is for.
//!
//! # The identity column, and why there are two
//!
//! `inchikey` if the table has one, `smiles` otherwise. Both are admitted
//! because a set named by structures has to be loadable: it is what
//! `ligase.chem.standardize` is handed, and a conversion whose input could
//! not be typed would have nothing to convert. They are not equivalent, and
//! the measurement is why — RDKit's canonical SMILES for cisplatin changed
//! between 2024.09.6 and 2025.09.6 while its InChIKey did not, so a set
//! named by structure strings has an identity its own toolkit can move under
//! it. The dimension says which of the two a file is in, and the two never
//! compare equal.
//!
//! # Why a missing representation is a gap and not `as given`
//!
//! A file with keys and no representation line could be a ChEMBL download,
//! which *is* standardised, or a vendor list, which is not. Assuming the
//! weaker state would be the engine inventing the most dangerous of the
//! possible answers, and it would be a *known* value, so two such files
//! would agree. So the axis is unknown with a reason, and because
//! `representation` is demanded wherever two of these kinds are joined, the
//! file cannot quietly join anything until somebody says.

use std::path::Path;

use super::{Declared, LoadError, Table, open};
use crate::dim::{Dim, Dimensions, Identity, Representation, Source};
use crate::types::{DataKind, Dataset};

/// The line a standardiser writes, and this loader reads.
///
/// Ligase's own prefix, so that reading it is not a guess about somebody
/// else's comment. The rest of the line is what [`Representation::parse`]
/// reads, which is the same spelling a declaration and a conversion target
/// use.
pub(crate) const REPRESENTATION_LINE: &str = "#ligase representation:";

/// Why the organism is unknown: a compound table does not record it.
const NO_ORGANISM: &str = "a molecule table does not record which organisms its compounds were \
                           measured against";

/// Why the subject is unknown.
const NO_SUBJECT: &str = "a molecule table does not record what it is a set of -- the target, \
                          the assay, the therapeutic area";

/// Why the release is unknown.
const NO_RELEASE: &str = "a molecule table does not record which release of a compound \
                          collection it was drawn from";

/// Why the representation is unknown when nothing stated one.
///
/// It deliberately does **not** mention `as given`, which is the state it
/// has just refused to assume: a reason that named the weakest state would
/// invite a user to declare it, and declaring it wrongly is the failure.
const NOT_STATED: &str = "this table names its molecules by InChIKey but does not say what \
                          standardisation produced them, and the keys cannot say -- declare \
                          representation, or standardise the table";

/// What each kind says about itself when it cannot supply a dimension.
///
/// Shared between the two kinds here because they are the same file read two
/// ways: unlike the named lists, where a gene list and a ranking differ in
/// what a user is holding, a molecule set and a molecule ranking are one
/// table and one sentence is true of both.
fn dimensions(path: &Path, declared: &Declared, comments: &[String], has_keys: bool) -> Dimensions {
    Dimensions {
        organism: super::organism(declared, NO_ORGANISM),
        representation: representation(path, declared, comments, has_keys),
        subject: super::subject(declared, NO_SUBJECT),
        release: super::release(declared, NO_RELEASE),
        ..Dimensions::default()
    }
}

/// Load a table of molecules as `kind`.
///
/// The table needs an `inchikey` column or a `smiles` column; every other
/// column is carried without being read, a score column included. What a
/// table says about its compounds is the user's business, and the dimension
/// is about what its names mean.
///
/// # Errors
///
/// [`LoadError::Io`] if the file cannot be read, and [`LoadError::Format`]
/// if it has no header row, has neither identity column, holds a row the
/// header cannot account for, or carries a representation line naming an
/// identity or a standardisation the engine does not know.
pub(super) fn molecule_table(
    path: &Path,
    declared: &Declared,
    kind: DataKind,
) -> Result<Dataset, LoadError> {
    let reader = open(path)?;
    let (mut table, comments) =
        Table::read_after_comments(reader).map_err(|detail| LoadError::format(path, detail))?;

    let has_keys = table.column(Identity::InChIKey.spelling()).is_some();
    if !has_keys && table.column(Identity::Smiles.spelling()).is_none() {
        return Err(LoadError::format(
            path,
            format!(
                "no `{}` column to compare molecules by, and no `{}` column either; this \
                 file's columns are {}",
                Identity::InChIKey.spelling(),
                Identity::Smiles.spelling(),
                table.header().join(", ")
            ),
        ));
    }

    // Read to the end even though no dimension comes out of the rows, so
    // that a table whose rows do not fit its header is refused here rather
    // than by the canonical form alone. A file the loader accepts and the
    // canonical form rejects would load successfully and then hash as its
    // own bytes, which costs early cutoff everything below it and says
    // nothing.
    while table
        .next_row()
        .map_err(|detail| LoadError::format(path, detail))?
        .is_some()
    {}

    // The representation is parsed before the dataset is built, so that a
    // line naming an operation the engine has never heard of is a load
    // error rather than a dimension quietly left unknown. A state carried
    // along unread is the lenient reading `canon` calls dangerous.
    if let Some(spelled) = spelled_on_a_line(&comments) {
        Representation::parse(spelled).map_err(|detail| LoadError::format(path, detail))?;
    }

    Ok(Dataset::new(
        kind,
        Some(path.to_path_buf()),
        dimensions(path, declared, &comments, has_keys),
    ))
}

/// What a representation line spells, if the file has one.
pub(crate) fn spelled_on_a_line(comments: &[String]) -> Option<&str> {
    comments
        .iter()
        .find_map(|line| line.strip_prefix(REPRESENTATION_LINE))
        .map(str::trim)
}

/// The representation dimension: declared if stated, else read off the line.
///
/// A declaration wins and is **recorded, not verified**, which is D38's
/// position on a declared namespace applied to this axis and for a sharper
/// reason: the engine has no chemistry, so it could not check the claim if
/// it wanted to. The step that does the standardising checks its own work
/// instead, which is the half that can be checked.
fn representation(
    path: &Path,
    declared: &Declared,
    comments: &[String],
    has_keys: bool,
) -> Dim<Representation> {
    if let Some(spelled) = declared.representation.as_deref()
        && let Ok(stated) = Representation::parse(spelled)
    {
        return Dim::known(stated, Source::Declared);
    }

    if let Some(spelled) = spelled_on_a_line(comments)
        && let Ok(read) = Representation::parse(spelled)
    {
        return Dim::known(
            read,
            Source::Inferred {
                path: path.to_path_buf(),
                detail: "the representation line the standardiser wrote",
            },
        );
    }

    // A table named by structures alone is `as given` by observation rather
    // than by assumption: there is nothing for a standardisation to have
    // been applied *to* a name that is the structure itself, so the state
    // is not a thing somebody forgot to record. A table of keys is the
    // opposite case, and gets the gap.
    if has_keys {
        Dim::unknown(NOT_STATED)
    } else {
        Dim::known(
            Representation::new(Identity::Smiles, []),
            Source::Inferred {
                path: path.to_path_buf(),
                detail: "the structures being the names themselves",
            },
        )
    }
}

/// Load a set of molecules, whose row order says nothing.
///
/// # Errors
///
/// As [`molecule_table`].
pub fn molecule_set(path: &Path, declared: &Declared) -> Result<Dataset, LoadError> {
    molecule_table(path, declared, DataKind::MoleculeSet)
}

/// Load a ranking of molecules, whose row order is the answer.
///
/// # Errors
///
/// As [`molecule_table`].
pub fn molecule_ranking(path: &Path, declared: &Declared) -> Result<Dataset, LoadError> {
    molecule_table(path, declared, DataKind::MoleculeRanking)
}
