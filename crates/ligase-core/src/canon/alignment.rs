//! SAM and BAM: the alignments, and which reference they are against.
//!
//! The header is kept down to the two things that are about biology rather
//! than about the file: the sequence dictionary (`@SQ`, in order — D14) and
//! the read groups' samples (`@RG`, by identifier). `@PG` goes, which is
//! the whole point: it holds the command line, and the command line holds
//! the output path, and the output path is named after the step's own key.
//! `@HD` goes with it — a version and a sort order describe the file, and
//! the file's layout is exactly what is being canonicalised away. So do
//! `@CO` comments.
//!
//! Record *tags* are kept, all of them, sorted by tag. They are results:
//! `NM`, `AS` and the rest are what the aligner concluded. Their order
//! within a record is not, so it is sorted away — and an integer is
//! rendered as a plain decimal whatever width it was stored in, so that a
//! BAM and the SAM it came from agree (D9).

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use noodles::sam::alignment::record::cigar::op::Kind;
use noodles::sam::alignment::record::data::field::Value;
use noodles::sam::alignment::record::data::field::value::Array;
use noodles::sam::header::record::value::map::read_group::tag::SAMPLE;
use noodles::sam::header::record::value::map::reference_sequence::tag::MD5_CHECKSUM;
use noodles::{bam, sam};

use super::{Digest, Note};
use crate::load::open;

/// Hash an alignment's canonical form.
pub(super) fn canonicalise(path: &Path, digest: &mut Digest) -> Result<(), String> {
    let is_bam = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bam"));

    if is_bam {
        let file = File::open(path).map_err(|error| error.to_string())?;
        let mut reader = bam::io::Reader::new(BufReader::new(file));
        let header = reader.read_header().map_err(|error| error.to_string())?;
        describe_header(&header, digest);
        for result in reader.records() {
            let record = result.map_err(|error| error.to_string())?;
            describe_record(&record, &header, digest)?;
        }
    } else {
        let mut reader = sam::io::Reader::new(open(path).map_err(|error| error.to_string())?);
        let header = reader.read_header().map_err(|error| error.to_string())?;
        describe_header(&header, digest);
        for result in reader.records() {
            let record = result.map_err(|error| error.to_string())?;
            describe_record(&record, &header, digest)?;
        }
    }

    Ok(())
}

/// The header, reduced to the reference it is against and whose reads it is.
fn describe_header(header: &sam::Header, digest: &mut Digest) {
    // In order: a sequence dictionary's order is semantic (D14).
    for (name, map) in header.reference_sequences() {
        digest.field("sq.name", name.as_ref());
        digest.text("sq.length", &map.length().get().to_string());
        digest.field(
            "sq.md5",
            map.other_fields()
                .get(&MD5_CHECKSUM)
                .map_or(&b""[..], |md5| md5.as_ref()),
        );
    }

    // By identifier: which read groups there are is semantic, the order
    // their lines happen to sit in is not.
    let mut groups: Vec<(&[u8], &[u8])> = header
        .read_groups()
        .iter()
        .map(|(id, map)| {
            let sample = map
                .other_fields()
                .get(&SAMPLE)
                .map_or(&b""[..], |sample| sample.as_ref());
            (id.as_ref(), sample)
        })
        .collect();
    groups.sort_unstable();
    for (id, sample) in groups {
        digest.field("rg.id", id);
        digest.field("rg.sample", sample);
        // Hashed either way: a read group's sample is part of what the
        // alignment is about. Noticed, because an identifier that names a
        // file is not a sample, and this is where that starts — a BAM with
        // no sample is what makes `bcftools` write a path (D26).
        if let Some(note) = Note::about_sample(&String::from_utf8_lossy(sample)) {
            digest.note(note);
        }
    }
}

