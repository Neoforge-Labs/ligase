//! FASTA: contig names, lengths and computed MD5 checksums.
//!
//! The scan is streaming and holds no sequence in memory. That is not
//! premature optimisation: a human reference is three gigabases, and a loader
//! that materialised each contig to checksum it would need gigabytes of RAM
//! to answer a question about a header.

use std::io::BufRead;
use std::path::Path;

use md5::{Digest as _, Md5};

use super::{Declared, LoadError, open};
use crate::dim::{Contig, Dim, Dimensions, Frame, ReferenceId, Source};
use crate::types::{DataKind, Dataset};

/// Load a reference FASTA, inferring its contigs.
///
/// The MD5 of each contig is computed here rather than read from anywhere:
/// it is the digest the SAM specification defines for `@SQ M5`, over the
/// uppercased sequence with whitespace removed. Computing it ourselves is
/// what lets us check an aligner's claim about which reference it used.
pub fn reference(path: &Path, declared: &Declared) -> Result<Dataset, LoadError> {
    let contigs = scan(path)?;

    let organism = super::organism(declared, "a FASTA does not record its organism; declare it");

    let (reference, contig_naming) = if contigs.is_empty() {
        (
            Dim::unknown("the FASTA contains no sequences"),
            Dim::unknown("the FASTA contains no sequences"),
        )
    } else {
        let identity = ReferenceId::new(declared.build.clone(), contigs).with_frame(
            declared
                .frame
                .as_ref()
                .map(|(name, first)| Frame::new(name, *first)),
        );
        let naming = identity.naming();
        (
            Dim::known(
                identity,
                Source::Inferred {
                    path: path.to_path_buf(),
                    detail: "FASTA contigs",
                },
            ),
            Dim::known(
                naming,
                Source::Inferred {
                    path: path.to_path_buf(),
                    detail: "FASTA deflines",
                },
            ),
        )
    };

    let dimensions = Dimensions {
        organism,
        release: super::release(
            declared,
            "a FASTA does not record which release it came from; declare it",
        ),
        reference,
        contig_naming,
        ..Dimensions::default()
    };

    Ok(Dataset::new(
        DataKind::Reference,
        Some(path.to_path_buf()),
        dimensions,
    ))
}

/// Stream the file, accumulating one contig per defline.
fn scan(path: &Path) -> Result<Vec<Contig>, LoadError> {
    let reader = open(path)?;
    let mut contigs: Vec<Contig> = Vec::new();
    let mut current: Option<(String, u64, Md5)> = None;
    let mut saw_defline = false;

    for line in reader.lines() {
        let line = line.map_err(|error| LoadError::io(path, error))?;

        if let Some(defline) = line.strip_prefix('>') {
            saw_defline = true;
            if let Some(contig) = current.take() {
                contigs.push(finish(contig));
            }
            // The name is the first whitespace-delimited token; the rest is
            // a description and carries no identity.
            let name = defline.split_whitespace().next().unwrap_or_default();
            if name.is_empty() {
                return Err(LoadError::format(path, "a defline has no sequence name"));
            }
            current = Some((name.to_owned(), 0, Md5::new()));
        } else if let Some((_, length, hasher)) = current.as_mut() {
            // Uppercased, whitespace stripped: the canonical form the M5
            // checksum is defined over. Soft-masked (lowercase) repeats must
            // not change a reference's identity.
            let bases: Vec<u8> = line
                .bytes()
                .filter(|byte| !byte.is_ascii_whitespace())
                .map(|byte| byte.to_ascii_uppercase())
                .collect();
            *length += bases.len() as u64;
            hasher.update(&bases);
        } else if !line.trim().is_empty() {
            return Err(LoadError::format(
                path,
                "sequence data appears before the first defline",
            ));
        }
    }

    if let Some(contig) = current.take() {
        contigs.push(finish(contig));
    }
    if !saw_defline {
        return Err(LoadError::format(path, "no FASTA deflines found"));
    }

    Ok(contigs)
}

fn finish((name, length, hasher): (String, u64, Md5)) -> Contig {
    let digest = hasher.finalize();
    Contig {
        name,
        length: Some(length),
        md5: Some(digest.iter().map(|byte| format!("{byte:02x}")).collect()),
    }
}
