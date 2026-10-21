//! BED: the intervals, as a bag.
//!
//! Intervals are **sorted**, and that is not an arbitrary choice but the one
//! [D14](https://ligase.neoforgelabs.tech/project/decisions/) already made: a
//! BED's record order is whatever order the intervals happened to be sorted
//! in, which is why the dimension checker does not compare it as contig
//! order. The same reasoning applies to its hash.
//!
//! `track` and `browser` lines are display instructions for a genome
//! browser, and `#` lines are comments. None of them is an interval.
//!
//! Parsed by hand, as `crate::load::intervals` is, because `noodles`' BED
//! reader is typed by column count and a BED in the wild has whichever
//! columns its author felt like writing.

use std::io::BufRead;
use std::path::Path;

use super::Digest;
use crate::load::open;

/// Hash an interval set's canonical form.
pub(super) fn canonicalise(path: &Path, digest: &mut Digest) -> Result<(), String> {
    let mut reader = open(path).map_err(|error| error.to_string())?;
    let mut line = String::new();
    let mut found = false;

    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        let text = line.trim_end_matches(['\n', '\r']);
        if text.is_empty() || is_decoration(text) {
            continue;
        }

        let fields: Vec<&str> = text.split('\t').collect();
        // Three columns is the minimum a BED record has: contig, start, end.
        if fields.len() < 3 {
            return Err(format!(
                "a line with {} columns, not 3 or more",
                fields.len()
            ));
        }
        if fields[1].parse::<u64>().is_err() || fields[2].parse::<u64>().is_err() {
            return Err("a start or end that is not a number".to_owned());
        }

        let parts: Vec<&[u8]> = fields.iter().map(|field| field.as_bytes()).collect();
        digest.record(&parts);
        found = true;
    }

    if !found {
        return Err("no intervals at all".to_owned());
    }
    Ok(())
}

/// Whether a line is for a genome browser rather than an interval.
fn is_decoration(line: &str) -> bool {
    line.starts_with('#') || line.starts_with("track") || line.starts_with("browser")
}
