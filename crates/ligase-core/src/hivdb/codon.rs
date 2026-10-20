//! From variant calls to the amino acids a protein holds.
//!
//! This is the step between "position 2750 of HXB2 is C rather than T" and
//! "reverse transcriptase codon 67 holds asparagine". Everything the
//! scoring engine reasons about comes from here, so the three judgements
//! this module makes are the ones that decide whether a resistance report
//! is honest.
//!
//! # A codon is assembled from every record that falls in it
//!
//! Real calls put two variants in one codon — HXB2 2750 and 2751 are
//! adjacent positions in the demo's own data. Translating each record on
//! its own would produce two amino acids where the virus has one.
//!
//! # A mixture is every amino acid it could be
//!
//! Where reads support both the reference and an alternate base above the
//! mixture threshold, both bases are present, and the codon translates to
//! every amino acid the combinations give. This is what an ambiguous
//! nucleotide means to Stanford's own interpreter, and the alternative —
//! taking the majority base — would discard the minority resistance that
//! deep sequencing exists to find.
//!
//! # A position with no coverage is not a wild-type position
//!
//! `bcftools call -mv` writes only the sites it called a variant at. A
//! position missing from a VCF is therefore ambiguous between "agrees with
//! the reference" and "nothing aligned here", and the two have opposite
//! clinical meanings: the first is a susceptible result, the second is no
//! result. So coverage is read from the alignment, and a codon whose bases
//! are not all covered to the required depth is left out of the reading
//! entirely — which makes it *unread* to the scorer, and makes any drug
//! whose rules mention it report a score that is a floor rather than an
//! answer.
//!
//! Where no alignment is supplied the reading says so, with a [`Note`],
//! and treats every uncalled position as reference. That is the
//! assumption a report must not make silently.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

use noodles::bam;
use noodles::sam::alignment::record::cigar::op::Kind;

use super::ruleset::Acids;
use super::score::{Observed, Protein};
use crate::load::{LoadError, open};

/// A coding region of the reference, as a nucleotide range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gene {
    /// The name the resistance algorithm knows it by, e.g. `RT`.
    pub name: &'static str,
    /// The first base of codon 1, 1-based.
    pub start: u32,
    /// The last base of the last codon, 1-based and inclusive.
    pub end: u32,
}

impl Gene {
    /// Which codon of this gene a reference position falls in, if any.
    #[must_use]
    pub const fn codon_of(&self, position: u32) -> Option<u32> {
        if position < self.start || position > self.end {
            return None;
        }
        Some((position - self.start) / 3 + 1)
    }

    /// How many codons the gene has.
    #[must_use]
    pub const fn codons(&self) -> u32 {
        (self.end - self.start + 1) / 3
    }

    /// The first base of a codon, 1-based.
    #[must_use]
    pub const fn base_of(&self, codon: u32) -> u32 {
        self.start + (codon - 1) * 3
    }
}

/// The HIV-1 coding regions this engine interprets, in HXB2 coordinates.
///
/// These are the standard HXB2 landmarks. Protease is 99 codons, reverse
/// transcriptase 560, integrase 288, and capsid (p24, inside gag) 231.
///
/// Reverse transcriptase runs the full 560 codons rather than stopping at
/// the 440 of the p51 subunit, because HIVdb numbers RT 1–560 and its
/// published mutation lists reach position 558. The highest position any
/// rule in HIVDB 9.8 or 10.2 names is RT 348, so the rules would fit in
/// the shorter window; the mutation *list* would not, and a list that
/// stopped short would differ from a published one for a reason that had
/// nothing to do with the data.
pub const GENES: [Gene; 4] = [
    Gene {
        name: "CA",
        start: 1186,
        end: 1878,
    },
    Gene {
        name: "PR",
        start: 2253,
        end: 2549,
    },
    Gene {
        name: "RT",
        start: 2550,
        end: 4229,
    },
    Gene {
        // 5093 rather than the 5096 where pol ends: the last three bases
        // are the stop codon, and integrase is 288 amino acids, which is
        // how HIVdb numbers it and how its mutation lists read.
        name: "IN",
        start: 4230,
        end: 5093,
    },
];

