//! A resistance report: a header row, and rows under it.
//!
//! Ligase's own output format, and the one place here where the canonical
//! form is a choice rather than a reading of someone else's specification.
//! The header row is kept as a row, because which columns a report has is
//! part of what it says; the rows below it are sorted, because a report is a
//! set of findings and the order they were written in is the order the
//! variants came off the caller.

use std::io::BufRead;
use std::path::Path;

use super::Digest;
use crate::load::open;

/// Hash a report's canonical form.
pub(super) fn canonicalise(path: &Path, digest: &mut Digest) -> Result<(), String> {
    let mut reader = open(path).map_err(|error| error.to_string())?;
    let mut line = String::new();
    let mut columns: Option<usize> = None;

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
        // A trailing newline leaves one empty line, which is not a row.
        if text.is_empty() {
            continue;
        }

        match columns {
            None => {
                digest.text("columns", text);
                columns = Some(text.split('\t').count());
            }
            Some(expected) => {
                let found = text.split('\t').count();
                if found != expected {
                    // A row that does not fit the header means this is not
                    // the table it claims to be.
                    return Err(format!(
                        "a row of {found} columns under a header of {expected}"
                    ));
                }
                digest.record(&[text.as_bytes()]);
            }
        }
    }

    if columns.is_none() {
        return Err("no header row".to_owned());
    }
    Ok(())
}
