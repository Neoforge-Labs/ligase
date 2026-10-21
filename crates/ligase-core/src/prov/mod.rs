//! Provenance: how each result was produced, kept for longer than the cache.
//!
//! The cache answers one question — "has this been computed?" — and is
//! designed to be thrown away, because everything in it can be recomputed
//! ([D19](https://ligase.neoforgelabs.tech/project/decisions/)). This module
//! answers a different one: "how was this produced?" And that answer cannot
//! be recomputed, because the machine, the tool versions and the command that
//! produced an object a year ago are gone.
//!
//! # Why this exists at all
//!
//! It is the debt D22 took on. Every tool in the reference pipeline records
//! its own command line inside the file it produces, and M4 drops those
//! fields before hashing — it had to, because `minimap2`'s `@PG CL` holds
//! the output path, the output path is the staging file named after the
//! step's own key, and so a step's output bytes contained that step's own
//! key. The cost, stated plainly at the end of D22: a cache hit can no
//! longer prove which command produced an object, because the field that
//! said so is not in the hash.
//!
//! So the command is recorded here instead, at the one moment it is true —
//! when the step runs — and read back when the step is served from the
//! cache. That is the whole design.
//!
//! # Why it is a separate database
//!
//! The cache drops its tables when [`crate::key::KEY_VERSION`] or
//! [`crate::canon::CANON_VERSION`] moves, because an entry written under
//! another format is a claim about ingredients that were combined
//! differently and must never be read. None of that reasoning applies to a
//! record of what a command did: that stays true however the engine later
//! decides to compose keys. So provenance lives in `provenance.sqlite`
//! beside `cache.sqlite`, with its own version, and a cache cleared to its
//! foundations takes nothing of it.
//!
//! # What a record is keyed on
//!
//! The step key, which is already the identity of the computation: the same
//! code, parameters, inputs and tool versions are the same step, and a
//! second run of it has nothing new to say. What is *not* here is anything
//! that depends on the pipeline a step was reached from — the sample label a
//! report puts beside its name, for instance, which [`crate::graph::Graph`]
//! can work out whenever it is needed and which would otherwise be a fact
//! about one run masquerading as a fact about the step.

pub mod bundle;
pub mod cite;
mod methods;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

pub use cite::Citation;
pub use methods::{Gap, Group, Methods, Noticed, methods};

use crate::canon::Note;
use crate::dim::ReferenceId;
use crate::hash::ContentHash;
use crate::key::StepKey;
use crate::types::DataKind;

/// The version of the provenance schema.
///
/// Bumped only when a column changes meaning. Unlike the cache's version,
/// this one does not clear anything: a record of how something was produced
/// is not invalidated by the engine learning to record more, so a bump adds
/// columns and leaves the rows alone.
///
/// 2 added `produced_notes`, for what canonicalising an output noticed
/// (D26). A record written under 1 has no notes rather than no meaning.
pub const PROV_VERSION: u32 = 2;

/// The file provenance is kept in, beside the cache's own.
const DATABASE: &str = "provenance.sqlite";

/// One file a step was given, by content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputRecord {
    /// The parameter it filled.
    pub param: String,
    /// What kind of data it was read as.
    pub kind: DataKind,
    /// Where it was at the time.
    pub path: PathBuf,
    /// The hash of its bytes.
    pub bytes: ContentHash,
    /// The hash of what it means, which is what the key was built from.
    pub canonical: ContentHash,
}

/// A reference a step was given, pinned as firmly as the file allows.
///
/// `CLAUDE.md` asks for remote data to be pinned by accession, version and
/// checksum. For a reference FASTA all three are already in the file and
/// already read by [`crate::load::reference`]: the contig name is the
/// accession with its version where the sequence was deposited under one,
/// and the per-contig MD5 is the digest the SAM specification defines for
/// `@SQ M5`, over the uppercased sequence with whitespace removed. Nothing
/// here is declared or invented.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceRecord {
    /// The parameter it filled.
    pub param: String,
    /// Where it was at the time.
    pub path: PathBuf,
    /// The label the user gave the build, e.g. `HXB2`. Display only.
    pub build: Option<String>,
    /// The accession with its version, e.g. `K03455.1`, when the sequence
    /// name is one.
    pub accession: Option<String>,
    /// How many sequences it holds.
    pub contigs: usize,
    /// Their total length in bases.
    pub length: u64,
    /// The MD5 of the sequence, for a single-sequence reference.
    pub checksum: Option<String>,
    /// A short digest of the whole identity, which is always available.
    pub fingerprint: String,
}

