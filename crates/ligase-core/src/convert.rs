//! The registered ways across a dimension.
//!
//! A dimension check refuses a join; it does not say how to get past one.
//! For most dimensions there is no past: two files about different organisms
//! are about different organisms, and no step changes that. For a few, there
//! is a real operation — a lookup, a coordinate translation, a renaming —
//! and the registry is where those live.
//!
//! # Why a registry rather than a function
//!
//! D5 calls for a **conversion graph**: the dimensions are nodes, the
//! conversions are edges, and v0.3's automatic fixes are a path search over
//! it. "Your HGNC list cannot join this Ensembl table, but
//! `map_identifiers` would get you there" is the one-edge case of that
//! search, and it is the only case v0.2 ships.
//!
//! A path search needs edges it can read, which is what makes this a table
//! and not three functions somebody calls. Each entry says which axis it
//! crosses, which kinds it crosses it for, and what the user must supply to
//! make the crossing — so a search can answer "what gets me from here to
//! there, and what will it cost me?" without running anything.
//!
//! # The shape, and the two entries that will test it
//!
//! `liftover` (`reference` to `reference`, given a chain file) and
//! `rename_contigs` (`contig_naming` to `contig_naming`, given nothing)
//! are the next two entries, and they are the reason for two choices here
//! that would otherwise look like over-design.
//!
//! `standardize` (`representation` to `representation`) is the second entry
//! and tested both choices early. It crosses one axis, it reads its
//! destination from the same `to=`, and it can fail for some inputs — a
//! structure that does not parse, a mixture with no single parent — which is
//! what keeps it a conversion rather than a rename. It is also the first
//! whose body is **Python**, because computing an InChIKey needs RDKit and
//! the engine core knows no chemistry (D49). Nothing here had to change for
//! that: an entry is metadata about an axis, and who runs the crossing is
//! the caller's business.
//!
//! The **axis** is a field rather than being implied by the conversion's
//! name, because [`Graph`](crate::graph)'s propagation needs to know which
//! one dimension of the output to overwrite and which to inherit — and
//! "inherit everything, then set one axis" is what makes a conversion carry
//! the organism, subject and release it was handed instead of losing them.
//!
//! The **target parameter is one spelling for every conversion**
//! ([`Conversion::TARGET`]). A conversion's output declares where it has
//! arrived, that declaration is per-call rather than per-step, and the
//! Python surface has to find it among a step's arguments without knowing
//! which conversion it is looking at. One name, checked when the step is
//! decorated, means `liftover(..., to="hg38")` and
//! `rename_contigs(..., to="chr-prefixed (UCSC style)")` are rows in
//! [`Conversion::ALL`] and nothing else.
//!
//! # What a conversion is not
//!
//! It is not a rename. Every entry here is an operation that can *fail* for
//! some inputs — an identifier with no mapping, a position in no chain
//! block — and what it does with those failures is the point: they are
//! named, never dropped in silence (D26). A conversion that could not fail
//! would be a rename, and a rename across a dimension is what D38 forbids.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::dim::{Dimension, Dimensions, IdentifierNamespace, Representation, Source};
use crate::load::{self, Table};
use crate::note::Note;
use crate::types::DataKind;

/// One registered way across a dimension, and what it needs to get there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conversion {
    /// The step name a user writes, and the name an error sends them to.
    pub name: &'static str,
    /// The one axis it crosses. Every other dimension is inherited.
    pub axis: Dimension,
    /// The kinds it converts. In and out are the same kind.
    pub kinds: &'static [DataKind],
    /// What the user has to supply, for a message that has to say so.
    pub supplies: &'static str,
}

