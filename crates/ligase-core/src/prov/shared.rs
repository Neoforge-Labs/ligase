//! Provenance that travels with a shared cache entry.
//!
//! # Why this exists
//!
//! M14's headline is a lab pointing every machine at one bucket: one
//! person aligns the cohort, and everybody else's `ligase run` hits the
//! cache. Built with only the cache published, that scenario quietly
//! breaks the *other* thing Ligase is for. A colleague's run completes
//! without running a step, and then `ligase methods` says
//!
//! > `align` (one input) is in the cache from a run that recorded no
//! > provenance, so how it was produced is not known here.
//!
//! for every step in the pipeline. The results are right and nobody can
//! write them up — which, for a tool whose claim is that a result can be
//! accounted for, is a worse failure than a cache miss.
//!
//! Provenance is a local SQLite store beside the cache's own, and the two
//! are deleted together on one machine, so the hole was unreachable in
//! practice until the cache could be shared. Now it is the normal case, so
//! a record goes into the store beside the output it explains, and a run
//! that takes an entry from the store takes the record with it.
//!
//! # Why a flat document rather than the six tables it came from
//!
//! A record is six tables in SQLite because SQLite is relational. In the
//! blob tier it is one object, because an object is what a key can name
//! and because half a record is not worth serving. The format is the
//! project's usual escaped TSV (D6's reasoning: a reviewer can read it),
//! line-tagged so that a later version adding a line type is ignored by an
//! older reader rather than misread by it.

use std::path::PathBuf;

use super::{InputRecord, PROV_VERSION, ProvError, Record, ReferenceRecord};
use crate::canon::Note;
use crate::hash::ContentHash;
use crate::key::StepKey;
use crate::types::{DataKind, Origin, RemoteVerdict};

/// The table published records live in.
pub const PROV: &str = "prov";

/// The format of the document below.
///
/// Separate from [`PROV_VERSION`], which versions the SQLite schema: this
/// one versions the serialisation, and the two move for different reasons.
const DOCUMENT_VERSION: u32 = 1;

/// Stands in for an absent optional value, which no real value can be
/// because every real one is written with a leading `=`.
const NONE: &str = "-";

/// A record as a document.
#[must_use]
pub fn encode(record: &Record) -> String {
    let mut lines = vec![row(&[
        "head",
        &DOCUMENT_VERSION.to_string(),
        &PROV_VERSION.to_string(),
        &record.step,
        &record.code,
        &option(record.command.as_deref()),
        record.output_kind.name(),
        &record.output_bytes.to_hex(),
        &record.output_canonical.to_hex(),
        &record.engine,
        &record.platform,
        &record.recorded_at.to_string(),
        &record.duration_ms.to_string(),
    ])];

    for note in &record.output_notes {
        lines.push(row(&["note", note.code(), &note.value()]));
    }
    for (name, value) in &record.parameters {
        lines.push(row(&["param", name, value]));
    }
    for input in &record.inputs {
        lines.push(row(&[
            "input",
            &input.param,
            input.kind.name(),
            &input.path.to_string_lossy(),
            &input.bytes.to_hex(),
            &input.canonical.to_hex(),
        ]));
    }
    for reference in &record.references {
        lines.push(row(&[
            "reference",
            &reference.param,
            &reference.path.to_string_lossy(),
            &option(reference.build.as_deref()),
            &option(reference.accession.as_deref()),
            &reference.contigs.to_string(),
            &reference.length.to_string(),
            &option(reference.checksum.as_deref()),
            &reference.fingerprint,
        ]));
    }
    for remote in &record.remotes {
        lines.push(row(&[
            "remote",
            &remote.locator,
            &remote.digest.to_hex(),
            &remote.bytes.to_hex(),
            &remote.fetched_at.to_string(),
            &remote.verdict.to_string(),
        ]));
    }
    for (name, version) in &record.tools {
        lines.push(row(&["tool", name, version]));
    }

    lines.join("\n")
}

