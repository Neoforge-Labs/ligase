//! FASTA: the contigs, in order, as sequence.
//!
//! The sequence is uppercased and unwrapped, which is the form the SAM
//! specification defines its `M5` contig checksum over — so this agrees with
//! what [`crate::dim::ReferenceId`] already fingerprints, rather than
//! inventing a second notion of what a reference is. Soft masking goes with
//! the case, deliberately: a lowercased repeat region is an annotation
//! applied to a sequence, not a different sequence.
//!
//! Contigs keep their order ([D14](../../../docs/decisions.md)): a reference
//! *is* a sequence dictionary, and two of them have to agree on order.
//!
//! The description after a contig's name is dropped. It is free text — "HIV-1
//! isolate HXB2, complete genome" — and two files whose sequences and names
//! agree are the same reference whatever prose follows the name.
//!
//! Scanned by hand rather than through `noodles`, for the same reason
//! `crate::load::fasta` is: a three-gigabase reference has to be reduced
//! without being held in memory.

use std::io::BufRead;
use std::path::Path;

use super::Digest;
use crate::load::open;

/// Hash a reference's canonical form.
///
/// Each contig's sequence is folded into a hash of its own as it is read,
/// so a chromosome is never held in memory — and so the line breaks, which
/// arrive as nothing more than the points between updates, cannot reach the
/// result.
pub(super) fn canonicalise(path: &Path, digest: &mut Digest) -> Result<(), String> {
    let mut reader = open(path).map_err(|error| error.to_string())?;
    let mut line = Vec::new();
    let mut sequence: Option<blake3::Hasher> = None;

    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        let text = trim(&line);
        if text.is_empty() {
            continue;
        }

        if text[0] == b'>' {
            if let Some(hasher) = sequence.take() {
                digest.field("sequence", hasher.finalize().as_bytes());
            }
            // The name runs to the first whitespace; the rest is prose.
            let name = text[1..]
                .split(|byte| byte.is_ascii_whitespace())
                .next()
                .unwrap_or_default();
            if name.is_empty() {
                return Err("a `>` line with no contig name".to_owned());
            }
            digest.field("contig", name);
            sequence = Some(blake3::Hasher::new());
        } else {
            let Some(hasher) = sequence.as_mut() else {
                // Sequence before any `>`: not a FASTA, whatever else it is.
                return Err("sequence before the first `>` line".to_owned());
            };
            // Uppercased: the form the `M5` checksum is defined over.
            hasher.update(&text.to_ascii_uppercase());
        }
    }

    match sequence {
        Some(hasher) => digest.field("sequence", hasher.finalize().as_bytes()),
        None => return Err("no `>` lines at all".to_owned()),
    }
    Ok(())
}

/// A line without its newline or trailing whitespace.
fn trim(line: &[u8]) -> &[u8] {
    let mut end = line.len();
    while end > 0 && line[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &line[..end]
}
