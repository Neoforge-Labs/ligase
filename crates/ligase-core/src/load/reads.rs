//! FASTQ: pairing, sample name, and quality encoding from the score range.

use std::path::{Path, PathBuf};

use noodles::fastq;

use super::{Declared, LoadError, open, stem};
use crate::dim::{Dim, Dimensions, Organism, Pairing, QualityEncoding, SampleSet, Source};
use crate::types::{DataKind, Dataset};

/// How many records to read before giving up on deciding the encoding.
///
/// Bounded because a FASTQ can be tens of gigabytes and this is a question
/// about a header, not a reason to read the whole file. The cost of the bound
/// is that a file whose only distinguishing scores appear later reports its
/// encoding as unknown — which is the safe direction to be wrong in.
const SAMPLED_RECORDS: usize = 10_000;

/// The lowest byte Phred+64 and Solexa+64 can produce. Anything below it can
/// only be Phred+33.
const SIXTY_FOUR_FLOOR: u8 = 59;

/// The highest byte Phred+33 produces in practice (Illumina 1.8+ caps at
/// `J`, 74). Anything above it can only be a 64-offset encoding.
const THIRTY_THREE_CEILING: u8 = 74;

/// Load sequencing reads, inferring what a FASTQ can tell us.
pub fn reads(r1: &Path, r2: Option<&Path>, declared: &Declared) -> Result<Dataset, LoadError> {
    let pairing = Dim::known(
        if r2.is_some() {
            Pairing::Paired
        } else {
            Pairing::Single
        },
        Source::Inferred {
            path: r1.to_path_buf(),
            detail: "whether a mate file was given",
        },
    );

    let sample_set = match &declared.samples {
        Some(samples) => Dim::known(SampleSet::new(samples.clone()), Source::Declared),
        None => Dim::known(
            SampleSet::new(vec![sample_name(r1)]),
            Source::Inferred {
                path: r1.to_path_buf(),
                detail: "the filename",
            },
        ),
    };

    let inferred_from_range = Source::Inferred {
        path: r1.to_path_buf(),
        detail: "the quality score range",
    };
    let quality_encoding = match quality_range(r1)? {
        Some((min, _)) if min < SIXTY_FOUR_FLOOR => {
            Dim::known(QualityEncoding::Phred33, inferred_from_range)
        }
        Some((_, max)) if max > THIRTY_THREE_CEILING => {
            Dim::known(QualityEncoding::Phred64, inferred_from_range)
        }
        // Scores in the overlap both encodings share. Guessing would shift
        // every quality value by 31, which no downstream tool would notice.
        Some(_) => Dim::unknown("the quality scores fall in the range both encodings share"),
        None => Dim::unknown("the file contains no quality scores"),
    };

    let organism = match &declared.organism {
        Some(organism) => Dim::known(Organism::new(organism), Source::Declared),
        None => Dim::unknown("a FASTQ does not record its organism; declare it"),
    };

    let dimensions = Dimensions {
        organism,
        reference: Dim::unknown("reads are not aligned to a reference"),
        sample_set,
        pairing,
        quality_encoding,
        ..Dimensions::default()
    };

    let dataset = Dataset::new(DataKind::Reads, Some(r1.to_path_buf()), dimensions);
    // The second mate travels with the first, so a step written as
    // `{reads.r1} {reads.r2}` has something to put there.
    Ok(match r2 {
        Some(mate) => dataset.with_mate(mate.to_path_buf()),
        None => dataset,
    })
}

