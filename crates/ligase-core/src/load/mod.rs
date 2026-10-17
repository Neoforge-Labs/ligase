//! Loading data and inferring its dimensions from file headers.
//!
//! Each loader reads only as much of a file as it needs, and reports a
//! dimension as unknown rather than guessing. `docs/dimensions.md` says which
//! dimensions each format can speak to; anything outside that is declared by
//! the user or left unknown.

mod alignment;
mod fasta;
mod reads;
mod variants;

use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use flate2::bufread::MultiGzDecoder;

pub use alignment::alignment;
pub use fasta::reference;
pub use reads::reads;
pub use variants::variants;

/// Dimensions the user states, for the ones a file cannot record.
///
/// `docs/dimensions.md` marks these "declared": stated once, then propagated.
#[derive(Clone, Debug, Default)]
pub struct Declared {
    /// Which organism the data is from, e.g. `HIV-1`.
    pub organism: Option<String>,
    /// A human label for the reference build, e.g. `HXB2` or `hg38`.
    ///
    /// Display only. Reference *identity* never rests on a label, so two
    /// files both labelled `hg38` still fail to match if their contigs
    /// disagree.
    pub build: Option<String>,
    /// The sample identifiers, in order, when the filename should not be
    /// trusted to supply them.
    pub samples: Option<Vec<String>>,
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
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Self::Format { path, detail } => {
                write!(f, "cannot parse {}: {detail}", path.display())
            }
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Format { .. } => None,
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
