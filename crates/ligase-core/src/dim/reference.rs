//! Reference identity: what makes two references the same reference.
//!
//! `docs/dimensions.md`: identity is contig names plus lengths plus MD5 where
//! available, never the filename. A label such as `hg38` is for humans and
//! carries no weight — two files can both claim to be hg38 and hold different
//! sequence, which is precisely the bug this exists to catch.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use md5::{Digest as _, Md5};

use super::ContigNaming;

/// One sequence in a reference: its name, its length, and its checksum when
/// the file records or lets us compute one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contig {
    /// The name as written: `chr1`, `1`, `K03455.1`.
    pub name: String,
    /// Length in bases, when the file records one.
    ///
    /// A VCF `##contig=<ID=1>` line without a length is legal and does
    /// occur. Storing a zero there would be a lie that compares equal to
    /// another lie, so absence is modelled honestly and simply not compared.
    pub length: Option<u64>,
    /// MD5 of the uppercased, whitespace-stripped sequence, as the SAM
    /// specification defines `@SQ M5`. Absent when the file neither records
    /// one nor contains the sequence to compute it from — VCF `##contig`
    /// lines frequently omit it.
    pub md5: Option<String>,
}

/// Whether a contig list's order carries meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContigOrder {
    /// The order a sequence dictionary states: FASTA deflines, SAM `@SQ`
    /// lines, VCF `##contig` lines. GATK requires two dictionaries to agree
    /// on it, so a difference is worth reporting.
    Dictionary,
    /// The order contigs happened to appear in a file's *records* — a BED's
    /// intervals, a GFF's features. There is no dictionary here, so the
    /// order means nothing and must never be compared. Reporting it would be
    /// a false alarm, and false alarms are how a correctness tool gets
    /// switched off.
    Observed,
}

/// The identity of a reference: its contigs, plus an optional human label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceId {
    label: Option<String>,
    contigs: Vec<Contig>,
    order: ContigOrder,
}

impl ReferenceId {
    /// Build a reference identity from a sequence dictionary, in file order.
    #[must_use]
    pub fn new(label: Option<String>, contigs: Vec<Contig>) -> Self {
        Self {
            label,
            contigs,
            order: ContigOrder::Dictionary,
        }
    }

    /// Build a reference identity from contigs observed in records.
    ///
    /// Used for BED and GFF, whose contigs come from the records rather than
    /// from a header. Their order is not compared.
    #[must_use]
    pub fn from_records(label: Option<String>, contigs: Vec<Contig>) -> Self {
        Self {
            label,
            contigs,
            order: ContigOrder::Observed,
        }
    }

    /// Whether this contig list's order carries meaning.
    #[must_use]
    pub const fn order(&self) -> ContigOrder {
        self.order
    }

    /// The declared or recorded label, if any. Display only.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// The contigs, in the order the file lists them.
    #[must_use]
    pub fn contigs(&self) -> &[Contig] {
        &self.contigs
    }

    /// The naming style of the contig names.
    ///
    /// A reference with no contigs reports [`ContigNaming::Bare`], having
    /// vacuously no prefixes; the loaders never build one, reporting the
    /// reference as unknown instead.
    #[must_use]
    pub fn naming(&self) -> ContigNaming {
        let prefixed = self
            .contigs
            .iter()
            .filter(|contig| strip_chr(&contig.name).is_some())
            .count();

        if prefixed == 0 {
            ContigNaming::Bare
        } else if prefixed == self.contigs.len() {
            ContigNaming::Prefixed
        } else {
            ContigNaming::Mixed
        }
    }

    /// A short human description: the label if there is one, otherwise a
    /// summary of the contigs.
    #[must_use]
    pub fn describe(&self) -> String {
        if let Some(label) = &self.label {
            return label.clone();
        }
        match self.contigs.as_slice() {
            [only] => match only.length {
                Some(length) => format!("{} ({length} bp)", only.name),
                None => format!("{} (length not recorded)", only.name),
            },
            contigs => {
                let total: u64 = contigs.iter().filter_map(|contig| contig.length).sum();
                format!("{} contigs, {total} bp", contigs.len())
            }
        }
    }

