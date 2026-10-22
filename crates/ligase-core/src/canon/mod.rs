//! Canonical forms: what a file means, as against which bytes it holds.
//!
//! A step's cache key is built from its inputs' *content*
//! ([`crate::key`]), and until now content meant bytes. Bytes are too
//! strict a test for biology. Every tool in the reference pipeline records
//! its own command line in its output — `minimap2` in `@PG CL`, `bcftools`
//! in `##bcftools_callCommand`, the latter with a timestamp beside it — so
//! two runs that computed the same alignments disagree on their bytes.
//!
//! That is not a cosmetic problem. The command line a tool records includes
//! its output path, and a step's output path is the cache's staging file,
//! which is named after the step's own key. So a step's bytes depend on its
//! own key: change anything about a step at all, even a comment, and every
//! step below it reruns however identical the science. Early cutoff
//! ([`crate::exec`]) could not reach past the first real tool.
//!
//! So an output is canonicalised before it is hashed (D4): records sorted,
//! volatile headers stripped, and what remains is the thing two runs are
//! compared on.
//!
//! # The canonical form belongs to the type
//!
//! A BAM, a VCF and a FASTA each have their own notion of what they mean
//! and their own volatile fields, and the file knows which it is from the
//! [`DataKind`] the graph gives it — never from the step that produced it.
//! Two steps writing the same kind are canonicalised the same way, and a
//! kind's rules are stated in one place.
//!
//! # Which formats sort, and which keep their order
//!
//! This is decided by
//! [D14](https://ligase.neoforgelabs.tech/project/decisions/), which already
//! drew the line for the dimension checker: a *sequence dictionary's* contig
//! order is semantic, because GATK requires two of them to agree on it, while
//! a BED's record order is "whatever order the intervals were sorted in". So
//! a reference's contigs and a SAM's `@SQ` lines keep their order, and
//! alignment records, variant records, intervals, reads and report rows are
//! sorted. Sorting is over each record's digest rather than its text: 32
//! bytes a record instead of a few hundred, and a sorted list of digests is
//! still a claim about the whole multiset, duplicates included.
//!
//! # What a file that will not parse hashes as
//!
//! Its bytes, tagged as such ([`Form::Raw`]). That is the conservative
//! direction — a stricter test can cost time but cannot produce a wrong
//! answer — and it is what keeps the engine working on a file that is not
//! what its name claims. The two are domain-separated, so a raw fallback
//! can never collide with a canonical form.
//!
//! Every parser here is therefore deliberately **strict**: a canonicaliser
//! that read what it could and ignored the rest would be the dangerous kind
//! of lenient, because then two files that differ in the part it skipped
//! would agree.
//!
//! # What a canonical form notices but cannot say
//!
//! Some fields cannot be dropped and are still wrong. A sample set is a
//! dimension, so a VCF's sample names and a BAM's read-group samples are
//! part of what the file is about and are hashed as the file writes them —
//! and a tool handed a BAM with no `@RG SM` writes the input file's *path*
//! there, which makes that file's canonical hash depend on where the cache
//! lives. Neither dropping the field nor repairing it is safe, so the hash
//! stands and a [`Note`] is carried beside it. A note is not an error,
//! never changes a hash, and travels with the result into the run report,
//! provenance, the bundle and the methods section (D26).

mod alignment;
mod entity_set;
mod fasta;
mod intervals;
mod reads;
mod report;
mod variants;

use std::fmt;
use std::io;
use std::path::Path;

use crate::hash::ContentHash;
use crate::types::DataKind;

pub use crate::note::Note;

/// The version of the canonicalisation rules.
///
/// Bumped whenever a rule changes, which retires every canonical hash
/// written under the old ones. A canonical hash is a fact about a file
/// *given these rules*, so one computed under others is a fact about
/// something else — the same reasoning that makes [`crate::cache`] discard
/// rather than migrate (D19).
pub const CANON_VERSION: u32 = 1;