/// How one step produced one output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The step's cache key, which is this record's identity.
    pub key: StepKey,
    /// The step's name, as written.
    pub step: String,
    /// The source of the step's function.
    pub code: String,
    /// The command that ran, with its placeholders filled in.
    ///
    /// `None` for a step whose body is Python: there was no command, and
    /// [`Self::code`] is the honest answer for one.
    pub command: Option<String>,
    /// What kind of data it produced.
    pub output_kind: DataKind,
    /// The hash of the output's bytes.
    pub output_bytes: ContentHash,
    /// The hash of what the output means.
    pub output_canonical: ContentHash,
    /// What canonicalising the output noticed ([`Note`]).
    ///
    /// Recorded rather than left to the run report, because a methods
    /// section and a bundle are written from provenance and both have to be
    /// able to say that an identifier in them is a file name (D26).
    pub output_notes: Vec<Note>,
    /// Plain parameters, as they were hashed.
    pub parameters: Vec<(String, String)>,
    /// Each input file, by content.
    pub inputs: Vec<InputRecord>,
    /// Each reference, pinned.
    pub references: Vec<ReferenceRecord>,
    /// External tools, as `(name, the whole version it printed)`.
    pub tools: Vec<(String, String)>,
    /// The version of Ligase that ran it.
    pub engine: String,
    /// The platform it ran on.
    pub platform: String,
    /// When it ran, as seconds since the Unix epoch.
    pub recorded_at: i64,
    /// How long it took, in milliseconds.
    pub duration_ms: u64,
}

/// Why provenance could not be read or written.
#[derive(Debug)]
pub enum ProvError {
    /// The directory could not be created.
    Io {
        /// What was being written.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// The database could not be read or written.
    Database(rusqlite::Error),
    /// A thread holding the database panicked.
    Poisoned,
    /// A stored record could not be read back as one.
    ///
    /// It is not treated as an absent record. "I never knew" and "I knew
    /// and the note is damaged" are different answers, and a provenance
    /// store that rounded the second to the first would be the one kind of
    /// store that must not.
    Corrupt {
        /// Which field, and what was there.
        detail: String,
    },
}

impl fmt::Display for ProvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Database(error) => write!(f, "provenance: {error}"),
            Self::Poisoned => f.write_str("provenance was left locked by a panicking thread"),
            Self::Corrupt { detail } => write!(f, "a provenance record is damaged: {detail}"),
        }
    }
}

impl std::error::Error for ProvError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Database(error) => Some(error),
            Self::Poisoned | Self::Corrupt { .. } => None,
        }
    }
}

impl From<rusqlite::Error> for ProvError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

/// Everything known about how results were produced.
#[derive(Debug)]
pub struct Prov {
    database: Mutex<Connection>,
}

impl Prov {
    /// Open the store in `root`, creating it if it is not there.
    ///
    /// # Errors
    ///
    /// If the directory cannot be created or the database cannot be opened.
    pub fn open(root: &Path) -> Result<Self, ProvError> {
        std::fs::create_dir_all(root).map_err(|source| ProvError::Io {
            path: root.to_path_buf(),
            source,
        })?;

        let database = Connection::open(root.join(DATABASE))?;
        // WAL and a timeout for the same reason the cache has them: workers
        // writing their own rows at once should wait, not fail.
        database.pragma_update(None, "journal_mode", "WAL")?;
        database.busy_timeout(std::time::Duration::from_secs(30))?;
        prepare(&database)?;

        Ok(Self {
            database: Mutex::new(database),
        })
    }

    /// Record how a step produced its output.
    ///
    /// Writing the same key twice replaces the row. The key is the identity
    /// of the computation, so the second record says the same thing about
    /// the same work — except for the machine and the moment, where the
    /// later answer is the better one.
    ///
    /// # Errors
    ///
    /// If the record cannot be written.
    pub fn record(&self, record: &Record) -> Result<(), ProvError> {
        let key = record.key.to_hex();
        let mut guard = self.database.lock().map_err(|_| ProvError::Poisoned)?;
        let transaction = guard.transaction()?;

        transaction.execute(
            "INSERT OR REPLACE INTO produced
                 (key, step, code, command, output_kind, output_bytes,
                  output_canonical, engine, platform, recorded_at, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                key,
                record.step,
                record.code,
                record.command,
                record.output_kind.name(),
                record.output_bytes.to_hex(),
                record.output_canonical.to_hex(),
                record.engine,
                record.platform,
                record.recorded_at,
                i64::try_from(record.duration_ms).unwrap_or(i64::MAX),
            ],
        )?;

        for table in ["parameters", "inputs", "references", "tools", "notes"] {
            transaction.execute(
                &format!("DELETE FROM produced_{table} WHERE key = ?1"),
                params![key],
            )?;
        }

        for (position, (name, value)) in record.parameters.iter().enumerate() {
            transaction.execute(
                "INSERT INTO produced_parameters (key, position, name, value)
                 VALUES (?1, ?2, ?3, ?4)",
                params![key, position_of(position), name, value],
            )?;
        }

        for (position, input) in record.inputs.iter().enumerate() {
            transaction.execute(
                "INSERT INTO produced_inputs
                     (key, position, param, kind, path, bytes, canonical)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    key,
                    position_of(position),
                    input.param,
                    input.kind.name(),
                    input.path.to_string_lossy(),
                    input.bytes.to_hex(),
                    input.canonical.to_hex(),
                ],
            )?;
        }

