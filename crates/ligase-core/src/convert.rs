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

use crate::dim::{Dimension, Dimensions, IdentifierNamespace, Source};
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
    /// One, in v0.2. The registry is the deliverable; see the module
    /// comment on the two entries that come next.
    pub const ALL: [Self; 1] = [Self {
        name: "map_identifiers",
        axis: Dimension::IdentifierNamespace,
        // Both kinds of named list, because a ranking crosses spaces as
        // readily as a set does and `graph::propagate` guards the axis
        // override on this list: a `map_identifiers` step annotated to
        // return a ranking and missing from here would carry the *input's*
        // namespace onto an output that has crossed. The guard is for a
        // kind with no such axis, and a ranked list has one.
        kinds: &[DataKind::EntitySet, DataKind::RankedList],
        supplies: "a mapping table",
    }];

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
}

impl Target {
    /// Which axis this sets.
    #[must_use]
    pub const fn axis(&self) -> Dimension {
        match self {
            Self::IdentifierNamespace(_) => Dimension::IdentifierNamespace,
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
        }
    }
}

// ------------------------------------------------- map_identifiers --

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
///   would be a guess about which the analysis meant;
/// - one whose target another identifier in the same list also reaches,
///   which is ordinary for a table holding both a deprecated symbol and its
///   replacement. Emitting both rows would make the output name one entity
///   twice with two sets of attributes, which is not a set, and keeping one
///   would be a guess about whose attributes to carry.
///
/// So the output is smaller than the input by exactly the identifiers the
/// notes name, and [`Note::IdentifiersDidNotCross`] states how many that is.
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
    let colliding = collisions(entities, id_column, &mapping)?;

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
        match mapping.get(&identifier).map(Vec::as_slice) {
            Some([target]) if !colliding.contains(target) => {
                let mut fields: Vec<&str> = row.line.split('\t').collect();
                fields[at] = target;
                written.push_str(&fields.join("\t"));
                written.push('\n');
            }
            Some([_]) => found.note(Note::IdentifierCollides { identifier }),
            Some(_) => found.note(Note::IdentifierAmbiguous { identifier }),
            None => found.note(Note::IdentifierUnmapped { identifier }),
        }
    }

    std::fs::write(out, written).map_err(|error| error.to_string())?;
    Ok(found.into_notes())
}

/// Which targets more than one of the list's identifiers reaches.
fn collisions(
    entities: &Path,
    id_column: &str,
    mapping: &BTreeMap<String, Vec<String>>,
) -> Result<BTreeSet<String>, String> {
    let reader = load::open(entities).map_err(|error| error.to_string())?;
    let mut rows = Table::read(reader).map_err(|error| error.to_string())?;
    let at = rows
        .column(id_column)
        .ok_or_else(|| missing(entities, id_column, rows.header()))?;

    let mut reached: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    while let Some(row) = rows.next_row().map_err(|error| error.to_string())? {
        let identifier = row.field(at).unwrap_or_default();
        // Only an unambiguous mapping puts an identifier anywhere; an
        // ambiguous one is left out on its own account and cannot collide.
        if let Some([target]) = mapping.get(identifier).map(Vec::as_slice) {
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
}

impl Noticed {
    fn note(&mut self, note: Note) {
        if !self.seen.insert(note.value()) {
            return;
        }
        self.count = self.count.saturating_add(1);
        if self.named.len() < NAMED_AT_MOST {
            self.named.push(note);
        }
    }

    /// The named notes, with the total beside them.
    ///
    /// The total is last so a report reads the examples and then the scale.
    fn into_notes(mut self) -> Vec<Note> {
        if self.count > 0 {
            self.named
                .push(Note::IdentifiersDidNotCross { count: self.count });
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
