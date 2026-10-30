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
//! # And why the marker-less shape is not inferred at all
//!
//! The order settles which shape *claims* an identifier. It cannot settle
//! which *space* a file of such identifiers is in, because the symbol shape
//! has no marker to be recognised by: `DHFR-TS` is ChEMBL's name for a
//! *P. falciparum* bifunctional enzyme and `MT-CYB`, `HLA-A` and
//! `NAT8B-AS1` are approved HGNC symbols, and nothing about the strings
//! tells them apart. A list of the first inferred `HGNC symbol` through
//! 0.3.0, and because both sides of a join inferred the same wrong answer,
//! the join passed.
//!
//! So [`KnownNamespace::infer`] answers [`Inference::Indistinguishable`]
//! wherever the agreeing shape carries no marker, the dimension comes back
//! unknown with a reason, and a join of two such lists is refused until the
//! user declares. That is D56, and it is the fourth of the refusals to
//! guess that D38 already lists. The six marker-bearing shapes infer as
//! they always have.
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
    marker: Option<&'static str>,
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
            marker: Some("the `ENS` prefix"),
            claims: ensembl_gene,
        },
        Self {
            name: "UniProt accession",
            marker: Some("digits in fixed positions, at six characters or ten"),
            claims: uniprot_accession,
        },
        Self {
            name: "RefSeq",
            marker: Some("a two-letter prefix and an underscore"),
            claims: refseq,
        },
        Self {
            name: "HGNC ID",
            marker: Some("the `HGNC:` prefix"),
            claims: hgnc_id,
        },
        Self {
            name: "ChEMBL target",
            marker: Some("the `CHEMBL` prefix"),
            claims: chembl_target,
        },
        Self {
            name: "Entrez gene",
            marker: Some("digits throughout"),
            claims: entrez_gene,
        },
        Self {
            name: "HGNC symbol",
            marker: None,
            claims: hgnc_symbol,
        },
    ];

    /// The name a user declares and an error prints, e.g. `Ensembl gene`.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// What tells this shape from every other shape in the catalogue, or
    /// `None` for the one shape that carries nothing.
    ///
    /// A field rather than a remark in prose, because [`Self::infer`] reads
    /// it. A shape with no marker cannot tell its own space from the several
    /// that look like it, and the engine's answer to a question it cannot
    /// settle is to say so rather than to pick (D56).
    #[must_use]
    pub const fn marker(&self) -> Option<&'static str> {
        self.marker
    }

    /// Whether this shape's marker tells it from the others.
    ///
    /// False for `HGNC symbol` alone, which is upper-case letters, digits
    /// and hyphens — the shape of a gene symbol, and equally the shape of
    /// ChEMBL's component synonyms, a consortium's internal names and most
    /// in-house panels.
    #[must_use]
    pub const fn discriminates(&self) -> bool {
        self.marker.is_some()
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
        // A date is in no identifier space, whatever shape it happens to
        // have. Asked before the shapes rather than left to them, because
        // the symbol shape is deliberately permissive -- it has to admit
        // `C9orf72` -- and `Sep-02` fits it exactly. One rule, in the one
        // place that decides what an identifier is.
        if crate::note::looks_like_a_date(identifier.trim()) {
            return None;
        }
        Self::ALL
            .iter()
            .find(|namespace| namespace.claims(identifier))
    }

    /// What the identifiers themselves say about which space they are in.
    ///
    /// Four answers, and only one of them is a namespace. An **empty** list
    /// settles nothing, because every shape vacuously fits all zero
    /// identifiers and answering would declare a space for a file that names
    /// nothing. A list resolving to **more than one** namespace settles
    /// nothing, because a file that mixes two spaces is the thing this
    /// dimension exists to catch and picking the majority would hide it. A
    /// list holding **one identifier nothing claims** settles nothing,
    /// because such a file is partly corrupt — an Excel date among the
    /// symbols — and a namespace declared over it would be a claim about
    /// identifiers the engine could not read.
    ///
    /// And a list that agrees on a shape carrying **no marker** settles
    /// nothing either, which is D56 and the fourth refusal to guess. Its
    /// own variant rather than a fourth way to be [`Inference::Unsettled`],
    /// because the remedy differs: these identifiers are perfectly readable
    /// and the user need only say which space they are in, where a file
    /// holding `2-Sep` must be re-exported and would be made permanently
    /// wrong by a declaration.
    #[must_use]
    pub fn infer<'a, I>(identifiers: I) -> Inference
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut found: Option<&'static Self> = None;
        for identifier in identifiers {
            let Some(resolved) = Self::resolve(identifier) else {
                return Inference::Unsettled;
            };
            match found {
                None => found = Some(resolved),
                Some(agreed) if agreed == resolved => {}
                Some(_) => return Inference::Unsettled,
            }
        }
        match found {
            None => Inference::Unsettled,
            Some(shape) if shape.discriminates() => Inference::Settled(shape),
            Some(shape) => Inference::Indistinguishable(shape),
        }
    }
}

