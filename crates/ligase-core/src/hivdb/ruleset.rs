//! Reading a vendored resistance algorithm.
//!
//! The file read here is a `.ruleset`, derived from Stanford's ASI2 XML by
//! `scripts/vendor_hivdb_algorithm.py` and committed beside it. Both are in
//! `crates/ligase-core/rulesets/`, and the test suite re-derives one from
//! the other on every run, so this reader can be strict about a small
//! format instead of lenient about a large one.
//!
//! Strict means every line is understood or the file is refused. That is
//! [D21](../../../docs/decisions.md)'s rule applied to a ruleset: a reader
//! that skipped what it did not recognise would silently drop a drug, and a
//! dropped drug reads as "susceptible" — a false negative in the direction
//! that matters.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

/// The format version this reader understands, as written on a
/// `.ruleset`'s first line.
const FORMAT: u32 = 1;

/// The amino acids a rule may name at one position.
///
/// A bitset rather than a sorted string: matching a rule against an
/// observation is an intersection test, done once per rule position per
/// drug per sample, and the whole alphabet fits in a `u32`.
///
/// Bits 0–25 are `A`–`Z`. Bit 26 is a deletion and bit 27 an insertion,
/// which ASI writes as lowercase `d` and `i`. Bit 28 is a stop codon,
/// which Stanford writes `*`.
///
/// A stop codon can be observed but is never named by a rule, so it can
/// never satisfy one. That is the right answer rather than a gap: a stop
/// in the middle of protease is a defective virus, not a resistant one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Acids(u32);

/// Where the deletion bit sits.
const DELETION: u32 = 26;

/// Where the insertion bit sits.
const INSERTION: u32 = 27;

/// Where the stop-codon bit sits.
const STOP: u32 = 28;

impl Acids {
    /// An empty set.
    #[must_use]
    pub const fn none() -> Self {
        Self(0)
    }

    /// The set holding one amino acid, a deletion (`d`), an insertion
    /// (`i`) or a stop codon (`*`).
    ///
    /// # Errors
    ///
    /// If the character is not an uppercase letter, `d` or `i`.
    pub fn of(acid: char) -> Result<Self, String> {
        match acid {
            'A'..='Z' => Ok(Self(1 << (acid as u32 - 'A' as u32))),
            'd' => Ok(Self(1 << DELETION)),
            'i' => Ok(Self(1 << INSERTION)),
            '*' => Ok(Self(1 << STOP)),
            other => Err(format!("not an amino acid: {other:?}")),
        }
    }

    /// Every amino acid in a string such as `KE`, `215FY` or `69i`.
    ///
    /// # Errors
    ///
    /// If any character is not one this set can hold.
    pub fn parse(acids: &str) -> Result<Self, String> {
        let mut set = Self::none();
        for acid in acids.chars() {
            set = set.with(Self::of(acid)?);
        }
        if set == Self::none() {
            return Err("no amino acids".to_owned());
        }
        Ok(set)
    }

    /// This set together with another.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether the two sets share an amino acid.
    ///
    /// This is what "the rule matches" means. A mixture is a set with more
    /// than one member, and a rule is satisfied when *any* of the amino
    /// acids present is one it names — which is how Stanford treats an
    /// ambiguous position, and the only safe direction: taking the
    /// majority would drop minority resistance, which is the reason
    /// anyone sequences deeply.
    #[must_use]
    pub const fn meets(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// Whether the set holds nothing.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Acids {
    /// The amino acids in alphabetical order, deletions and insertions
    /// last, as ASI writes them.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for bit in 0..26 {
            if self.0 & (1 << bit) != 0 {
                write!(formatter, "{}", char::from(b'A' + bit))?;
            }
        }
        if self.0 & (1 << DELETION) != 0 {
            write!(formatter, "d")?;
        }
        if self.0 & (1 << INSERTION) != 0 {
            write!(formatter, "i")?;
        }
        if self.0 & (1 << STOP) != 0 {
            write!(formatter, "*")?;
        }
        Ok(())
    }
}

/// One position of a protein, and the amino acids that satisfy a rule
/// there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Residue {
    /// The position in the protein, counting from one.
    pub position: u32,
    /// The amino acids the rule names.
    pub acids: Acids,
}

/// One alternative within a scoring group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// Every residue that must be present. More than one is the rule
    /// language's `AND`.
    pub residues: Vec<Residue>,
    /// What this contributes when it matches. Negative scores are real —
    /// `184V => -10` for zidovudine — and are never clamped.
    pub score: i32,
}

