//! BED and GFF: contig names from records, coordinates from the format.
//!
//! This is where the `coordinates` dimension starts doing real work. BED is
//! 0-based half-open and GFF is 1-based closed, and feeding one to a tool
//! expecting the other is a bug no checksum comparison would catch — the
//! numbers come out plausible and off by one.
//!
//! Parsing is hand-rolled, as FASTA is, because only the first column is
//! needed: `noodles`' BED reader is generic over the field count, and
//! materialising nine typed columns per record to read one of them would buy
//! nothing. What it would cost is a pre-1.0 API surface in a hot loop.

use std::collections::HashMap;
use std::io::BufRead as _;
use std::path::Path;

use super::{Declared, LoadError, open};
use crate::dim::{Contig, CoordinateSystem, Dim, Dimensions, Organism, ReferenceId, Source};
use crate::types::{DataKind, Dataset};

/// Which interval format a file holds.
///
/// The distinction is not cosmetic: it is the coordinate convention, and
/// guessing it wrong shifts every interval by one base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntervalFormat {
    /// BED: 0-based, end-exclusive.
    Bed,
    /// GFF3 or GTF: 1-based, end-inclusive.
    Gff,
}

impl IntervalFormat {
    /// The coordinate convention the format defines.
    #[must_use]
    pub const fn coordinates(self) -> CoordinateSystem {
        match self {
            Self::Bed => CoordinateSystem::ZeroBasedHalfOpen,
            Self::Gff => CoordinateSystem::OneBasedClosed,
        }
    }

    /// The name used in messages and in the Python API.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bed => "bed",
            Self::Gff => "gff",
        }
    }

    /// Look a format up by the name [`Self::name`] gives it.
    ///
    /// `gff3` and `gtf` are accepted as spellings of `gff`, since they share
    /// the coordinate convention that is the only thing the format decides.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "bed" => Some(Self::Bed),
            "gff" | "gff3" | "gtf" => Some(Self::Gff),
            _ => None,
        }
    }

    /// The format a filename implies, ignoring any compression suffix.
    #[must_use]
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        let trimmed = name
            .strip_suffix(".gz")
            .or_else(|| name.strip_suffix(".bgz"))
            .unwrap_or(name.as_str());
        Self::from_name(trimmed.rsplit_once('.')?.1)
    }

    /// How a message should describe where the coordinate convention came
    /// from.
    const fn coordinate_detail(self) -> &'static str {
        match self {
            Self::Bed => "BED coordinate convention",
            Self::Gff => "GFF coordinate convention",
        }
    }

    /// How a message should describe where the contigs came from.
    const fn record_detail(self) -> &'static str {
        match self {
            Self::Bed => "BED records",
            Self::Gff => "GFF records",
        }
    }

    /// Why an interval file with no records knows no contigs.
    const fn empty_reason(self) -> &'static str {
        match self {
            Self::Bed => "the BED has no intervals, so it names no contigs",
            Self::Gff => "the GFF has no features, so it names no contigs",
        }
    }

    /// The least number of tab-separated fields a record must have.
    ///
    /// Enough to reject a file that is not this format at all — a CSV, say,
    /// whose every line would otherwise be read as one long contig name.
    const fn minimum_fields(self) -> usize {
        match self {
            // chrom, start, end.
            Self::Bed => 3,
            // seqid, source, type, start, end, score, strand, phase — the
            // ninth attributes column is sometimes written empty.
            Self::Gff => 8,
        }
    }
}

