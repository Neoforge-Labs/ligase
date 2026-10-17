//! FASTQ: pairing, sample name, and quality encoding from the score range.

use std::path::Path;

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

    Ok(Dataset::new(
        DataKind::Reads,
        Some(r1.to_path_buf()),
        dimensions,
    ))
}

/// The sample name from the filename, with a mate suffix removed.
///
/// `docs/dimensions.md` allows a FASTQ's sample set to come "from
/// filename/declared". A filename is weak evidence, so the provenance says
/// plainly that it came from the filename and [`Declared::samples`] overrides
/// it.
fn sample_name(path: &Path) -> String {
    let base = stem(path);
    for suffix in ["_R1", "_R2", "_r1", "_r2", "_1", "_2"] {
        if let Some(trimmed) = base.strip_suffix(suffix) {
            return trimmed.to_owned();
        }
    }
    base
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
