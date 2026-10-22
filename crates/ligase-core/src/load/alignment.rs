//! SAM and BAM: the reference from `@SQ`, the sample set from `@RG`.

use std::fs::File;
use std::path::Path;

use noodles::sam::header::record::value::map::read_group::tag::SAMPLE;
use noodles::sam::header::record::value::map::reference_sequence::tag::{
    ASSEMBLY_ID, MD5_CHECKSUM,
};
use noodles::{bam, sam};

use super::{Declared, LoadError, open};
use crate::dim::{Contig, CoordinateSystem, Dim, Dimensions, ReferenceId, SampleSet, Source};
use crate::types::{DataKind, Dataset};

/// Load an alignment, inferring its dimensions from the header.
///
/// BAM and SAM are read through the same code path once the header is
/// parsed, and must produce identical dimensions: a lossless
/// `samtools view -b` changes nothing biological (decision D9).
pub fn alignment(path: &Path, declared: &Declared) -> Result<Dataset, LoadError> {
    let is_bam = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bam"));

    let header = if is_bam {
        let file = File::open(path).map_err(|error| LoadError::io(path, error))?;
        bam::io::Reader::new(file)
            .read_header()
            .map_err(|error| LoadError::format(path, format!("not a readable BAM: {error}")))?
    } else {
        sam::io::Reader::new(open(path)?)
            .read_header()
            .map_err(|error| LoadError::format(path, format!("not a readable SAM: {error}")))?
    };

    let sq_detail: &'static str = if is_bam {
        "BAM @SQ header"
    } else {
        "SAM @SQ header"
    };
    let rg_detail: &'static str = if is_bam {
        "BAM @RG header"
    } else {
        "SAM @RG header"
    };

    // -- reference and naming, from @SQ -----------------------------------
    let contigs = contigs(&header);
    let (reference, contig_naming) = if contigs.is_empty() {
        (
            Dim::unknown("no @SQ lines in the header; the reads are unaligned"),
            Dim::unknown("no @SQ lines in the header"),
        )
    } else {
        let label = declared.build.clone().or_else(|| assembly_id(&header));
        let identity = ReferenceId::new(label, contigs);
        let naming = identity.naming();
        (
            Dim::known(
                identity,
                Source::Inferred {
                    path: path.to_path_buf(),
                    detail: sq_detail,
                },
            ),
            Dim::known(
                naming,
                Source::Inferred {
                    path: path.to_path_buf(),
                    detail: sq_detail,
                },
            ),
        )
    };

    // -- sample set, from @RG SM ------------------------------------------
    let sample_set = match &declared.samples {
        Some(samples) => Dim::known(SampleSet::new(samples.clone()), Source::Declared),
        None => {
            let samples = samples(&header);
            if samples.is_empty() {
                // Not an empty sample set: an empty one would compare equal
                // to another empty one and let a real mismatch through.
                Dim::unknown("no @RG lines with an SM tag in the header")
            } else {
                Dim::known(
                    SampleSet::new(samples),
                    Source::Inferred {
                        path: path.to_path_buf(),
                        detail: rg_detail,
                    },
                )
            }
        }
    };

    // -- organism ----------------------------------------------------------
    // `web/content/reference/dimensions.mdx` says a BAM's organism comes
    // "from reference", and we cannot resolve a reference to an organism. The
    // free-text `@SQ SP` tag is deliberately not used as a source: matching
    // "Homo sapiens" against a declared "human" would need an alias table,
    // and a wrong guess there passes `check()` silently.
    let organism = super::organism(declared, "not recorded in SAM/BAM headers; declare it");

    let dimensions = Dimensions {
        organism,
        reference,
        contig_naming,
        sample_set,
        // Decision D9: semantics, not encoding. SAM and BAM both present
        // 1-based closed records, whatever the bytes look like.
        coordinates: Dim::known(
            CoordinateSystem::OneBasedClosed,
            Source::Inferred {
                path: path.to_path_buf(),
                detail: "SAM/BAM record convention",
            },
        ),
        // Pairing is in the SAM flags, which means reading records rather
        // than the header. Deferred until the engine reads records, and said
        // plainly in the meantime rather than left to a generic reason.
        pairing: Dim::unknown(
            "recorded in the SAM flags, which reading the header alone does not reach",
        ),
        ..Dimensions::default()
    };

    Ok(Dataset::new(
        DataKind::Alignment,
        Some(path.to_path_buf()),
        dimensions,
    ))
}

fn contigs(header: &sam::Header) -> Vec<Contig> {
    header
        .reference_sequences()
        .iter()
        .map(|(name, map)| Contig {
            name: String::from_utf8_lossy(name).into_owned(),
            length: Some(map.length().get() as u64),
            md5: map
                .other_fields()
                .get(&MD5_CHECKSUM)
                .map(|value| String::from_utf8_lossy(value).into_owned()),
        })
        .collect()
}

/// The `@SQ AS` assembly identifier, if every contig agrees on one.
///
/// A label is display only, so disagreement is not an error here — it just
/// means there is no single name worth showing.
fn assembly_id(header: &sam::Header) -> Option<String> {
    let mut found: Option<String> = None;
    for (_, map) in header.reference_sequences() {
        let value = map.other_fields().get(&ASSEMBLY_ID)?;
        let value = String::from_utf8_lossy(value).into_owned();
        match &found {
            Some(existing) if *existing != value => return None,
            Some(_) => {}
            None => found = Some(value),
        }
    }
    found
}

/// Sample names from `@RG SM`, in header order, each counted once.
///
/// Several read groups routinely share a sample — one per lane — and the
/// sample *set* is the biological identity, not the read-group list.
fn samples(header: &sam::Header) -> Vec<String> {
    let mut samples: Vec<String> = Vec::new();
    for (_, map) in header.read_groups() {
        if let Some(value) = map.other_fields().get(&SAMPLE) {
            let sample = String::from_utf8_lossy(value).into_owned();
            if !samples.contains(&sample) {
                samples.push(sample);
            }
        }
    }
    samples
}
