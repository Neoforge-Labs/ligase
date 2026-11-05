//! Loading data and inferring its dimensions from file headers.
//!
//! Each loader reads only as much of a file as it needs, and reports a
//! dimension as unknown rather than guessing.
//! `web/content/reference/dimensions.mdx` says which dimensions each format
//! can speak to; anything outside that is declared by the user or left
//! unknown.

mod alignment;
mod entity_set;
mod fasta;
mod graph;
mod intervals;
mod knowledge_graph;
mod molecule_table;
mod named_list;
mod ranked_list;
mod reads;
mod table;
mod variants;

use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use flate2::bufread::MultiGzDecoder;

use crate::dim::{Dim, Organism, Release, Source, SourceSet, Subject};

pub use alignment::alignment;
pub use entity_set::entity_set;
pub use fasta::reference;
pub use intervals::{IntervalFormat, intervals};
pub use knowledge_graph::knowledge_graph;
pub use molecule_table::{molecule_ranking, molecule_set};
pub use ranked_list::ranked_list;
pub use reads::{reads, samples};
pub use variants::variants;

pub(crate) use graph::GraphReader;
pub(crate) use molecule_table::{REPRESENTATION_LINE, spelled_on_a_line};
pub(crate) use table::{Row, Table};

/// Dimensions the user states, for the ones a file cannot record.
///
/// `web/content/reference/dimensions.mdx` marks these "declared": stated
/// once, then propagated.
#[derive(Clone, Debug, Default)]
pub struct Declared {
    /// Which organisms the data is from, e.g. `["HIV-1"]`.
    ///
    /// A list because a host–pathogen dataset, a metagenome or a xenograft is
    /// legitimately about more than one. An empty list, or one of only blank
    /// names, declares nothing and is treated as not declared.
    pub organism: Option<Vec<String>>,
    /// A human label for the reference build, e.g. `HXB2` or `hg38`.
    ///
    /// Display only. Reference *identity* never rests on a label, so two
    /// files both labelled `hg38` still fail to match if their contigs
    /// disagree.
    pub build: Option<String>,
    /// The sample identifiers, in order, when the filename should not be
    /// trusted to supply them.
    pub samples: Option<Vec<String>>,
    /// The coordinate frame a reference's positions are numbered along, as
    /// `(frame, first)`: base 1 of the file is base `first` of `frame`.
    ///
    /// Declared and never inferred, for the reason [`crate::dim::Frame`]
    /// gives: no FASTA records where its first base sits in anything else.
    pub frame: Option<(String, u64)>,
    /// The release of the reference, e.g. `GRCh38.p14`.
    ///
    /// A reference and either kind of named list carry one; the other
    /// loaders ignore it. A blank release declares nothing and is treated
    /// as not declared.
    pub release: Option<String>,
    /// Which identifier space a set of entities is named in, e.g.
    /// `HGNC symbol`.
    ///
    /// Read by the two loaders of named lists, `entity_set` and
    /// `ranked_list`. Left unset, the loader reads the space off the
    /// identifiers themselves where they agree unambiguously; a blank
    /// declares nothing and is treated as not declared. It is deliberately
    /// not checked against the engine's catalogue of known spaces, because
    /// a curated list's own spelling is a space too — see
    /// `load::named_list`.
    pub identifier_namespace: Option<String>,
    /// What the data is about when that is one named thing: a disease, a
    /// cell line, a cohort.
    ///
    /// Read by the two loaders of named lists and by the graph loader. A
    /// blank declares nothing.
    pub subject: Option<String>,
    /// The sources the data was assembled from, e.g.
    /// `["open-targets", "disgenet"]`.
    ///
    /// Read by `knowledge_graph`, the one kind that is assembled. Left
    /// unset, the loader reads the sources off the graph's own nodes and
    /// edges; a list naming none declares nothing and is treated as not
    /// declared. Compared **exactly**, case included, because a source is
    /// usually an object key or a URL and both are case-sensitive by
    /// specification (D36).
    pub source_set: Option<Vec<String>>,
    /// How a molecule table's names were arrived at, e.g.
    /// `inchikey; salt-stripped, charge-neutralised`.
    ///
    /// Read by the two loaders of molecule tables. Left unset, the loader
    /// reads the table's own representation line; a blank, or a spelling
    /// the engine cannot read, declares nothing and is treated as not
    /// declared. It is deliberately not checked against the molecules,
    /// because the engine has no chemistry to check it with (D49) -- which
    /// is D38's position on a declared namespace, arrived at by a shorter
    /// road.
    pub representation: Option<String>,
}

/// The organism dimension a loader reports from what the user declared.
///
/// One helper rather than a block in each loader so the rule that an empty
/// declaration is a gap is stated once. Were `[]` taken as a known, empty
/// set, two datasets that each declared nothing would *agree* on organism,
/// which is the false agreement this engine exists to refuse. `reason` stays
/// the caller's because each format says something different about why it
/// cannot supply the answer itself.
pub(crate) fn organism(declared: &Declared, reason: &'static str) -> Dim<Organism> {
    match declared.organism.as_deref().map(Organism::of) {
        Some(organisms) if !organisms.is_empty() => Dim::known(organisms, Source::Declared),
        _ => Dim::unknown(reason),
    }
}

