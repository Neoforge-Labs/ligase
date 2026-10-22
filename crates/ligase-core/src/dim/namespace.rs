//! The identifier spaces the engine can recognise by sight.
//!
//! A gene called `TP53`, `ENSG00000141510`, `7157` and `P04637` is four
//! names in four spaces, and joining a list written in one to a table
//! written in another is the commonest silent bug in biology: the overlap
//! comes out empty, or small, and nothing says why. So the space is a
//! dimension, and this is the catalogue that lets a loader fill it in
//! without being told.
//!
//! # Why the catalogue is in the core and not on the Python surface
//!
//! `ligase inspect` reads a file's dimensions without going through Python
//! at all, so inference written on the Python surface would be inference
//! `inspect` did not have. Worse, two implementations of it could disagree
//! about what a file's identifiers are, and then the engine would hold two
//! answers to the question this dimension exists to settle. One
//! implementation, in the loader (spec §8, question 2).
//!
//! # Why the order of [`KnownNamespace::ALL`] is load-bearing
//!
//! Six of the seven shapes carry a marker: a prefix, an underscore, a
//! colon, or being digits throughout. `HGNC symbol` carries none — it is
//! upper-case letters and digits, which is also what a UniProt accession
//! is made of, so `P04637` fits the symbol shape as well as its own. The
//! symbol is therefore **last**, and [`KnownNamespace::resolve`] returns
//! the first match rather than complaining about several: the specific
//! shapes get first refusal and the symbol takes what is left. Adding a
//! namespace means deciding where in this order it belongs.
//!
//! There is no regex here on purpose. The shapes are short enough to read
//! as code, and a dependency is a thing to ask about rather than reach for.

/// One identifier space, and the shape of the names in it.
///
/// Held as a name and a predicate rather than as an enum with a method per
/// variant, so that the catalogue reads as a table and its order — which
/// [`Self::resolve`] depends on — is visible in one place.
#[derive(Clone, Copy)]
pub struct KnownNamespace {
    name: &'static str,
    claims: fn(&str) -> bool,
}

impl PartialEq for KnownNamespace {
    /// Two entries are the same namespace when they have the same name.
    ///
    /// Function pointers compare by address, which is not a fact anyone
    /// means, so identity rests on the name a user declares and an error
    /// prints — the only part of an entry that is a promise.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for KnownNamespace {}

impl std::fmt::Debug for KnownNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("KnownNamespace").field(&self.name).finish()
    }
}

impl KnownNamespace {
    /// Every namespace the engine recognises, most specific first.
    ///
    /// The order is the resolution order; see the module comment on why
    /// `HGNC symbol` is last.
    pub const ALL: [Self; 7] = [
        Self {
            name: "Ensembl gene",
            claims: ensembl_gene,
        },
        Self {
            name: "UniProt accession",
            claims: uniprot_accession,
        },
        Self {
            name: "RefSeq",
            claims: refseq,
        },
        Self {
            name: "HGNC ID",
            claims: hgnc_id,
        },
        Self {
            name: "ChEMBL target",
            claims: chembl_target,
        },
        Self {
            name: "Entrez gene",
            claims: entrez_gene,
        },
        Self {
            name: "HGNC symbol",
            claims: hgnc_symbol,
        },
    ];

    /// The name a user declares and an error prints, e.g. `Ensembl gene`.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Whether this namespace claims the identifier.
    ///
    /// Several namespaces can claim one identifier — that is the whole
    /// reason [`Self::ALL`] is ordered — so this answers for one namespace
    /// and [`Self::resolve`] answers which one wins.
    #[must_use]
    pub fn claims(&self, identifier: &str) -> bool {
        (self.claims)(identifier.trim())
    }

    /// The most specific namespace that claims this identifier.
    ///
    /// `None` for an identifier no shape fits, which is the answer for a
    /// blank, for anything holding a space or a lower-case letter, and for
    /// the dates Excel makes of gene symbols.
    #[must_use]
    pub fn resolve(identifier: &str) -> Option<&'static Self> {
        Self::ALL
            .iter()
            .find(|namespace| namespace.claims(identifier))
    }

    /// The one namespace every identifier resolves to, if there is one.
    ///
    /// `None` in three cases, and each is a refusal to guess rather than a
    /// failure. An **empty** list infers nothing, because every shape
    /// vacuously fits all zero identifiers and answering would declare a
    /// space for a file that names nothing. A list resolving to **more than
    /// one** namespace infers nothing, because a file that mixes two spaces
    /// is the thing this dimension exists to catch and picking the majority
    /// would hide it. And a list holding **one identifier nothing claims**
    /// infers nothing, because such a file is partly corrupt — an Excel
    /// date among the symbols — and a namespace declared over it would be
    /// a claim about identifiers the engine could not read.
    #[must_use]
    pub fn infer<'a, I>(identifiers: I) -> Option<&'static Self>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut found: Option<&'static Self> = None;
        for identifier in identifiers {
            let resolved = Self::resolve(identifier)?;
            match found {
                None => found = Some(resolved),
                Some(agreed) if agreed == resolved => {}
                Some(_) => return None,
            }
        }
        found
    }
}

