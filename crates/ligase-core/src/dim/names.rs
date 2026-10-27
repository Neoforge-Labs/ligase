//! The value types the dimension axes are built from: a set of names
//! compared as a set, and a scalar label compared exactly.
//!
//! Five dimensions need a value and three comparison rules sit between them.
//! `organism` and `source_set` are both "which of these things is the data
//! about", where order is noise and a missing member is the whole point, so
//! both are [`NameSet`]; `release`, `subject` and `identifier_namespace` are
//! all "one label, and a different label is a different thing", so all three
//! are [`Label`]. What the two sets do *not* share is how two members are
//! compared, and that is the one thing [`NameSet`] is generic over: an
//! organism folds ASCII case, a source does not ([`Organism`],
//! [`SourceSet`]).
//!
//! Decision D36 forbids *one dimension* carrying *two* comparison rules,
//! because that is how a pipeline ends up passing `check()` under one rule
//! and failing under another. It says nothing against *two dimensions*
//! sharing *one* rule, and the rule is the thing worth having exactly once.
//! The set *algebra* — insertion-order dedup, difference, English rendering,
//! equality by length and containment — is therefore written once for both
//! sets, while the member comparison is written once per rule. The wording an
//! error uses still differs per dimension, because that lives on
//! `Dimension::leading_question`, not on the value.

use std::fmt;
use std::marker::PhantomData;

/// How two members of a [`NameSet`] are compared.
///
/// A type parameter rather than a field, because a field would let one
/// dimension carry two rules at runtime — the thing D36 forbids outright —
/// and would make the choice an argument a caller can get wrong silently.
/// As a parameter it is the compiler's business: [`Organism`] and
/// [`SourceSet`] are different types, so a `check::compare` arm that read
/// the wrong axis's field no longer builds. Before the split they were one
/// type, and only a test over a kind that applied the axis could catch it
/// (M7's standing requirement).
pub trait Matching {
    /// Whether these two names are the same name.
    ///
    /// Both arrive already trimmed, so this rules on the names themselves.
    fn same(left: &str, right: &str) -> bool;

    /// How to describe this rule to a user who needs to know it.
    const DESCRIBE: &'static str;
}

/// Members differing only in ASCII case are one member.
///
/// Right for a closed conventional vocabulary. Latin binomials are one:
/// `HIV-1` and `hiv-1` are one organism by convention, and nothing is lost
/// by folding them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoldsAsciiCase;

impl Matching for FoldsAsciiCase {
    fn same(left: &str, right: &str) -> bool {
        left.eq_ignore_ascii_case(right)
    }

    const DESCRIBE: &'static str = "compared without regard to case";
}

/// Members are the same only if they are spelled the same.
///
/// Right for anything a specification makes case-sensitive.
/// `s3://bucket/A.vcf` and `s3://bucket/a.vcf` are two objects, and a URL
/// path is case-sensitive wherever it is served — so folding the case here
/// would *drop* a member, turning two sets that differ into two that agree.
/// That is a false agreement, which is the thing this engine exists to
/// refuse. Not folding it can only *add* a member, which turns agreement
/// into a loud refusal, fixed by one consistent spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exactly;

impl Matching for Exactly {
    fn same(left: &str, right: &str) -> bool {
        left == right
    }

    const DESCRIBE: &'static str = "compared exactly";
}

/// An ordered set of names, compared as a set.
///
/// Order never matters, and surrounding space is always forgiven. Whether
/// case matters is the type parameter's business: see [`Matching`],
/// [`Organism`] and [`SourceSet`]. A host–pathogen dataset about `HIV-1` and
/// `Homo sapiens` is the same dataset however the two were listed. There is
/// no alias table — equating "human" with "Homo sapiens" would mean
/// guessing, and a wrong guess passes `check()` silently. Declare one
/// spelling and use it.
///
/// Insertion order is kept (so a message can echo what the user wrote) but
/// is not part of the identity, and a name given twice is recorded once in
/// its first spelling.  Blank names are dropped rather than stored, since an
/// empty member would compare equal to every other empty member and mean
/// nothing.
///
/// # An empty set must never be a known dimension
///
/// An empty set is a legal *value*: it is what [`NameSet::of`] returns for
/// no names, and `Display` renders it as an empty string. It must never reach
/// a `Dim::known`, because two known empty sets compare equal, and that is a
/// false agreement, the thing this engine exists to refuse. The type does not
/// enforce this; the loaders do, by building the dimension through
/// `load::organism` and `load::source_set`, which turn an empty declaration
/// into an unknown one with a reason. A new loader of a set-valued dimension
/// must go through that path (or an equivalent that checks
/// [`NameSet::is_empty`] first), and `tests/declared_dimensions.rs` fails if
/// one does not.
///
/// # There is no default rule
///
/// `M` has no default, so this type cannot be named without naming the rule
/// it compares by. That is deliberate: the two axes differ in exactly that
/// rule, and a `NameSet` written without one would be whichever rule the
/// default happened to be, which is how the axes came to share a rule
/// nobody had chosen for both. Write [`Organism`] or [`SourceSet`].
#[derive(Clone, Debug)]
pub struct NameSet<M: Matching> {
    members: Vec<String>,
    how: PhantomData<M>,
}

