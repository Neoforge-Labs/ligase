//! The dimensions a biological type carries, and where each value came from.
//!
//! The catalogue is `web/content/reference/dimensions.mdx`: organism,
//! reference, contig naming, coordinates, sample set, pairing, quality
//! encoding. Every value travels with its provenance, so an error message can
//! say *where* a value came from without reconstructing it after the fact.
//! That provenance is the part that makes the error useful: "these two
//! references disagree" sends a user hunting, while "one came from a BAM
//! `@SQ` header and the other from the FASTA you passed" tells them what to
//! change.

mod names;
mod reference;

use std::fmt;
use std::path::PathBuf;

pub(crate) use names::english_list;
pub use names::{Label, NameSet, Organism, Release, SourceSet, Subject};
pub use reference::{Contig, ContigOrder, Frame, ReferenceId, Relation, Verdict, relate};

/// Where a dimension's value came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Read out of a file's header or contents.
    Inferred {
        /// The file it was read from.
        path: PathBuf,
        /// What part of it, e.g. `BAM @SQ header`.
        detail: &'static str,
    },
    /// Stated by the user when the data was declared.
    Declared,
    /// Carried through from an upstream step's output.
    Derived {
        /// The step it came from.
        step: String,
    },
}

impl Source {
    /// A phrase describing this source, for an error message.
    ///
    /// The preposition belongs to the source rather than to the template
    /// around it: "from a BAM @SQ header" reads naturally where "from
    /// declared" does not.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Inferred { path, detail } => {
                format!("from {detail}: {}", display_path(path))
            }
            Self::Declared => "declared".to_owned(),
            Self::Derived { step } => format!("from step `{step}`"),
        }
    }
}

/// Render a path relative to the working directory when it sits underneath
/// it, so messages stay short and comparable between machines.
pub(crate) fn display_path(path: &std::path::Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

/// A dimension's value, together with where it came from — or an honest gap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dim<T> {
    /// A value, and its provenance.
    Known {
        /// The value.
        value: T,
        /// Where it came from.
        source: Source,
    },
    /// No value, and why not.
    Unknown {
        /// Why the value could not be determined, phrased for a user.
        reason: &'static str,
    },
}

impl<T> Dim<T> {
    /// A known value.
    pub const fn known(value: T, source: Source) -> Self {
        Self::Known { value, source }
    }

    /// An unknown value, with a reason to show the user.
    #[must_use]
    pub const fn unknown(reason: &'static str) -> Self {
        Self::Unknown { reason }
    }

    /// The value, if known.
    #[must_use]
    pub const fn value(&self) -> Option<&T> {
        match self {
            Self::Known { value, .. } => Some(value),
            Self::Unknown { .. } => None,
        }
    }

    /// Where the value came from, if known.
    #[must_use]
    pub const fn source(&self) -> Option<&Source> {
        match self {
            Self::Known { source, .. } => Some(source),
            Self::Unknown { .. } => None,
        }
    }

    /// Why the value is unknown, if it is.
    #[must_use]
    pub const fn reason(&self) -> Option<&'static str> {
        match self {
            Self::Known { .. } => None,
            Self::Unknown { reason } => Some(reason),
        }
    }

    /// Whether a value is present.
    #[must_use]
    pub const fn is_known(&self) -> bool {
        matches!(self, Self::Known { .. })
    }
}

/// The dimension axes, for iteration and error reporting.
///
/// The first seven are in catalogue order. Newer axes are appended rather than
/// slotted in beside the ones they resemble: [`Dimension::ALL`] is the order
/// `check_step` walks, so it is also the order a user's multi-error report
/// comes out in, and inserting an axis mid-list would silently reorder every
/// report that already exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dimension {
    /// Which organism the data is from.
    Organism,
    /// Which reference build the data is on.
    Reference,
    /// `chr1` against `1`.
    ContigNaming,
    /// 0-based half-open against 1-based closed.
    Coordinates,
    /// The ordered sample identifiers.
    SampleSet,
    /// Single-end against paired-end.
    Pairing,
    /// Phred+33 against Phred+64.
    QualityEncoding,
    /// Which release of a reference the data is on.
    Release,
    /// Who or what the data is about.
    Subject,
    /// The upstream sources a dataset was assembled from.
    SourceSet,
}

impl Dimension {
    /// Every dimension: the catalogue's seven in catalogue order, then the
    /// axes added since, in the order they were added.
    pub const ALL: [Self; 10] = [
        Self::Organism,
        Self::Reference,
        Self::ContigNaming,
        Self::Coordinates,
        Self::SampleSet,
        Self::Pairing,
        Self::QualityEncoding,
        Self::Release,
        Self::Subject,
        Self::SourceSet,
    ];