impl Conversion {
    /// Every registered conversion.
    ///
    /// Two: `map_identifiers` from v0.2 and `standardize` from v0.3. The
    /// registry is the deliverable; see the module comment on the two
    /// entries that come next.
    pub const ALL: [Self; 2] = [
        Self {
            name: "map_identifiers",
            axis: Dimension::IdentifierNamespace,
            // Both kinds of named list, because a ranking crosses spaces as
            // readily as a set does and `graph::propagate` guards the axis
            // override on this list: a `map_identifiers` step annotated to
            // return a ranking and missing from here would carry the
            // *input's* namespace onto an output that has crossed. The
            // guard is for a kind with no such axis, and a ranked list has
            // one.
            kinds: &[DataKind::EntitySet, DataKind::RankedList],
            supplies: "a mapping table",
        },
        // The second edge, and the first whose body is Python rather than
        // Rust: computing an InChIKey needs RDKit, and the engine core knows
        // no chemistry (D49). The registry takes it without changing shape,
        // which is worth knowing before `liftover` arrives.
        //
        // Both molecule kinds, because a ranking is standardised as readily
        // as a set is and `graph::propagate` guards the axis override on
        // this list: a `standardize` step annotated to return a ranking and
        // missing from here would carry the *input's* representation onto an
        // output that has crossed.
        Self {
            name: "standardize",
            axis: Dimension::Representation,
            kinds: &[DataKind::MoleculeSet, DataKind::MoleculeRanking],
            supplies: "nothing but the state to arrive in",
        },
    ];

    /// The parameter every conversion reads its target from.
    ///
    /// One spelling for all of them, so that the Python surface can find a
    /// call's declared target without knowing which conversion it is.
    pub const TARGET: &'static str = "to";

    /// The conversion of this name, if one is registered.
    #[must_use]
    pub fn by_name(name: &str) -> Option<&'static Self> {
        Self::ALL.iter().find(|entry| entry.name == name)
    }

    /// The conversion that crosses this axis, if one is registered.
    ///
    /// `None` for every axis but one today, and that is the honest answer:
    /// there is no way across `organism`, and the two axes that will get
    /// one — `reference` and `contig_naming` — have not got it yet.
    #[must_use]
    pub fn across(axis: Dimension) -> Option<&'static Self> {
        Self::ALL.iter().find(|entry| entry.axis == axis)
    }

    /// Whether this conversion applies to that kind of data.
    #[must_use]
    pub fn converts(&self, kind: DataKind) -> bool {
        self.kinds.contains(&kind)
    }

    /// The target this conversion declares, read from a user's `to=`.
    ///
    /// # Errors
    ///
    /// If the axis cannot take that target. A blank is the case that
    /// matters: a blank label compares equal to another blank label, so a
    /// conversion that accepted one would produce two outputs that *agree*
    /// about being in no particular space.
    pub fn target(&self, declared: &str) -> Result<Target, String> {
        match self.axis {
            Dimension::IdentifierNamespace => {
                let namespace = IdentifierNamespace::new(declared);
                if namespace.is_empty() {
                    return Err(format!(
                        "`{}` needs a namespace to convert to; `{}` was blank",
                        self.name,
                        Self::TARGET
                    ));
                }
                Ok(Target::IdentifierNamespace(namespace))
            }
            Dimension::Representation => Representation::parse(declared)
                .map(Target::Representation)
                .map_err(|detail| format!("`{}`: {detail}", self.name)),
            // Unreachable while `ALL` holds one entry, and the arm that
            // `liftover` and `rename_contigs` will each replace. Stated as
            // an error rather than a panic: a registry entry whose axis has
            // no target type is a bug in the registry, and a message naming
            // the axis is what tells whoever added the row.
            other => Err(format!(
                "`{}` is registered across {other}, which has no target type yet",
                self.name
            )),
        }
    }
}

/// What a conversion sets on its output's one axis.
///
/// An enum rather than a string, so that the value a conversion declares is
/// the same type the dimension holds and cannot be put in the wrong field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// The identifier space the output is in.
    IdentifierNamespace(IdentifierNamespace),
    /// The standardisation state the output's names were arrived at by.
    Representation(Representation),
}

impl Target {
    /// Which axis this sets.
    #[must_use]
    pub const fn axis(&self) -> Dimension {
        match self {
            Self::IdentifierNamespace(_) => Dimension::IdentifierNamespace,
            Self::Representation(_) => Dimension::Representation,
        }
    }

    /// Set it on a propagated output, replacing whatever was inherited.
    ///
    /// Replacing rather than filling a gap: the input states the axis, the
    /// output has just inherited that statement, and the whole point of the
    /// step is that it is no longer true.
    pub fn apply(&self, dimensions: &mut Dimensions, source: Source) {
        match self {
            Self::IdentifierNamespace(namespace) => {
                dimensions.identifier_namespace = crate::dim::Dim::known(namespace.clone(), source);
            }
            Self::Representation(representation) => {
                dimensions.representation = crate::dim::Dim::known(representation.clone(), source);
            }
        }
    }
}

