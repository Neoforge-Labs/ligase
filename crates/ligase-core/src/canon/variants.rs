//! VCF: the calls, and which reference and samples they are about.
//!
//! The volatile half of a VCF header is where this milestone started.
//! `bcftools` 1.24 writes, verbatim:
//!
//! ```text
//! ##bcftools_callCommand=call -mv --ploidy 1 -Oz -o /tmp/x.vcf.gz; Date=Sat Oct 17 21:14:12 2026
//! ```
//!
//! A command line, an output path and a timestamp, on a file whose records
//! are identical between runs. `##fileDate`, `##source` and every
//! `*_Version`/`*_Command` key a tool writes go the same way.
//!
//! `##reference` goes too, and that one is worth stating: it holds a path,
//! and `docs/dimensions.md` is explicit that reference identity never rests
//! on a filename. `crate::load::variants` already refuses to read identity
//! from it; the `##contig` lines carry it, and they are kept in order (D14).
//!
//! # Why the records are read as text
//!
//! `noodles` reads and validates the header — which is where format
//! compliance matters and where every one of the decisions above lives — and
//! then the records are canonicalised as text: tab-split, validated, with
//! `QUAL` quantised and the `INFO` and `FILTER` keys sorted.
//!
//! The alternative was to rebuild each record from the typed accessors.
//! That would mean choosing a rendering for every `INFO` value type, and a
//! rendering that differs from the tool's own is a chance to call two
//! identical results different. What is being compared here is two outputs
//! of the same tool, and for that the tool's own text is the most faithful
//! record of what it concluded. What *is* normalised is the part that is
//! genuinely incidental: `DP=10;AF=0.5` and `AF=0.5;DP=10` are one set of
//! annotations written two ways, so the keys are sorted. `ALT` is left
//! alone, because a genotype's `1` means the first alternate allele and
//! sorting them would renumber every call in the file.

use std::io::BufRead;
use std::path::Path;

use noodles::vcf;
use noodles::vcf::header::record::value::Collection;
use noodles::vcf::header::record::value::map::format::Number as FormatNumber;
use noodles::vcf::header::record::value::map::info::Number as InfoNumber;

use super::{Digest, Note, tolerance};
use crate::load::open;
use crate::types::DataKind;

/// Header keys that say when or how a file was made rather than what is in
/// it.
///
/// Matched in full. Tool command lines are matched by suffix instead, since
/// every one of them is named after its subcommand —
/// `bcftools_callCommand`, `bcftools_viewCommand`, `GATKCommandLine`.
const VOLATILE: [&str; 5] = [
    "fileDate",
    "fileDay",
    "source",
    "reference",
    "bcftoolsVersion",
];

/// Suffixes of header keys holding a command line or a tool version.
const VOLATILE_SUFFIXES: [&str; 4] = ["Command", "CommandLine", "Version", "Date"];

/// The number of fixed columns a VCF record has before `FORMAT`.
const FIXED_COLUMNS: usize = 8;

/// Hash variant calls' canonical form.
pub(super) fn canonicalise(path: &Path, digest: &mut Digest) -> Result<(), String> {
    let mut reader = vcf::io::Reader::new(open(path).map_err(|error| error.to_string())?);
    let header = reader.read_header().map_err(|error| error.to_string())?;
    describe_header(&header, digest);

    // The reader has consumed the header, so what is left is the records.
    let mut body = reader.into_inner();
    let mut line = String::new();
    loop {
        line.clear();
        if body
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        let text = line.trim_end_matches(['\n', '\r']);
        if text.is_empty() {
            continue;
        }
        describe_record(text, digest)?;
    }

    Ok(())
}

/// The header, reduced to what the file is about.
fn describe_header(header: &vcf::Header, digest: &mut Digest) {
    // In order: these are a sequence dictionary (D14).
    for (name, map) in header.contigs() {
        digest.text("contig", name);
        digest.text(
            "contig.length",
            &map.length()
                .map(|length| length.to_string())
                .unwrap_or_default(),
        );
        digest.text("contig.md5", map.md5().unwrap_or_default());
    }

    // Which fields a record can carry, and how they are shaped. The
    // description is prose: a bcftools release that rewords it has not
    // changed a single call.
    for (id, map) in header.infos() {
        digest.text("info", id);
        digest.text("info.number", &info_number(map.number()));
        digest.text("info.type", map.ty().as_ref());
    }
    for (id, map) in header.formats() {
        digest.text("format", id);
        digest.text("format.number", &format_number(map.number()));
        digest.text("format.type", map.ty().as_ref());
    }
    for id in header.filters().keys() {
        digest.text("filter", id);
    }
    for id in header.alternative_alleles().keys() {
        digest.text("alt", id);
    }

    // In order: the columns of every record are in this order.
    //
    // Hashed as the file writes them — a sample set is a dimension and
    // cannot be dropped the way `@PG` was (D22) — and noticed when what is
    // written is a file rather than a sample (D26).
    for sample in header.sample_names() {
        digest.text("sample", sample);
        if let Some(note) = Note::about_sample(sample) {
            digest.note(note);
        }
    }

    // Whatever else the header says, minus the keys that say when it was
    // written. Sorted: these lines' order is the order the tools that wrote
    // them happened to run.
    let mut others: Vec<String> = header
        .other_records()
        .iter()
        .filter(|(key, _)| !is_volatile(key.as_ref()))
        .map(|(key, collection)| format!("{}={}", key.as_ref(), render(collection)))
        .collect();
    others.sort();
    for other in others {
        digest.text("other", &other);
    }
}