impl<M: Matching> NameSet<M> {
    /// A set of one name, as the user spelled it.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self::of([name])
    }

    /// A set of several names.
    ///
    /// Each is trimmed, blanks are dropped, and a name already present —
    /// by this set's own rule for "already present" — is skipped so the
    /// first spelling wins.
    #[must_use]
    pub fn of<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut members: Vec<String> = Vec::new();
        for name in names {
            let name = name.as_ref().trim();
            if !name.is_empty() && !members.iter().any(|m| M::same(m, name)) {
                members.push(name.to_owned());
            }
        }
        Self {
            members,
            how: PhantomData,
        }
    }

    /// The members, in the order they were first given.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.members
    }

    /// Whether the set has no members.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// The members of `self` that are not in `other`.
    ///
    /// This is what lets an error name the organism, or the source, that is
    /// missing instead of only saying that two sets differ.
    #[must_use]
    pub fn difference(&self, other: &Self) -> Vec<&str> {
        self.members
            .iter()
            .filter(|mine| !other.contains(mine))
            .map(String::as_str)
            .collect()
    }

    fn contains(&self, name: &str) -> bool {
        self.members.iter().any(|m| M::same(m, name))
    }
}

/// Join names as English does: `a`, `a and b`, `a, b and c`.
///
/// Shared with the error text in `check`, which lists the members one input
/// has and the other lacks. Defining the join once means a set and the
/// difference between two sets can never be punctuated differently.
pub(crate) fn english_list<S: AsRef<str>>(names: &[S]) -> String {
    match names {
        [] => String::new(),
        [only] => only.as_ref().to_owned(),
        [init @ .., last] => {
            let init: Vec<&str> = init.iter().map(AsRef::as_ref).collect();
            format!("{} and {}", init.join(", "), last.as_ref())
        }
    }
}

impl<M: Matching> PartialEq for NameSet<M> {
    /// Same members, in any order, by this set's own rule for "same".
    ///
    /// The scan is quadratic on purpose. A set holds a handful of organisms
    /// or sources, so sorting or hashing would cost more than it saves, and
    /// a case-folding set would need a folded copy of every member to be
    /// correct either way.
    ///
    /// Equal length plus "every member of `self` is in `other`" is only
    /// enough because [`NameSet::of`] collapses repeats under the same rule,
    /// so members are pairwise distinct and the check is a bijection rather
    /// than merely a surjection. The reverse check is defensive: it makes
    /// the symmetry obvious but is not load-bearing.
    fn eq(&self, other: &Self) -> bool {
        self.members.len() == other.members.len()
            && self.members.iter().all(|m| other.contains(m))
            && other.members.iter().all(|m| self.contains(m))
    }
}

impl<M: Matching> Eq for NameSet<M> {}

impl<M: Matching> fmt::Display for NameSet<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&english_list(&self.members))
    }
}

/// The organisms the data is about.
///
/// Folds ASCII case, because Latin binomials are a closed conventional
/// vocabulary in which `HIV-1` and `hiv-1` are one organism (D37).
pub type Organism = NameSet<FoldsAsciiCase>;

/// The sources a dataset was assembled from.
///
/// Compared **exactly**, which is D36's reserved right carried out. The two
/// failures are not symmetric: folding case *drops* a member, turning two
/// sets that differ into two that agree — a false agreement, which is the
/// thing this engine exists to refuse. Not folding it can only *add* a
/// member, which turns agreement into a refusal: loud, legible, and fixed by
/// declaring one spelling. This project has taken that trade twice already,
/// in D37's refusal to equate `human` with `Homo sapiens` and in D38's
/// refusal of an alias table.
///
/// The folding is also the unusual rule rather than the default. [`Release`],
/// [`Subject`] and [`IdentifierNamespace`] are all [`Label`] and compared
/// case-sensitively, because a label is a name the user chose. [`Organism`]
/// folds because of the closed vocabulary above. Sources are not that:
/// `s3://bucket/A.vcf` and `s3://bucket/a.vcf` are two objects *by
/// specification*, and a URL path is case-sensitive wherever it is served.
/// `GSE12345` matching `gse12345` is real but is the minority case, and it
/// costs a user one consistent spelling.
///
/// M10a settled this rule and left it unimplemented, because until a kind
/// applied the axis `check_step` never reached its arms and shipping an
/// unexercised value type is the condition M7's standing requirement exists
/// to prevent. `KnowledgeGraph` applies it, so the split lands with it,
/// together with that axis's `check::compare` and `graph::propagate` arms.
pub type SourceSet = NameSet<Exactly>;

/// A scalar label, compared exactly.
///
/// Only surrounding space is forgiven. Case is not: `hgnc 2026-09` and
/// `HGNC 2026-09` may well be different releases, and a label is a name the
/// user chose, not a free-text description to be fuzzily matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label(String);

impl Label {
    /// A label as the user spelled it, minus surrounding space.
    #[must_use]
    pub fn new(value: &str) -> Self {
        Self(value.trim().to_owned())
    }

    /// The label.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the label is blank.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The release of an annotation or database the data is on.
pub type Release = Label;

/// Who or what the data is about, when that is one named thing.
pub type Subject = Label;

/// Which identifier space a set of entities is named in.
///
/// A [`Label`] because the rule is the same as `release`'s: one name, and a
/// different name is a different space. It is deliberately not a closed set
/// of the namespaces the engine recognises — a curated list's own spelling
/// is a namespace too, and refusing it would make the dimension unusable for
/// exactly the in-house truth sets that most need it
/// ([`KnownNamespace`](crate::dim::KnownNamespace) is what inference reads,
/// not what a declaration is checked against).
pub type IdentifierNamespace = Label;
