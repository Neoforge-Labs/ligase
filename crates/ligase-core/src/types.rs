//! Biological data types, and which dimensions each one can speak to.

use std::path::{Path, PathBuf};

use crate::dim::{Dimension, Dimensions};

/// What kind of biological data a dataset holds.
///
/// These are the types in the golden example, plus intervals: the reference
/// demo does not use them, but they are where the `coordinates` dimension
/// earns its keep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataKind {
    /// Unaligned sequencing reads (FASTQ).
    Reads,
    /// A reference sequence (FASTA).
    Reference,
    /// Reads placed on a reference (SAM/BAM).
    Alignment,
    /// Variant calls (VCF).
    Variants,
    /// Genomic intervals (BED/GFF).
    Intervals,
    /// An interpreted drug-resistance report.
    ResistanceReport,
}

impl DataKind {
    /// Every kind, so the Python layer can enumerate them.
    pub const ALL: [Self; 6] = [
        Self::Reads,
        Self::Reference,
        Self::Alignment,
        Self::Variants,
        Self::Intervals,
        Self::ResistanceReport,
    ];

    /// The name used in messages and in the Python API.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Reads => "Reads",
            Self::Reference => "Reference",
            Self::Alignment => "Alignment",
            Self::Variants => "Variants",
            Self::Intervals => "Intervals",
            Self::ResistanceReport => "ResistanceReport",
        }
    }

    /// Look a kind up by the name [`Self::name`] gives it.
    ///
    /// Used by the Python layer to turn a step's type annotation into a
    /// kind, so the two surfaces cannot drift apart.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// Whether a dimension means anything for this kind.
    ///
    /// This encodes the `n/a` cells of the table in
    /// `web/content/reference/dimensions.mdx`. A dimension that does not
    /// apply is never compared and never demanded: asking a FASTA for its
    /// quality encoding is not a gap to be filled, it is a category error.
    #[must_use]
    pub const fn applies(self, dimension: Dimension) -> bool {
        match (self, dimension) {
            // Organism and sample set are meaningful everywhere they can be
            // stated; a report inherits both from what it was built from.
            (_, Dimension::Organism) => true,

            (Self::Reads, Dimension::SampleSet | Dimension::Pairing) => true,
            (Self::Reads, Dimension::QualityEncoding) => true,
            (Self::Reads, _) => false,

            (Self::Reference, Dimension::Reference | Dimension::ContigNaming) => true,
            (Self::Reference, _) => false,

            // Pairing applies to an alignment (the SAM flags record it), but
            // inferring it means reading records rather than headers, which
            // M1 does not do. It therefore stays unknown for now.
            (
                Self::Alignment,
                Dimension::Reference
                | Dimension::ContigNaming
                | Dimension::Coordinates
                | Dimension::SampleSet
                | Dimension::Pairing,
            ) => true,
            (Self::Alignment, _) => false,

            (
                Self::Variants,
                Dimension::Reference
                | Dimension::ContigNaming
                | Dimension::Coordinates
                | Dimension::SampleSet,
            ) => true,
            (Self::Variants, _) => false,

            // An interval file describes regions, not samples: no sample
            // set, no pairing, no quality scores.
            (
                Self::Intervals,
                Dimension::Reference | Dimension::ContigNaming | Dimension::Coordinates,
            ) => true,
            (Self::Intervals, _) => false,

            (Self::ResistanceReport, Dimension::SampleSet) => true,
            (Self::ResistanceReport, _) => false,
        }
    }
}

/// A piece of data, its kind, and the dimensions it carries.
#[derive(Clone, Debug)]
pub struct Dataset {
    kind: DataKind,
    path: Option<PathBuf>,
    /// The other files of the same dataset: the second mate of a pair.
    ///
    /// Kept beside `path` rather than replacing it because every other kind
    /// of data is one file, and a dataset's *primary* file is what a
    /// message names.
    mates: Vec<PathBuf>,
    dimensions: Dimensions,
}

impl Dataset {
    /// Build a dataset from its kind and dimensions.
    #[must_use]
    pub const fn new(kind: DataKind, path: Option<PathBuf>, dimensions: Dimensions) -> Self {
        Self {
            kind,
            path,
            mates: Vec::new(),
            dimensions,
        }
    }

    /// The same dataset, with a second file belonging to it.
    #[must_use]
    pub fn with_mate(mut self, mate: PathBuf) -> Self {
        self.mates.push(mate);
        self
    }

    /// What kind of data this is.
    #[must_use]
    pub const fn kind(&self) -> DataKind {
        self.kind
    }

    /// The file it was loaded from, if it came from one.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Every file this dataset is, in order.
    ///
    /// One file for everything except a paired sample, which is two — and
    /// their order is the pairing, so a step is handed `r1` and `r2` the way
    /// round they were read.
    #[must_use]
    pub fn files(&self) -> Vec<PathBuf> {
        self.path.iter().chain(self.mates.iter()).cloned().collect()
    }

    /// The dimensions it carries.
    #[must_use]
    pub const fn dimensions(&self) -> &Dimensions {
        &self.dimensions
    }
}
