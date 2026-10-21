//! What a result rests on and a hash cannot say.
//!
//! A [`Note`] is a finding about one step's output that never changes a
//! hash and never fails a run. It is carried beside the result into the run
//! report, provenance, the reproducibility bundle and the methods section,
//! so that every surface says the same thing in the same words (D26).
//!
//! Notes reach a result from two directions.
//!
//! # What canonicalisation notices
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
//! ## Why that is noticed rather than refused or repaired
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
//! and the calls below it carry the sample — and a user cannot make it
//! without being told.
//!
//! # What a step notices about its own work
//!
//! The second kind cannot be found by reading an output at all. A
//! resistance reading made without an alignment is taking every position
//! the caller did not report to be reference, and two reports — one fully
//! covered, one with no coverage at all — can come out byte for byte the
//! same. The assumption is visible only to the code that made it, so the
//! step says it and the engine carries it (D32).
//!
//! These notes are stored against the step's cache key rather than against
//! the output's hash, for the reason D26 gives for storing the others: a
//! cache hit does not rerun the body, and a hedge that stopped appearing on
//! the second run would be worse than none.

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

/// Something true of a step's output that its hash does not express.
///
/// A note never changes a hash and never fails a run. It is carried beside
/// the result, so that a run report, a reproducibility bundle and a methods
/// section can all say the same thing about the same output.
///
/// The variants are spelled out rather than carrying free text, because a
/// note is written into a cache, a provenance database and a bundle a
/// reviewer may read next year: a code names the kind, and the wording,
/// the advice and the consequence belong to the kind rather than to
/// whichever report is printing it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Note {
    /// A sample identifier that is a file rather than a sample.
    SampleIsAFile {
        /// The identifier, exactly as the file writes it.
        sample: String,
    },
    /// A resistance reading made without an alignment to read coverage
    /// from, so every position the caller did not report was taken to be
    /// reference (D29).
    ReadingWithoutCoverage,
    /// A gene no codon of which reached the depth asked for.
    GeneUnread {
        /// The gene.
        gene: String,
        /// The depth a position had to reach to count as read.
        depth: u32,
    },
    /// A gene too little of which was read to be assessed at all (D34).
    GeneBarelyRead {
        /// The gene.
        gene: String,
        /// How many of its codons were read.
        read: u32,
        /// How many codons it has.
        codons: u32,
        /// How many had to be read for it to be assessed.
        needed: u32,
    },
    /// A gene some of whose codons did not reach the depth asked for.
    CodonsUnread {
        /// The gene.
        gene: String,
        /// How many of its codons were read.
        read: u32,
        /// How many codons it has.
        codons: u32,
        /// The depth a position had to reach to count as read.
        depth: u32,
    },
    /// Codons read base by base because no read spans them (D31).
    CodonsUnphased {
        /// The gene.
        gene: String,
        /// How many of its codons were read that way.
        unphased: u32,
        /// The depth a codon had to be spanned to count as phased.
        depth: u32,
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
            Self::ReadingWithoutCoverage => "reading-without-coverage",
            Self::GeneUnread { .. } => "gene-unread",
            Self::GeneBarelyRead { .. } => "gene-barely-read",
            Self::CodonsUnread { .. } => "codons-unread",
            Self::CodonsUnphased { .. } => "codons-unphased",
        }
    }

    /// What the note is about, as one column of a table.
    ///
    /// A note with more than one thing to say spells its fields out as
    /// `name=value` pairs, because [`Self::parse`] has to be able to turn
    /// this back into the note: the wording belongs to this type, and a
    /// sentence stored in a cache could not be reworded by the version that
    /// prints it.
    #[must_use]
    pub fn value(&self) -> String {
        match self {
            Self::SampleIsAFile { sample } => sample.clone(),
            Self::ReadingWithoutCoverage => String::new(),
            Self::GeneUnread { gene, depth } => format!("gene={gene} depth={depth}"),
            Self::GeneBarelyRead {
                gene,
                read,
                codons,
                needed,
            } => format!("gene={gene} read={read} codons={codons} needed={needed}"),
            Self::CodonsUnread {
                gene,
                read,
                codons,
                depth,
            } => format!("gene={gene} read={read} codons={codons} depth={depth}"),
            Self::CodonsUnphased {
                gene,
                unphased,
                depth,
            } => format!("gene={gene} unphased={unphased} depth={depth}"),
        }
    }

    /// Whether this note can explain a canonical hash that moved.
    ///
    /// True only for a note about something that travels badly. The rest
    /// say what a result rested on, which is worth reading and is not a
    /// reason for two machines to disagree — and a bundle verification that
    /// offered one as an excuse for a `differs` would be sending a reader
    /// after the wrong thing.
    #[must_use]
    pub const fn explains_a_difference(&self) -> bool {
        matches!(self, Self::SampleIsAFile { .. })
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
            "reading-without-coverage" => Some(Self::ReadingWithoutCoverage),
            "gene-unread" => Some(Self::GeneUnread {
                gene: named(value, "gene")?.to_owned(),
                depth: counted(value, "depth")?,
            }),
            "gene-barely-read" => Some(Self::GeneBarelyRead {
                gene: named(value, "gene")?.to_owned(),
                read: counted(value, "read")?,
                codons: counted(value, "codons")?,
                needed: counted(value, "needed")?,
            }),
            "codons-unread" => Some(Self::CodonsUnread {
                gene: named(value, "gene")?.to_owned(),
                read: counted(value, "read")?,
                codons: counted(value, "codons")?,
                depth: counted(value, "depth")?,
            }),
            "codons-unphased" => Some(Self::CodonsUnphased {
                gene: named(value, "gene")?.to_owned(),
                unphased: counted(value, "unphased")?,
                depth: counted(value, "depth")?,
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
            Self::ReadingWithoutCoverage => {
                "Pass the alignment — `coverage=aln` — so uncalled positions are reported as \
                 unread rather than as reference."
            }
            Self::GeneUnread { .. } => {
                "Sequence deeper, or lower the depth this reading asks for — `depth=` — so this \
                 gene's drugs can be assessed at all."
            }
            Self::GeneBarelyRead { .. } => {
                "Sequence more of this gene, or lower the share this reading asks for — \
                 `gene_coverage=` — if a floor is of more use to you than no answer."
            }
            Self::CodonsUnread { .. } => {
                "Read a drug whose `unread` column is not empty as a floor, and sequence deeper \
                 to close it."
            }
            Self::CodonsUnphased { .. } => {
                "Read a mutation at such a codon as a candidate, and sequence longer reads to \
                 settle it."
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
            Self::ReadingWithoutCoverage => {
                "A position missing from a VCF is ambiguous between agreeing with the reference \
                 and nothing having aligned there, and the two have opposite clinical meanings: \
                 the first is a susceptible result and the second is no result at all. Taking it \
                 as reference makes every score in this report a floor that the report itself \
                 cannot mark, because its `unread` column is empty for want of coverage to read \
                 rather than because everything was read."
            }
            Self::GeneUnread { .. } => {
                "A drug with nothing to score against is left out of the report rather than \
                 called susceptible, so the absence of this gene's drugs means no data and not \
                 a negative result."
            }
            Self::GeneBarelyRead { .. } => {
                "A gene read in part refutes nothing and shows nothing, so every drug scored \
                 against it would land at zero, which this algorithm's own ranges read as \
                 susceptible. The gene is left out of the report instead, so the absence of \
                 its drugs means no data and not a negative result."
            }
            Self::CodonsUnread { .. } => {
                "A drug whose rules name an unread position has that position listed in the \
                 report's `unread` column and its score is a floor: the mutations it looks for \
                 were not all looked at."
            }
            Self::CodonsUnphased { .. } => {
                "A codon read base by base holds every combination of the alleles the caller \
                 credited at its three bases, so it can name a mutation no single read carries, \
                 which overstates resistance where a phased reading would not."
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
            Self::ReadingWithoutCoverage => f.write_str(
                "no alignment was supplied, so coverage is unknown, no codon can be phased, \
                 and every position the caller did not report is taken to be reference",
            ),
            Self::GeneUnread { gene, depth } => write!(
                f,
                "no codon of {gene} reached {depth}-fold coverage, so no drug scored \
                 against it could be assessed"
            ),
            Self::GeneBarelyRead {
                gene,
                read,
                codons,
                needed,
            } => write!(
                f,
                "only {read} of {gene}'s {codons} codons were read, where this reading \
                 required {needed}, so {gene} was not assessed rather than scored against \
                 what little was seen"
            ),
            Self::CodonsUnread {
                gene,
                read,
                codons,
                depth,
            } => write!(
                f,
                "{read} of {gene}'s {codons} codons reached {depth}-fold coverage; \
                 the rest are reported as unread rather than as reference"
            ),
            Self::CodonsUnphased {
                gene,
                unphased,
                depth,
            } => write!(
                f,
                "{unphased} of {gene}'s codons were read base by base because no read spans \
                 all three of their bases {depth} times over, so a combination of bases no \
                 read carries may be reported there"
            ),
        }
    }
}

/// One `name=value` field of a note's value, if it is there.
fn named<'a>(value: &'a str, name: &str) -> Option<&'a str> {
    value
        .split(' ')
        .filter_map(|field| field.split_once('='))
        .find_map(|(key, found)| (key == name && !found.is_empty()).then_some(found))
}

