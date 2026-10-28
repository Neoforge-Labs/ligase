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
//!   columns, so there is no identifier column to find. A file of nothing
//!   but comment lines is such a file: a representation with no table under
//!   it is not a molecule set.
//! - **A row the header cannot account for is an error**, not a short row
//!   padded with blanks. A table whose rows do not fit its header is not the
//!   table it claims to be.
//!
//! `\r` is a line ending here, not a character, so a file Excel wrote means
//! what the same file written by anything else means. That is not a nicety:
//! Excel is the tool that corrupts gene symbols, so it is the tool whose
//! files arrive here most often, and `TP53\r` would match no namespace, find
//! no mapping and be reported as a gene nobody has heard of.
//!
//! **A lone `\r` ends a line too.** Excel for Mac 2011 and the classic Mac
//! tools wrote CR alone, and such files are still in circulation. Read by a
//! reader that only breaks on `\n`, a four-hundred-gene list is one line: it
//! loads *successfully* as a header and no rows, so four hundred genes
//! become none and every check downstream is satisfied. That is the worst
//! shape a bug can have here, so the line splitter handles all three endings
//! rather than the two that are common.

use std::collections::VecDeque;
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
    /// Lines a physical read produced beyond the first.
    ///
    /// A CR-terminated file is one physical line and many logical ones, so
    /// the splitter can hand back more than it was asked for.
    pending: VecDeque<String>,
}

impl<R: BufRead> Table<R> {
    /// Read the header, failing if there is not one.
    pub(crate) fn read(mut reader: R) -> Result<Self, String> {
        let mut pending = VecDeque::new();
        match next_line(&mut reader, &mut pending)? {
            Some(line) => Ok(Self {
                header: line
                    .split('\t')
                    .map(|name| name.trim().to_owned())
                    .collect(),
                reader,
                // Whatever the header's physical read produced beyond the
                // header itself. Discarding it here is what made a
                // CR-terminated file load as a header and no rows.
                pending,
            }),
            None => Err("no header row".to_owned()),
        }
    }

    /// The same, after a block of leading `#` lines, which are returned.
    ///
    /// A molecule table carries its representation on such a line, and both
    /// the loader and the canonical form have to read it — so the skipping
    /// lives here, with the reader, rather than being done twice. A reader
    /// that disagreed with a canonical form about where the header starts
    /// would hash a file the engine had described differently, which is the
    /// bug this module exists as one copy to prevent.
    ///
    /// The comment lines come back **verbatim and in order**, because
    /// nothing may drop them: one of them is a dimension, and the rest are
    /// somebody's own words about their own file.
    pub(crate) fn read_after_comments(mut reader: R) -> Result<(Self, Vec<String>), String> {
        let mut pending = VecDeque::new();
        let mut comments = Vec::new();
        loop {
            match next_line(&mut reader, &mut pending)? {
                Some(line) if line.starts_with('#') => comments.push(line),
                // A blank line before the header is skipped as one after it
                // is, by `next_row`, and for the same reason.
                Some(line) if line.is_empty() => {}
                Some(line) => {
                    return Ok((
                        Self {
                            header: line
                                .split('\t')
                                .map(|name| name.trim().to_owned())
                                .collect(),
                            reader,
                            pending,
                        },
                        comments,
                    ));
                }
                None => return Err("no header row".to_owned()),
            }
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
            let Some(line) = next_line(&mut self.reader, &mut self.pending)? else {
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
///
/// `pending` holds whatever a physical read produced beyond the first line,
/// which is how a CR-terminated file is read as the rows it holds rather
/// than as one very long header.
fn next_line<R: BufRead>(
    reader: &mut R,
    pending: &mut VecDeque<String>,
) -> Result<Option<String>, String> {
    if let Some(line) = pending.pop_front() {
        return Ok(Some(line));
    }

    let mut line = String::new();
    if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
        return Ok(None);
    }
    while line.ends_with('\n') || line.ends_with('\r') {
        line.pop();
    }

    // A lone `\r` inside what `read_line` returned is a line ending, not a
    // character: `split` rather than `trim` is what makes a CR-terminated
    // file several rows instead of one.
    if line.contains('\r') {
        let mut parts = line.split('\r');
        let first = parts.next().unwrap_or_default().to_owned();
        pending.extend(parts.map(ToOwned::to_owned));
        return Ok(Some(first));
    }
    Ok(Some(line))
}