/// The default fraction of reads an allele must reach to count as present.
///
/// 0.15 matches the `Amb15` convention of the published results this
/// engine is validated against: an allele seen in at least fifteen per
/// cent of reads becomes part of the ambiguous consensus. It is a
/// parameter because it is a judgement, and a judgement belongs in a
/// cache key rather than in a constant nobody can see.
pub const MIXTURE: f64 = 0.15;

/// The default depth a position must reach to count as read.
pub const DEPTH: u32 = 100;

/// What was read, and what could not be.
#[derive(Clone, Debug)]
pub struct Reading {
    /// The amino acids present, ready to score.
    pub observed: Observed,
    /// The reference amino acid at every position that was read, so a
    /// report can render `M184V` rather than `184V`.
    pub reference: BTreeMap<(String, u32), char>,
    /// What the reading noticed about itself.
    ///
    /// A note never changes a result; it says what a result rests on, in
    /// the spirit of D26. These are the reading's own notes rather than
    /// `canon::Note`s: that type is stored in the cache, in provenance and
    /// in a bundle, and what is said here is about one report.
    pub notes: Vec<String>,
}

/// Read a reference and its variant calls into amino acids.
///
/// # Arguments
///
/// - `reference` — the FASTA the variants were called against.
/// - `variants` — the VCF, optionally compressed.
/// - `alignment` — the BAM the variants were called from, for coverage.
///   Without it no position can be known to be unread, and the reading
///   says so.
/// - `mixture` — the fraction an allele must reach to count as present.
/// - `depth` — the depth a position must reach to count as read.
///
/// # Errors
///
/// If any file cannot be read or does not parse as its format.
pub fn read(
    reference: &Path,
    variants: &Path,
    alignment: Option<&Path>,
    mixture: f64,
    depth: u32,
) -> Result<Reading, LoadError> {
    let (contig, sequence) = sequence(reference)?;
    let calls = calls(variants)?;
    let mut notes = Vec::new();

    let covered = match alignment {
        Some(path) => Some(coverage(path, depth)?),
        None => {
            notes.push(
                "no alignment was supplied, so coverage is unknown and every \
                 position the caller did not report is taken to be reference"
                    .to_owned(),
            );
            None
        }
    };

    let mut observed = Observed::new();
    let mut reference_acids = BTreeMap::new();

    for gene in GENES {
        if gene.end as usize > sequence.len() {
            notes.push(format!(
                "{} ends at {} of {contig}, which is {} bases long, so it was not read",
                gene.name,
                gene.end,
                sequence.len()
            ));
            continue;
        }

        let mut protein = Protein::new();
        for codon in 1..=gene.codons() {
            let first = gene.base_of(codon);
            let bases = [first, first + 1, first + 2];

            // The whole codon has to be readable. Two bases out of three
            // is not two thirds of an amino acid.
            if covered
                .as_ref()
                .is_some_and(|read| bases.iter().any(|base| !read.contains(base)))
            {
                continue;
            }

            let reference_codon: [u8; 3] = [
                sequence[bases[0] as usize - 1],
                sequence[bases[1] as usize - 1],
                sequence[bases[2] as usize - 1],
            ];
            reference_acids.insert(
                (gene.name.to_owned(), codon),
                translate(&reference_codon).unwrap_or('X'),
            );

            let mut acids = Acids::none();
            let mut deleted = false;
            let mut inserted = false;
            let mut alternatives: Vec<Vec<u8>> = reference_codon
                .iter()
                .map(|base| vec![base.to_ascii_uppercase()])
                .collect();

            for base in bases {
                let Some(called) = calls.get(&(contig.clone(), base)) else {
                    continue;
                };
                let index = (base - first) as usize;
                let mut present: Vec<u8> = Vec::new();
                for (allele, fraction) in &called.alleles {
                    if *fraction < mixture {
                        continue;
                    }
                    match allele {
                        Allele::Base(nucleotide) => present.push(*nucleotide),
                        Allele::Deletion => deleted = true,
                        Allele::Insertion => inserted = true,
                    }
                }
                if !present.is_empty() {
                    alternatives[index] = present;
                }
            }

            for first_base in &alternatives[0] {
                for second in &alternatives[1] {
                    for third in &alternatives[2] {
                        if let Some(acid) = translate(&[*first_base, *second, *third]) {
                            acids = acids.with(Acids::of(acid).unwrap_or_else(|_| Acids::none()));
                        }
                    }
                }
            }
            if deleted {
                acids = acids.with(Acids::of('d').unwrap_or_else(|_| Acids::none()));
            }
            if inserted {
                acids = acids.with(Acids::of('i').unwrap_or_else(|_| Acids::none()));
            }

            protein.saw(codon, acids);
        }

        if protein.read() == 0 {
            notes.push(format!(
                "no codon of {} reached {depth}-fold coverage, so no drug \
                 scored against it could be assessed",
                gene.name
            ));
            continue;
        }
        if protein.read() < gene.codons() as usize {
            notes.push(format!(
                "{} of {}'s {} codons reached {depth}-fold coverage; the rest \
                 are reported as unread rather than as reference",
                protein.read(),
                gene.name,
                gene.codons()
            ));
        }
        observed.with(gene.name, protein);
    }

    Ok(Reading {
        observed,
        reference: reference_acids,
        notes,
    })
}