    /// A short digest of the identity, for telling apart two references that
    /// [`Self::describe`] renders the same way.
    ///
    /// Computed over normalised contig names, so a reference fingerprints the
    /// same as its `chr`-prefixed twin: naming is a separate dimension.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        let mut hasher = Md5::new();
        for contig in &self.contigs {
            hasher.update(normalise(&contig.name).as_bytes());
            hasher.update(b"\t");
            hasher.update(
                contig
                    .length
                    .map_or_else(String::new, |n| n.to_string())
                    .as_bytes(),
            );
            hasher.update(b"\t");
            hasher.update(contig.md5.as_deref().unwrap_or("").to_ascii_lowercase());
            hasher.update(b"\n");
        }
        let digest = hasher.finalize();
        digest
            .iter()
            .take(6)
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// How two references must relate for a step to be sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    /// Two references compared with each other: same contigs, same order.
    Identical,
    /// Data checked against a reference: every contig the data mentions must
    /// appear in the reference, but the reference may hold more (decision
    /// D10).
    DataWithin,
    /// Two datasets compared with each other, where neither is the
    /// reference: shared contigs must agree, and neither contains the other.
    Compatible,
}

/// The outcome of comparing two references.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// They agree under the requested relation.
    Agree,
    /// The same contigs under different names.
    NamingDiffers {
        /// The left side's naming style.
        left: ContigNaming,
        /// The right side's naming style.
        right: ContigNaming,
        /// One pair of names that differ, for the message.
        example: (String, String),
    },
    /// The same contigs, listed in a different order.
    OrderDiffers {
        /// The left side's contig names, in order.
        left: Vec<String>,
        /// The right side's contig names, in order.
        right: Vec<String>,
    },
    /// A contig on the left is absent on the right.
    MissingContig {
        /// The contig that is missing.
        name: String,
        /// What it is missing from.
        from: String,
    },
    /// A shared contig disagrees on length or checksum.
    ContigConflict {
        /// The contig name.
        name: String,
        /// What disagrees, in words.
        detail: String,
    },
}

/// What one pass of comparison found.
///
/// [`Outcome::Disjoint`] is not a verdict and is never reported. It exists
/// because two datasets that share no contigs *under their names as written*
/// may still be the same contigs spelled differently, and a pass that agreed
/// vacuously would hide that. Having found nothing in common, it cannot
/// claim agreement either.
enum Outcome {
    /// They agree under the requested relation.
    Agree,
    /// Nothing was compared: the two sides name no contig in common.
    Disjoint,
    /// A real disagreement.
    Differs(Verdict),
}

/// Compare `left` against `right` under `relation`.
///
/// Comparison runs twice: once on the names as written, and if that fails,
/// once on normalised names. A failure that becomes a success under
/// normalisation is a naming difference, not a different reference — which is
/// what makes `chr1` against `1` a distinct error.
#[must_use]
pub fn relate(left: &ReferenceId, right: &ReferenceId, relation: Relation) -> Verdict {
    if matches!(
        compare(left, right, relation, Cow::Borrowed),
        Outcome::Agree
    ) {
        return Verdict::Agree;
    }

    match compare(left, right, relation, normalise) {
        Outcome::Agree => Verdict::NamingDiffers {
            left: left.naming(),
            right: right.naming(),
            example: naming_example(left, right),
        },
        // No contig in common even after normalising: two datasets covering
        // different regions. A chr20-only VCF and a chr21-only VCF are both
        // correct, and reporting that as a mismatch would be a false alarm.
        Outcome::Disjoint => Verdict::Agree,
        Outcome::Differs(verdict) => verdict,
    }
}

/// One pair of differing names, for a message that shows rather than tells.
fn naming_example(left: &ReferenceId, right: &ReferenceId) -> (String, String) {
    let index: HashMap<Cow<'_, str>, &Contig> = right
        .contigs
        .iter()
        .map(|contig| (normalise(&contig.name), contig))
        .collect();

    for contig in &left.contigs {
        if let Some(other) = index.get(&normalise(&contig.name))
            && other.name != contig.name
        {
            return (contig.name.clone(), other.name.clone());
        }
    }
    (String::new(), String::new())
}