/// One alignment record, as the fields that say where a read went.
fn describe_record(
    record: &impl sam::alignment::Record,
    header: &sam::Header,
    digest: &mut Digest,
) -> Result<(), String> {
    let mut parts: Vec<Vec<u8>> = Vec::with_capacity(12);
    let mut push = |value: Vec<u8>| parts.push(value);

    push(record.name().map(|name| name.to_vec()).unwrap_or_default());
    push(
        record
            .flags()
            .map_err(|error| error.to_string())?
            .bits()
            .to_string()
            .into_bytes(),
    );
    push(number(
        record
            .reference_sequence_id(header)
            .transpose()
            .map_err(|error| error.to_string())?,
    ));
    push(number(
        record
            .alignment_start()
            .transpose()
            .map_err(|error| error.to_string())?
            .map(usize::from),
    ));
    push(number(
        record
            .mapping_quality()
            .transpose()
            .map_err(|error| error.to_string())?
            .map(|quality| usize::from(u8::from(quality))),
    ));

    let mut cigar = Vec::new();
    for operation in record.cigar().iter() {
        let operation = operation.map_err(|error| error.to_string())?;
        cigar.extend_from_slice(operation.len().to_string().as_bytes());
        cigar.push(letter(operation.kind()));
    }
    push(cigar);

    push(number(
        record
            .mate_reference_sequence_id(header)
            .transpose()
            .map_err(|error| error.to_string())?,
    ));
    push(number(
        record
            .mate_alignment_start()
            .transpose()
            .map_err(|error| error.to_string())?
            .map(usize::from),
    ));
    push(
        record
            .template_length()
            .map_err(|error| error.to_string())?
            .to_string()
            .into_bytes(),
    );
    push(record.sequence().iter().collect());

    let mut qualities = Vec::new();
    for score in record.quality_scores().iter() {
        qualities.push(score.map_err(|error| error.to_string())?);
    }
    push(qualities);

    // Sorted by tag: which tags a record carries is a result, the order
    // they were written in is not.
    let mut tags: Vec<Vec<u8>> = Vec::new();
    for field in record.data().iter() {
        let (tag, value) = field.map_err(|error| error.to_string())?;
        let mut rendered = tag.as_ref().to_vec();
        rendered.push(b':');
        rendered.extend_from_slice(render(&value).as_bytes());
        tags.push(rendered);
    }
    tags.sort_unstable();
    for tag in tags {
        push(tag);
    }

    let borrowed: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
    digest.record(&borrowed);
    Ok(())
}

/// A number that may be absent, as text, so that absent is not zero.
fn number(value: Option<usize>) -> Vec<u8> {
    value.map_or_else(|| b"*".to_vec(), |value| value.to_string().into_bytes())
}

/// A CIGAR operation's letter, as the SAM specification writes it.
const fn letter(kind: Kind) -> u8 {
    match kind {
        Kind::Match => b'M',
        Kind::Insertion => b'I',
        Kind::Deletion => b'D',
        Kind::Skip => b'N',
        Kind::SoftClip => b'S',
        Kind::HardClip => b'H',
        Kind::Pad => b'P',
        Kind::SequenceMatch => b'=',
        Kind::SequenceMismatch => b'X',
    }
}

/// A tag's value, with an integer's storage width normalised away.
///
/// SAM writes `NM:i:3`; a BAM may store the same 3 in a single byte. They
/// are the same number, and decision D9 says re-encoding is not a change.
fn render(value: &Value<'_>) -> String {
    match value {
        Value::Character(byte) => format!("A{}", char::from(*byte)),
        Value::Int8(_)
        | Value::UInt8(_)
        | Value::Int16(_)
        | Value::UInt16(_)
        | Value::Int32(_)
        | Value::UInt32(_) => value
            .as_int()
            .map_or_else(|| "i?".to_owned(), |number| format!("i{number}")),
        Value::Float(number) => format!("f{number}"),
        Value::String(text) => format!("Z{text}"),
        Value::Hex(text) => format!("H{text}"),
        Value::Array(array) => format!("B{}", array_values(array)),
    }
}

/// An array tag's values, as text.
///
/// An array whose values cannot be read renders as a marker rather than as
/// nothing: an unreadable array must not look like an empty one.
fn array_values(array: &Array<'_>) -> String {
    fn join<T: ToString>(values: impl Iterator<Item = std::io::Result<T>>) -> String {
        let mut text = String::new();
        for value in values {
            text.push(',');
            match value {
                Ok(value) => text.push_str(&value.to_string()),
                Err(_) => text.push('?'),
            }
        }
        text
    }

    match array {
        Array::Int8(values) => format!("c{}", join(values.iter())),
        Array::UInt8(values) => format!("C{}", join(values.iter())),
        Array::Int16(values) => format!("s{}", join(values.iter())),
        Array::UInt16(values) => format!("S{}", join(values.iter())),
        Array::Int32(values) => format!("i{}", join(values.iter())),
        Array::UInt32(values) => format!("I{}", join(values.iter())),
        Array::Float(values) => format!("f{}", join(values.iter())),
    }
}