    /// The name used in messages and in the Python API.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Organism => "organism",
            Self::Reference => "reference",
            Self::ContigNaming => "contig_naming",
            Self::Coordinates => "coordinates",
            Self::SampleSet => "sample_set",
            Self::Pairing => "pairing",
            Self::QualityEncoding => "quality_encoding",
            Self::Release => "release",
            Self::Subject => "subject",
            Self::SourceSet => "source_set",
        }
    }

    /// Look a dimension up by the name [`Self::name`] gives it.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|dimension| dimension.name() == name)
    }

    /// The leading question an error on this dimension should ask.
    #[must_use]
    pub const fn leading_question(self) -> &'static str {
        match self {
            Self::Organism => "These must match. Did you pass data from the wrong organism?",
            Self::Reference => "These must match. Did you pass the wrong reference?",
            Self::ContigNaming => {
                "The contigs are otherwise identical. Add a renaming step, or use one \
                 naming style throughout."
            }
            Self::Coordinates => {
                "These must match. Did you mean to convert between coordinate systems?"
            }
            Self::SampleSet => "These must match, in order. Did the sample order change upstream?",
            Self::Pairing => "These must match. Did you mix single-end and paired-end reads?",
            Self::QualityEncoding => "These must match. Did you mix Phred+33 and Phred+64 reads?",
            Self::Release => "These must match. Did one input come from a different release?",
            Self::Subject => "These must match. Did you join data about different subjects?",
            Self::SourceSet => "These must match. Was a source added or removed upstream?",
        }
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// How contigs are named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContigNaming {
    /// Every name carries a `chr` prefix, as UCSC references do.
    Prefixed,
    /// No name carries a `chr` prefix, as Ensembl and NCBI references do.
    Bare,
    /// Some names are prefixed and some are not — usually a merged
    /// reference, and worth knowing about on its own.
    Mixed,
}

impl ContigNaming {
    /// How to render this for a user.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Prefixed => "chr-prefixed (UCSC style)",
            Self::Bare => "bare (Ensembl/NCBI style)",
            Self::Mixed => "mixed",
        }
    }
}

/// Which coordinate convention a step sees.
///
/// This is semantics, not encoding (decision D9): SAM and BAM both present
/// 1-based closed records, so a lossless re-encoding does not change it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinateSystem {
    /// BED-style: 0-based, end-exclusive.
    ZeroBasedHalfOpen,
    /// SAM/VCF/GFF-style: 1-based, end-inclusive.
    OneBasedClosed,
}

impl CoordinateSystem {
    /// Both conventions, for listing them in an error message.
    pub const ALL: [Self; 2] = [Self::ZeroBasedHalfOpen, Self::OneBasedClosed];

    /// How to render this for a user.
    ///
    /// Doubles as the name the Python API accepts: there is no second
    /// spelling to keep in step, and the string a user writes is the string
    /// an error shows them.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::ZeroBasedHalfOpen => "0-based half-open",
            Self::OneBasedClosed => "1-based closed",
        }
    }

    /// Look a convention up by the name [`Self::describe`] gives it.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|system| system.describe() == name.trim())
    }
}

/// The samples a dataset covers, in order.
///
/// Order is part of the identity: `web/content/reference/dimensions.mdx`
/// requires sample sets to match by identity *and* order wherever steps join
/// samples, because a shuffled sample column produces plausible numbers for
/// the wrong patients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampleSet(Vec<String>);

impl SampleSet {
    /// Record a sample set in the order the file lists it.
    #[must_use]
    pub const fn new(samples: Vec<String>) -> Self {
        Self(samples)
    }

    /// The samples, in order.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    /// Whether this holds the same samples as `other`, in any order.
    #[must_use]
    pub fn same_members(&self, other: &Self) -> bool {
        let mut mine = self.0.clone();
        let mut theirs = other.0.clone();
        mine.sort_unstable();
        theirs.sort_unstable();
        mine == theirs
    }
}

impl fmt::Display for SampleSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]", self.0.join(", "))
    }
}

/// Whether reads are single-end or paired-end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pairing {
    /// One read per fragment.
    Single,
    /// Two reads per fragment.
    Paired,
}

impl Pairing {
    /// How to render this for a user.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Single => "single-end",
            Self::Paired => "paired-end",
        }
    }
}

/// How FASTQ quality scores are offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualityEncoding {
    /// Sanger / Illumina 1.8+: ASCII minus 33.
    Phred33,
    /// Illumina 1.3–1.7: ASCII minus 64.
    Phred64,
}

