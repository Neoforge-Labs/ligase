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
    /// An identifier that is a date rather than a name.
    ///
    /// What a spreadsheet does to `SEPT7`, `MARCH1` and `DEC1`.
    IdentifierIsADate {
        /// The identifier, exactly as the file writes it.
        identifier: String,
    },
    /// An identifier a mapping table had no entry for.
    IdentifierUnmapped {
        /// The identifier, in the space it came from.
        identifier: String,
    },
    /// An identifier a mapping table sent to more than one target.
    IdentifierAmbiguous {
        /// The identifier, in the space it came from.
        identifier: String,
    },
    /// An identifier that arrives where another identifier also arrives.
    IdentifierCollides {
        /// The identifier, in the space it came from.
        identifier: String,
    },
    /// How many identifiers did not cross into the target space.
    ///
    /// Carried beside the named ones because the named ones are capped: a
    /// list of twenty thousand converted through a table covering most of
    /// it leaves thousands behind, and a report that named every one and
    /// totalled none is not a report. The spec asks for them noticed,
    /// *counted* and listed.
    IdentifiersDidNotCross {
        /// How many, named or not.
        count: u32,
    },
    /// A remote whose bytes the source rewrote without changing what they
    /// mean (D39).
    ///
    /// NCBI reflows a FASTA or rewords a defline from time to time. The pin
    /// is over what the file means, so such a rewrite does not break it —
    /// but a user who is told nothing will not know that the archive has
    /// started serving different bytes, and that is worth knowing before the
    /// next change is not cosmetic.
    RemoteRewritten {
        /// The locator, as the pipeline wrote it.
        locator: String,
    },
    /// A recorded query whose source now answers something else.
    ///
    /// What `--refresh` is for. Not an error and not a silent re-record: the
    /// earlier record is kept, so the run that used it is still replayable,
    /// and this says which query moved and between which two bodies. A
    /// source that promises nothing is *entitled* to change its answer, so
    /// the honest statement is "this is different now", not "this pipeline
    /// is broken".
    QueryDrifted {
        /// The locator, as the pipeline wrote it.
        locator: String,
        /// The body the earlier record holds.
        was: String,
        /// The body the new record holds.
        now: String,
    },
    /// A top-level field dropped from a recorded response body.
    ///
    /// A `requestId` or a `generatedAt` is the server's account of how it
    /// served the answer rather than part of the answer, so it is dropped
    /// before the body is hashed, on D22's precedent. But even at the top
    /// level the key might have been the user's own data — a payload that
    /// really is a bare `{"timestamp": ...}` is possible — so the engine
    /// says which field it dropped rather than leaving the user to wonder
    /// why a body in the store is shorter than the one on the wire (D26).
    QueryFieldDropped {
        /// The locator, as the pipeline wrote it, so a run making several
        /// queries can say which of them this was.
        locator: String,
        /// The field, spelled as the server spelled it.
        field: String,
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

    /// The note for an identifier, if something is wrong with it.
    ///
    /// `None` for an ordinary identifier, which is nearly all of them and
    /// the case that must stay silent.
    ///
    /// Only one corruption is detected, and it is the one a spreadsheet
    /// causes: a gene symbol read as a date. `SEPT7` becomes `2-Sep`,
    /// `MARCH1` becomes `1-Mar`, and the gene is then absent from every
    /// join for the rest of the analysis. The other thing Excel does —
    /// turning a symbol into a date *serial* such as `42615` — is
    /// deliberately not attempted, because `42615` is also a perfectly good
    /// Entrez gene ID and a rule that flagged it would report real
    /// identifiers as mistakes.
    #[must_use]
    pub fn about_identifier(identifier: &str) -> Option<Self> {
        looks_like_a_date(identifier.trim()).then(|| Self::IdentifierIsADate {
            identifier: identifier.trim().to_owned(),
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
            Self::RemoteRewritten { .. } => "remote-rewritten",
            Self::QueryFieldDropped { .. } => "query-field-dropped",
            Self::QueryDrifted { .. } => "query-drifted",
            Self::IdentifierIsADate { .. } => "identifier-is-a-date",
            Self::IdentifierUnmapped { .. } => "identifier-unmapped",
            Self::IdentifierAmbiguous { .. } => "identifier-ambiguous",
            Self::IdentifierCollides { .. } => "identifier-collides",
            Self::IdentifiersDidNotCross { .. } => "identifiers-did-not-cross",
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
            Self::RemoteRewritten { locator } => locator.clone(),
            Self::QueryFieldDropped { locator, field } => {
                format!("field={field} locator={locator}")
            }
            Self::QueryDrifted { locator, was, now } => {
                format!("was={was} now={now} locator={locator}")
            }
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
            // One value and nothing else, so there is no separator to
            // escape: an identifier read out of a TSV holds no tab and no
            // newline, which is all `parse` needs of it.
            Self::IdentifierIsADate { identifier }
            | Self::IdentifierUnmapped { identifier }
            | Self::IdentifierAmbiguous { identifier }
            | Self::IdentifierCollides { identifier } => identifier.clone(),
            Self::IdentifiersDidNotCross { count } => format!("count={count}"),
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
            "identifier-is-a-date" => Some(Self::IdentifierIsADate {
                identifier: value.to_owned(),
            }),
            "identifier-unmapped" => Some(Self::IdentifierUnmapped {
                identifier: value.to_owned(),
            }),
            "identifier-ambiguous" => Some(Self::IdentifierAmbiguous {
                identifier: value.to_owned(),
            }),
            "identifier-collides" => Some(Self::IdentifierCollides {
                identifier: value.to_owned(),
            }),
            "identifiers-did-not-cross" => Some(Self::IdentifiersDidNotCross {
                count: counted(value, "count")?,
            }),
            "remote-rewritten" => Some(Self::RemoteRewritten {
                locator: value.to_owned(),
            }),
            "query-field-dropped" => Some(Self::QueryFieldDropped {
                field: named(value, "field")?.to_owned(),
                locator: named(value, "locator")?.to_owned(),
            }),
            "query-drifted" => Some(Self::QueryDrifted {
                was: named(value, "was")?.to_owned(),
                now: named(value, "now")?.to_owned(),
                locator: named(value, "locator")?.to_owned(),
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
            Self::RemoteRewritten { .. } => {
                "Nothing needs doing: the pin still holds. Update it only if you would rather \
                 pin the new bytes."
            }
            Self::QueryFieldDropped { .. } => {
                "Nothing needs doing where the field was the server's own. If it was your data, \
                 ask for it under another name: this one is on the list the engine drops."
            }
            Self::QueryDrifted { .. } => {
                "Read what changed before you rely on the new answer. Nothing else needs doing: \
                 both records are kept, so run without `--refresh` to go on replaying the \
                 earlier one."
            }
            Self::IdentifierIsADate { .. } => {
                "Export the file again with the identifier column typed as text, or recover the \
                 symbol from the date -- `2-Sep` was `SEPT7`."
            }
            Self::IdentifierUnmapped { .. } => {
                "Check the mapping table's release: an identifier missing from it is usually one \
                 renamed or retired since the table was downloaded."
            }
            Self::IdentifierAmbiguous { .. } => {
                "Decide which target you mean and add a one-to-one row for it, or keep the \
                 identifier in the space it is already in."
            }
            Self::IdentifierCollides { .. } => {
                "Decide which of the colliding identifiers this analysis is about and drop the \
                 others from the input, or convert in the other direction."
            }
            Self::IdentifiersDidNotCross { .. } => {
                "Compare this against the size of your input before reading the result: check \
                 the mapping table's release if it is more than a small fraction."
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
            Self::RemoteRewritten { .. } => {
                "Nothing downstream moves: the pin is over what the file means, and that is \
                 unchanged. What has changed is the source's habits, which is worth knowing \
                 before a change that is not cosmetic arrives."
            }
            Self::QueryFieldDropped { .. } => {
                "The field is in neither the stored body nor the hash, so the record repeats \
                 when the server's own bookkeeping changes -- which is the point. The cost is \
                 that a field of yours that shares one of those names is not in the record \
                 either, and this is how you find out."
            }
            Self::QueryDrifted { .. } => {
                "Everything below this input has a new content hash, so every step that uses \
                 it reruns and its results are the new source's results rather than the \
                 earlier ones. The earlier record is untouched: the run that produced the \
                 earlier results can still be replayed exactly as it ran."
            }
            Self::IdentifierIsADate { .. } => {
                "A date is in no identifier space, so this entity matches nothing in any other \
                 file and silently drops out of every join downstream. It also stops the space \
                 being inferred for the file at all, because one identifier the engine cannot \
                 place is a file it will not guess about."
            }
            Self::IdentifierUnmapped { .. } => {
                "An identifier that does not cross is left out of the output rather than carried \
                 over unchanged, because carrying it would put a name from the old space into a \
                 set that declares the new one. The output is therefore smaller than the input, \
                 by exactly the identifiers named in these notes."
            }
            Self::IdentifierAmbiguous { .. } => {
                "One identifier with several targets is left out rather than resolved: emitting \
                 a row for each would make the set larger than the input without saying so, and \
                 choosing one of them would be a guess about which the analysis meant."
            }
            Self::IdentifierCollides { .. } => {
                "Two identifiers arriving at one target would make the output name a single \
                 entity twice, with two sets of attributes, which is not a set. Keeping one of \
                 them would be a guess about which row's attributes to carry, so every \
                 identifier that collides is left out and named."
            }
            Self::IdentifiersDidNotCross { .. } => {
                "The output is smaller than the input by this many entities. A conversion is a \
                 lookup, not a rename, so a count that is a large share of the input usually \
                 means the mapping table is from a different release than the list."
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
            Self::RemoteRewritten { locator } => write!(
                f,
                "the source rewrote `{locator}`: its bytes differ from the ones recorded, and \
                 what it means does not, so the pin still holds"
            ),
            Self::QueryFieldDropped { locator, field } => write!(
                f,
                "`{field}` was dropped from the recorded response to `{locator}`: a field of \
                 that name is the server's account of how it answered, not part of the answer"
            ),
            Self::QueryDrifted { locator, was, now } => write!(
                f,
                "the source of `{locator}` has changed since it was recorded: the record held \
                 {was} and it now answers {now}. Both records are kept, so the earlier run is \
                 still replayable"
            ),
            Self::IdentifierIsADate { identifier } => write!(
                f,
                "the identifier `{identifier}` is a date, not a name; a spreadsheet does this \
                 to SEPT7, MARCH1 and DEC1"
            ),
            Self::IdentifierUnmapped { identifier } => write!(
                f,
                "the identifier `{identifier}` is in no row of the mapping table, so it is not \
                 in the output"
            ),
            Self::IdentifierAmbiguous { identifier } => write!(
                f,
                "the identifier `{identifier}` maps to more than one target, so it is not in \
                 the output"
            ),
            Self::IdentifierCollides { identifier } => write!(
                f,
                "the identifier `{identifier}` maps to a target another identifier also maps \
                 to, so it is not in the output"
            ),
            Self::IdentifiersDidNotCross { count } => write!(
                f,
                "{count} identifiers did not cross into the target space, so the output is \
                 {count} entities smaller than the input"
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

/// The month names a spreadsheet writes, lower-cased: the abbreviation it
/// uses by default, and the full name it uses when the column is wide.
const MONTHS: [&str; 25] = [
    "jan",
    "feb",
    "mar",
    "apr",
    "may",
    "jun",
    "jul",
    "aug",
    "sep",
    "sept",
    "oct",
    "nov",
    "dec",
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Whether this reads as a date rather than as a name.
///
/// Three shapes, all of them things a spreadsheet produces from a gene
/// symbol and none of them things a real identifier is: a day and a month
/// in either order (`2-Sep`, `Sep-02`), and a numeric date (`2026-09-02`,
/// `9/2/2026`). A month name on its own is not one, because `MAY` is a
/// gene; nor is a hyphenated symbol such as `NKX2-1`, because neither half
/// is a month.
pub(crate) fn looks_like_a_date(identifier: &str) -> bool {
    if identifier.is_empty() {
        return false;
    }
    let parts: Vec<&str> = identifier.split(['-', '/']).collect();
    match parts[..] {
        // A day and a month, in either order.
        [left, right] => is_month(left) != is_month(right) && (is_day(left) || is_day(right)),
        // A numeric date: three numbers, or two numbers and a month.
        [a, b, c] => {
            let numbers = [a, b, c].iter().filter(|part| is_number(part)).count();
            let months = [a, b, c].iter().filter(|part| is_month(part)).count();
            numbers == 3 || (numbers == 2 && months == 1)
        }
        _ => false,
    }
}

/// Whether this part is a month name, in full or abbreviated.
///
/// An exact match, never a prefix: `SEPT7` begins with `sep` and `MARCH1`
/// with `mar`, and those are the very symbols this exists to recognise the
/// corruption *of*. Matching them by prefix would report the intact symbol
/// as the damage.
fn is_month(part: &str) -> bool {
    let lowered = part.to_ascii_lowercase();
    MONTHS.contains(&lowered.as_str())
}

fn is_number(part: &str) -> bool {
    !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())
}

/// A number that could be a day of a month.
fn is_day(part: &str) -> bool {
    is_number(part) && part.parse::<u32>().is_ok_and(|day| (1..=31).contains(&day))
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
            Note::IdentifierIsADate {
                identifier: "2-Sep".to_owned(),
            },
            Note::IdentifierUnmapped {
                identifier: "SEPT7".to_owned(),
            },
            Note::IdentifierAmbiguous {
                identifier: "NKX2-1".to_owned(),
            },
            Note::IdentifierCollides {
                identifier: "TP53P1".to_owned(),
            },
            Note::IdentifiersDidNotCross { count: 4000 },
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

    #[test]
    fn a_symbol_excel_turned_into_a_date_is_noticed() {
        for identifier in [
            "2-Sep",
            "1-Mar",
            "Sep-02",
            "Dec-01",
            "2026-09-02",
            "9/2/2026",
        ] {
            assert!(
                matches!(
                    Note::about_identifier(identifier),
                    Some(Note::IdentifierIsADate { .. })
                ),
                "{identifier}"
            );
        }
    }

    #[test]
    fn a_real_gene_symbol_is_left_alone() {
        // The symbols Excel corrupts are themselves fine, and a hyphen in a
        // symbol is ordinary: NKX2-1 and HLA-A are not dates.
        for identifier in [
            "SEPT7",
            "MARCH1",
            "DEC1",
            "NKX2-1",
            "HLA-A",
            "TP53",
            "ENSG00000141510",
            "7157",
            "P04637",
            "",
            "   ",
            // A symbol that merely *starts* with a month's letters is not a
            // date: only a whole month name counts, or `SEPT7-1` and
            // `MARCH1-2` would be reported as dates on the strength of
            // their first three letters.
            "SEPT7-1",
            "MARCH1-2",
            "MAY",
        ] {
            assert_eq!(Note::about_identifier(identifier), None, "{identifier}");
        }
    }

    #[test]
    fn an_identifier_note_says_what_it_found_and_names_it() {
        let note = Note::IdentifierIsADate {
            identifier: "2-Sep".to_owned(),
        };
        assert!(note.to_string().contains("2-Sep"));
        assert!(note.to_string().contains("date"));
        // And none of the three explains a hash that moved: they describe
        // what the data is, which is no reason for two machines to differ.
        for note in every_kind()
            .into_iter()
            .filter(|note| note.code().starts_with("identifier-"))
        {
            assert!(!note.explains_a_difference(), "{}", note.code());
        }
    }
}
