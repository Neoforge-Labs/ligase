//! Reading a tab-separated table with a header, strictly.
//!
//! One reader, shared by the three places that need it: the entity-set
//! loader, the entity-set canonical form, and `map_identifiers`. They have
//! to agree about what a row is — a loader that read a file one way and a
//! canonical form that read it another would hash a file the engine had
//! described differently — and the way to make them agree is for there to be
//! one of them.
//!
//! Strict on two points, both for the reason `canon` gives for every parser
//! here being strict: a reader that took what it could and ignored the rest
//! would let two files that differ in the ignored part compare equal.
//!
//! - **A file with no header is not a table.** There is nothing to name the
//!   columns, so there is no identifier column to find.
//! - **A row the header cannot account for is an error**, not a short row
//!   padded with blanks. A table whose rows do not fit its header is not the
//!   table it claims to be.
//!
//! `\r` is stripped with `\n`, so a file Excel wrote means what the same
//! file written by anything else means. That is not a nicety: Excel is the
//! tool that corrupts gene symbols, so it is the tool whose files arrive
//! here most often, and `TP53\r` would match no namespace, find no mapping
//! and be reported as a gene nobody has heard of.

use std::io::BufRead;

/// One row of a table, as text and as fields.
///
/// Both, because the callers want different things and neither should
/// re-split what the other already split: a canonical form hashes the
/// line, and a loader reads one column out of the fields.
pub(crate) struct Row {
    /// The line as written, without its line ending.
    pub line: String,
    /// Its fields, split on tabs.
    fields: Vec<String>,
}

impl Row {
    /// One field by position, trimmed, or `None` past the end of the row.
    pub(crate) fn field(&self, at: usize) -> Option<&str> {
        self.fields.get(at).map(|field| field.trim())
    }
}

/// A tab-separated table being read, header first.
pub(crate) struct Table<R> {
    reader: R,
    header: Vec<String>,
}

impl<R: BufRead> Table<R> {
    /// Read the header, failing if there is not one.
    pub(crate) fn read(mut reader: R) -> Result<Self, String> {
        match next_line(&mut reader)? {
            Some(line) => Ok(Self {
                header: line
                    .split('\t')
                    .map(|name| name.trim().to_owned())
                    .collect(),
                reader,
            }),
            None => Err("no header row".to_owned()),
        }
    }

    /// The column names, in the order the header lists them.
    pub(crate) fn header(&self) -> &[String] {
        &self.header
    }

    /// Which column has this name, if one does.
    pub(crate) fn column(&self, name: &str) -> Option<usize> {
        self.header.iter().position(|column| column == name.trim())
    }

    /// The next row, or `None` at the end of the file.
    ///
    /// A blank line is skipped rather than read as a row of one empty
    /// field: a file ending in a newline leaves one, and every TSV does.
    pub(crate) fn next_row(&mut self) -> Result<Option<Row>, String> {
        loop {
            let Some(line) = next_line(&mut self.reader)? else {
                return Ok(None);
            };
            if line.is_empty() {
                continue;
            }
            let fields: Vec<String> = line.split('\t').map(ToOwned::to_owned).collect();
            if fields.len() != self.header.len() {
                return Err(format!(
                    "a row of {} columns under a header of {}",
                    fields.len(),
                    self.header.len()
                ));
            }
            return Ok(Some(Row { line, fields }));
        }
    }
}

/// One line, without its line ending, or `None` at the end of the file.
fn next_line<R: BufRead>(reader: &mut R) -> Result<Option<String>, String> {
    let mut line = String::new();
    if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
        return Ok(None);
    }
    while line.ends_with('\n') || line.ends_with('\r') {
        line.pop();
    }
    Ok(Some(line))
}