impl QualityEncoding {
    /// How to render this for a user.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Phred33 => "Phred+33",
            Self::Phred64 => "Phred+64",
        }
    }
}

/// Every dimension of one dataset.
///
/// Fields are public because this is a record, and the loaders fill in the
/// ones their format can speak to. Everything starts unknown: a dimension a
/// format cannot express must not masquerade as a value.
#[derive(Clone, Debug)]
pub struct Dimensions {
    /// Which organism the data is from.
    pub organism: Dim<Organism>,
    /// Which reference build the data is on.
    pub reference: Dim<ReferenceId>,
    /// How the contigs are named.
    pub contig_naming: Dim<ContigNaming>,
    /// Which coordinate convention the records use.
    pub coordinates: Dim<CoordinateSystem>,
    /// The samples covered, in order.
    pub sample_set: Dim<SampleSet>,
    /// Single-end or paired-end.
    pub pairing: Dim<Pairing>,
    /// The FASTQ quality offset.
    pub quality_encoding: Dim<QualityEncoding>,
    /// Which release of the reference this is.
    pub release: Dim<Release>,
    /// Who or what the data is about.
    pub subject: Dim<Subject>,
    /// The upstream sources the data was assembled from.
    pub source_set: Dim<SourceSet>,
}

impl Default for Dimensions {
    fn default() -> Self {
        const NOT_SET: &str = "not recorded in the file and not declared";
        Self {
            organism: Dim::unknown(NOT_SET),
            reference: Dim::unknown(NOT_SET),
            contig_naming: Dim::unknown(NOT_SET),
            coordinates: Dim::unknown(NOT_SET),
            sample_set: Dim::unknown(NOT_SET),
            pairing: Dim::unknown(NOT_SET),
            quality_encoding: Dim::unknown(NOT_SET),
            release: Dim::unknown(NOT_SET),
            subject: Dim::unknown(NOT_SET),
            source_set: Dim::unknown(NOT_SET),
        }
    }
}

impl Dimensions {
    /// Render a dimension's value, if known.
    #[must_use]
    pub fn render(&self, dimension: Dimension) -> Option<String> {
        match dimension {
            Dimension::Organism => self.organism.value().map(ToString::to_string),
            Dimension::Reference => self.reference.value().map(ReferenceId::describe),
            Dimension::ContigNaming => self
                .contig_naming
                .value()
                .map(|naming| naming.describe().to_owned()),
            Dimension::Coordinates => self
                .coordinates
                .value()
                .map(|system| system.describe().to_owned()),
            Dimension::SampleSet => self.sample_set.value().map(ToString::to_string),
            Dimension::Pairing => self.pairing.value().map(|p| p.describe().to_owned()),
            Dimension::QualityEncoding => self
                .quality_encoding
                .value()
                .map(|encoding| encoding.describe().to_owned()),
            Dimension::Release => self.release.value().map(ToString::to_string),
            Dimension::Subject => self.subject.value().map(ToString::to_string),
            Dimension::SourceSet => self.source_set.value().map(ToString::to_string),
        }
    }

    /// Where a dimension's value came from, if known.
    #[must_use]
    pub const fn source(&self, dimension: Dimension) -> Option<&Source> {
        match dimension {
            Dimension::Organism => self.organism.source(),
            Dimension::Reference => self.reference.source(),
            Dimension::ContigNaming => self.contig_naming.source(),
            Dimension::Coordinates => self.coordinates.source(),
            Dimension::SampleSet => self.sample_set.source(),
            Dimension::Pairing => self.pairing.source(),
            Dimension::QualityEncoding => self.quality_encoding.source(),
            Dimension::Release => self.release.source(),
            Dimension::Subject => self.subject.source(),
            Dimension::SourceSet => self.source_set.source(),
        }
    }

    /// Why a dimension is unknown, if it is.
    #[must_use]
    pub const fn reason(&self, dimension: Dimension) -> Option<&'static str> {
        match dimension {
            Dimension::Organism => self.organism.reason(),
            Dimension::Reference => self.reference.reason(),
            Dimension::ContigNaming => self.contig_naming.reason(),
            Dimension::Coordinates => self.coordinates.reason(),
            Dimension::SampleSet => self.sample_set.reason(),
            Dimension::Pairing => self.pairing.reason(),
            Dimension::QualityEncoding => self.quality_encoding.reason(),
            Dimension::Release => self.release.reason(),
            Dimension::Subject => self.subject.reason(),
            Dimension::SourceSet => self.source_set.reason(),
        }
    }

    /// Whether a dimension has a value.
    #[must_use]
    pub const fn is_known(&self, dimension: Dimension) -> bool {
        self.source(dimension).is_some()
    }
}