/// How many values a field holds, as the VCF specification writes it.
///
/// Spelled out rather than taken from a derived `Debug`, so that a rename
/// inside `noodles` cannot quietly change what every key in every cache is
/// made of.
fn info_number(number: InfoNumber) -> String {
    match number {
        InfoNumber::Count(count) => count.to_string(),
        InfoNumber::AlternateBases => "A".to_owned(),
        InfoNumber::ReferenceAlternateBases => "R".to_owned(),
        InfoNumber::Samples => "G".to_owned(),
        InfoNumber::Unknown => ".".to_owned(),
    }
}

/// How many values a sample field holds, as the specification writes it.
///
/// VCF 4.5 added the local-allele and ploidy codes, so there are more of
/// them here than for `INFO`.
fn format_number(number: FormatNumber) -> String {
    match number {
        FormatNumber::Count(count) => count.to_string(),
        FormatNumber::AlternateBases => "A".to_owned(),
        FormatNumber::ReferenceAlternateBases => "R".to_owned(),
        FormatNumber::Samples => "G".to_owned(),
        FormatNumber::LocalAlternateBases => "LA".to_owned(),
        FormatNumber::LocalReferenceAlternateBases => "LR".to_owned(),
        FormatNumber::LocalSamples => "LG".to_owned(),
        FormatNumber::Ploidy => "P".to_owned(),
        FormatNumber::BaseModifications => "M".to_owned(),
        FormatNumber::Unknown => ".".to_owned(),
    }
}

/// Everything a nonstandard header key holds, as text.
fn render(collection: &Collection) -> String {
    match collection {
        Collection::Unstructured(values) => values.join(","),
        Collection::Structured(maps) => maps
            .iter()
            .map(|(id, map)| {
                let fields: Vec<String> = map
                    .other_fields()
                    .iter()
                    .map(|(tag, value)| format!("{tag}={value}"))
                    .collect();
                format!("{id}<{}>", fields.join(","))
            })
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// Whether a header key records when or how the file was made.
fn is_volatile(key: &str) -> bool {
    VOLATILE.contains(&key)
        || VOLATILE_SUFFIXES
            .iter()
            .any(|suffix| key.len() > suffix.len() && key.ends_with(suffix))
}

/// One record, as the call it states.
fn describe_record(line: &str, digest: &mut Digest) -> Result<(), String> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < FIXED_COLUMNS {
        return Err(format!(
            "a record with {} columns, not {FIXED_COLUMNS} or more",
            fields.len()
        ));
    }
    if fields[1].parse::<u64>().is_err() {
        return Err(format!("a POS that is not a number: {}", fields[1]));
    }

    let mut parts: Vec<String> = Vec::with_capacity(fields.len());
    parts.push(fields[0].to_owned()); // CHROM
    parts.push(fields[1].to_owned()); // POS
    parts.push(fields[2].to_owned()); // ID
    parts.push(fields[3].to_owned()); // REF
    // ALT keeps its order: a genotype's `1` is the first alternate allele.
    parts.push(fields[4].to_owned());
    parts.push(quality(fields[5]));
    parts.push(sorted_keys(fields[6], ';')); // FILTER
    parts.push(sorted_keys(fields[7], ';')); // INFO
    // FORMAT and the sample columns line up with each other, so neither is
    // touched.
    for field in &fields[FIXED_COLUMNS..] {
        parts.push((*field).to_owned());
    }

    let borrowed: Vec<&[u8]> = parts.iter().map(|part| part.as_bytes()).collect();
    digest.record(&borrowed);
    Ok(())
}

/// `QUAL`, in the bucket its declared tolerance puts it in.
///
/// A quality that is not a number — `.`, for a caller that did not say — is
/// kept as it is written. There is nothing to quantise and nothing is
/// gained by pretending there is.
fn quality(field: &str) -> String {
    let Some(width) = tolerance(DataKind::Variants, "QUAL") else {
        return field.to_owned();
    };
    match field
        .parse::<f64>()
        .ok()
        .and_then(|value| width.bucket(value))
    {
        Some(bucket) => format!("~{bucket}"),
        None => field.to_owned(),
    }
}

/// A `;`-separated list, sorted, so the order it was written in is gone.
fn sorted_keys(field: &str, separator: char) -> String {
    let mut parts: Vec<&str> = field.split(separator).collect();
    parts.sort_unstable();
    parts.join(&separator.to_string())
}