// ------------------------------------------------- map_identifiers --

/// What becomes of an identifier the mapping table sends to several
/// targets.
///
/// A parameter rather than a default, because the two answers mean different
/// things about the biology and the engine cannot tell which holds. A symbol
/// in two rows because somebody's table is untidy wants
/// [`Self::Drop`](Ambiguous::Drop); *P. falciparum*'s `DHFR-TS`, one
/// polypeptide with two catalytic domains answering to human `DHFR` and
/// `TYMS`, wants [`Self::FanOut`](Ambiguous::FanOut) — and "decide which
/// target you mean" has no correct answer for it. So the caller declares,
/// the note says the choice was theirs, and the parameter joins the cache
/// key like any other (D60).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ambiguous {
    /// Leave the identifier out of the output and name it. The default, and
    /// what D38 has always done.
    #[default]
    Drop,
    /// Emit a row for each target, and say how many identifiers fanned and
    /// into how many rows.
    FanOut,
}

impl Ambiguous {
    /// The spelling a caller writes, for the Python surface.
    pub const DROP: &'static str = "drop";
    /// The spelling a caller writes to ask for a fan-out.
    pub const FAN_OUT: &'static str = "fan-out";

    /// Read a caller's spelling.
    ///
    /// # Errors
    ///
    /// For anything else, naming both spellings. A silent fallback to the
    /// default would turn a typo into the quiet loss of every ambiguous
    /// identifier.
    pub fn parse(declared: &str) -> Result<Self, String> {
        match declared.trim() {
            Self::DROP => Ok(Self::Drop),
            Self::FAN_OUT => Ok(Self::FanOut),
            other => Err(format!(
                "`ambiguous` is `{}` or `{}`; `{other}` is neither",
                Self::DROP,
                Self::FAN_OUT
            )),
        }
    }

    /// Whether an identifier with several targets emits a row for each.
    #[must_use]
    pub const fn fans_out(self) -> bool {
        matches!(self, Self::FanOut)
    }
}

/// How many identifiers that did not cross are named individually.
///
/// A list of twenty thousand converted through a table covering most of it
/// leaves thousands behind. Naming every one puts a row in provenance for
/// each and interpolates all of them into a single methods sentence, so the
/// named ones stop here and [`Note::IdentifiersDidNotCross`] carries the
/// total. Twenty is enough to recognise a pattern — a whole gene family, a
/// prefix, a release boundary — which is what naming them was for.
const NAMED_AT_MOST: usize = 20;