/// What a file's own identifiers say about which space they are in.
///
/// Three variants rather than an `Option`, because the two ways of settling
/// nothing need different advice and the loader is what phrases it. A
/// loader that could only see `None` would have to send the user of a
/// perfectly good gene-symbol list and the user of a spreadsheet-corrupted
/// one to the same remedy, and one of those two remedies makes the
/// corruption permanent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inference {
    /// One namespace, whose marker tells it from every other shape.
    Settled(&'static KnownNamespace),
    /// The identifiers do not agree on one shape, or hold a name no shape
    /// claims, or there are none of them.
    Unsettled,
    /// They all fit one shape, and that shape carries no marker — so it
    /// cannot tell its own space from the several that look like it.
    ///
    /// Carries the shape anyway, because a message worth reading names what
    /// the identifiers looked like.
    Indistinguishable(&'static KnownNamespace),
}

impl Inference {
    /// The namespace, where the identifiers settled one.
    #[must_use]
    pub const fn settled(self) -> Option<&'static KnownNamespace> {
        match self {
            Self::Settled(namespace) => Some(namespace),
            Self::Unsettled | Self::Indistinguishable(_) => None,
        }
    }

    /// The shape every identifier fitted, where one did but could not
    /// settle the question.
    #[must_use]
    pub const fn indistinguishable(self) -> Option<&'static KnownNamespace> {
        match self {
            Self::Indistinguishable(namespace) => Some(namespace),
            Self::Settled(_) | Self::Unsettled => None,
        }
    }
}

// ------------------------------------------------------------- shapes --