/// The release dimension of a reference, from what the user declared.
///
/// Called by the loaders of the kinds that have a release: a reference, a
/// gene list and a ranked list.
/// A blank release is a gap for the same reason an empty organism list is:
/// a blank that equals another blank is a false agreement. `reason` is the
/// caller's, as with [`organism`], so this module names no file format.
pub(crate) fn release(declared: &Declared, reason: &'static str) -> Dim<Release> {
    match declared.release.as_deref().map(Release::new) {
        Some(release) if !release.is_empty() => Dim::known(release, Source::Declared),
        _ => Dim::unknown(reason),
    }
}

/// The subject dimension, from what the user declared.
///
/// A blank subject is a gap for the same reason a blank release is: a blank
/// that equals another blank is a false agreement. `reason` is the
/// caller's, as with [`organism`] and [`release`].
pub(crate) fn subject(declared: &Declared, reason: &'static str) -> Dim<Subject> {
    match declared.subject.as_deref().map(Subject::new) {
        Some(subject) if !subject.is_empty() => Dim::known(subject, Source::Declared),
        _ => Dim::unknown(reason),
    }
}

/// The source-set dimension: declared if stated, else what the file named.
///
/// The one set-valued axis a file can speak to, so this helper takes both
/// paths rather than only the declaration. `found` is what the loader read
/// out of the data, and `where_from` describes it for the error message a
/// mismatch produces — which is the part that makes the error actionable: a
/// user told that two graphs disagree on their sources needs to know whether
/// anybody declared them.
///
/// An empty declaration *and* an empty reading are both gaps, for the reason
/// [`organism`] gives: two graphs that each named no sources would otherwise
/// agree on the axis, and that false agreement is what the dimension exists
/// to break.
pub(crate) fn source_set(
    declared: &Declared,
    found: Vec<&str>,
    where_from: Source,
    reason: &'static str,
) -> Dim<SourceSet> {
    if let Some(sources) = declared.source_set.as_deref().map(SourceSet::of)
        && !sources.is_empty()
    {
        return Dim::known(sources, Source::Declared);
    }

    let read = SourceSet::of(found);
    if read.is_empty() {
        Dim::unknown(reason)
    } else {
        Dim::known(read, where_from)
    }
}

/// Why a file could not be loaded.
#[derive(Debug)]
pub enum LoadError {
    /// The file could not be read.
    Io {
        /// The file in question.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// Files could not be grouped into samples.
    Pairing {
        /// What went wrong, phrased for a user.
        detail: String,
    },
    /// A reference is not numbered along the frame the reader's own
    /// coordinates are stated in.
    ///
    /// Its own variant rather than a [`Self::Format`]: the file parsed, and
    /// the fix is not a different file but the same sequence put back where
    /// its numbers mean what they say.
    Frame {
        /// The reference in question.
        path: PathBuf,
        /// What is wrong and what to do, phrased for a user.
        detail: String,
    },
    /// A molecule table's own keys refute the standardisation it claims.
    ///
    /// Its own variant rather than a [`Self::Format`], on [`Self::Frame`]'s
    /// grounds: the file parsed, and the fix is not a different file but a
    /// declaration that matches the keys -- or a table standardised the way
    /// the declaration says.
    Representation {
        /// The file in question.
        path: PathBuf,
        /// What the keys say and what to do, phrased for a user.
        detail: String,
    },
    /// The file was read but did not parse as the expected format.
    Format {
        /// The file in question.
        path: PathBuf,
        /// What went wrong, phrased for a user.
        detail: String,
    },
}

impl LoadError {
    pub(crate) fn io(path: &Path, source: io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    pub(crate) fn format(path: &Path, detail: impl Into<String>) -> Self {
        Self::Format {
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }

    pub(crate) fn representation(path: &Path, detail: impl Into<String>) -> Self {
        Self::Representation {
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }

    pub(crate) fn frame(path: &Path, detail: impl Into<String>) -> Self {
        Self::Frame {
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Self::Format { path, detail } => {
                write!(f, "cannot parse {}: {detail}", path.display())
            }
            Self::Frame { path, detail } => {
                write!(f, "cannot interpret against {}: {detail}", path.display())
            }
            Self::Representation { path, detail } => {
                write!(
                    f,
                    "cannot read {} in the state it says it is in: {detail}",
                    path.display()
                )
            }
            Self::Pairing { detail } => {
                write!(f, "cannot group reads into samples: {detail}")
            }
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Format { .. }
            | Self::Frame { .. }
            | Self::Representation { .. }
            | Self::Pairing { .. } => None,
        }
    }
}

/// Open a file for reading, transparently decompressing gzip.
///
/// [`MultiGzDecoder`] rather than a single-member decoder, because BGZF — the
/// block-gzip that `bgzip`, `samtools` and `bcftools` produce — is a sequence
/// of gzip members. A single-member decoder silently stops after the first
/// block, which would mean reading the start of a file and believing it to be
/// all of it.
pub(crate) fn open(path: &Path) -> Result<Box<dyn BufRead>, LoadError> {
    let file = File::open(path).map_err(|error| LoadError::io(path, error))?;
    let buffered = BufReader::new(file);

    if path.extension().is_some_and(|extension| extension == "gz") {
        Ok(Box::new(BufReader::new(MultiGzDecoder::new(buffered))))
    } else {
        Ok(Box::new(buffered))
    }
}

/// The filename with any compression suffix and format suffix removed.
///
/// `reads_1.fastq.gz` becomes `reads_1`.
pub(crate) fn stem(path: &Path) -> String {
    let mut name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    for suffix in [".gz", ".bgz"] {
        if let Some(trimmed) = name.strip_suffix(suffix) {
            name = trimmed.to_owned();
        }
    }
    match name.rsplit_once('.') {
        Some((base, _)) => base.to_owned(),
        None => name,
    }
}
