//! From variant calls to the amino acids a protein holds.
//!
//! This is the step between "position 2750 of HXB2 is C rather than T" and
//! "reverse transcriptase codon 67 holds asparagine". Everything the
//! scoring engine reasons about comes from here, so the four judgements
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
//! every amino acid those bases can give. This is what an ambiguous
//! nucleotide means to Stanford's own interpreter, and the alternative —
//! taking the majority base — would discard the minority resistance that
//! deep sequencing exists to find.
//!
//! # A codon is only the combinations the reads carry
//!
//! A VCF says what is at each position, not what travels together. Where
//! two bases of one codon are each ambiguous, the combinations of them
//! include codons no read carries, and translating those invents amino
//! acids the virus does not hold.
//!
//! So the alignment settles it. Reads spanning all three bases of a codon
//! are counted as the triplets they actually carry, and the codon holds
//! the amino acids whose share of those reads reaches the mixture
//! threshold — the amino acid's share rather than each triplet's, because
//! two synonymous codons each below the threshold can spell one amino acid
//! well above it.
//!
//! Phasing narrows a reading and never widens one: which alleles are
//! *present* at a position is still the caller's own quality-aware allele
//! depths, and a codon no read spans keeps its per-base reading and says
//! so (D31).
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
//!
//! # A gene barely read cannot be called susceptible
//!
//! Scoring survives a few unread codons honestly: an unread position makes
//! every rule that names it undetermined, so the score it would have
//! contributed is absent, the score is reported as a floor and the unread
//! positions are listed. What that does not survive is a gene that was
//! mostly *not* seen. Nothing there refutes resistance and nothing there
//! shows it, so every drug comes out at zero, which the algorithm's own
//! ranges read as susceptible — a clean clinical negative from a gene the
//! run never covered. SAWA found this in the field: a sequence with 32% of
//! integrase reported all five integrase inhibitors susceptible, where
//! Stanford's own service declines to report the gene at all.
//!
//! So a gene read below `gene_coverage` of its codons is left out of the
//! reading, which makes its drugs *not assessed* — the same state as a gene
//! the pipeline never supplied, which is what it effectively is.
//!
//! The share is counted over the codons the algorithm *scores*, not over
//! the gene. A position no rule names cannot change an answer however it
//! was read, so reading it is not evidence that the gene was read well
//! enough to score — and counting it made the default close to
//! unsatisfiable for the records clinical labs actually hold. HIVDB 9.8
//! scores 40 of reverse transcriptase's 560 codons and the highest is 348,
//! so a partial-pol genotype that read every position the rules consult
//! still covered 62% of the gene and was refused. Counted over the scored
//! codons the same reading passes, and the gene it is really about — 32% of
//! integrase, five of the twenty-six positions any rule names — is still
//! refused.
//!
//! # The windows mean nothing without the frame they are stated in
//!
//! [`GENES`] holds positions in HXB2 K03455 counted from the first base of
//! the genome. A reference too short to hold them cannot be the frame they
//! are stated in, and reading it anyway puts a different gene under each
//! window; [`super::frame`] has the whole argument and the rest of the
//! check, which happens before any compute.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::Path;

use noodles::bam;
use noodles::sam::alignment::record::cigar::op::Kind;

use super::frame;
use super::ruleset::{Acids, Ruleset};
use super::score::{Observed, Protein};
use crate::load::{LoadError, open};
use crate::note::Note;

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

/// The default share of a gene's *scored* codons that must be read for the
/// gene to be assessed at all.
///
/// A policy rather than a fact, which is why it is a parameter: how much of
/// a gene a surveillance programme is willing to call susceptible on is its
/// own decision, and a judgement belongs in a cache key rather than in a
/// constant nobody can see. Four fifths is the default because the failure
/// it guards against is a *susceptible* call made from a gene that was
/// largely not sequenced, and a gene missing more than a fifth of the
/// positions its drugs are scored on cannot support one.
///
/// The denominator is [`Ruleset::scored`] rather than [`Gene::codons`]
/// because a position no rule names cannot change an answer, so reading it
/// says nothing about whether the gene was read well enough to score.
/// Counted over the gene, four fifths asked for 448 of reverse
/// transcriptase's 560 codons while the highest position HIVDB 9.8 scores
/// is 348 — a floor no partial-pol record could clear, whatever it covered
/// of what the rules look at.
///
/// It is still blunt. The exact question is whether what the unread
/// positions could have contributed would change the *level*, which the
/// report already gestures at per drug by marking the score as a floor and
/// listing what was unread; turning that into a refusal to state a level is
/// D34's deferred half.
pub const GENE_COVERAGE: f64 = 0.8;

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
    /// A note never changes a result; it says what a result rests on
    /// (D26). The step that made the reading hands these back to the
    /// engine, which carries them into the run report, provenance, a
    /// bundle and the methods section — because nothing in the report they
    /// are about could be read to find them out (D32).
    pub notes: Vec<Note>,
}