/// A document as a record, under the key it was found for.
///
/// `None` for a document written by a format this build does not read:
/// the cost is one recomputation, where reading half of one would be a
/// methods paragraph that left something out and said nothing about it.
///
/// # Errors
///
/// [`ProvError::Corrupt`] for a document of the right format that does not
/// parse. "I never knew" and "I knew and the note is damaged" are
/// different answers, and provenance is the one store that must not round
/// the second to the first.
pub fn decode(key: StepKey, document: &str) -> Result<Option<Record>, ProvError> {
    let mut lines = document.lines();
    let head: Vec<String> = lines.next().map(cells).unwrap_or_default();
    let corrupt = |detail: &str| ProvError::Corrupt {
        detail: format!("published record {}: {detail}", key.to_hex()),
    };

    if head.first().map(String::as_str) != Some("head") {
        return Err(corrupt("no head line"));
    }
    if head.get(1).map(String::as_str) != Some(&DOCUMENT_VERSION.to_string()) {
        return Ok(None);
    }
    let [
        _,
        _,
        _,
        step,
        code,
        command,
        kind,
        bytes,
        canonical,
        engine,
        platform,
        at,
        duration,
    ] = head.as_slice()
    else {
        return Err(corrupt("a head line of the wrong width"));
    };

    let mut record = Record {
        key,
        step: step.clone(),
        code: code.clone(),
        command: some(command),
        output_kind: data_kind(kind).ok_or_else(|| corrupt("an output kind it does not know"))?,
        output_bytes: hash(bytes).ok_or_else(|| corrupt("an output hash it cannot read"))?,
        output_canonical: hash(canonical)
            .ok_or_else(|| corrupt("a canonical hash it cannot read"))?,
        output_notes: Vec::new(),
        parameters: Vec::new(),
        inputs: Vec::new(),
        references: Vec::new(),
        remotes: Vec::new(),
        tools: Vec::new(),
        engine: engine.clone(),
        platform: platform.clone(),
        recorded_at: at.parse().unwrap_or_default(),
        duration_ms: duration.parse().unwrap_or_default(),
    };

    for line in lines {
        let cells = cells(line);
        let tag = cells.first().map(String::as_str).unwrap_or_default();
        match (tag, cells.as_slice()) {
            ("note", [_, code, value]) => {
                record.output_notes.extend(Note::parse(code, value));
            }
            ("param", [_, name, value]) => {
                record.parameters.push((name.clone(), value.clone()));
            }
            ("input", [_, param, kind, path, bytes, canonical]) => {
                record.inputs.push(InputRecord {
                    param: param.clone(),
                    kind: data_kind(kind)
                        .ok_or_else(|| corrupt("an input kind it does not know"))?,
                    path: PathBuf::from(path),
                    bytes: hash(bytes).ok_or_else(|| corrupt("an input hash it cannot read"))?,
                    canonical: hash(canonical)
                        .ok_or_else(|| corrupt("an input hash it cannot read"))?,
                });
            }
            (
                "reference",
                [
                    _,
                    param,
                    path,
                    build,
                    accession,
                    contigs,
                    length,
                    checksum,
                    fingerprint,
                ],
            ) => {
                record.references.push(ReferenceRecord {
                    param: param.clone(),
                    path: PathBuf::from(path),
                    build: some(build),
                    accession: some(accession),
                    contigs: contigs.parse().unwrap_or_default(),
                    length: length.parse().unwrap_or_default(),
                    checksum: some(checksum),
                    fingerprint: fingerprint.clone(),
                });
            }
            ("remote", [_, locator, digest, bytes, at, verdict]) => {
                record.remotes.push(Origin {
                    locator: locator.clone(),
                    digest: hash(digest).ok_or_else(|| corrupt("a remote digest"))?,
                    bytes: hash(bytes).ok_or_else(|| corrupt("a remote digest"))?,
                    fetched_at: at.parse().unwrap_or_default(),
                    verdict: match verdict.as_str() {
                        "pinned" => RemoteVerdict::Pinned,
                        "replayed" => RemoteVerdict::Replayed,
                        "live" => RemoteVerdict::Live,
                        _ => return Err(corrupt("a remote verdict it does not know")),
                    },
                });
            }
            ("tool", [_, name, version]) => {
                record.tools.push((name.clone(), version.clone()));
            }
            // A line this version has no use for. Ignored rather than
            // refused, so that a newer engine publishing more does not
            // make its records unreadable here.
            _ => {}
        }
    }

    Ok(Some(record))
}

fn row(cells: &[&str]) -> String {
    cells
        .iter()
        .map(|cell| escape(cell))
        .collect::<Vec<_>>()
        .join("\t")
}

fn cells(line: &str) -> Vec<String> {
    line.split('\t').map(unescape).collect()
}

fn option(value: Option<&str>) -> String {
    value.map_or_else(|| NONE.to_owned(), |value| format!("={value}"))
}

fn some(cell: &str) -> Option<String> {
    cell.strip_prefix('=').map(ToOwned::to_owned)
}

fn data_kind(name: &str) -> Option<DataKind> {
    DataKind::ALL
        .iter()
        .copied()
        .find(|kind| kind.name() == name)
}

fn hash(hex: &str) -> Option<ContentHash> {
    ContentHash::parse(hex)
}

fn escape(cell: &str) -> String {
    cell.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn unescape(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    let mut characters = cell.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}
