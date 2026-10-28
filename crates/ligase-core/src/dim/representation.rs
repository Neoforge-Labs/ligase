//! What a name in a molecule table *is*, and what was done before it was
//! computed.
//!
//! The same substance has different InChIKeys depending on its
//! standardisation, and the difference is not marginal. Measured over six
//! RDKit releases spanning four years, twelve of thirty ordinary drugs get a
//! different key under a different state. Aspirin gets three:
//!
//! | From | State | Key |
//! |---|---|---|
//! | its sodium salt | as given | `JZLOKWGVGHYBKD-UHFFFAOYSA-M` |
//! | its sodium salt | salt-stripped | `BSYNRYMUTXBXSQ-UHFFFAOYSA-M` |
//! | its sodium salt | salt-stripped, charge-neutralised | `BSYNRYMUTXBXSQ-UHFFFAOYSA-N` |
//!
//! The last two differ in the final character, which is the charge block.
//! Nothing about a column of those strings tells a reader which
//! standardisation produced them, so two compound sets prepared differently
//! overlap in nothing for the substances affected while the unaffected ones
//! still match — an overlap that is small, non-zero and reads as poor
//! enrichment rather than as a mismatch.
//!
//! That failure is documented rather than conceivable. ChEMBL built a
//! three-stage standardiser for it, and says so: incoming compounds are not
//! standardised consistently, and standardisation is what makes it possible
//! to compare and integrate data on the same compound from different sources
//! (Bento *et al.*, J Cheminform 12:51, 2020). Fourches, Muratov & Tropsha
//! name the same operations — counterion and salt removal, chemotype
//! normalisation, curation of tautomeric forms, deletion of duplicates — as
//! mandatory curation, with case studies where doing them changed the result
//! (J Chem Inf Model 50:1189, 2010).
//!
//! # Why the identity is part of the value
//!
//! Because the other candidate moves. In the same six releases, RDKit's
//! canonical SMILES for cisplatin changed from `Cl[Pt]Cl.N.N` to
//! `N.N.[Cl][Pt][Cl]` while its InChIKey never moved at all. A set whose
//! names are structure strings therefore has an identity its own toolkit can
//! change under it, which is a weaker thing than a set named by InChIKey and
//! must not compare equal to one.
//!
//! # Why the operations are a closed set
//!
//! [`CoordinateSystem`](super::CoordinateSystem)'s grounds: a value the
//! engine cannot render is a value it cannot explain. A file naming an
//! operation the engine has never heard of is an error naming the four it
//! knows, rather than a state carried along unread — which is the lenient
//! reading `canon`'s module comment calls the dangerous kind.
//!
//! # Why `stereo-discarded` is an operation and not a setting
//!
//! Because stereo loss is a *consequence*, not a choice. RDKit's tautomer
//! canonicaliser removes stereochemistry at an sp3 centre inside a
//! tautomeric system, and a carboxylic acid's alpha-carbon is inside one —
//! so a tautomer-normalised set cannot tell ibuprofen's two enantiomers
//! apart, and both arrive as `HEFNNWSXXWATRW-UHFFFAOYSA-N`. For a drug set
//! that matters: two enantiomers are routinely one active compound and one
//! inactive one.
//!
//! §4.4 of the v0.2 design describes the state as keeping stereo. It does
//! not. So this type records what was *done*, stereo loss included, and
//! `ligase.chem.standardize` checks its own work rather than trusting a
//! user to have predicted it.

use std::fmt;

use super::english_list;

/// What a name in a molecule table is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Identity {
    /// A standard InChIKey, which four years of RDKit releases did not move.
    InChIKey,
    /// A structure string, which they did move.
    Smiles,
}

impl Identity {
    /// Both, for listing them in an error message.
    pub const ALL: [Self; 2] = [Self::InChIKey, Self::Smiles];

    /// How to render this for a user.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::InChIKey => "InChIKey",
            Self::Smiles => "SMILES",
        }
    }

    /// The spelling a file's representation line and a declaration use.
    ///
    /// Lower case and without punctuation, because it doubles as the name of
    /// the column the identity is read from: a table's `inchikey` column and
    /// `inchikey` in a representation line are the same word on purpose.
    #[must_use]
    pub const fn spelling(self) -> &'static str {
        match self {
            Self::InChIKey => "inchikey",
            Self::Smiles => "smiles",
        }
    }

    /// Look an identity up by the name [`Self::spelling`] gives it.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|identity| identity.spelling() == name)
    }
}

impl fmt::Display for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.describe())
    }
}