/// Separates this hash's domain from every other BLAKE3 hash in the engine.
const DOMAIN: &str = "ligase.canonical";

/// How a file's semantic hash was arrived at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// The file parsed as its kind, and its canonical form was hashed.
    Canonical,
    /// The file did not parse as its kind, so its bytes stood in.
    Raw,
}

impl Form {
    /// The form this name stands for.
    ///
    /// The inverse of [`Display`](fmt::Display), for reading back a form
    /// the cache wrote down. `None` for a name this version does not know.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "canonical" => Some(Self::Canonical),
            "raw" => Some(Self::Raw),
            _ => None,
        }
    }
}

impl fmt::Display for Form {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Canonical => "canonical",
            Self::Raw => "raw",
        })
    }
}

/// A file's semantic hash, how it was arrived at, and what was noticed on
/// the way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canonical {
    /// The hash of the canonical form, or of the bytes where there was none.
    pub hash: ContentHash,
    /// Which of those it is.
    pub form: Form,
    /// What the canonical form noticed and the hash cannot say ([`Note`]).
    ///
    /// Empty for almost every file. A note is not an error and never
    /// changes a hash: it is carried beside one so that the run report, the
    /// bundle and the methods section can all say the same thing about the
    /// same file.
    pub notes: Vec<Note>,
}

/// The semantic hash of a file read as `kind`.
///
/// `raw` is the hash of the file's bytes, which the caller already knows
/// from the cache's prefilter; it is what a file that does not parse falls
/// back to, so this never reads a file twice.
///
/// # Errors
///
/// If the file cannot be opened or read at all. A file that reads but does
/// not *parse* is not an error: it comes back as [`Form::Raw`].
pub fn hash(kind: DataKind, path: &Path, raw: ContentHash) -> io::Result<Canonical> {
    let mut digest = Digest::new(kind, Form::Canonical);

    match canonicalise(kind, path, &mut digest) {
        Ok(()) => {
            let notes = std::mem::take(&mut digest.notes);
            Ok(Canonical {
                hash: digest.finish(),
                form: Form::Canonical,
                notes,
            })
        }
        Err(_) => {
            // The bytes stand in, tagged so they cannot be confused for a
            // canonical form of anything. Whatever the partial parse
            // noticed is discarded with it: a note about a file that did
            // not parse would be a remark about the half that was read.
            let mut fallback = Digest::new(kind, Form::Raw);
            fallback.field("bytes", raw.as_bytes());
            Ok(Canonical {
                hash: fallback.finish(),
                form: Form::Raw,
                notes: Vec::new(),
            })
        }
    }
}

fn canonicalise(kind: DataKind, path: &Path, digest: &mut Digest) -> Result<(), String> {
    match kind {
        DataKind::Reads => reads::canonicalise(path, digest),
        DataKind::Reference => fasta::canonicalise(path, digest),
        DataKind::Alignment => alignment::canonicalise(path, digest),
        DataKind::Variants => variants::canonicalise(path, digest),
        DataKind::Intervals => intervals::canonicalise(path, digest),
        DataKind::ResistanceReport => report::canonicalise(path, digest),
        DataKind::EntitySet => entity_set::canonicalise(path, digest),
    }
}

/// What a kind's canonical form keeps and drops, in one line, for a user who
/// wants to know why two files agreed.
///
/// `None` would mean a kind with no canonical form at all. There is none:
/// a kind added later has to be given one deliberately rather than falling
/// back to its bytes because nobody decided.
#[must_use]
pub const fn describe(kind: DataKind) -> Option<&'static str> {
    Some(match kind {
        DataKind::Reads => {
            "reads sorted; the description after a read's name, and the `+` line's repeat of it, \
             dropped"
        }
        DataKind::Reference => {
            "contigs in order, sequences uppercased and unwrapped; descriptions dropped"
        }
        DataKind::Alignment => {
            "records sorted; `@SQ` in order, `@RG` by sample; `@HD`, `@PG` and `@CO` dropped"
        }
        DataKind::Variants => {
            "records sorted, QUAL to a tolerance, INFO and FILTER keys sorted; dates, sources, \
             `##reference` paths and tool command lines dropped"
        }
        DataKind::Intervals => "intervals sorted; `track`, `browser` and comment lines dropped",
        DataKind::ResistanceReport => "the header row kept, the rows below it sorted",
        DataKind::EntitySet => "the header row kept, the rows below it sorted by identifier",
    })
}

