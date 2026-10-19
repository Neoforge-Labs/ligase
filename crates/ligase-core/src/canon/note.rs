//! What a canonical form noticed and a hash cannot say.
//!
//! The canonical form of a VCF includes its sample names, and the canonical
//! form of a BAM includes its read groups' samples. That is not negotiable
//! the way `@PG` was: a command line says how a file was made (D22), while
//! a sample identifier is part of what the file is *about*, and a sample set
//! is one of the dimensions this engine exists to keep straight.
//!
//! Which leaves a gap, and `bcftools` walks into it. Given a BAM with no
//! `@RG SM`, it names the VCF's sample column after the input file's path —
//! and a Ligase run hands it a file inside the content-addressed store. The
//! calls then carry something like
//! `/Users/kelyn/ligase/.ligase/store/62/7db9….bam` where the sample
//! identifier should be. Every one of those is wrong in the same way: the
//! canonical hash of such a VCF depends on where the cache directory lives,
//! so a reproducibility bundle written from one cache cannot verify against
//! a clean one, and a methods paragraph built from it names a cache path
//! where it should name a run accession.
//!
//! # Why this is noticed rather than refused or repaired
//!
//! Refusing is what D21 rules out: canonicalisation must not turn a format
//! problem into an engine failure, and a VCF whose sample column holds a
//! path is perfectly valid VCF. Repairing it — reducing the path to its
//! basename before hashing — is the silent repair of a wrong file that this
//! project refuses on principle, and in this case it would not even work:
//! the basename of a file in the store is the hash of its *bytes*, and an
//! alignment's bytes hold the staging path that `samtools sort -o` recorded,
//! so the basename moves with the cache root just as the whole path does.
//!
//! So the identifier is hashed exactly as the file writes it, and the engine
//! says what it saw. The fix belongs to the pipeline — write the read group,
//! and the calls below carry the sample — and a user cannot make it without
//! being told.

use std::fmt;

/// Extensions that name a file of data rather than a sample.
///
/// Every format this engine reads, and the spellings that sit beside them.
/// A sample identifier ending in one of these is a file name: a sample
/// called `sample_a.bam` does not happen, and `bcftools` handed a relative
/// path produces exactly that.
const DATA_EXTENSIONS: [&str; 13] = [
    "bam", "cram", "sam", "vcf", "bcf", "fastq", "fq", "fasta", "fa", "fna", "bed", "gff", "gtf",
];

/// Compression suffixes stripped before the extension is read.
const COMPRESSION: [&str; 3] = ["gz", "bgz", "bz2"];

/// What a sample identifier turned out to be, when it is not a sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Shape {
    /// An absolute path. The one that cannot travel between machines.
    Absolute,
    /// A relative path: a file somewhere, named from somewhere else.
    Relative,
    /// A bare file name.
    Name,
}

impl Shape {
    /// How this reads in a sentence.
    const fn describe(self) -> &'static str {
        match self {
            Self::Absolute => "an absolute path",
            Self::Relative => "a file path",
            Self::Name => "a file name",
        }
    }
}

/// Something true of a file that its canonical hash does not express.
///
/// A note never changes a hash. It is carried beside one, so that a run
/// report, a reproducibility bundle and a methods section can all say the
/// same thing about the same file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Note {
    /// A sample identifier that is a file rather than a sample.
    SampleIsAFile {
        /// The identifier, exactly as the file writes it.
        sample: String,
    },
}

impl Note {
    /// The note for a sample identifier, if it is not one.
    ///
    /// `None` for an ordinary sample identifier, which is the common case
    /// and the one that must stay silent.
    #[must_use]
    pub fn about_sample(sample: &str) -> Option<Self> {
        shape(sample).map(|_| Self::SampleIsAFile {
            sample: sample.to_owned(),
        })
    }