/// One standardisation applied before the names were computed.
///
/// Phrased as what was *applied*, so that an absent operation means it was
/// not done. `stereo-discarded` rather than `stereo-kept` for that reason:
/// the dangerous state is the one that lost something, so it is the one with
/// a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Standardisation {
    /// Counterions, solvents and the other fragments of a salt removed,
    /// leaving the parent.
    ///
    /// On its own this is **not** enough to match a neutral set: RDKit's
    /// `FragmentParent` leaves the deprotonated carboxylate, so a
    /// salt-stripped aspirin is `BSYNRYMUTXBXSQ-UHFFFAOYSA-M` where plain
    /// aspirin is `BSYNRYMUTXBXSQ-UHFFFAOYSA-N`. That one character is the
    /// commonest version of this failure.
    SaltStripped,
    /// One tautomer chosen for every tautomeric form, so that warfarin's
    /// keto and enol forms are one compound rather than two.
    ///
    /// The operation that also discards stereochemistry, which is why
    /// [`Self::StereoDiscarded`] usually travels with it.
    TautomerNormalised,
    /// Formal charges removed where a neutral form exists.
    ChargeNeutralised,
    /// Stereochemistry lost — not asked for, observed.
    StereoDiscarded,
}

impl Standardisation {
    /// Every operation the engine knows, in the order a representation
    /// renders them.
    ///
    /// Declaration order is pipeline order: salts come off first, tautomers
    /// are chosen, charges are removed, and stereo loss is the thing noticed
    /// afterwards.
    pub const ALL: [Self; 4] = [
        Self::SaltStripped,
        Self::TautomerNormalised,
        Self::ChargeNeutralised,
        Self::StereoDiscarded,
    ];

    /// How to render this for a user, which is also how a file spells it.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::SaltStripped => "salt-stripped",
            Self::TautomerNormalised => "tautomer-normalised",
            Self::ChargeNeutralised => "charge-neutralised",
            Self::StereoDiscarded => "stereo-discarded",
        }
    }

    /// Look an operation up by the name [`Self::describe`] gives it.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|operation| operation.describe() == name)
    }

    /// Every operation's name, for an error that has to list them.
    fn all_named() -> String {
        english_list(
            &Self::ALL
                .into_iter()
                .map(Self::describe)
                .collect::<Vec<&str>>(),
        )
    }
}

impl fmt::Display for Standardisation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.describe())
    }
}

/// How a molecule table's names were arrived at.
///
/// Compared by equality of the identity and set equality of the operations.
/// The operations are held in [`Standardisation::ALL`] order with repeats
/// collapsed, so the set comparison is a slice comparison and the rendering
/// of one state is the same sentence every time — which matters because the
/// sentence goes in an error message, in a file's own representation line and
/// in a conversion's target, and those three must agree.
///
/// # An empty value is legal; an unknown one is not
///
/// A representation with no operations is `as given`, which is a real state:
/// it is what a vendor file is in. What must never happen is a `Dim::known`
/// holding a representation the loader *guessed*, because a set whose state
/// nobody recorded is not in the `as given` state — it is in a state nobody
/// knows. `load::molecule_table` therefore leaves the axis unknown with a
/// reason rather than assuming the weakest state, which would be the engine
/// inventing the most dangerous of the possible answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Representation {
    identity: Identity,
    applied: Vec<Standardisation>,
}

impl Representation {
    /// A representation from its identity and the operations applied.
    ///
    /// Repeats are collapsed and the order is canonicalised, so that two
    /// callers listing the same operations differently build the same value.
    #[must_use]
    pub fn new<I>(identity: Identity, applied: I) -> Self
    where
        I: IntoIterator<Item = Standardisation>,
    {
        let given: Vec<Standardisation> = applied.into_iter().collect();
        Self {
            identity,
            applied: Standardisation::ALL
                .into_iter()
                .filter(|operation| given.contains(operation))
                .collect(),
        }
    }

    /// Read a representation from the one spelling used everywhere.
    ///
    /// `inchikey; salt-stripped, charge-neutralised`, or `inchikey; as given`
    /// — and `inchikey` alone means the same as `as given`. One spelling for
    /// a file's own line, a user's declaration and a conversion's target,
    /// because a value written three ways is a value that will eventually be
    /// parsed three ways.
    ///
    /// # Errors
    ///
    /// If the text is blank, if the identity is not one the engine can
    /// compute, or if it names a standardisation the engine does not know. A
    /// blank is the case that matters: a blank compares equal to another
    /// blank, so accepting one would produce two sets that *agree* about
    /// being in no particular state.
    pub fn parse(spelled: &str) -> Result<Self, String> {
        let spelled = spelled.trim();
        if spelled.is_empty() {
            return Err(format!(
                "a representation needs an identity to compare by; name one of {}",
                english_list(
                    &Identity::ALL
                        .into_iter()
                        .map(Identity::spelling)
                        .collect::<Vec<&str>>()
                )
            ));
        }

        let (identity, operations) = match spelled.split_once(';') {
            Some((identity, operations)) => (identity, operations.trim()),
            None => (spelled, ""),
        };

        let identity = Identity::from_name(identity).ok_or_else(|| {
            format!(
                "`{}` is not an identity this engine can compare molecules by; name one of {}",
                identity.trim(),
                english_list(
                    &Identity::ALL
                        .into_iter()
                        .map(Identity::spelling)
                        .collect::<Vec<&str>>()
                )
            )
        })?;

        if operations.is_empty() || operations.eq_ignore_ascii_case(AS_GIVEN) {
            return Ok(Self::new(identity, []));
        }

        let mut applied = Vec::new();
        for named in operations.split(',') {
            if named.trim().is_empty() {
                continue;
            }
            applied.push(Standardisation::from_name(named).ok_or_else(|| {
                format!(
                    "`{}` is not a standardisation this engine knows; it knows {}",
                    named.trim(),
                    Standardisation::all_named()
                )
            })?);
        }
        Ok(Self::new(identity, applied))
    }