/// Load genomic intervals, inferring what a BED or GFF can tell us.
///
/// Contigs come from the records, as `docs/dimensions.md` specifies, and
/// carry neither a length nor a checksum — a BED records neither, and a GFF
/// records a length only in a `##sequence-region` directive. Comparing only
/// what is present is what lets these files be checked against a reference
/// at all; inventing the rest would be the lie the whole system guards
/// against.
///
/// Because the contigs come from records rather than from a sequence
/// dictionary, their order carries no meaning and is never compared. A BED
/// that happens to list contig 2 before contig 1 is not a contig order
/// mismatch.
pub fn intervals(
    path: &Path,
    format: Option<IntervalFormat>,
    declared: &Declared,
) -> Result<Dataset, LoadError> {
    let format = match format.or_else(|| IntervalFormat::from_path(path)) {
        Some(format) => format,
        None => {
            return Err(LoadError::format(
                path,
                "cannot tell whether this is a bed or a gff from its name; \
                 pass the format explicitly",
            ));
        }
    };

    let contigs = scan(path, format)?;

    let (reference, contig_naming) = if contigs.is_empty() {
        (
            Dim::unknown(format.empty_reason()),
            Dim::unknown(format.empty_reason()),
        )
    } else {
        let identity = ReferenceId::from_records(declared.build.clone(), contigs);
        let naming = identity.naming();
        let source = Source::Inferred {
            path: path.to_path_buf(),
            detail: format.record_detail(),
        };
        (
            Dim::known(identity, source.clone()),
            Dim::known(naming, source),
        )
    };

    let organism = match &declared.organism {
        Some(organism) => Dim::known(Organism::new(organism), Source::Declared),
        None => Dim::unknown("an interval file does not record its organism; declare it"),
    };

    let dimensions = Dimensions {
        organism,
        reference,
        contig_naming,
        coordinates: Dim::known(
            format.coordinates(),
            Source::Inferred {
                path: path.to_path_buf(),
                detail: format.coordinate_detail(),
            },
        ),
        ..Dimensions::default()
    };

    Ok(Dataset::new(
        DataKind::Intervals,
        Some(path.to_path_buf()),
        dimensions,
    ))
}

/// The contigs the records mention, in order of first appearance.
fn scan(path: &Path, format: IntervalFormat) -> Result<Vec<Contig>, LoadError> {
    let reader = open(path)?;
    let mut names: Vec<String> = Vec::new();
    let mut lengths: HashMap<String, u64> = HashMap::new();

    for line in reader.lines() {
        let line = line.map_err(|error| LoadError::io(path, error))?;
        let line = line.trim_end_matches(['\r', '\n']);

        if line.trim().is_empty() {
            continue;
        }

        if format == IntervalFormat::Gff {
            // Everything after this directive is sequence, not features. A
            // parser that kept reading would take `>1` for a contig name.
            if line.trim_end() == "##FASTA" {
                break;
            }
            if let Some(region) = line.strip_prefix("##sequence-region") {
                if let Some((name, length)) = sequence_region(region) {
                    lengths.insert(name, length);
                }
                continue;
            }
        }

        // `track` and `browser` lines are display directives that a real BED
        // carries; `#` is a comment in both formats.
        if line.starts_with('#')
            || (format == IntervalFormat::Bed
                && (line.starts_with("track") || line.starts_with("browser")))
        {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < format.minimum_fields() {
            return Err(LoadError::format(
                path,
                format!(
                    "a record has {} tab-separated fields; a {} record needs at least {}",
                    fields.len(),
                    format.name(),
                    format.minimum_fields()
                ),
            ));
        }

        let name = fields[0];
        if name.is_empty() {
            return Err(LoadError::format(path, "a record has an empty contig name"));
        }
        if !names.iter().any(|seen| seen == name) {
            names.push(name.to_owned());
        }
    }

    Ok(names
        .into_iter()
        .map(|name| Contig {
            length: lengths.get(&name).copied(),
            name,
            md5: None,
        })
        .collect())
}

/// A contig length from a `##sequence-region seqid start end` directive.
///
/// The length is taken only when the region starts at base 1. A directive
/// describing part of a contig says nothing about how long the contig is,
/// and a wrong length here would read as a reference mismatch.
fn sequence_region(rest: &str) -> Option<(String, u64)> {
    let mut fields = rest.split_whitespace();
    let name = fields.next()?;
    let start: u64 = fields.next()?.parse().ok()?;
    let end: u64 = fields.next()?.parse().ok()?;
    (start == 1).then(|| (name.to_owned(), end))
}
