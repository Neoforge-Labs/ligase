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
use crate::dim::{Dim, Dimensions, Identity, Representation, Source, Standardisation};
use crate::note::Note;
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

    // Read to the end even though no *dimension* comes out of the rows, so
    // that a table whose rows do not fit its header is refused here rather
    // than by the canonical form alone. A file the loader accepts and the
    // canonical form rejects would load successfully and then hash as its
    // own bytes, which costs early cutoff everything below it and says
    // nothing.
    //
    // One thing does come out of them, and it is the one part of a
    // representation the engine can check without any chemistry: an
    // InChIKey's third block is its protonation layer.
    let at = table.column(Identity::InChIKey.spelling());
    let mut charge = ChargeState::default();
    let mut row_number = 0_usize;
    while let Some(row) = table
        .next_row()
        .map_err(|detail| LoadError::format(path, detail))?
    {
        row_number += 1;
        if let Some(at) = at {
            charge.read(row_number, row.field(at).unwrap_or_default().trim());
        }
    }

    // The representation is parsed before the dataset is built, so that a
    // line naming an operation the engine has never heard of is a load
    // error rather than a dimension quietly left unknown. A state carried
    // along unread is the lenient reading `canon` calls dangerous.
    if let Some(spelled) = spelled_on_a_line(&comments) {
        Representation::parse(spelled).map_err(|detail| LoadError::format(path, detail))?;
    }

    let notes = charge.against(path, stated(declared, &comments).as_ref())?;

    Ok(Dataset::new(
        kind,
        Some(path.to_path_buf()),
        dimensions(path, declared, &comments, has_keys),
    )
    .with_notes(notes))
}

/// The value a protonation block has when nothing was added or removed.
///
/// An InChIKey's third block is the protonation layer: `N` for a balance of
/// zero, `M`, `L`, `K` for one, two, three protons removed, and `O`, `P`,
/// `Q` for as many added. A molecule's net charge is not always in there --
/// a quaternary ammonium carries its charge in the InChI's `/q` layer and
/// still keys `N` -- which is exactly why this check is one-sided: a block
/// that is not `N` is a molecule the charge-neutraliser would have
/// neutralised, and a block that is `N` proves nothing.
const NEUTRAL: char = 'N';

/// What the table's keys say about its charge state.
///
/// Counted while the rows go past, since they are being read anyway.
#[derive(Default)]
struct ChargeState {
    /// How many well-formed InChIKeys were read.
    keys: u32,
    /// How many of them are not protonation-neutral.
    charged: u32,
    /// The first that is not, with its row and its block, for an error that
    /// names one thing rather than counting (D15).
    first_charged: Option<(usize, String, char)>,
}

impl ChargeState {
    /// Take in one row's key.
    fn read(&mut self, row: usize, key: &str) {
        let Some(block) = protonation_block(key) else {
            return;
        };
        self.keys += 1;
        if block != NEUTRAL {
            self.charged += 1;
            if self.first_charged.is_none() {
                self.first_charged = Some((row, key.to_owned(), block));
            }
        }
    }

    /// The keys read against the state the table says it is in.
    ///
    /// # Errors
    ///
    /// [`LoadError::Representation`] where the state says
    /// `charge-neutralised` and a key says it is not. That is the half of a
    /// representation the file can refute: D38 holds that a declaration is
    /// recorded rather than verified, and it holds because the engine
    /// usually *cannot* verify one -- salt-stripping and tautomer
    /// normalisation leave no trace in a key. Charge state does leave one,
    /// so refusing here is the same rule as a contig-naming mismatch rather
    /// than an exception to D38.
    fn against(
        &self,
        path: &Path,
        stated: Option<&(Representation, bool)>,
    ) -> Result<Vec<Note>, LoadError> {
        let Some((stated, by_declaration)) = stated else {
            return Ok(Vec::new());
        };
        if stated.identity() != Identity::InChIKey || self.keys == 0 {
            return Ok(Vec::new());
        }
        let applied = |operation| stated.applied().contains(&operation);

        if applied(Standardisation::ChargeNeutralised) {
            if let Some((row, key, block)) = &self.first_charged {
                return Err(LoadError::representation(
                    path,
                    refuted(
                        stated,
                        *by_declaration,
                        row,
                        key,
                        *block,
                        self.charged,
                        self.keys,
                    ),
                ));
            }
            return Ok(Vec::new());
        }

        // The suspicious direction, which the keys cannot settle, so a note
        // (#40). Public collections hand out neutral parents, so an
        // all-neutral table that declares only `salt-stripped` is more often
        // a table that was neutralised by a step nobody wrote down than one
        // whose compounds had nothing to neutralise.
        if applied(Standardisation::SaltStripped) && self.charged == 0 {
            return Ok(vec![Note::KeysLookNeutralised { keys: self.keys }]);
        }
        Ok(Vec::new())
    }
}