    /// What a name in this set is.
    #[must_use]
    pub const fn identity(&self) -> Identity {
        self.identity
    }

    /// The operations applied, in [`Standardisation::ALL`] order.
    #[must_use]
    pub fn applied(&self) -> &[Standardisation] {
        &self.applied
    }

    /// The operations this applied that `other` did not.
    ///
    /// What lets an error name the operation that makes two states differ
    /// instead of only saying that they do. For a set standardised four ways
    /// against one standardised two, that operation is the only part worth
    /// reading.
    #[must_use]
    pub fn operations_beyond(&self, other: &Self) -> Vec<&'static str> {
        self.applied
            .iter()
            .filter(|operation| !other.applied.contains(operation))
            .map(|operation| operation.describe())
            .collect()
    }

    /// This representation in the spelling [`Self::parse`] reads.
    ///
    /// What a standardiser writes into a file's representation line, so the
    /// line a step writes is a line the loader reads back to the same value.
    #[must_use]
    pub fn spell(&self) -> String {
        let operations = if self.applied.is_empty() {
            AS_GIVEN.to_owned()
        } else {
            self.applied
                .iter()
                .map(|operation| operation.describe())
                .collect::<Vec<&str>>()
                .join(", ")
        };
        format!("{}; {operations}", self.identity.spelling())
    }
}

/// How a file, a declaration and a message all spell "nothing was applied".
const AS_GIVEN: &str = "as given";

impl fmt::Display for Representation {
    /// `InChIKey, salt-stripped and charge-neutralised`.
    ///
    /// English rather than the parsed spelling, because this is what a user
    /// reads in an error. [`Representation::spell`] is the other direction.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, ", self.identity)?;
        if self.applied.is_empty() {
            return f.write_str(AS_GIVEN);
        }
        f.write_str(&english_list(
            &self
                .applied
                .iter()
                .map(|operation| operation.describe())
                .collect::<Vec<&str>>(),
        ))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn the_operations_are_held_in_one_order_however_they_arrive() {
        let forwards = Representation::new(
            Identity::InChIKey,
            [
                Standardisation::SaltStripped,
                Standardisation::ChargeNeutralised,
            ],
        );
        let backwards = Representation::new(
            Identity::InChIKey,
            [
                Standardisation::ChargeNeutralised,
                Standardisation::SaltStripped,
            ],
        );
        assert_eq!(forwards, backwards);
        assert_eq!(forwards.applied(), backwards.applied());
    }

    #[test]
    fn an_operation_given_twice_is_applied_once() {
        assert_eq!(
            Representation::new(
                Identity::InChIKey,
                [Standardisation::SaltStripped, Standardisation::SaltStripped]
            )
            .applied(),
            [Standardisation::SaltStripped]
        );
    }

    #[test]
    fn what_a_standardiser_writes_is_what_the_loader_reads() {
        // The round trip the file format rests on. Were these to drift, a
        // step would write a line its own loader read as something else.
        for representation in [
            Representation::new(Identity::InChIKey, []),
            Representation::new(Identity::Smiles, []),
            Representation::new(
                Identity::InChIKey,
                [
                    Standardisation::SaltStripped,
                    Standardisation::TautomerNormalised,
                    Standardisation::ChargeNeutralised,
                    Standardisation::StereoDiscarded,
                ],
            ),
        ] {
            assert_eq!(
                Representation::parse(&representation.spell()).as_ref(),
                Ok(&representation)
            );
        }
    }

    #[test]
    fn a_blank_is_refused_rather_than_read_as_as_given() {
        // Two blanks would compare equal, which is the false agreement the
        // axis exists to refuse.
        for blank in ["", "   ", "; as given"] {
            assert!(Representation::parse(blank).is_err(), "{blank:?}");
        }
    }

    #[test]
    fn an_error_names_what_the_engine_does_know() {
        let said = Representation::parse("inchikey; desalted-twice").unwrap_err();
        assert!(said.contains("desalted-twice"), "{said}");
        for operation in Standardisation::ALL {
            assert!(said.contains(operation.describe()), "{said}");
        }

        let said = Representation::parse("fingerprint").unwrap_err();
        assert!(said.contains("fingerprint"), "{said}");
        assert!(said.contains("inchikey"), "{said}");
        assert!(said.contains("smiles"), "{said}");
    }
}