/// One drug, and how its score is reached.
#[derive(Clone, Debug)]
pub struct Drug {
    /// The abbreviation the algorithm uses, e.g. `3TC` or `ATV/r`.
    pub abbreviation: String,
    /// The full name, e.g. `lamivudine`.
    pub name: String,
    /// The drug class, e.g. `NRTI`.
    pub class: String,
    /// The gene the class is scored against, e.g. `RT`.
    pub gene: String,
    /// The scoring groups. The drug's score is the sum over groups of the
    /// largest matching item in each, which is what `SCORE FROM` with
    /// `MAX` means. A group with nothing matching contributes nothing.
    pub groups: Vec<Vec<Item>>,
}

/// One rung of the resistance scale.
#[derive(Clone, Debug)]
pub struct Level {
    /// Its order, 1 (susceptible) to 5 (high-level resistance).
    pub order: u8,
    /// Susceptible, Intermediate or Resistant — the coarse three-way
    /// reading of the five levels.
    pub sir: String,
    /// The wording the algorithm gives, e.g. `High-Level Resistance`.
    pub name: String,
}

/// A vendored resistance algorithm, ready to interpret against.
#[derive(Clone, Debug)]
pub struct Ruleset {
    /// The algorithm's name, e.g. `HIVDB`.
    pub algorithm: String,
    /// Its version, e.g. `9.8`.
    pub version: String,
    /// The date the algorithm carries, e.g. `2025-01-05`.
    pub date: String,
    /// Every drug, in the order the algorithm declares them.
    pub drugs: Vec<Drug>,
    levels: Vec<Level>,
    default_level: u8,
    ranges: Vec<(Option<i32>, Option<i32>, u8)>,
}

impl Ruleset {
    /// How a methods section or a cache key names this algorithm.
    ///
    /// `HIVDB 9.8 (2025-01-05)` — the name, the version and the date the
    /// algorithm itself carries, so that two rulesets claiming one version
    /// but holding different rules cannot be confused for each other by
    /// anyone reading a report.
    #[must_use]
    pub fn describe(&self) -> String {
        format!("{} {} ({})", self.algorithm, self.version, self.date)
    }

    /// Every gene any drug is scored against, in declaration order.
    #[must_use]
    pub fn genes(&self) -> Vec<&str> {
        let mut genes: Vec<&str> = Vec::new();
        for drug in &self.drugs {
            if !genes.contains(&drug.gene.as_str()) {
                genes.push(&drug.gene);
            }
        }
        genes
    }

    /// The level a score falls in.
    ///
    /// # Errors
    ///
    /// If no range covers the score. A ruleset whose ranges leave a gap is
    /// refused when it is read, so this cannot happen for a ruleset that
    /// loaded; it is an error rather than a default because answering
    /// "susceptible" for a score nothing described would be a guess in the
    /// unsafe direction.
    pub fn level(&self, score: i32) -> Result<&Level, String> {
        let order = self
            .ranges
            .iter()
            .find(|(low, high, _)| {
                low.is_none_or(|bound| score >= bound) && high.is_none_or(|b| score <= b)
            })
            .map_or(self.default_level, |(_, _, order)| *order);
        self.levels
            .iter()
            .find(|level| level.order == order)
            .ok_or_else(|| format!("no level {order}"))
    }

    /// Read a `.ruleset` from disk.
    ///
    /// # Errors
    ///
    /// If the file cannot be read, is not a `.ruleset` of a format version
    /// this engine understands, or holds a line this reader does not
    /// recognise.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        Self::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    /// Read a `.ruleset` from text.
    ///
    /// # Errors
    ///
    /// If any line is not one this reader recognises, or a declaration a
    /// later line depends on is missing.
    #[allow(clippy::too_many_lines)]
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        let first = lines.next().unwrap_or_default();
        let format = first
            .strip_prefix("#ligase-hivdb-ruleset ")
            .ok_or("not a ruleset: no `#ligase-hivdb-ruleset` on the first line")?;
        if format.trim() != FORMAT.to_string() {
            return Err(format!(
                "ruleset format {}, but this engine reads {FORMAT}",
                format.trim()
            ));
        }

        let mut algorithm = None;
        let mut levels = Vec::new();
        let mut default_level = None;
        let mut ranges = Vec::new();
        let mut gene_of_class: BTreeMap<String, String> = BTreeMap::new();
        let mut class_of_drug: BTreeMap<String, String> = BTreeMap::new();
        let mut drugs: Vec<Drug> = Vec::new();

        for (number, line) in lines.enumerate() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let at = |what: &str| format!("line {}: {what}", number + 2);
            let fields: Vec<&str> = line.split('\t').collect();
            let number_at = |index: usize, what: &str| -> Result<i32, String> {
                fields
                    .get(index)
                    .ok_or_else(|| at(&format!("no {what}")))?
                    .parse::<i32>()
                    .map_err(|_| at(&format!("{what} is not a number")))
            };
            let field = |index: usize, what: &str| -> Result<String, String> {
                fields
                    .get(index)
                    .filter(|value| !value.is_empty())
                    .map(|value| (*value).to_owned())
                    .ok_or_else(|| at(&format!("no {what}")))
            };