// ---------------------------------------------------------- tolerance --

/// How close two numbers have to be to count as the same number.
///
/// `CLAUDE.md` allows a numeric type to declare one. The only way a hash
/// can express it is to **quantise**: put the value in a bucket of this
/// width and hash the bucket. "Within a tolerance" is not transitive — `a`
/// near `b` and `b` near `c` does not make `a` near `c` — so it is not an
/// equality, and a hash can only ever express an equality. The honest
/// consequence is that two values closer together than the tolerance can
/// still straddle a bucket edge and hash differently.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerance(f64);

impl Tolerance {
    /// A tolerance of `width`, which must be a positive, finite number.
    ///
    /// `None` otherwise: zero would divide, a negative width would invert
    /// the ordering, and neither is a tolerance anyone meant.
    #[must_use]
    pub fn new(width: f64) -> Option<Self> {
        (width.is_finite() && width > 0.0).then_some(Self(width))
    }

    /// Which bucket a value falls in.
    ///
    /// A value too large to bucket keeps its own identity rather than being
    /// folded in with every other large value.
    #[must_use]
    pub fn bucket(self, value: f64) -> Option<i64> {
        let scaled = (value / self.0).round();
        (scaled.is_finite() && scaled.abs() < 9.0e18).then_some(scaled as i64)
    }

    /// The width, for a message that has to state it.
    #[must_use]
    pub const fn width(self) -> f64 {
        self.0
    }
}

/// The tolerance a field of this kind is compared with, if it declares one.
///
/// Exactness is the default: a tolerance nobody has measured would be a
/// guess baked into every cache key, and the project's bias is the other way
/// — a cache that is too cold costs time, one that is too warm is wrong.
///
/// `QUAL` declares one because it is the field with the motive: a
/// Phred-scaled floating-point accumulation whose last digits can move with
/// the order the terms were added in. A tenth of a Phred unit is two orders
/// of magnitude below the smallest threshold anyone filters on, so it cannot
/// merge two qualities a user would treat differently.
#[must_use]
pub fn tolerance(kind: DataKind, field: &str) -> Option<Tolerance> {
    match (kind, field) {
        (DataKind::Variants, "QUAL") => Tolerance::new(0.1),
        _ => None,
    }
}

// ------------------------------------------------------------- digest --

/// The canonical form, as it is built.
///
/// Fields are written in the order they are given; records are collected and
/// sorted at the end. Both are length-prefixed, so no two different forms
/// can produce one byte stream.
pub(crate) struct Digest {
    hasher: blake3::Hasher,
    records: Vec<[u8; 32]>,
    notes: Vec<Note>,
}

impl Digest {
    fn new(kind: DataKind, form: Form) -> Self {
        let mut digest = Self {
            hasher: blake3::Hasher::new(),
            records: Vec::new(),
            notes: Vec::new(),
        };
        digest.field("domain", DOMAIN.as_bytes());
        digest.field("canon-version", &CANON_VERSION.to_le_bytes());
        // The kind is hashed, so the same bytes read as two kinds are two
        // different claims rather than one.
        digest.field("kind", kind.name().as_bytes());
        digest.field("form", form.to_string().as_bytes());
        digest
    }

    /// One field of the canonical form, in the order it was written.
    pub(crate) fn field(&mut self, label: &str, value: &[u8]) {
        write(&mut self.hasher, label.as_bytes());
        write(&mut self.hasher, value);
    }