/// Group FASTQ files into samples, pairing mates by filename.
///
/// Mis-pairing two samples' files is exactly the silent wrongness Ligase
/// exists to catch: nothing crashes, and every result belongs to a mixture of
/// two people. So a file whose mate cannot be identified is an error rather
/// than a guess, and a lone mate is an error rather than a single-end sample.
///
/// `paths` are taken in the order given, and the samples come back in the
/// order their first file appeared.
pub fn samples(
    paths: &[PathBuf],
    paired: bool,
    declared: &Declared,
) -> Result<Vec<Dataset>, LoadError> {
    if !paired {
        return paths
            .iter()
            .map(|path| reads(path, None, declared))
            .collect();
    }

    let mut groups: Vec<(String, Option<PathBuf>, Option<PathBuf>)> = Vec::new();

    for path in paths {
        let Some((key, mate)) = mate(&stem(path)) else {
            return Err(LoadError::Pairing {
                detail: format!(
                    "cannot tell which mate {} is; name paired files \
                     `<sample>_1` and `<sample>_2`, or `<sample>_R1` and `<sample>_R2`",
                    path.display()
                ),
            });
        };

        let group = match groups.iter_mut().find(|(seen, ..)| *seen == key) {
            Some(group) => group,
            None => {
                groups.push((key, None, None));
                // `last_mut` cannot be None: a group was just pushed.
                match groups.last_mut() {
                    Some(group) => group,
                    None => continue,
                }
            }
        };

        let slot = match mate {
            Mate::One => &mut group.1,
            Mate::Two => &mut group.2,
        };
        if let Some(existing) = slot {
            return Err(LoadError::Pairing {
                detail: format!(
                    "sample `{}` has two files for the same mate: {} and {}",
                    group.0,
                    existing.display(),
                    path.display()
                ),
            });
        }
        *slot = Some(path.clone());
    }

    groups
        .into_iter()
        .map(|(key, first, second)| match (first, second) {
            (Some(r1), Some(r2)) => reads(&r1, Some(&r2), declared),
            (Some(lone), None) | (None, Some(lone)) => Err(LoadError::Pairing {
                detail: format!(
                    "sample `{key}` has only one of its two mates ({}); \
                     pass paired=False for single-end reads",
                    lone.display()
                ),
            }),
            // A group exists only because a file was filed under it.
            (None, None) => Err(LoadError::Pairing {
                detail: format!("sample `{key}` has no files"),
            }),
        })
        .collect()
}

/// Which of a pair a file holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mate {
    /// The first mate: `_1`, `_R1`.
    One,
    /// The second mate: `_2`, `_R2`.
    Two,
}

/// Every marker that names a mate, longest first.
const MARKERS: [(&str, Mate); 6] = [
    ("_R1", Mate::One),
    ("_R2", Mate::Two),
    ("_r1", Mate::One),
    ("_r2", Mate::Two),
    ("_1", Mate::One),
    ("_2", Mate::Two),
];

/// The sample name and which mate, from a filename stem.
///
/// The marker is not always a suffix — Illumina writes
/// `SAMPLE_R1_001.fastq.gz` — so everything from the marker onwards is
/// dropped. The *last* marker wins, so `rep1_R1` is mate one of `rep1`
/// rather than mate one of `rep`.
fn mate(stem: &str) -> Option<(String, Mate)> {
    let mut best: Option<(usize, Mate)> = None;

    for (marker, which) in MARKERS {
        // A marker at position zero would leave no sample name behind, so a
        // file called `_1.fastq` is not mate one of nothing.
        if let Some(at) = stem.rfind(marker).filter(|at| *at > 0)
            && best.is_none_or(|(seen, _)| at > seen)
        {
            best = Some((at, which));
        }
    }

    best.map(|(at, which)| (stem[..at].to_owned(), which))
}

/// The sample name from the filename, with any mate marker removed.
///
/// `web/content/reference/dimensions.mdx` allows a FASTQ's sample set to come
/// "from filename/declared". A filename is weak evidence, so the provenance
/// says plainly that it came from the filename and [`Declared::samples`]
/// overrides it.
fn sample_name(path: &Path) -> String {
    let base = stem(path);
    mate(&base).map_or(base, |(name, _)| name)
}

/// The lowest and highest quality bytes in the sampled records.
fn quality_range(path: &Path) -> Result<Option<(u8, u8)>, LoadError> {
    let mut reader = fastq::io::Reader::new(open(path)?);
    let mut range: Option<(u8, u8)> = None;

    for result in reader.records().take(SAMPLED_RECORDS) {
        let record = result.map_err(|error| LoadError::format(path, format!("{error}")))?;
        for &byte in record.quality_scores() {
            range = Some(match range {
                Some((min, max)) => (min.min(byte), max.max(byte)),
                None => (byte, byte),
            });
        }
        // A byte below the 64-offset floor settles it; no need to read on.
        if range.is_some_and(|(min, _)| min < SIXTY_FOUR_FLOOR) {
            break;
        }
    }

    Ok(range)
}