            match *fields.first().unwrap_or(&"") {
                "algorithm" => {
                    algorithm = Some((
                        field(1, "algorithm name")?,
                        field(2, "version")?,
                        field(3, "date")?,
                    ));
                }
                "level" => levels.push(Level {
                    order: u8::try_from(number_at(1, "level order")?)
                        .map_err(|_| at("level order is out of range"))?,
                    sir: field(2, "SIR")?,
                    name: field(3, "level name")?,
                }),
                "default" => {
                    default_level = Some(
                        u8::try_from(number_at(1, "default level")?)
                            .map_err(|_| at("default level is out of range"))?,
                    );
                }
                "range" => {
                    let bound = |index: usize, what: &str| -> Result<Option<i32>, String> {
                        match field(index, what)?.as_str() {
                            "*" => Ok(None),
                            value => value
                                .parse::<i32>()
                                .map(Some)
                                .map_err(|_| at(&format!("{what} is not a number"))),
                        }
                    };
                    ranges.push((
                        bound(1, "range lower bound")?,
                        bound(2, "range upper bound")?,
                        u8::try_from(number_at(3, "range level")?)
                            .map_err(|_| at("range level is out of range"))?,
                    ));
                }
                "gene" => {
                    let gene = field(1, "gene name")?;
                    for class in field(2, "drug classes")?.split(',') {
                        gene_of_class.insert(class.to_owned(), gene.clone());
                    }
                }
                "class" => {
                    let class = field(1, "class name")?;
                    for drug in field(2, "drugs")?.split(',') {
                        class_of_drug.insert(drug.to_owned(), class.clone());
                    }
                }
                "drug" => {
                    let abbreviation = field(1, "drug abbreviation")?;
                    let class = field(3, "drug class")?;
                    let gene = gene_of_class
                        .get(&class)
                        .ok_or_else(|| at(&format!("no gene declares the class {class}")))?
                        .clone();
                    drugs.push(Drug {
                        abbreviation,
                        name: field(2, "drug name")?,
                        class,
                        gene,
                        groups: Vec::new(),
                    });
                }
                "rule" => {
                    let abbreviation = field(1, "drug abbreviation")?;
                    let group = usize::try_from(number_at(2, "group")?)
                        .map_err(|_| at("group is out of range"))?;
                    let score = number_at(3, "score")?;
                    let residues = field(4, "residues")?
                        .split('+')
                        .map(parse_residue)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|error| at(&error))?;

                    let drug = drugs
                        .iter_mut()
                        .find(|drug| drug.abbreviation == abbreviation)
                        .ok_or_else(|| at(&format!("rule for undeclared drug {abbreviation}")))?;
                    if group > drug.groups.len() {
                        return Err(at(&format!(
                            "group {group} follows group {}, leaving a gap",
                            drug.groups.len().saturating_sub(1)
                        )));
                    }
                    if group == drug.groups.len() {
                        drug.groups.push(Vec::new());
                    }
                    drug.groups[group].push(Item { residues, score });
                }
                other => return Err(at(&format!("unknown record {other:?}"))),
            }
        }

        let (algorithm, version, date) = algorithm.ok_or("no `algorithm` record")?;
        if levels.is_empty() {
            return Err("no `level` records".to_owned());
        }
        if ranges.is_empty() {
            return Err("no `range` records".to_owned());
        }
        if drugs.is_empty() {
            return Err("no `drug` records".to_owned());
        }
        let default_level = default_level.ok_or("no `default` record")?;
        for (_, _, order) in &ranges {
            if !levels.iter().any(|level| level.order == *order) {
                return Err(format!("a range names level {order}, which is not defined"));
            }
        }
        if !levels.iter().any(|level| level.order == default_level) {
            return Err(format!("the default level {default_level} is not defined"));
        }

        Ok(Self {
            algorithm,
            version,
            date,
            drugs,
            levels,
            default_level,
            ranges,
        })
    }
}

/// Read one residue, e.g. `215FY`, `67d` or `69i`.
fn parse_residue(residue: &str) -> Result<Residue, String> {
    let split = residue
        .find(|character: char| !character.is_ascii_digit())
        .ok_or_else(|| format!("residue {residue:?} names no amino acid"))?;
    if split == 0 {
        return Err(format!("residue {residue:?} has no position"));
    }
    let (position, acids) = residue.split_at(split);
    Ok(Residue {
        position: position
            .parse()
            .map_err(|_| format!("residue {residue:?} has no position"))?,
        acids: Acids::parse(acids).map_err(|error| format!("residue {residue:?}: {error}"))?,
    })
}