    /// A stable name for this kind of note.
    ///
    /// Spelled out rather than derived, because it is written into a cache,
    /// a provenance database and a bundle a reviewer may read next year.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SampleIsAFile { .. } => "sample-is-a-file",
        }
    }

    /// What the note is about.
    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            Self::SampleIsAFile { sample } => sample,
        }
    }

    /// Rebuild a note from its [`Self::code`] and [`Self::value`].
    ///
    /// `None` for a code this version does not know, so a cache or a bundle
    /// written by a later one is read as far as it can be rather than
    /// rejected.
    #[must_use]
    pub fn parse(code: &str, value: &str) -> Option<Self> {
        match code {
            "sample-is-a-file" => Some(Self::SampleIsAFile {
                sample: value.to_owned(),
            }),
            _ => None,
        }
    }

    /// What to do about it, in one line.
    ///
    /// Kept beside the finding rather than in each report that prints one,
    /// so the run report, the bundle and the methods section give a user
    /// the same instruction. Short on purpose: it is printed unwrapped,
    /// beside a step's name, in a terminal.
    #[must_use]
    pub const fn advice(&self) -> &'static str {
        match self {
            Self::SampleIsAFile { .. } => {
                "Write the read group — `samtools addreplacerg -r ID:<sample> -r SM:<sample>` — \
                 so the calls below it name the sample."
            }
        }
    }

    /// Why it matters, for a report with room to say so.
    #[must_use]
    pub const fn consequence(&self) -> &'static str {
        match self {
            Self::SampleIsAFile { .. } => {
                "A sample set is a dimension, so the identifier is hashed exactly as the file \
                 writes it. The canonical hash of this output therefore depends on the file it \
                 was named after, which is not the same file in two places: a bundle written \
                 here cannot verify elsewhere, and a methods section built from it names a \
                 file where it should name a sample."
            }
        }
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SampleIsAFile { sample } => {
                let shape = shape(sample).map_or("a file", Shape::describe);
                write!(
                    f,
                    "the sample identifier `{sample}` is {shape}, not a sample"
                )
            }
        }
    }
}

/// What a sample identifier is, when it is not a sample.
///
/// Two shapes count and nothing else, because a rule that guessed would
/// report a perfectly good sample identifier as a mistake: a separator, or
/// an extension naming a format. `K03455.1` has a dot and is not a file;
/// `SRR8525904` and `sample_a` have neither.
fn shape(sample: &str) -> Option<Shape> {
    let name = sample.trim();
    if name.is_empty() {
        return None;
    }
    if name.starts_with('/') {
        return Some(Shape::Absolute);
    }
    if name.contains('/') || name.contains('\\') {
        return Some(Shape::Relative);
    }
    names_a_format(name).then_some(Shape::Name)
}

/// Whether a bare name ends in an extension naming a format.
fn names_a_format(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    let mut stem = lowered.as_str();
    for suffix in COMPRESSION {
        if let Some(trimmed) = stem.strip_suffix(suffix)
            && trimmed.ends_with('.')
        {
            stem = &trimmed[..trimmed.len() - 1];
            break;
        }
    }
    stem.rsplit_once('.')
        .is_some_and(|(_, extension)| DATA_EXTENSIONS.contains(&extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_inside_the_store_is_a_file() {
        // Verbatim, as `bcftools` wrote it in the run that found this.
        let sample = "/Users/kelyn/ligase/.ligase/store/62/7db9fa0cbac.bam";
        assert_eq!(shape(sample), Some(Shape::Absolute));
        assert!(Note::about_sample(sample).is_some());
    }

    #[test]
    fn a_relative_path_and_a_bare_file_name_are_files_too() {
        assert_eq!(shape("alignments/sample_a.bam"), Some(Shape::Relative));
        assert_eq!(shape(r"C:\data\sample_a.bam"), Some(Shape::Relative));
        assert_eq!(shape("sample_a.bam"), Some(Shape::Name));
        assert_eq!(shape("calls.vcf.gz"), Some(Shape::Name));
        assert_eq!(shape("READS.FASTQ.GZ"), Some(Shape::Name));
    }

    #[test]
    fn an_ordinary_sample_identifier_is_left_alone() {
        for sample in [
            "SRR8525904",
            "sample_a",
            "HG002",
            "patient-04",
            "K03455.1",
            "HG002.final",
            "bam",
            "",
            "  ",
        ] {
            assert_eq!(shape(sample), None, "{sample} is a sample identifier");
            assert_eq!(Note::about_sample(sample), None, "{sample}");
        }
    }

    #[test]
    fn a_note_round_trips_through_its_code_and_value() {
        let note = Note::SampleIsAFile {
            sample: "/tmp/x.bam".to_owned(),
        };
        assert_eq!(Note::parse(note.code(), note.value()), Some(note));
    }

    #[test]
    fn a_code_from_a_later_version_is_not_a_note_here() {
        assert_eq!(Note::parse("something-else", "x"), None);
    }

    #[test]
    fn a_note_says_what_it_found_and_what_to_do() {
        let note = Note::SampleIsAFile {
            sample: "/tmp/x.bam".to_owned(),
        };
        assert!(note.to_string().contains("/tmp/x.bam"));
        assert!(note.to_string().contains("an absolute path"));
        assert!(note.advice().contains("addreplacerg"));
        assert!(note.consequence().contains("dimension"));
    }
}
