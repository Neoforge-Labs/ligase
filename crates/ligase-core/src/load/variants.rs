//! VCF: contigs from `##contig`, samples from the header columns.

use std::path::Path;

use noodles::vcf;

use super::{Declared, LoadError, open};
use crate::dim::{
    Contig, CoordinateSystem, Dim, Dimensions, Organism, ReferenceId, SampleSet, Source,
};
use crate::types::{DataKind, Dataset};

/// Load variant calls, inferring dimensions from the header.
///
/// `##reference` is deliberately **not** used for identity. It holds a path or
/// URL, and `docs/dimensions.md` is explicit that reference identity never
/// rests on a filename: two runs pointing at `/refs/genome.fa` can be
/// pointing at different genomes. Identity comes from `##contig` alone, and a
/// VCF with no `##contig` lines reports its reference as unknown rather than
/// trusting the path.
pub fn variants(path: &Path, declared: &Declared) -> Result<Dataset, LoadError> {
    let header = vcf::io::Reader::new(open(path)?)
        .read_header()
        .map_err(|error| LoadError::format(path, format!("not a readable VCF: {error}")))?;

    let contigs = contigs(&header);
    let (reference, contig_naming) = if contigs.is_empty() {
        (
            Dim::unknown("no ##contig lines in the header, and ##reference names only a path"),
            Dim::unknown("no ##contig lines in the header"),
        )
    } else {
        let identity = ReferenceId::new(declared.build.clone(), contigs);
        let naming = identity.naming();
        let source = Source::Inferred {
            path: path.to_path_buf(),
            detail: "VCF ##contig lines",
        };
        (
            Dim::known(identity, source.clone()),
            Dim::known(naming, source),
        )
    };

    let sample_set = match &declared.samples {
        Some(samples) => Dim::known(SampleSet::new(samples.clone()), Source::Declared),
        None => {
            let samples: Vec<String> = header.sample_names().iter().cloned().collect();
            if samples.is_empty() {
                Dim::unknown("no sample columns in the VCF header")
            } else {
                Dim::known(
                    SampleSet::new(samples),
                    Source::Inferred {
                        path: path.to_path_buf(),
                        detail: "VCF header sample columns",
                    },
                )
            }
        }
    };

    let organism = match &declared.organism {
        Some(organism) => Dim::known(Organism::new(organism), Source::Declared),
        None => Dim::unknown("a VCF does not record its organism; declare it"),
    };

    let dimensions = Dimensions {
        organism,
        reference,
        contig_naming,
        sample_set,
        coordinates: Dim::known(
            CoordinateSystem::OneBasedClosed,
            Source::Inferred {
                path: path.to_path_buf(),
                detail: "VCF coordinate convention",
            },
        ),
        ..Dimensions::default()
    };

    Ok(Dataset::new(
        DataKind::Variants,
        Some(path.to_path_buf()),
        dimensions,
    ))
}

fn contigs(header: &vcf::Header) -> Vec<Contig> {
    header
        .contigs()
        .iter()
        .map(|(name, map)| Contig {
            name: name.clone(),
            length: map.length().map(|length| length as u64),
            md5: map.md5().map(str::to_owned),
        })
        .collect()
}