/// What to say to a table whose keys refute its own charge state.
///
/// Both halves of the remedy, because the engine cannot tell which of the
/// two is the mistake: a declaration naming work that was never done, or
/// work that was done after these keys were computed.
fn refuted(
    stated: &Representation,
    by_declaration: bool,
    row: &usize,
    key: &str,
    block: char,
    charged: u32,
    keys: u32,
) -> String {
    let said = if by_declaration {
        format!("representation is declared `{}`", stated.spell())
    } else {
        format!("this file's representation line says `{}`", stated.spell())
    };
    let counted = if charged == 1 {
        String::new()
    } else {
        format!(" ({charged} of {keys} keys are not)")
    };
    let honest = Representation::new(
        stated.identity(),
        stated
            .applied()
            .iter()
            .copied()
            .filter(|operation| *operation != Standardisation::ChargeNeutralised),
    );
    format!(
        "{said}, and the key in row {row} is not charge-neutralised{counted}: \
         `{key}` ends `{block}`, and an InChIKey's third block is its protonation \
         layer -- `{NEUTRAL}` is the only value it takes for a molecule with no proton \
         added or removed. So this is the one part of a representation a table's own keys \
         can refute. Either the state names standardisation this table did not get, in \
         which case `{honest}` is what these keys are in, or it was standardised after \
         the keys were computed and they have to be recomputed from the structures.",
        honest = honest.spell(),
        block = block,
    )
}

/// The protonation block of an InChIKey, if this is one.
///
/// `BSYNRYMUTXBXSQ-UHFFFAOYSA-M` is fourteen characters of skeleton, ten of
/// everything else, and one of protonation. Anything else is not a standard
/// InChIKey and is passed over rather than refused: the loader does not
/// validate identifiers, and a table whose keys are something else has a
/// bigger problem than its charge state, which `representation` is not the
/// axis to report.
fn protonation_block(key: &str) -> Option<char> {
    let mut blocks = key.split('-');
    let skeleton = blocks.next()?;
    let middle = blocks.next()?;
    let mut protonation = blocks.next()?.chars();
    if blocks.next().is_some() {
        return None;
    }
    let block = protonation.next()?;
    if protonation.next().is_some() {
        return None;
    }
    let shaped = |part: &str, length: usize| {
        part.len() == length && part.chars().all(|character| character.is_ascii_uppercase())
    };
    (shaped(skeleton, 14) && shaped(middle, 10) && block.is_ascii_uppercase()).then_some(block)
}

/// The representation a declaration or the file's own line states.
///
/// The second half of the pair says whether it was the declaration, which
/// decides nothing about the value and everything about how an error reads:
/// "representation is declared" and "this file's representation line says"
/// send a user to different places.
fn stated(declared: &Declared, comments: &[String]) -> Option<(Representation, bool)> {
    if let Some(spelled) = declared.representation.as_deref()
        && let Ok(stated) = Representation::parse(spelled)
    {
        return Some((stated, true));
    }
    spelled_on_a_line(comments)
        .and_then(|spelled| Representation::parse(spelled).ok())
        .map(|read| (read, false))
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
    match stated(declared, comments) {
        Some((stated, true)) => return Dim::known(stated, Source::Declared),
        Some((read, false)) => {
            return Dim::known(
                read,
                Source::Inferred {
                    path: path.to_path_buf(),
                    detail: "the representation line the standardiser wrote",
                },
            );
        }
        None => {}
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