fn compare<'a, F>(
    left: &'a ReferenceId,
    right: &'a ReferenceId,
    relation: Relation,
    key: F,
) -> Outcome
where
    F: Fn(&'a str) -> Cow<'a, str>,
{
    let index: HashMap<Cow<'a, str>, (usize, &Contig)> = right
        .contigs
        .iter()
        .enumerate()
        .map(|(position, contig)| (key(&contig.name), (position, contig)))
        .collect();

    let mut matched = Vec::with_capacity(left.contigs.len());
    for contig in &left.contigs {
        match index.get(&key(&contig.name)) {
            Some((position, other)) => {
                if let Some(detail) = disagreement(contig, other) {
                    return Outcome::Differs(Verdict::ContigConflict {
                        name: contig.name.clone(),
                        detail,
                    });
                }
                matched.push(*position);
            }
            None if relation == Relation::Compatible => {}
            None => {
                return Outcome::Differs(Verdict::MissingContig {
                    name: contig.name.clone(),
                    from: right.describe(),
                });
            }
        }
    }

    // Identity is symmetric: a reference holding contigs the other lacks is
    // a different reference, even though containment would allow it.
    if relation == Relation::Identical && right.contigs.len() != left.contigs.len() {
        let left_keys: Vec<Cow<'a, str>> = left
            .contigs
            .iter()
            .map(|contig| key(&contig.name))
            .collect();
        if let Some(extra) = right
            .contigs
            .iter()
            .find(|contig| !left_keys.contains(&key(&contig.name)))
        {
            return Outcome::Differs(Verdict::MissingContig {
                name: extra.name.clone(),
                from: left.describe(),
            });
        }
    }

    // Order is compared only between two sequence dictionaries. Contigs
    // observed in records have no dictionary order to disagree about.
    let ordered = left.order == ContigOrder::Dictionary && right.order == ContigOrder::Dictionary;
    if ordered && matched.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Outcome::Differs(Verdict::OrderDiffers {
            left: names(left),
            right: names(right),
        });
    }

    // Two datasets with nothing in common were not compared, so this pass
    // cannot claim they agree. Normalising the names may yet find the
    // overlap -- that is how a `chr1` against `1` mismatch between two
    // datasets, neither of which is the reference, gets caught at all.
    if relation == Relation::Compatible
        && matched.is_empty()
        && !left.contigs.is_empty()
        && !right.contigs.is_empty()
    {
        return Outcome::Disjoint;
    }

    Outcome::Agree
}

fn names(reference: &ReferenceId) -> Vec<String> {
    reference
        .contigs
        .iter()
        .map(|contig| contig.name.clone())
        .collect()
}

/// Why two same-named contigs are not the same contig, if they are not.
///
/// A checksum absent on either side is not evidence of agreement, so we
/// compare what we have and say nothing about what we do not. Pretending an
/// absent digest matches would be a lie; refusing to compare at all would
/// make the check useless on VCFs, which routinely omit it.
fn disagreement(left: &Contig, right: &Contig) -> Option<String> {
    if let (Some(a), Some(b)) = (left.length, right.length)
        && a != b
    {
        return Some(format!("length {a} against {b}"));
    }
    match (&left.md5, &right.md5) {
        (Some(a), Some(b)) if !a.eq_ignore_ascii_case(b) => Some(format!(
            "same name and length but different sequence (MD5 {} against {})",
            short(a),
            short(b)
        )),
        // One side has no checksum. We compared what we have; claiming
        // agreement about the rest would be the lie this guards against,
        // and refusing to compare at all would make VCFs uncheckable.
        _ => None,
    }
}

fn short(md5: &str) -> String {
    md5.chars().take(8).collect()
}

/// Strip a leading `chr`, case-insensitively, if there is anything after it.
fn strip_chr(name: &str) -> Option<&str> {
    let rest = name
        .get(..3)
        .filter(|head| head.eq_ignore_ascii_case("chr"))?;
    let _ = rest;
    let tail = &name[3..];
    (!tail.is_empty()).then_some(tail)
}

/// The name used for identity comparison.
///
/// A leading `chr` is dropped, because naming style is its own dimension. A
/// stripped `M` becomes `MT`, reconciling UCSC's `chrM` with Ensembl's `MT`.
/// The alias applies *only* to the prefixed form: a bare `M` is left alone,
/// so it does not silently equate to `MT`. That matters, because hg19's
/// `chrM` is the older NC_001807 at 16571 bp while GRCh37's `MT` is the rCRS
/// at 16569 bp — different sequences. Normalising the name is safe only
/// because length and MD5 are still compared.
fn normalise(name: &str) -> Cow<'_, str> {
    match strip_chr(name) {
        Some(rest) if rest.eq_ignore_ascii_case("m") => Cow::Borrowed("MT"),
        Some(rest) => Cow::Borrowed(rest),
        None => Cow::Borrowed(name),
    }
}

impl fmt::Display for ReferenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.describe())
    }
}