/// Rewrite a gene list's identifiers through a mapping table.
///
/// The table is read as `table_id_column` to `column`: one row per source
/// identifier, or several where a source identifier has several targets.
/// The output is the input's header and rows with the identifier column's
/// values replaced.
///
/// An identifier that does not cross is **left out and named** in a
/// [`Note`], never dropped in silence and never carried over unchanged
/// (D26). Carrying it would put a name from the old space into a set that
/// declares the new one, which is the implicit rename D38 exists to forbid.
/// Three kinds do not cross, and the first two are mirror images:
///
/// - one the table has no row for, which is usually a symbol renamed or
///   retired since the table was downloaded;
/// - one the table sends to several targets, which is ordinary for
///   symbol-to-gene mappings. Emitting a row for each would make the output
///   larger than the input without saying so, and choosing one of them
///   would be a guess about which the analysis meant — unless the caller
///   has declared `ambiguous` to be [`Ambiguous::FanOut`], which is the one
///   case where a row per target is what was asked for (D60);
/// - one whose target another identifier in the same list also reaches,
///   which is ordinary for a table holding both a deprecated symbol and its
///   replacement. Emitting both rows would make the output name one entity
///   twice with two sets of attributes, which is not a set, and keeping one
///   would be a guess about whose attributes to carry. A fan-out does not
///   suspend this: a target two identifiers arrive at is left out however
///   either of them arrived.
///
/// So the output is smaller than the input by exactly the identifiers the
/// notes name, and [`Note::IdentifiersDidNotCross`] states how many that
/// is — or, where a fan-out was asked for, larger by exactly what
/// [`Note::IdentifiersFannedOut`] states.
///
/// # Errors
///
/// If either file cannot be read, has no header, has no column of the name
/// asked for, or holds a row its header cannot account for — and if
/// `column` names the table's own identifier column, which would map every
/// identifier to itself and then declare a space the identifiers are not
/// in.
pub fn map_identifiers(
    entities: &Path,
    id_column: &str,
    table: &Path,
    table_id_column: &str,
    column: &str,
    ambiguous: Ambiguous,
    out: &Path,
) -> Result<Vec<Note>, String> {
    if column.trim() == table_id_column.trim() {
        return Err(format!(
            "`{column}` is the mapping table's own identifier column, so every identifier \
             would map to itself; name the column holding the identifiers to convert to"
        ));
    }

    let mapping = read_mapping(table, table_id_column, column)?;

    // Read the list once to find out which targets more than one of its
    // identifiers reaches. A collision cannot be decided row by row: the
    // second identifier to arrive is what makes the first one a problem,
    // and by then the first has already been written.
    let colliding = collisions(entities, id_column, &mapping, ambiguous)?;

    let reader = load::open(entities).map_err(|error| error.to_string())?;
    let mut rows = Table::read(reader).map_err(|error| error.to_string())?;
    let at = rows
        .column(id_column)
        .ok_or_else(|| missing(entities, id_column, rows.header()))?;

    let mut written = String::new();
    written.push_str(&rows.header().join("\t"));
    written.push('\n');

    let mut found = Noticed::default();
    while let Some(row) = rows.next_row().map_err(|error| error.to_string())? {
        let identifier = row.field(at).unwrap_or_default().to_owned();
        let write = |written: &mut String, target: &str| {
            let mut fields: Vec<&str> = row.line.split('\t').collect();
            fields[at] = target;
            written.push_str(&fields.join("\t"));
            written.push('\n');
        };
        match mapping.get(&identifier).map(Vec::as_slice) {
            Some([target]) if !colliding.contains(target) => write(&mut written, target),
            Some([_]) => found.note(Note::IdentifierCollides { identifier }),
            // Several targets, and the caller has said a row for each is
            // what they mean. Every target still has to clear the collision
            // rule, and an identifier all of whose targets collide is a
            // collision rather than a fan: it reached the output nowhere.
            Some(targets) if ambiguous.fans_out() => {
                let crossed: Vec<&String> = targets
                    .iter()
                    .filter(|target| !colliding.contains(*target))
                    .collect();
                for target in &crossed {
                    write(&mut written, target);
                }
                if crossed.is_empty() {
                    found.note(Note::IdentifierCollides { identifier });
                } else {
                    found.fanned(crossed.len(), Note::IdentifierFannedOut { identifier });
                }
            }
            Some(_) => found.note(Note::IdentifierAmbiguous { identifier }),
            None => found.note(Note::IdentifierUnmapped { identifier }),
        }
    }

    std::fs::write(out, written).map_err(|error| error.to_string())?;
    Ok(found.into_notes())
}

/// Which targets more than one of the list's identifiers reaches.
///
/// What counts as "reaches" depends on `ambiguous`, and getting that wrong
/// would be a silent way to break the set property: under
/// [`Ambiguous::Drop`] an ambiguous identifier lands nowhere and so cannot
/// collide, but under [`Ambiguous::FanOut`] it lands at every one of its
/// targets and can collide at each.
fn collisions(
    entities: &Path,
    id_column: &str,
    mapping: &BTreeMap<String, Vec<String>>,
    ambiguous: Ambiguous,
) -> Result<BTreeSet<String>, String> {
    let reader = load::open(entities).map_err(|error| error.to_string())?;
    let mut rows = Table::read(reader).map_err(|error| error.to_string())?;
    let at = rows
        .column(id_column)
        .ok_or_else(|| missing(entities, id_column, rows.header()))?;

    let mut reached: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    while let Some(row) = rows.next_row().map_err(|error| error.to_string())? {
        let identifier = row.field(at).unwrap_or_default();
        let targets = match mapping.get(identifier).map(Vec::as_slice) {
            // One target always puts the identifier there.
            Some(targets @ [_]) => targets,
            // Several put it at each where a fan-out was asked for, and
            // nowhere otherwise.
            Some(targets) if ambiguous.fans_out() => targets,
            Some(_) | None => continue,
        };
        for target in targets {
            reached
                .entry(target.clone())
                .or_default()
                .insert(identifier.to_owned());
        }
    }

    Ok(reached
        .into_iter()
        .filter(|(_, sources)| sources.len() > 1)
        .map(|(target, _)| target)
        .collect())
}