/// Read a reference and its variant calls into amino acids.
///
/// # Arguments
///
/// - `reference` — the FASTA the variants were called against.
/// - `variants` — the VCF, optionally compressed.
/// - `alignment` — the BAM the variants were called from, for coverage
///   and for phasing. Without it no position can be known to be unread
///   and no codon can be read as the triplets its reads carry, and the
///   reading says so.
/// - `ruleset` — the algorithm the reading will be scored against, which
///   is what says which of a gene's codons are scored at all. The reading
///   does no scoring; it needs the positions to count `gene_coverage`
///   over.
/// - `mixture` — the fraction an allele must reach to count as present.
/// - `depth` — the depth a position must reach to count as read.
/// - `gene_coverage` — the share of a gene's *scored* codons that must be
///   read for the gene to be assessed rather than left out.
///
/// # Errors
///
/// If any file cannot be read or does not parse as its format, or if the
/// reference cannot be the frame [`GENES`] is stated in.
pub fn read(
    reference: &Path,
    variants: &Path,
    alignment: Option<&Path>,
    ruleset: &Ruleset,
    mixture: f64,
    depth: u32,
    gene_coverage: f64,
) -> Result<Reading, LoadError> {
    let (contig, sequence) = sequence(reference)?;
    // Before anything is translated: these windows are positions in a
    // particular genome, and a reference that cannot hold them is not it.
    // A pipeline is refused earlier than this, at check time, where the
    // reference's declared frame is known; this is the floor under a
    // caller who reached the engine without one.
    if (sequence.len() as u64) < frame::LAST {
        return Err(LoadError::frame(
            reference,
            frame::Mismatch::TooShort {
                bases: sequence.len() as u64,
            }
            .describe(),
        ));
    }
    let calls = calls(variants)?;
    let mut notes = Vec::new();

    let scanned = match alignment {
        Some(path) => Some(scan(path, depth)?),
        None => {
            notes.push(Note::ReadingWithoutCoverage);
            None
        }
    };

    let mut observed = Observed::new();
    let mut reference_acids = BTreeMap::new();

    for gene in GENES {
        let mut protein = Protein::new();
        let mut unphased = 0_u32;
        for codon in 1..=gene.codons() {
            let first = gene.base_of(codon);
            let bases = [first, first + 1, first + 2];

            // The whole codon has to be readable. Two bases out of three
            // is not two thirds of an amino acid.
            if scanned
                .as_ref()
                .is_some_and(|scan| bases.iter().any(|base| !scan.positions.contains(base)))
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

            // Every combination of the alleles the caller credited: the
            // candidates, before the reads are asked which of them travel
            // together. A bit per place in the codon table.
            let mut candidates = 0_u64;
            for one in &alternatives[0] {
                for two in &alternatives[1] {
                    for three in &alternatives[2] {
                        if let Some(place) = places(&[*one, *two, *three]) {
                            candidates |= 1 << place;
                        }
                    }
                }
            }

            let phased = scanned
                .as_ref()
                .and_then(|scan| scan.at(first))
                .and_then(|carried| phase(candidates, carried, mixture, depth));
            match phased {
                Some(phased) => acids = acids.with(phased),
                None => {
                    // Only where there was something to phase: a codon
                    // whose reference base is not a base has no candidate
                    // reading at all, which is a different complaint.
                    if scanned.is_some() && candidates != 0 {
                        unphased += 1;
                    }
                    for place in 0..64 {
                        if candidates & (1 << place) != 0 {
                            let acid = translated(place);
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

        let codons = gene.codons() as usize;
        if protein.read() == 0 {
            notes.push(Note::GeneUnread {
                gene: gene.name.to_owned(),
                depth,
            });
            continue;
        }
        // A gene read this thinly refutes nothing and shows nothing, so
        // every drug scored against it would come out at zero, which the
        // algorithm's own ranges read as susceptible. Not assessed is the
        // honest answer, and the same one Stanford's service gives (D34).
        //
        // Counted over the codons the algorithm scores: a position no rule
        // names cannot change an answer, so reading it is not evidence the
        // gene was read well enough to score. A position the ruleset names
        // beyond the end of the gene is left out of the count, because no
        // reading could ever cover it and a floor nothing can clear is not
        // a floor.
        let scored: Vec<u32> = ruleset
            .scored(gene.name)
            .into_iter()
            .filter(|position| *position <= gene.codons())
            .collect();
        let read_scored = scored
            .iter()
            .filter(|position| protein.at(**position).is_some())
            .count();
        let needed = needed(
            u32::try_from(scored.len()).unwrap_or(u32::MAX),
            gene_coverage,
        );
        if read_scored < needed as usize {
            notes.push(Note::GeneBarelyRead {
                gene: gene.name.to_owned(),
                read: u32::try_from(read_scored).unwrap_or(u32::MAX),
                scored: u32::try_from(scored.len()).unwrap_or(u32::MAX),
                needed,
            });
            continue;
        }
        if protein.read() < codons {
            notes.push(Note::CodonsUnread {
                gene: gene.name.to_owned(),
                read: u32::try_from(protein.read()).unwrap_or(u32::MAX),
                codons: gene.codons(),
                depth,
            });
        }
        if unphased > 0 {
            notes.push(Note::CodonsUnphased {
                gene: gene.name.to_owned(),
                unphased,
                depth,
            });
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

/// How many of a gene's scored codons must be read, at this share of them.
///
/// Rounded up, so the share is a floor: a gene cannot be a fraction of a
/// codon short of it. Counted rather than kept as a share because a note
/// is read by a person and stored in a bundle, and `21 of 26` says what
/// `80%` leaves them to work out.
///
/// Zero where the algorithm scores nothing in the gene, which is the
/// honest answer rather than a gap: there is nothing a reading could have
/// covered, and no drug that could come out susceptible for want of it.
///
/// A share above one asks for more codons than the gene has, which no
/// reading could satisfy; it is taken as one, which asks for all of them.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the share is clamped to [0, 1], so the result is at most `codons`"
)]
fn needed(codons: u32, share: f64) -> u32 {
    (f64::from(codons) * share.clamp(0.0, 1.0)).ceil() as u32
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

/// What an alignment says about the codons: what it covers, and what
/// travels together.
#[derive(Debug)]
struct Scan {
    /// Every reference position covered to at least `depth`.
    positions: BTreeSet<u32>,
    /// The first reference position `triplets` is indexed from.
    first: u32,
    /// How many of the reads spanning each codon carried each of the
    /// sixty-four triplets: indexed by the codon's first base, and then
    /// by the triplet's place in the codon table.
    ///
    /// Dense because it is written once per codon per read, which on a
    /// deep amplicon is a few million times, and because the whole table
    /// is a megabyte — cheaper to allocate once than to look a key up
    /// that often.
    ///
    /// A read only counts here if it says what all three bases are. One
    /// that ends inside the codon, or carries an indel across it, or
    /// holds an `N` in it, says nothing about which amino acid the codon
    /// holds — and a read that says nothing should not dilute the reads
    /// that do, so it is left out of the denominator as well.
    triplets: Vec<[u32; 64]>,
}

impl Scan {
    /// What the reads spanning a codon carried, by triplet.
    fn at(&self, base: u32) -> Option<&[u32; 64]> {
        self.triplets.get(base.checked_sub(self.first)? as usize)
    }
}

/// Which amino acids the reads support, or `None` if they cannot say.
///
/// `candidates` is the alleles the caller credited, as a bit per place in
/// the codon table: phasing decides which combinations of called alleles
/// are real, and is not a second opinion about which alleles there are.
///
/// The threshold applies to the amino acid rather than to the triplet
/// spelling it. Two synonymous codons each below `mixture` can hold an
/// amino acid well above it, and losing an amino acid that is there is the
/// one direction D29 forbids.
///
/// `None` where fewer than `depth` reads span the codon, and where the
/// reads support no candidate reading of it at all. Both mean the same
/// thing — the reads cannot settle this codon — and the caller then falls
/// back to the per-base reading, which is wider rather than narrower
/// (D31).
fn phase(candidates: u64, carried: &[u32; 64], mixture: f64, depth: u32) -> Option<Acids> {
    let spanning: u32 = carried.iter().sum();
    if spanning < depth {
        return None;
    }

    // Summed by amino acid, so a lysine spelled two ways counts once.
    let mut by_acid: BTreeMap<char, u32> = BTreeMap::new();
    for (place, &count) in carried.iter().enumerate() {
        if count > 0 && candidates & (1 << place) != 0 {
            *by_acid.entry(translated(place)).or_default() += count;
        }
    }

    let mut acids = Acids::none();
    let mut kept = false;
    for (acid, count) in &by_acid {
        if f64::from(*count) / f64::from(spanning) >= mixture {
            acids = acids.with(Acids::of(*acid).unwrap_or_else(|_| Acids::none()));
            kept = true;
        }
    }
    kept.then_some(acids)
}

/// Read an alignment's coverage of the genes, and its codon triplets.
///
/// One pass, because the reads are the expensive part of this module and
/// both questions are asked of the same bases.
fn scan(path: &Path, depth: u32) -> Result<Scan, LoadError> {
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
    let width = (last - first + 1) as usize;
    let mut depths = vec![0_u32; width];
    let mut triplets = vec![[0_u32; 64]; width];

    // Which of those positions a codon begins at, so a read is asked
    // about codons rather than about every base it covers.
    let mut starts = vec![false; width];
    for gene in GENES {
        for codon in 1..=gene.codons() {
            let base = gene.base_of(codon);
            if base >= first && base <= last {
                starts[(base - first) as usize] = true;
            }
        }
    }

    for record in reader.records() {
        let record = record.map_err(|error| LoadError::format(path, format!("record: {error}")))?;
        let Some(Ok(start)) = record.alignment_start() else {
            continue;
        };
        let mut position = u32::try_from(usize::from(start)).unwrap_or(u32::MAX);
        let sequence = record.sequence();
        let mut index = 0_usize;

        for operation in record.cigar().iter() {
            let Ok(operation) = operation else { continue };
            let length = u32::try_from(operation.len()).unwrap_or(0);
            match operation.kind() {
                Kind::Match | Kind::SequenceMatch | Kind::SequenceMismatch => {
                    for step in 0..length {
                        let base = position.saturating_add(step);
                        if base < first || base > last {
                            continue;
                        }
                        let at = (base - first) as usize;
                        depths[at] += 1;

                        // A codon this block holds whole is one this
                        // read can settle: three bases of a single
                        // aligned run, so nothing was inserted or
                        // deleted between them.
                        if !starts[at] || step + 2 >= length {
                            continue;
                        }
                        let read_at = index + step as usize;
                        let codon = [
                            sequence.get(read_at).unwrap_or(b'N'),
                            sequence.get(read_at + 1).unwrap_or(b'N'),
                            sequence.get(read_at + 2).unwrap_or(b'N'),
                        ];
                        if let Some(place) = places(&codon) {
                            triplets[at][place] += 1;
                        }
                    }

                    position = position.saturating_add(length);
                    index += length as usize;
                }
                Kind::Deletion | Kind::Skip => position = position.saturating_add(length),
                Kind::Insertion | Kind::SoftClip => index += length as usize,
                _ => {}
            }
        }
    }

    Ok(Scan {
        positions: (first..=last)
            .filter(|base| depths[(base - first) as usize] >= depth)
            .collect(),
        first,
        triplets,
    })
}

/// Where a base sits in the codon table: two bits, `A C G T`.
///
/// `None` for anything that is not a base. An `N` in a read says nothing
/// about which amino acid a codon holds, and neither does the `=` a BAM
/// writes where a base matches the reference it was not given.
const fn place(base: u8) -> Option<usize> {
    match base {
        b'A' => Some(0),
        b'C' => Some(1),
        b'G' => Some(2),
        b'T' | b'U' => Some(3),
        _ => None,
    }
}

/// Where a codon sits in the codon table, or `None` if it holds a base
/// that is not a base.
const fn places(codon: &[u8; 3]) -> Option<usize> {
    match (place(codon[0]), place(codon[1]), place(codon[2])) {
        (Some(one), Some(two), Some(three)) => Some(one * 16 + two * 4 + three),
        _ => None,
    }
}

/// What the codon at a place in the table translates to.
fn translated(place: usize) -> char {
    // Stops are `*`, as Stanford writes them, not `X`.
    const CODE: &[u8; 64] = b"KNKNTTTTRSRSIIMIQHQHPPPPRRRRLLLLEDEDAAAAGGGGVVVV*Y*YSSSS*CWCLFLF";
    char::from(CODE[place & 0x3f])
}

/// Translate one codon, or `None` if it holds a base that is not a base.
fn translate(codon: &[u8; 3]) -> Option<char> {
    places(codon).map(translated)
}
