//! Biological data types, and which dimensions each one can speak to.

use std::path::{Path, PathBuf};

use crate::dim::{Dimension, Dimensions};

/// What kind of biological data a dataset holds.
///
/// These are the types in the golden example. Intervals (BED/GFF) arrive with
/// the graph in M2: the reference demo does not use them, and their dimensions
/// are almost entirely declared rather than inferred.
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
    /// An interpreted drug-resistance report.
    ResistanceReport,
}

impl DataKind {
    /// The name used in messages and in the Python API.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Reads => "Reads",
            Self::Reference => "Reference",
            Self::Alignment => "Alignment",
            Self::Variants => "Variants",
            Self::ResistanceReport => "ResistanceReport",
        }
    }

    /// Whether a dimension means anything for this kind.
    ///
    /// This encodes the `n/a` cells of the table in `docs/dimensions.md`. A
    /// dimension that does not apply is never compared and never demanded:
    /// asking a FASTA for its quality encoding is not a gap to be filled, it
    /// is a category error.
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
    dimensions: Dimensions,
}

impl Dataset {
    /// Build a dataset from its kind and dimensions.
    #[must_use]
    pub const fn new(kind: DataKind, path: Option<PathBuf>, dimensions: Dimensions) -> Self {
        Self {
            kind,
            path,
            dimensions,
        }
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

    /// The dimensions it carries.
    #[must_use]
    pub const fn dimensions(&self) -> &Dimensions {
        &self.dimensions
    }
}