/// The notes a conversion has gathered, named up to a cap and counted in
/// full.
///
/// Deduplication is by a set rather than by scanning the notes, because the
/// scan is quadratic in the number of notes and the number of notes is the
/// size of the user's data.
#[derive(Default)]
struct Noticed {
    named: Vec<Note>,
    seen: BTreeSet<String>,
    count: u32,
    fanned: u32,
    fanned_rows: u32,
}

impl Noticed {
    fn note(&mut self, note: Note) {
        if !self.remember(note.clone()) {
            return;
        }
        self.count = self.count.saturating_add(1);
    }

    /// An identifier that crossed into `rows` rows rather than failing to
    /// cross at all.
    ///
    /// Counted apart from [`Self::note`] because the two totals say
    /// opposite things about the output's size, and one counter serving
    /// both would report a fan-out as a loss.
    fn fanned(&mut self, rows: usize, note: Note) {
        if !self.remember(note) {
            return;
        }
        self.fanned = self.fanned.saturating_add(1);
        self.fanned_rows = self
            .fanned_rows
            .saturating_add(u32::try_from(rows).unwrap_or(u32::MAX));
    }

    /// Record a note unless this identifier already has one, and say
    /// whether it was new.
    ///
    /// One note per identifier: the dedup key is the identifier itself, so
    /// an identifier cannot be both named as fanned and named as dropped,
    /// and the arm that noticed it is the one that decided which it was.
    fn remember(&mut self, note: Note) -> bool {
        if !self.seen.insert(note.value()) {
            return false;
        }
        if self.named.len() < NAMED_AT_MOST {
            self.named.push(note);
        }
        true
    }

    /// The named notes, with the totals beside them.
    ///
    /// The totals are last so a report reads the examples and then the
    /// scale. Both can be present at once: a mapping table may send one
    /// identifier nowhere and another to two places in the same pass, and
    /// the output's size is then the input's less the first and plus the
    /// second.
    fn into_notes(mut self) -> Vec<Note> {
        if self.count > 0 {
            self.named
                .push(Note::IdentifiersDidNotCross { count: self.count });
        }
        if self.fanned > 0 {
            self.named.push(Note::IdentifiersFannedOut {
                identifiers: self.fanned,
                rows: self.fanned_rows,
            });
        }
        self.named
    }
}

/// The mapping table as source identifier to every target it names.
///
/// A `Vec` per key rather than one value, because the one-to-many case is
/// the whole reason the ambiguous note exists: a map that kept the last row
/// would resolve it silently, and which row came last is not a fact about
/// the genes.
fn read_mapping(
    path: &Path,
    id_column: &str,
    column: &str,
) -> Result<BTreeMap<String, Vec<String>>, String> {
    let reader = load::open(path).map_err(|error| error.to_string())?;
    let mut table = Table::read(reader).map_err(|error| error.to_string())?;
    let from = table
        .column(id_column)
        .ok_or_else(|| missing(path, id_column, table.header()))?;
    let to = table
        .column(column)
        .ok_or_else(|| missing(path, column, table.header()))?;

    let mut mapping: BTreeMap<String, Vec<String>> = BTreeMap::new();
    while let Some(row) = table.next_row().map_err(|error| error.to_string())? {
        let (Some(source), Some(target)) = (row.field(from), row.field(to)) else {
            continue;
        };
        if source.is_empty() || target.is_empty() {
            continue;
        }
        let targets = mapping.entry(source.to_owned()).or_default();
        if !targets.iter().any(|known| known == target) {
            targets.push(target.to_owned());
        }
    }
    Ok(mapping)
}

/// Why a column a caller asked for is not there, naming the ones that are.
fn missing(path: &Path, wanted: &str, header: &[String]) -> String {
    format!(
        "{} has no column named `{wanted}`; its columns are {}",
        crate::dim::display_path(path),
        header.join(", ")
    )
}
