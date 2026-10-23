//! The two value types the dimension axes are built from: a set of names
//! compared as a set, and a scalar label compared exactly.
//!
//! Four dimensions need a value and only two comparison rules sit between
//! them. `organism` and `source_set` are both "which of these things is the
//! data about", where order is noise and a missing member is the whole
//! point; `release` and `subject` are both "one label, and a different label
//! is a different thing". So there are two types and four aliases, rather
//! than four types that each re-derive the same `PartialEq`.
//!
//! Decision D36 forbids *one dimension* carrying *two* comparison rules,
//! because that is how a pipeline ends up passing `check()` under one rule
//! and failing under another. It says nothing against *two dimensions*
//! sharing *one* rule, and the rule is the thing worth having exactly once.
//! The wording an error uses still differs per dimension, because that
//! lives on `Dimension::leading_question`, not on the value.

use std::fmt;

/// An ordered set of names, compared as a set.
///
/// Members are compared case-insensitively and ignoring surrounding space,
/// and order never matters. "Case-insensitively" means ASCII case only
/// (`eq_ignore_ascii_case`): `HIV-1` and `hiv-1` are one name, but no Unicode
/// folding is attempted. That is immaterial for Latin binomials and worth
/// knowing before a set holds arbitrary labels. A host–pathogen dataset about
/// `HIV-1` and `Homo sapiens` is the same dataset however the two were
/// listed. There is no alias table — equating "human" with "Homo sapiens"
/// would mean guessing, and a wrong guess passes `check()` silently. Declare
/// one spelling and use it.
///
/// Insertion order is kept (so a message can echo what the user wrote) but
/// is not part of the identity, and a name given twice is recorded once in
/// its first spelling. Blank names are dropped rather than stored, since an
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
/// `load::organism`, which turns an empty declaration into an unknown one
/// with a reason. A new loader of a set-valued dimension must go through that
/// path (or an equivalent that checks [`NameSet::is_empty`] first), and
/// `tests/declared_dimensions.rs` fails if one does not.
#[derive(Clone, Debug)]
pub struct NameSet {
    members: Vec<String>,
}

impl NameSet {
    /// A set of one name, as the user spelled it.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self::of([name])
    }

    /// A set of several names.
    ///
    /// Each is trimmed, blanks are dropped, and a name already present
    /// (ignoring ASCII case) is skipped so the first spelling wins.
    #[must_use]
    pub fn of<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut members: Vec<String> = Vec::new();
        for name in names {
            let name = name.as_ref().trim();
            if !name.is_empty() && !members.iter().any(|m| m.eq_ignore_ascii_case(name)) {
                members.push(name.to_owned());
            }
        }
        Self { members }
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
    /// This is what lets an error name the organism that is missing instead
    /// of only saying that two sets differ.
    #[must_use]
    pub fn difference(&self, other: &Self) -> Vec<&str> {
        self.members
            .iter()
            .filter(|mine| !other.contains(mine))
            .map(String::as_str)
            .collect()
    }

    fn contains(&self, name: &str) -> bool {
        self.members.iter().any(|m| m.eq_ignore_ascii_case(name))
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

impl PartialEq for NameSet {
    /// Same members, in any order, ignoring ASCII case.
    ///
    /// The scan is quadratic on purpose. A set holds a handful of organisms
    /// or sources, so sorting or hashing would cost more than it saves, and
    /// both would need a case-folded copy of every member to be correct.
    ///
    /// Equal length plus "every member of `self` is in `other`" is only
    /// enough because [`NameSet::of`] collapses case-insensitive repeats, so
    /// members are pairwise distinct and the check is a bijection rather
    /// than merely a surjection. The reverse check is defensive: it makes
    /// the symmetry obvious but is not load-bearing.
    fn eq(&self, other: &Self) -> bool {
        self.members.len() == other.members.len()
            && self.members.iter().all(|m| other.contains(m))
            && other.members.iter().all(|m| self.contains(m))
    }
}

impl Eq for NameSet {}

impl fmt::Display for NameSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&english_list(&self.members))
    }
}

/// The organisms the data is about.
pub type Organism = NameSet;

/// The sources a dataset was assembled from.
///
/// Shares [`NameSet`]'s ASCII-case-insensitive dedup for now, which is right
/// for study accessions (`GSE12345` and `gse12345` are one study) and wrong
/// for object keys (`s3://bucket/A.vcf` and `s3://bucket/a.vcf` are two
/// objects, and the second would be dropped). No kind applies this dimension
/// yet. The type is to be split when one does; see D36.
pub type SourceSet = NameSet;

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