/// An allele a caller reported at one position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Allele {
    /// A single nucleotide.
    Base(u8),
    /// Bases removed relative to the reference.
    Deletion,
    /// Bases added relative to the reference.
    Insertion,
}

/// What a caller said about one position.
#[derive(Clone, Debug, Default)]
struct Called {
    /// Each allele and the fraction of reads supporting it.
    alleles: Vec<(Allele, f64)>,
}

/// Read a FASTA's first contig: its name, and its bases uppercased.
///
/// The first contig only, because the references this interprets against
/// are viral genomes of one sequence. A FASTA with several is refused
/// rather than silently read as its first.
fn sequence(path: &Path) -> Result<(String, Vec<u8>), LoadError> {
    let mut reader = open(path)?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut reader, &mut text)
        .map_err(|error| LoadError::io(path, error))?;

    let mut name = None;
    let mut bases = Vec::new();
    for line in text.lines() {
        if let Some(header) = line.strip_prefix('>') {
            if name.is_some() {
                return Err(LoadError::format(
                    path,
                    "more than one sequence; a resistance reference is one viral genome",
                ));
            }
            name = Some(
                header
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
            );
        } else {
            bases.extend(line.trim().bytes().map(|base| base.to_ascii_uppercase()));
        }
    }

    match name {
        Some(name) if !bases.is_empty() => Ok((name, bases)),
        Some(_) => Err(LoadError::format(path, "the sequence has no bases")),
        None => Err(LoadError::format(path, "no `>` line; not a FASTA")),
    }
}

