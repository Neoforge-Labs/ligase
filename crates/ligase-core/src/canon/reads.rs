//! FASTQ: the reads, as a bag of name, sequence and qualities.
//!
//! Reads are **sorted**: a run re-fetched from the SRA comes back in a
//! different order often enough to be worth seeing through, and nothing
//! downstream depends on which read came first — an aligner is handed the
//! file, not the order.
//!
//! Two things go. The description after a read's name is free text that
//! `fasterq-dump` and friends write differently between versions. The `+`
//! line's optional repeat of the name is redundant by construction: the SAM
//! specification says that where it is present it must equal the name.
//!
//! Sequence and qualities both stay, in their own case: a quality string is
//! the measurement, and `docs/dimensions.md` makes the Phred offset a
//! dimension precisely because reading it wrongly is a silent error.

use std::path::Path;

use noodles::fastq;

use super::Digest;
use crate::load::open;

/// Hash a sample's canonical form.
pub(super) fn canonicalise(path: &Path, digest: &mut Digest) -> Result<(), String> {
    let mut reader = fastq::io::Reader::new(open(path).map_err(|error| error.to_string())?);
    let mut record = fastq::Record::default();
    let mut found = false;

    while reader
        .read_record(&mut record)
        .map_err(|error| error.to_string())?
        != 0
    {
        // A read with more bases than measurements is not a read. `noodles`
        // accepts four lines of any lengths, so this is checked here: a
        // canonical form computed over a record nothing understands is a
        // claim this layer has no business making.
        if record.sequence().len() != record.quality_scores().len() {
            return Err(format!(
                "a read of {} bases with {} quality scores",
                record.sequence().len(),
                record.quality_scores().len()
            ));
        }

        // The name runs to the first whitespace; `noodles` has already
        // split the description off it.
        digest.record(&[record.name(), record.sequence(), record.quality_scores()]);
        found = true;
    }

    if !found {
        return Err("no reads at all".to_owned());
    }
    Ok(())
}