/// One field of a note's value, as the number it should be.
fn counted(value: &str, name: &str) -> Option<u32> {
    named(value, name)?.parse().ok()
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

    /// Every kind there is, so a new one cannot be added without one here.
    fn every_kind() -> Vec<Note> {
        vec![
            Note::SampleIsAFile {
                sample: "/tmp/x.bam".to_owned(),
            },
            Note::ReadingWithoutCoverage,
            Note::GeneUnread {
                gene: "CA".to_owned(),
                depth: 100,
            },
            Note::GeneBarelyRead {
                gene: "IN".to_owned(),
                read: 92,
                codons: 288,
                needed: 231,
            },
            Note::CodonsUnread {
                gene: "RT".to_owned(),
                read: 548,
                codons: 560,
                depth: 100,
            },
            Note::CodonsUnphased {
                gene: "RT".to_owned(),
                unphased: 6,
                depth: 100,
            },
        ]
    }

    #[test]
    fn a_note_round_trips_through_its_code_and_value() {
        // What the cache, provenance and a bundle all keep: a code and one
        // value. A note that could not be read back from those two would
        // stop being said the moment a step was served from the cache.
        for note in every_kind() {
            assert_eq!(
                Note::parse(note.code(), &note.value()),
                Some(note.clone()),
                "{}",
                note.code()
            );
        }
    }

    #[test]
    fn a_value_is_one_column_of_a_table() {
        // Written into a TSV, so a tab or a newline in one would make a
        // note unreadable rather than wrong.
        for note in every_kind() {
            let value = note.value();
            assert!(!value.contains('\t'), "{value}");
            assert!(!value.contains('\n'), "{value}");
        }
    }

    #[test]
    fn every_kind_says_what_it_found_what_it_means_and_what_to_do() {
        for note in every_kind() {
            assert!(!note.to_string().is_empty(), "{}", note.code());
            assert!(note.advice().ends_with('.'), "{}", note.code());
            assert!(note.consequence().ends_with('.'), "{}", note.code());
        }
    }

    #[test]
    fn only_a_sample_that_is_a_file_explains_a_hash_that_moved() {
        // The others are about what a result rested on, which is worth
        // reading and is no reason for two machines to disagree.
        for note in every_kind() {
            assert_eq!(
                note.explains_a_difference(),
                note.code() == "sample-is-a-file",
                "{}",
                note.code()
            );
        }
    }

    #[test]
    fn a_reading_note_names_its_gene_and_its_counts() {
        let note = Note::CodonsUnread {
            gene: "RT".to_owned(),
            read: 548,
            codons: 560,
            depth: 100,
        };
        let said = note.to_string();
        assert!(said.contains("548 of RT's 560 codons"), "{said}");
        assert!(said.contains("100-fold"), "{said}");

        // A field that is not there leaves the note unreadable rather than
        // half-read: a note with a gene and no count could not be worded.
        assert_eq!(Note::parse("codons-unread", "gene=RT read=548"), None);
        assert_eq!(Note::parse("codons-unread", ""), None);
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
