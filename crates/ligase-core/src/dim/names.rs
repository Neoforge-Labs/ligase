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
/// and order never matters: a host–pathogen dataset about `HIV-1` and
/// `Homo sapiens` is the same dataset however the two were listed. There is
/// no alias table — equating "human" with "Homo sapiens" would mean
/// guessing, and a wrong guess passes `check()` silently. Declare one
/// spelling and use it.
///
/// Insertion order is kept (so a message can echo what the user wrote) but
/// is not part of the identity, and a name given twice is recorded once in
/// its first spelling. Blank names are dropped rather than stored, since an
/// empty member would compare equal to every other empty member and mean
/// nothing.
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
    /// (ignoring case) is skipped so the first spelling wins.
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
fn render(members: &[String]) -> String {
    match members {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

impl PartialEq for NameSet {
    /// Same members, in any order, ignoring case.
    ///
    /// The scan is quadratic on purpose. A set holds a handful of organisms
    /// or sources, so sorting or hashing would cost more than it saves, and
    /// both would need a case-folded copy of every member to be correct.
    fn eq(&self, other: &Self) -> bool {
        self.members.len() == other.members.len()
            && self.members.iter().all(|m| other.contains(m))
            && other.members.iter().all(|m| self.contains(m))
    }
}

impl Eq for NameSet {}

impl fmt::Display for NameSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&render(&self.members))
    }
}

/// The organisms the data is about.
pub type Organism = NameSet;

/// The sources a dataset was assembled from.
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