// ------------------------------------------------------------- shapes --

/// `ENS`, up to three upper-case letters naming a species, `G`, 11 digits.
///
/// Only the gene form. `ENST…` is a transcript and `ENSP…` a protein, and
/// those are different spaces rather than spellings of this one.
fn ensembl_gene(identifier: &str) -> bool {
    let Some(rest) = identifier.strip_prefix("ENS") else {
        return false;
    };
    // Each possible species-code length is tried rather than consuming
    // letters greedily: `G` is itself an upper-case letter, so a greedy
    // scan would eat the `G` of `ENSG…` and then find no `G` after it.
    (0..=3).any(|species| {
        rest.len() > species
            && rest[..species].chars().all(|c| c.is_ascii_uppercase())
            && rest[species..]
                .strip_prefix('G')
                .is_some_and(|digits| digits.len() == 11 && all_digits(digits))
    })
}

/// A six-character UniProt accession, in either of its two shapes.
fn uniprot_accession(identifier: &str) -> bool {
    let characters: Vec<char> = identifier.chars().collect();
    let [first, second, third, fourth, fifth, sixth] = characters[..] else {
        return false;
    };
    if !second.is_ascii_digit() || !sixth.is_ascii_digit() {
        return false;
    }
    let middle_any = [third, fourth, fifth]
        .iter()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());

    // `[OPQ][0-9][A-Z0-9]{3}[0-9]`, or `[A-NR-Z][0-9][A-Z][A-Z0-9]{2}[0-9]`.
    let opq = matches!(first, 'O' | 'P' | 'Q') && middle_any;
    let other = first.is_ascii_uppercase()
        && !matches!(first, 'O' | 'P' | 'Q')
        && third.is_ascii_uppercase()
        && middle_any;
    opq || other
}

/// `NM_`, `NR_`, `NP_`, `XM_`, `XR_` or `XP_`, digits, optionally a version.
fn refseq(identifier: &str) -> bool {
    let mut characters = identifier.chars();
    let (Some(molecule), Some(kind), Some('_')) =
        (characters.next(), characters.next(), characters.next())
    else {
        return false;
    };
    if !matches!(molecule, 'N' | 'X') || !matches!(kind, 'M' | 'R' | 'P') {
        return false;
    }
    let rest: String = characters.collect();
    match rest.split_once('.') {
        Some((number, version)) => {
            all_digits(number) && !number.is_empty() && all_digits(version) && !version.is_empty()
        }
        None => all_digits(&rest) && !rest.is_empty(),
    }
}

/// `HGNC:` and a number — the stable identifier, not the symbol.
fn hgnc_id(identifier: &str) -> bool {
    identifier
        .strip_prefix("HGNC:")
        .is_some_and(|number| !number.is_empty() && all_digits(number))
}

/// `CHEMBL` and a number.
fn chembl_target(identifier: &str) -> bool {
    identifier
        .strip_prefix("CHEMBL")
        .is_some_and(|number| !number.is_empty() && all_digits(number))
}

/// Digits throughout: an NCBI Gene ID.
fn entrez_gene(identifier: &str) -> bool {
    !identifier.is_empty() && all_digits(identifier)
}

/// An upper-case letter, then letters, digits or hyphens.
///
/// The shape with no marker, so it is tried last. The length cap is what
/// keeps a sentence or a file path out: the longest approved human symbol
/// is well under it, and a name this shape would otherwise accept at any
/// length is almost never an identifier.
fn hgnc_symbol(identifier: &str) -> bool {
    const LONGEST: usize = 12;
    let mut characters = identifier.chars();
    characters.next().is_some_and(|c| c.is_ascii_uppercase())
        && identifier.len() <= LONGEST
        && characters.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
}

fn all_digits(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii_digit())
}