/// Read a VCF's calls, with each allele's fraction of the reads.
///
/// Records are read as text, as `crate::canon::variants` reads them and
/// for the same reason: what matters here is what the caller wrote, and
/// the caller's own text is the most faithful record of it.
fn calls(path: &Path) -> Result<BTreeMap<(String, u32), Called>, LoadError> {
    let mut reader = open(path)?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut reader, &mut text)
        .map_err(|error| LoadError::io(path, error))?;

    let mut calls: BTreeMap<(String, u32), Called> = BTreeMap::new();
    let mut sample_depths: Vec<u32> = Vec::new();

    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 5 {
            return Err(LoadError::format(
                path,
                format!("a record of {} columns, which is not a VCF", fields.len()),
            ));
        }
        let contig = fields[0].to_owned();
        let position: u32 = fields[1]
            .parse()
            .map_err(|_| LoadError::format(path, format!("position {:?}", fields[1])))?;
        let reference = fields[3].as_bytes();

        sample_depths.clear();
        if let (Some(format), Some(sample)) = (fields.get(8), fields.get(9))
            && let Some(index) = format.split(':').position(|key| key == "AD")
            && let Some(depths) = sample.split(':').nth(index)
        {
            sample_depths = depths
                .split(',')
                .map(|depth| depth.parse::<u32>().unwrap_or(0))
                .collect();
        }
        let total: u32 = sample_depths.iter().sum();
        // Without allele depths nothing can be said about proportions, so
        // every allele the caller reported counts as present. A consensus
        // caller reporting an allele at all is a strong statement.
        let fraction = |index: usize| -> f64 {
            if total == 0 {
                1.0
            } else {
                f64::from(sample_depths.get(index).copied().unwrap_or(0)) / f64::from(total)
            }
        };

        let mut alleles = Vec::new();
        if reference.len() == 1 {
            alleles.push((Allele::Base(reference[0].to_ascii_uppercase()), fraction(0)));
        }
        for (offset, alternate) in fields[4].split(',').enumerate() {
            let allele = if alternate == "<*>" || alternate == "." {
                continue;
            } else if alternate.len() < reference.len() {
                Allele::Deletion
            } else if alternate.len() > reference.len() {
                Allele::Insertion
            } else if alternate.len() == 1 {
                Allele::Base(alternate.as_bytes()[0].to_ascii_uppercase())
            } else {
                // A multi-base substitution: each base lands on its own
                // position, and the record's own position is the first.
                for (step, base) in alternate.bytes().enumerate() {
                    calls
                        .entry((contig.clone(), position + step as u32))
                        .or_default()
                        .alleles
                        .push((
                            Allele::Base(base.to_ascii_uppercase()),
                            fraction(offset + 1),
                        ));
                }
                continue;
            };
            alleles.push((allele, fraction(offset + 1)));
        }

        calls
            .entry((contig, position))
            .or_default()
            .alleles
            .extend(alleles);
    }

    Ok(calls)
}

/// Every reference position an alignment covers to at least `depth`.
fn coverage(path: &Path, depth: u32) -> Result<std::collections::BTreeSet<u32>, LoadError> {
    let file = File::open(path).map_err(|error| LoadError::io(path, error))?;
    let mut reader = bam::io::Reader::new(file);
    reader
        .read_header()
        .map_err(|error| LoadError::format(path, format!("not a readable BAM: {error}")))?;

    // One counter per base of the genes, rather than per base of the
    // genome: the regions interpreted are a few thousand bases, and a
    // dense count over them is cheaper than a map over the whole thing.
    let first = GENES.iter().map(|gene| gene.start).min().unwrap_or(1);
    let last = GENES.iter().map(|gene| gene.end).max().unwrap_or(1);
    let mut depths = vec![0_u32; (last - first + 1) as usize];

    for record in reader.records() {
        let record = record.map_err(|error| LoadError::format(path, format!("record: {error}")))?;
        let Some(Ok(start)) = record.alignment_start() else {
            continue;
        };
        let mut position = u32::try_from(usize::from(start)).unwrap_or(u32::MAX);
        for operation in record.cigar().iter() {
            let Ok(operation) = operation else { continue };
            let length = u32::try_from(operation.len()).unwrap_or(0);
            match operation.kind() {
                Kind::Match | Kind::SequenceMatch | Kind::SequenceMismatch => {
                    for base in position..position.saturating_add(length) {
                        if base >= first && base <= last {
                            depths[(base - first) as usize] += 1;
                        }
                    }
                    position = position.saturating_add(length);
                }
                Kind::Deletion | Kind::Skip => position = position.saturating_add(length),
                _ => {}
            }
        }
    }

    Ok((first..=last)
        .filter(|base| depths[(base - first) as usize] >= depth)
        .collect())
}

/// Translate one codon, or `None` if it holds a base that is not a base.
fn translate(codon: &[u8; 3]) -> Option<char> {
    // Stops are `*`, as Stanford writes them, not `X`.
    const CODE: &str = "KNKNTTTTRSRSIIMIQHQHPPPPRRRRLLLLEDEDAAAAGGGGVVVV*Y*YSSSS*CWCLFLF";
    let index = |base: u8| -> Option<usize> {
        match base {
            b'A' => Some(0),
            b'C' => Some(1),
            b'G' => Some(2),
            b'T' | b'U' => Some(3),
            _ => None,
        }
    };
    let offset = index(codon[0])? * 16 + index(codon[1])? * 4 + index(codon[2])?;
    CODE.chars().nth(offset)
}