    /// One field whose value is text.
    pub(crate) fn text(&mut self, label: &str, value: &str) {
        self.field(label, value.as_bytes());
    }

    /// Something noticed about the file, which the hash is not told.
    ///
    /// Repeats are dropped: a BAM whose four read groups all name the same
    /// file has one thing wrong with it, not four.
    pub(crate) fn note(&mut self, note: Note) {
        if !self.notes.contains(&note) {
            self.notes.push(note);
        }
    }

    /// One record, whose position in the file says nothing.
    ///
    /// Its parts are hashed to 32 bytes now and sorted with the others at
    /// the end, which is what makes the order it arrived in invisible.
    pub(crate) fn record(&mut self, parts: &[&[u8]]) {
        let mut hasher = blake3::Hasher::new();
        for part in parts {
            write(&mut hasher, part);
        }
        self.records.push(*hasher.finalize().as_bytes());
    }

    /// The canonical hash.
    fn finish(mut self) -> ContentHash {
        self.records.sort_unstable();

        write(&mut self.hasher, b"records");
        write(&mut self.hasher, &(self.records.len() as u64).to_le_bytes());
        for record in &self.records {
            self.hasher.update(record);
        }

        ContentHash::from_bytes(*self.hasher.finalize().as_bytes())
    }
}

/// One length-prefixed field.
fn write(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn the_order_records_arrive_in_does_not_change_the_hash() {
        let mut forwards = Digest::new(DataKind::Alignment, Form::Canonical);
        forwards.record(&[b"one"]);
        forwards.record(&[b"two"]);

        let mut backwards = Digest::new(DataKind::Alignment, Form::Canonical);
        backwards.record(&[b"two"]);
        backwards.record(&[b"one"]);

        assert_eq!(forwards.finish(), backwards.finish());
    }

    #[test]
    fn the_order_fields_arrive_in_does_change_the_hash() {
        let mut forwards = Digest::new(DataKind::Reference, Form::Canonical);
        forwards.text("contig", "1");
        forwards.text("contig", "2");

        let mut backwards = Digest::new(DataKind::Reference, Form::Canonical);
        backwards.text("contig", "2");
        backwards.text("contig", "1");

        assert_ne!(forwards.finish(), backwards.finish());
    }

    #[test]
    fn a_repeated_record_is_not_the_same_as_one_record() {
        let mut once = Digest::new(DataKind::Alignment, Form::Canonical);
        once.record(&[b"one"]);

        let mut twice = Digest::new(DataKind::Alignment, Form::Canonical);
        twice.record(&[b"one"]);
        twice.record(&[b"one"]);

        assert_ne!(once.finish(), twice.finish());
    }

    #[test]
    fn a_field_cannot_be_confused_with_the_one_beside_it() {
        // Length prefixing: `x` = `yz` must not equal `xy` = `z`.
        let mut left = Digest::new(DataKind::Reference, Form::Canonical);
        left.text("x", "yz");

        let mut right = Digest::new(DataKind::Reference, Form::Canonical);
        right.text("xy", "z");

        assert_ne!(left.finish(), right.finish());
    }

    #[test]
    fn a_form_round_trips_through_its_name() {
        for form in [Form::Canonical, Form::Raw] {
            assert_eq!(Form::parse(&form.to_string()), Some(form));
        }
        assert_eq!(Form::parse("something-else"), None);
    }

    #[test]
    fn a_tolerance_buckets_by_its_width() {
        let whole = Tolerance::new(1.0).unwrap();
        assert_eq!(whole.bucket(10.4), Some(10));
        assert_eq!(whole.bucket(10.6), Some(11));
        assert_eq!(whole.bucket(-10.4), Some(-10));
    }

    #[test]
    fn a_value_too_large_to_bucket_is_left_alone() {
        let tenth = Tolerance::new(0.1).unwrap();
        assert_eq!(tenth.bucket(f64::MAX), None);
        assert_eq!(tenth.bucket(f64::NAN), None);
    }
}
