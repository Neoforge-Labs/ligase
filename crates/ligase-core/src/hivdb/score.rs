//! Scoring an observation against a resistance algorithm.
//!
//! The arithmetic is small: a drug's score is the sum, over its groups, of
//! the largest matching item in each. What takes care is everything around
//! it — what "matching" means when a position holds a mixture, and what to
//! say about a position that was never read.
//!
//! Both of those are decided in the direction that cannot invent a
//! susceptible result:
//!
//! - **A mixture satisfies a rule if any amino acid in it does.** An
//!   ambiguous position is two viruses, not an average of one, and taking
//!   the majority would discard exactly the minority resistance that deep
//!   sequencing exists to find.
//! - **A position that was not read is not a wild-type position.** It is
//!   recorded as unread, and a drug whose rules mention one is reported
//!   with its score marked as a lower bound rather than as an answer.

use std::collections::BTreeMap;

use super::ruleset::{Acids, Drug, Ruleset};

/// What was seen in one protein.
///
/// Only positions that were actually read appear. A position that was read
/// and agrees with the reference is here, holding the reference amino
/// acid; a position with no coverage is absent, and that absence is the
/// whole point — it is what lets a report distinguish "wild-type" from
/// "we could not see".
#[derive(Clone, Debug, Default)]
pub struct Protein {
    acids: BTreeMap<u32, Acids>,
}

impl Protein {
    /// A protein with nothing read yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record what was read at one position.
    pub fn saw(&mut self, position: u32, acids: Acids) {
        if !acids.is_empty() {
            self.acids.insert(position, acids);
        }
    }

    /// What was read at a position, if it was read at all.
    #[must_use]
    pub fn at(&self, position: u32) -> Option<Acids> {
        self.acids.get(&position).copied()
    }

    /// How many positions were read.
    #[must_use]
    pub fn read(&self) -> usize {
        self.acids.len()
    }
}

/// What was seen, across every protein a pipeline supplied.
///
/// A gene that is absent was not supplied at all, which is different from
/// a gene that was supplied and found to be wild-type. Drugs scored
/// against an absent gene are reported as not assessed rather than as
/// susceptible, because an algorithm that scores lenacapavir against
/// capsid cannot say anything at all about a pipeline that sequenced only
/// pol — and saying "susceptible" there would be a fabricated negative.
#[derive(Clone, Debug, Default)]
pub struct Observed {
    proteins: BTreeMap<String, Protein>,
}

impl Observed {
    /// An observation of nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a protein that was supplied.
    pub fn with(&mut self, gene: &str, protein: Protein) {
        self.proteins.insert(gene.to_owned(), protein);
    }

    /// The protein for a gene, if that gene was supplied.
    #[must_use]
    pub fn protein(&self, gene: &str) -> Option<&Protein> {
        self.proteins.get(gene)
    }
}

/// What the algorithm says about one drug.
#[derive(Clone, Debug)]
pub struct Assessment {
    /// The drug's abbreviation, e.g. `3TC`.
    pub drug: String,
    /// Its full name, e.g. `lamivudine`.
    pub name: String,
    /// Its class, e.g. `NRTI`.
    pub class: String,
    /// The gene it is scored against, e.g. `RT`.
    pub gene: String,
    /// The score, or `None` when the gene was not supplied.
    pub score: Option<i32>,
    /// The level, 1 to 5, or `None` when the gene was not supplied.
    pub level: Option<u8>,
    /// The algorithm's wording — `High-Level Resistance` — or
    /// `Not assessed` when the gene was not supplied.
    pub interpretation: String,
    /// The positions that contributed to the score, with the amino acids
    /// seen there, so a report can name them.
    pub mutations: Vec<(u32, Acids)>,
    /// Positions this drug's rules mention that were never read.
    ///
    /// Empty is the ordinary case. Non-empty means the score is a lower
    /// bound: a rule naming one of these might have matched had the
    /// position been covered.
    pub unread: Vec<u32>,
}

impl Assessment {
    /// Whether the score rests on a fully-read set of positions.
    ///
    /// A `false` here does not make the score wrong; it makes it a floor.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.unread.is_empty()
    }
}

/// Score every drug in a ruleset against an observation.
///
/// # Errors
///
/// If the ruleset's ranges do not cover a score it produced, which a
/// ruleset that loaded cannot do.
pub fn interpret(ruleset: &Ruleset, observed: &Observed) -> Result<Vec<Assessment>, String> {
    ruleset
        .drugs
        .iter()
        .map(|drug| assess(ruleset, drug, observed))
        .collect()
}

/// Score one drug.
fn assess(ruleset: &Ruleset, drug: &Drug, observed: &Observed) -> Result<Assessment, String> {
    let Some(protein) = observed.protein(&drug.gene) else {
        return Ok(Assessment {
            drug: drug.abbreviation.clone(),
            name: drug.name.clone(),
            class: drug.class.clone(),
            gene: drug.gene.clone(),
            score: None,
            level: None,
            interpretation: "Not assessed".to_owned(),
            mutations: Vec::new(),
            unread: Vec::new(),
        });
    };

    let mut total = 0;
    let mut mutations: BTreeMap<u32, Acids> = BTreeMap::new();
    let mut unread: Vec<u32> = Vec::new();

    for group in &drug.groups {
        // A group contributes the largest of its matching items, and
        // nothing at all when none match. `None` rather than a running
        // maximum seeded at zero: scores can be negative, and seeding at
        // zero would silently turn a -10 into a 0.
        let mut best: Option<(i32, Vec<(u32, Acids)>)> = None;

        for item in group {
            let mut seen = Vec::with_capacity(item.residues.len());
            let mut refuted = false;
            let mut missing = Vec::new();
            for residue in &item.residues {
                match protein.at(residue.position) {
                    Some(acids) if acids.meets(residue.acids) => {
                        seen.push((residue.position, acids));
                    }
                    // A position that was read and holds something else
                    // settles the item: it cannot match, whatever else is
                    // unread, so nothing here is uncertain.
                    Some(_) => refuted = true,
                    None => missing.push(residue.position),
                }
            }

            if refuted {
                continue;
            }
            if missing.is_empty() {
                if best.as_ref().is_none_or(|(score, _)| item.score > *score) {
                    best = Some((item.score, seen));
                }
            } else {
                // Every residue that was read matches, and at least one
                // was not read. This item is undetermined rather than
                // failed, and that is worth saying: the drug's score is a
                // floor. An item already refuted by a read position does
                // not get here, so an unread position is only reported
                // where it could genuinely have changed the answer.
                for position in missing {
                    if !unread.contains(&position) {
                        unread.push(position);
                    }
                }
            }
        }

        if let Some((score, seen)) = best {
            total += score;
            for (position, acids) in seen {
                mutations.insert(position, acids);
            }
        }
    }

    let level = ruleset.level(total)?;
    unread.sort_unstable();
    Ok(Assessment {
        drug: drug.abbreviation.clone(),
        name: drug.name.clone(),
        class: drug.class.clone(),
        gene: drug.gene.clone(),
        score: Some(total),
        level: Some(level.order),
        interpretation: level.name.clone(),
        mutations: mutations.into_iter().collect(),
        unread,
    })
}