        for (position, reference) in record.references.iter().enumerate() {
            transaction.execute(
                "INSERT INTO produced_references
                     (key, position, param, path, build, accession, contigs,
                      length, checksum, fingerprint)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    key,
                    position_of(position),
                    reference.param,
                    reference.path.to_string_lossy(),
                    reference.build,
                    reference.accession,
                    position_of(reference.contigs),
                    i64::try_from(reference.length).unwrap_or(i64::MAX),
                    reference.checksum,
                    reference.fingerprint,
                ],
            )?;
        }

        for (position, (tool, version)) in record.tools.iter().enumerate() {
            transaction.execute(
                "INSERT INTO produced_tools (key, position, tool, version)
                 VALUES (?1, ?2, ?3, ?4)",
                params![key, position_of(position), tool, version],
            )?;
        }

        for (position, note) in record.output_notes.iter().enumerate() {
            transaction.execute(
                "INSERT INTO produced_notes (key, position, code, value)
                 VALUES (?1, ?2, ?3, ?4)",
                params![key, position_of(position), note.code(), note.value()],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    /// How the step with this key produced its output, if it is known.
    ///
    /// # Errors
    ///
    /// If the record cannot be read, or is there but damaged.
    pub fn lookup(&self, key: StepKey) -> Result<Option<Record>, ProvError> {
        let hex = key.to_hex();
        let guard = self.database.lock().map_err(|_| ProvError::Poisoned)?;

        let head: Option<Head> = guard
            .query_row(
                "SELECT step, code, command, output_kind, output_bytes,
                        output_canonical, engine, platform, recorded_at, duration_ms
                 FROM produced WHERE key = ?1",
                params![hex],
                |row| {
                    Ok(Head {
                        step: row.get(0)?,
                        code: row.get(1)?,
                        command: row.get(2)?,
                        output_kind: row.get(3)?,
                        output_bytes: row.get(4)?,
                        output_canonical: row.get(5)?,
                        engine: row.get(6)?,
                        platform: row.get(7)?,
                        recorded_at: row.get(8)?,
                        duration_ms: row.get(9)?,
                    })
                },
            )
            .optional()?;

        let Some(head) = head else {
            return Ok(None);
        };

        let mut parameters = Vec::new();
        {
            let mut statement = guard.prepare(
                "SELECT name, value FROM produced_parameters
                 WHERE key = ?1 ORDER BY position",
            )?;
            let rows = statement.query_map(params![hex], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                parameters.push(row?);
            }
        }

        let mut inputs = Vec::new();
        {
            let mut statement = guard.prepare(
                "SELECT param, kind, path, bytes, canonical FROM produced_inputs
                 WHERE key = ?1 ORDER BY position",
            )?;
            let rows = statement.query_map(params![hex], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            for row in rows {
                let (param, kind, path, bytes, canonical) = row?;
                inputs.push(InputRecord {
                    param,
                    kind: kind_of(&kind)?,
                    path: PathBuf::from(path),
                    bytes: hash_of(&bytes)?,
                    canonical: hash_of(&canonical)?,
                });
            }
        }

        let mut references = Vec::new();
        {
            let mut statement = guard.prepare(
                "SELECT param, path, build, accession, contigs, length,
                        checksum, fingerprint
                 FROM produced_references WHERE key = ?1 ORDER BY position",
            )?;
            let rows = statement.query_map(params![hex], |row| {
                Ok(ReferenceRecord {
                    param: row.get(0)?,
                    path: PathBuf::from(row.get::<_, String>(1)?),
                    build: row.get(2)?,
                    accession: row.get(3)?,
                    contigs: usize::try_from(row.get::<_, i64>(4)?).unwrap_or_default(),
                    length: u64::try_from(row.get::<_, i64>(5)?).unwrap_or_default(),
                    checksum: row.get(6)?,
                    fingerprint: row.get(7)?,
                })
            })?;
            for row in rows {
                references.push(row?);
            }
        }

        let mut tools = Vec::new();
        {
            let mut statement = guard.prepare(
                "SELECT tool, version FROM produced_tools
                 WHERE key = ?1 ORDER BY position",
            )?;
            let rows = statement.query_map(params![hex], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                tools.push(row?);
            }
        }

        let mut output_notes = Vec::new();
        {
            let mut statement = guard.prepare(
                "SELECT code, value FROM produced_notes
                 WHERE key = ?1 ORDER BY position",
            )?;
            let rows = statement.query_map(params![hex], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (code, value) = row?;
                // A code this version does not know is skipped rather than
                // read as an error: a database written by a later engine is
                // still a record of how something was produced.
                if let Some(note) = Note::parse(&code, &value) {
                    output_notes.push(note);
                }
            }
        }

        Ok(Some(Record {
            key,
            step: head.step,
            code: head.code,
            command: head.command,
            output_kind: kind_of(&head.output_kind)?,
            output_bytes: hash_of(&head.output_bytes)?,
            output_canonical: hash_of(&head.output_canonical)?,
            output_notes,
            parameters,
            inputs,
            references,
            tools,
            engine: head.engine,
            platform: head.platform,
            recorded_at: head.recorded_at,
            duration_ms: u64::try_from(head.duration_ms).unwrap_or_default(),
        }))
    }
}

/// The columns of `produced` itself, before its child tables are read.
struct Head {
    step: String,
    code: String,
    command: Option<String>,
    output_kind: String,
    output_bytes: String,
    output_canonical: String,
    engine: String,
    platform: String,
    recorded_at: i64,
    duration_ms: i64,
}

/// A hash read back from a row, or a complaint naming what was there.
fn hash_of(text: &str) -> Result<ContentHash, ProvError> {
    ContentHash::parse(text).ok_or_else(|| ProvError::Corrupt {
        detail: format!("`{text}` is not a content hash"),
    })
}

/// A data kind read back from a row, or a complaint naming what was there.
fn kind_of(text: &str) -> Result<DataKind, ProvError> {
    DataKind::from_name(text).ok_or_else(|| ProvError::Corrupt {
        detail: format!("`{text}` is not a kind of data"),
    })
}

/// Everything about a reference that can be pinned, read from its identity.
///
/// Nothing is guessed: the accession is the sequence name where the name is
/// one, the checksum is the MD5 the loader computed, and the fingerprint is
/// there whether or not either of the others is.
#[must_use]
pub fn pin(param: &str, path: &Path, identity: &ReferenceId) -> ReferenceRecord {
    let contigs = identity.contigs();
    let single = match contigs {
        [only] => Some(only),
        _ => None,
    };

    ReferenceRecord {
        param: param.to_owned(),
        path: path.to_path_buf(),
        build: identity.label().map(str::to_owned),
        accession: single
            .map(|contig| contig.name.as_str())
            .filter(|name| is_accession(name))
            .map(str::to_owned),
        contigs: contigs.len(),
        length: contigs.iter().filter_map(|contig| contig.length).sum(),
        checksum: single.and_then(|contig| contig.md5.clone()),
        fingerprint: identity.fingerprint(),
    }
}

/// Whether a sequence name is an accession, by its shape.
///
/// INSDC and RefSeq accessions are letters, an optional underscore, five to
/// ten digits, and an optional `.version`: `K03455.1`, `NC_001802.1`,
/// `CP001187`. A name is only *called* an accession when it looks like one,
/// because a methods section that cited `chr1` as an accession would be
/// worse than one that admitted it had none.
fn is_accession(name: &str) -> bool {
    let (body, version) = match name.split_once('.') {
        Some((body, version)) => (body, Some(version)),
        None => (name, None),
    };
    if let Some(version) = version
        && (version.is_empty() || !version.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return false;
    }

    let letters = body
        .bytes()
        .take_while(|byte| byte.is_ascii_uppercase())
        .count();
    if letters == 0 || letters > 6 {
        return false;
    }
    let rest = &body[letters..];
    let digits = rest.strip_prefix('_').unwrap_or(rest);
    (5..=10).contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// The platform a run happened on, as a short string.
///
/// Part of provenance because a result can depend on it: a tool built
/// against a different libc is a different tool, however it reports its
/// version.
#[must_use]
pub fn platform() -> String {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    format!("{os}-{arch}")
}

/// Now, as seconds since the Unix epoch.
#[must_use]
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| i64::try_from(since.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

/// A position as the integer SQLite stores, saturating rather than wrapping.
fn position_of(position: usize) -> i64 {
    i64::try_from(position).unwrap_or(i64::MAX)
}

/// Create the tables.
///
/// Nothing is ever dropped here. A record of what a command did stays true
/// however the engine later composes its keys, which is the whole reason
/// this is not in the cache's database.
fn prepare(database: &Connection) -> Result<(), ProvError> {
    database.execute(
        "CREATE TABLE IF NOT EXISTS meta (name TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    )?;
    database.execute(
        "INSERT OR REPLACE INTO meta (name, value) VALUES ('prov_version', ?1)",
        params![PROV_VERSION.to_string()],
    )?;

    database.execute(
        "CREATE TABLE IF NOT EXISTS produced (
             key              TEXT PRIMARY KEY,
             step             TEXT NOT NULL,
             code             TEXT NOT NULL,
             command          TEXT,
             output_kind      TEXT NOT NULL,
             output_bytes     TEXT NOT NULL,
             output_canonical TEXT NOT NULL,
             engine           TEXT NOT NULL,
             platform         TEXT NOT NULL,
             recorded_at      INTEGER NOT NULL,
             duration_ms      INTEGER NOT NULL
         )",
        [],
    )?;

    // Child tables rather than JSON blobs, so that `sqlite3 provenance.sqlite
    // 'select * from produced_tools'` answers a question about a paper
    // without a parser. The cache makes the same bet (D6).
    database.execute(
        "CREATE TABLE IF NOT EXISTS produced_parameters (
             key      TEXT NOT NULL,
             position INTEGER NOT NULL,
             name     TEXT NOT NULL,
             value    TEXT NOT NULL,
             PRIMARY KEY (key, position)
         )",
        [],
    )?;
    database.execute(
        "CREATE TABLE IF NOT EXISTS produced_inputs (
             key       TEXT NOT NULL,
             position  INTEGER NOT NULL,
             param     TEXT NOT NULL,
             kind      TEXT NOT NULL,
             path      TEXT NOT NULL,
             bytes     TEXT NOT NULL,
             canonical TEXT NOT NULL,
             PRIMARY KEY (key, position)
         )",
        [],
    )?;
    database.execute(
        "CREATE TABLE IF NOT EXISTS produced_references (
             key         TEXT NOT NULL,
             position    INTEGER NOT NULL,
             param       TEXT NOT NULL,
             path        TEXT NOT NULL,
             build       TEXT,
             accession   TEXT,
             contigs     INTEGER NOT NULL,
             length      INTEGER NOT NULL,
             checksum    TEXT,
             fingerprint TEXT NOT NULL,
             PRIMARY KEY (key, position)
         )",
        [],
    )?;
    database.execute(
        "CREATE TABLE IF NOT EXISTS produced_tools (
             key      TEXT NOT NULL,
             position INTEGER NOT NULL,
             tool     TEXT NOT NULL,
             version  TEXT NOT NULL,
             PRIMARY KEY (key, position)
         )",
        [],
    )?;
    // What canonicalising the output noticed and its hash cannot say
    // (D26). Almost always empty, and a row here is not a failure: the step
    // ran and its result stands.
    database.execute(
        "CREATE TABLE IF NOT EXISTS produced_notes (
             key      TEXT NOT NULL,
             position INTEGER NOT NULL,
             code     TEXT NOT NULL,
             value    TEXT NOT NULL,
             PRIMARY KEY (key, position)
         )",
        [],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn an_accession_is_recognised_by_its_shape() {
        assert!(is_accession("K03455.1"), "INSDC, with a version");
        assert!(is_accession("K03455"), "INSDC, without one");
        assert!(is_accession("NC_001802.1"), "RefSeq");
        assert!(is_accession("CP001187"), "two letters, six digits");
    }

    #[test]
    fn a_sequence_name_is_not_an_accession() {
        assert!(!is_accession("chr1"), "too few digits");
        assert!(!is_accession("1"), "no letters");
        assert!(!is_accession("chrX"), "no digits");
        assert!(!is_accession("K03455.x"), "a version is digits");
        assert!(!is_accession("SCAF_00012345678"), "too many digits");
        assert!(!is_accession("scaffold1"), "an accession is uppercase");
        assert!(!is_accession(""), "nothing at all");
    }
}