/// `ENS`, up to three upper-case letters naming a species, `G`, 11 digits,
/// and optionally `.` and a version.
///
/// Only the gene form. `ENST…` is a transcript and `ENSP…` a protein, and
/// those are different spaces rather than spellings of this one.
///
/// The version is accepted because `ENSG00000141510.18` is what GENCODE,
/// GTEx and `featureCounts` emit, and refusing it would decline to infer a
/// space for the commonest gene identifier in RNA-seq. It is kept as part of
/// the identifier rather than stripped: two releases of a gene are two
/// different sequences, so quietly equating `.18` with `.19` would be the
/// coercion this dimension exists to refuse. The consequence is that a
/// versioned list will not find its rows in an unversioned mapping table --
/// which is reported, identifier by identifier, rather than guessed at.
fn ensembl_gene(identifier: &str) -> bool {
    let Some(rest) = identifier.strip_prefix("ENS") else {
        return false;
    };
    let rest = match rest.split_once('.') {
        Some((stem, version)) if !version.is_empty() && all_digits(version) => stem,
        Some(_) => return false,
        None => rest,
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

/// A UniProtKB accession: six characters, or ten, with an optional isoform.
///
/// UniProt's own pattern is
/// `[OPQ][0-9][A-Z0-9]{3}[0-9]|[A-NR-Z][0-9]([A-Z][A-Z0-9]{2}[0-9]){1,2}`,
/// and the `{1,2}` is the part that matters most here: it is the
/// ten-character form, which every TrEMBL `A0A…` entry uses, and TrEMBL is
/// most of UniProt. Accepting only six characters left a list of them to be
/// claimed by the symbol shape — upper-case letters and digits, which is
/// exactly what an accession looks like — so a protein list and a gene list
/// both came out as `HGNC symbol` and **agreed**. That is the false
/// agreement this whole dimension exists to break, caused by the engine
/// rather than by the user, which is why the length is spelled out here.
fn uniprot_accession(identifier: &str) -> bool {
    // An isoform of an entry is in the same space as the entry.
    let stem = match identifier.split_once('-') {
        Some((base, isoform)) if !isoform.is_empty() && all_digits(isoform) => base,
        Some(_) => return false,
        None => identifier,
    };

    let characters: Vec<char> = stem.chars().collect();
    let Some((&first, rest)) = characters.split_first() else {
        return false;
    };
    if !first.is_ascii_uppercase() {
        return false;
    }

    // `[OPQ][0-9][A-Z0-9]{3}[0-9]`: the six-character reviewed form, whose
    // middle three characters may each be a letter or a digit.
    if matches!(first, 'O' | 'P' | 'Q') {
        return rest.len() == 5
            && rest[0].is_ascii_digit()
            && rest[1..4].iter().all(char::is_ascii_alphanumeric)
            && rest[4].is_ascii_digit();
    }

    // `[A-NR-Z][0-9]([A-Z][A-Z0-9]{2}[0-9]){1,2}`: a digit, then one block
    // of four or two of them -- six characters in all, or ten. The second
    // block is what every TrEMBL `A0A...` accession uses.
    let (Some(&digit), blocks) = (rest.first(), &rest[1.min(rest.len())..]) else {
        return false;
    };
    digit.is_ascii_digit()
        && matches!(blocks.len(), 4 | 8)
        && blocks.chunks(4).all(is_accession_block)
}

/// One `[A-Z][A-Z0-9]{2}[0-9]` block of an accession.
fn is_accession_block(block: &[char]) -> bool {
    let [letter, middle_one, middle_two, last] = *block else {
        return false;
    };
    letter.is_ascii_uppercase()
        && [middle_one, middle_two]
            .iter()
            .all(char::is_ascii_alphanumeric)
        && last.is_ascii_digit()
}

/// A two-letter RefSeq prefix, `_`, digits, and optionally a version.
///
/// Listed rather than taken as "any two letters and an underscore", because
/// a closed list is a claim the engine can be held to and a wildcard would
/// quietly claim anything shaped like it. `NC_` (a chromosome) and `NG_` (a
/// gene region) are what a clinical-genetics list holds, and leaving them
/// out declined to infer a space for an ordinary file.
fn refseq(identifier: &str) -> bool {
    const PREFIXES: [&str; 14] = [
        "NM", "NR", "NP", "XM", "XR", "XP", "NC", "NG", "NT", "NW", "NZ", "AP", "YP", "WP",
    ];
    let Some((prefix, rest)) = identifier.split_once('_') else {
        return false;
    };
    if !PREFIXES.contains(&prefix) {
        return false;
    }
    let rest = rest.to_owned();
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
/// The shape with no marker, so it is tried last and has to be permissive
/// in two ways that are easy to get wrong.
///
/// **Case.** Approved symbols are not all upper case: `C9orf72` is in most
/// neurology gene lists and `C1orf43` is a standard housekeeping gene, and
/// the whole `Cxorfy` family is spelled this way. Requiring upper case
/// declined to infer a space for an ordinary human gene list. Permitting
/// lower case is what makes [`KnownNamespace::resolve`] ask about dates
/// first, since `Sep-02` fits this shape exactly.
///
/// **Length.** The cap admits the approved readthrough symbols, which are
/// the longest there are: `TNFSF12-TNFSF13` is fifteen characters and
/// `TMEM189-UBE2V1` fourteen. It exists at all to keep a sentence or a file
/// path out, since a name of this shape at any length is rarely an
/// identifier.
fn hgnc_symbol(identifier: &str) -> bool {
    const LONGEST: usize = 24;
    let mut characters = identifier.chars();
    characters.next().is_some_and(|c| c.is_ascii_uppercase())
        && identifier.len() <= LONGEST
        && characters.all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn all_digits(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii_digit())
}
