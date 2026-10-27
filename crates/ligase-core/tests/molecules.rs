//! The two molecule kinds, and the axis that admits them.
//!
//! The same substance has different InChIKeys depending on what was done to
//! it before the key was computed. Measured over six RDKit releases spanning
//! four years, twelve of thirty ordinary drugs get a different key under a
//! different standardisation state, and aspirin gets three — differing, for
//! two of them, in the final character of a twenty-seven-character string
//! nobody reads. So two compound sets standardised differently overlap in
//! nothing for the substances affected, the overlap is rarely zero because
//! the unaffected ones still match, and a hit rate that is small, non-zero
//! and plausible is what a reader sees. That is the silent failure
//! `representation` exists to break, and it is why `MoleculeSet` is admitted
//! under [D35](https://ligase.neoforgelabs.tech/project/decisions/).
//!
//! # M7's standing requirement lands here
//!
//! `representation` is a new axis, so the first kind to claim it must pin
//! that axis's `check::compare` and `graph::propagate` arms with a named
//! test. The compiler catches less than it looks here: `Representation` is
//! a type of its own, so an arm reading another axis's field does not build,
//! but `propagate`'s arms name their fields by hand whatever the types are
//! and the requirement asks for the test either way. The two under "M7's
//! standing requirement" below are that obligation met.
//!
//! # D48's second limb is what admits the ranked sibling
//!
//! `MoleculeRanking` brings no axis `MoleculeSet` does not bring. It is
//! admitted on the limb D35 never wrote down and M12 relied on anyway: a
//! canonical form whose absence makes the cache serve a wrong answer. So the
//! two kinds claim identical axes, and
//! `the_two_molecule_kinds_claim_the_same_axes` is what fails if that ever
//! stops being true.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use ligase::canon::{self, Form};
use ligase::check::{self, Binding, check_step};
use ligase::convert::Conversion;
use ligase::dim::{
    Dim, Dimension, Dimensions, Identity, Organism, Release, Representation, Source,
    Standardisation, Subject,
};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::hash::ContentHash;
use ligase::load::{self, Declared};
use ligase::note::Note;
use ligase::types::{DataKind, Dataset};

mod common;

use common::fixture;

/// The two kinds this milestone adds, for the loops that must cover both.
const MOLECULE_KINDS: [DataKind; 2] = [DataKind::MoleculeSet, DataKind::MoleculeRanking];

// ----------------------------------------------------------- scaffold --

/// The canonical hash of a file read as `kind`.
fn canonical(kind: DataKind, path: &Path) -> ContentHash {
    let raw = ContentHash::of_file(path).unwrap();
    canon::hash(kind, path, raw).unwrap().hash
}

/// How a file read as `kind` was hashed, and what that noticed.
fn hashed(kind: DataKind, path: &Path) -> (Form, Vec<Note>) {
    let raw = ContentHash::of_file(path).unwrap();
    let canonical = canon::hash(kind, path, raw).unwrap();
    (canonical.form, canonical.notes)
}

fn bind(name: &str, dataset: Dataset) -> Binding {
    Binding {
        name: name.to_owned(),
        dataset,
    }
}

/// Load a committed molecule fixture as a set, declaring nothing.
fn molecules(name: &str) -> Dataset {
    load::molecule_set(&fixture(name), &Declared::default()).unwrap()
}

/// A representation spelled the way a file's own line spells it.
fn representation(spelled: &str) -> Representation {
    Representation::parse(spelled).unwrap()
}

/// A molecule set carrying one axis's value, and the representation every
/// join of two molecule kinds needs.
fn molecules_with(dimension: Dimension, value: &str) -> Dataset {
    let mut dimensions = Dimensions {
        representation: Dim::known(representation("inchikey; as given"), Source::Declared),
        ..Dimensions::default()
    };
    match dimension {
        Dimension::Representation => {
            dimensions.representation = Dim::known(representation(value), Source::Declared);
        }
        Dimension::Subject => {
            dimensions.subject = Dim::known(Subject::new(value), Source::Declared);
        }
        Dimension::Release => {
            dimensions.release = Dim::known(Release::new(value), Source::Declared);
        }
        Dimension::Organism => {
            dimensions.organism = Dim::known(Organism::new(value), Source::Declared);
        }
        other => panic!("{other} is not an axis a molecule set claims"),
    }
    Dataset::new(DataKind::MoleculeSet, None, dimensions)
}

/// A set declaring every axis it claims, for the propagation test.
fn set_declaring_everything() -> Dataset {
    Dataset::new(
        DataKind::MoleculeSet,
        None,
        Dimensions {
            organism: Dim::known(Organism::new("Homo sapiens"), Source::Declared),
            representation: Dim::known(
                representation("inchikey; salt-stripped, charge-neutralised"),
                Source::Declared,
            ),
            subject: Dim::known(Subject::new("EGFR"), Source::Declared),
            release: Dim::known(Release::new("ChEMBL 34"), Source::Declared),
            ..Dimensions::default()
        },
    )
}

fn spec(name: &str, output: DataKind, inputs: &[(&str, NodeId)]) -> StepSpec {
    StepSpec {
        name: name.to_owned(),
        output,
        coordinates: None,
        conversion: None,
        arguments: inputs
            .iter()
            .map(|(param, node)| Argument {
                param: (*param).to_owned(),
                declared: output,
                node: *node,
            })
            .collect(),
    }
}

// -------------------------------------------------------- the dimensions --

#[test]
fn a_molecule_set_has_no_coordinate_system() {
    // The `n/a` cells, and the place the catalogue earns its warning. A
    // table of compounds has no contigs, no coordinate system, no read
    // pairing, no quality scores and no samples -- not as gaps to be filled
    // in, but because a list of substances has no such properties to have.
    for kind in MOLECULE_KINDS {
        for dimension in [
            Dimension::ContigNaming,
            Dimension::Coordinates,
            Dimension::Pairing,
            Dimension::QualityEncoding,
            Dimension::Reference,
            Dimension::SampleSet,
        ] {
            assert!(!kind.applies(dimension), "{kind:?} / {dimension}");
        }
    }
}

#[test]
fn the_molecule_kinds_claim_four_axes() {
    for kind in MOLECULE_KINDS {
        for dimension in [
            Dimension::Organism,
            Dimension::Representation,
            Dimension::Subject,
            Dimension::Release,
        ] {
            assert!(kind.applies(dimension), "{kind:?} / {dimension}");
        }
    }
}

#[test]
fn an_identifier_namespace_is_not_a_molecule_sets_business() {
    // An InChIKey *is* an identifier space, and `representation` already
    // says which one this file is in. Claiming both axes would be one fact
    // in two places -- D36's objection in mirror image -- and would make
    // `required_pair` demand two things where one will do.
    for kind in MOLECULE_KINDS {
        assert!(!kind.applies(Dimension::IdentifierNamespace), "{kind:?}");
        assert!(!kind.applies(Dimension::SourceSet), "{kind:?}");
    }
}

#[test]
fn representation_is_the_axis_these_kinds_were_admitted_for() {
    // D35's test for a kind is that it brings an axis no other kind brings.
    // A gene list has no standardisation state and a BAM is a measurement,
    // so a third kind appearing here is a question, not a detail.
    for kind in DataKind::ALL {
        assert_eq!(
            kind.applies(Dimension::Representation),
            MOLECULE_KINDS.contains(&kind),
            "{kind:?}"
        );
    }
}

#[test]
fn the_two_molecule_kinds_claim_the_same_axes() {
    // D41's confinement rule, which is also what D48 rests on: a set and
    // its ranked sibling differ in their canonical form and in nothing
    // else. If they ever diverge on an axis, this is where it surfaces --
    // and it would mean the distinction had leaked out of the one place it
    // is meant to live.
    for dimension in Dimension::ALL {
        assert_eq!(
            DataKind::MoleculeSet.applies(dimension),
            DataKind::MoleculeRanking.applies(dimension),
            "{dimension}"
        );
    }
}

#[test]
fn two_molecule_sets_must_both_say_how_they_were_standardised() {
    // M12's lesson, applied before it can become the same bug a third
    // time: every combination is written out rather than left to a
    // catch-all. Scoring a ranking against a library is the commonest
    // reason to join these two kinds at all, and a catch-all would have
    // demanded nothing of it.
    for left in MOLECULE_KINDS {
        for right in MOLECULE_KINDS {
            assert!(
                check::required_pair(left, right).contains(&Dimension::Representation),
                "{left:?} / {right:?}"
            );
        }
    }

    // And two sets that each say nothing are refused rather than agreeing
    // by silence, which is the failure itself in its purest form.
    let errors = check_step(
        "overlap",
        &[
            bind("hits", molecules("molecules_no_representation.tsv")),
            bind("library", molecules("molecules_no_representation.tsv")),
        ],
    );
    assert!(
        errors
            .iter()
            .any(|error| error.dimension() == Some(Dimension::Representation)),
        "got: {errors:#?}"
    );
}

#[test]
fn a_representation_is_demanded_of_nothing_else() {
    // Against any other kind a molecule table shares only `organism`,
    // which is demanded nowhere, so there is nothing to insist on.
    for kind in DataKind::ALL {
        if MOLECULE_KINDS.contains(&kind) {
            continue;
        }
        for molecules in MOLECULE_KINDS {
            assert!(
                !check::required_pair(molecules, kind).contains(&Dimension::Representation),
                "{molecules:?} / {kind:?}"
            );
            assert!(
                !check::required_pair(kind, molecules).contains(&Dimension::Representation),
                "{kind:?} / {molecules:?}"
            );
        }
    }
}

#[test]
fn the_molecule_kinds_round_trip_through_their_names() {
    assert_eq!(DataKind::MoleculeSet.name(), "MoleculeSet");
    assert_eq!(DataKind::MoleculeRanking.name(), "MoleculeRanking");
    for kind in MOLECULE_KINDS {
        assert_eq!(DataKind::from_name(kind.name()), Some(kind));
    }
}

#[test]
fn a_molecule_table_is_written_as_a_table() {
    for kind in MOLECULE_KINDS {
        assert_eq!(ligase::exec::output_suffix(kind), ".tsv");
    }
}

// ------------------------------------------------------- the headline --

#[test]
fn two_sets_standardised_differently_refuse_to_join() {
    // The milestone's headline, and the measurement in a fixture. Both
    // files hold aspirin and metformin. One holds them salt-stripped and
    // neutralised, the other as the vendor wrote them -- a sodium salt and
    // a hydrochloride -- so their InChIKeys have nothing in common and an
    // overlap of the two is empty. Nothing about either column of keys
    // says why.
    let errors = check_step(
        "overlap",
        &[
            bind("library", molecules("molecules_library.tsv")),
            bind("hits", molecules("molecules_hits_salts.tsv")),
        ],
    );

    let found: Vec<_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::Representation))
        .collect();
    assert_eq!(found.len(), 1, "got: {errors:#?}");

    let said = found[0].to_string();
    // Both representations in full, the operations that make them differ
    // named, and the question a user can act on.
    assert!(said.contains("salt-stripped"), "{said}");
    assert!(said.contains("as given"), "{said}");
    assert!(
        said.contains("Were the two sets standardised differently?"),
        "{said}"
    );
    // And where each value came from, which is what makes it actionable:
    // nobody declared these, the engine read them off the files.
    assert!(
        said.contains("the representation line the standardiser wrote"),
        "{said}"
    );
}

#[test]
fn a_standardisation_difference_names_the_operation_one_side_lacks() {
    // The two value lines already state both representations in full. What
    // they cannot do is point at the operation that makes them differ, and
    // for a set standardised four ways against one standardised two, that
    // operation is the only part worth reading.
    let errors = check_step(
        "overlap",
        &[
            bind("library", molecules("molecules_library.tsv")),
            bind("normalised", molecules("molecules_tautomer_normalised.tsv")),
        ],
    );
    let found: Vec<_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::Representation))
        .collect();
    assert_eq!(found.len(), 1, "got: {errors:#?}");

    let said = found[0].to_string();
    assert!(
        said.contains("normalised also lists tautomer-normalised and stereo-discarded"),
        "{said}"
    );
}

#[test]
fn stereo_discarded_is_what_makes_two_enantiomers_one_compound() {
    // §3.4 of the design, in a fixture. RDKit's tautomer canonicaliser
    // discards stereochemistry at an sp3 centre inside a tautomeric
    // system, and a carboxylic acid's alpha-carbon is inside one -- so
    // ibuprofen's two enantiomers become one key. Two enantiomers are
    // routinely one active compound and one inactive one.
    //
    // The two fixtures hold the same drug under the two keys. The engine
    // cannot tell a reader which enantiomer the second file meant, because
    // the file no longer says; what it can do is refuse to join it to one
    // that still knows.
    let library = molecules("molecules_library.tsv");
    let normalised = molecules("molecules_tautomer_normalised.tsv");
    let standardisations = |dataset: &Dataset| {
        dataset
            .dimensions()
            .representation
            .value()
            .unwrap()
            .applied()
            .to_vec()
    };
    assert!(!standardisations(&library).contains(&Standardisation::StereoDiscarded));
    assert!(standardisations(&normalised).contains(&Standardisation::StereoDiscarded));
}

// ------------------------------- M7's standing requirement, for it --

#[test]
fn each_axis_a_molecule_set_claims_compares_its_own_field() {
    // M7's standing requirement. `Release` and `Subject` are one type, so
    // an arm in `check::compare` written against the wrong field compiles,
    // and only a test over a kind that *applies* the axis can reach it.
    // `representation` has never been reached by any kind until now, which
    // is exactly the condition the requirement exists to prevent.
    for (dimension, left, right) in [
        (
            Dimension::Representation,
            "inchikey; salt-stripped",
            "inchikey; charge-neutralised",
        ),
        (Dimension::Subject, "EGFR", "ABL1"),
        (Dimension::Release, "ChEMBL 34", "ChEMBL 35"),
        (Dimension::Organism, "Homo sapiens", "Mus musculus"),
    ] {
        let errors = check_step(
            "compare",
            &[
                bind("a", molecules_with(dimension, left)),
                bind("b", molecules_with(dimension, right)),
            ],
        );
        let found: Vec<_> = errors
            .iter()
            .filter(|error| error.dimension() == Some(dimension))
            .collect();
        assert_eq!(found.len(), 1, "{dimension}: {errors:#?}");
    }
}

#[test]
fn a_step_over_a_molecule_set_carries_every_axis_it_claims() {
    // The other half of the standing requirement. `graph::propagate` has an
    // arm per axis and each names a field by hand; a `representation` arm
    // reading `subject` would have compiled and nothing could have caught
    // it, because no kind's output applied the axis.
    let mut graph = Graph::new();
    let input = graph.add_input("library", set_declaring_everything());
    let node = graph
        .add_step(spec("filter", DataKind::MoleculeSet, &[("library", input)]))
        .unwrap();
    let dimensions = graph.dataset(node).unwrap().dimensions();

    for (dimension, expected) in [
        (
            Dimension::Representation,
            "InChIKey, salt-stripped and charge-neutralised",
        ),
        (Dimension::Subject, "EGFR"),
        (Dimension::Release, "ChEMBL 34"),
        (Dimension::Organism, "Homo sapiens"),
    ] {
        assert_eq!(
            dimensions.render(dimension).as_deref(),
            Some(expected),
            "{dimension}"
        );
        assert_eq!(
            dimensions
                .source(dimension)
                .map(Source::describe)
                .as_deref(),
            Some("from step `filter`"),
            "{dimension}"
        );
    }
}

// ----------------------------------------------------------- loading --

#[test]
fn a_sets_representation_is_read_off_the_line_the_standardiser_wrote() {
    // D44's habit on a second axis: the answer is in the file, written by
    // whatever did the work, so asking the user for it would be the engine
    // demanding something it is holding.
    let loaded = molecules("molecules_library.tsv");
    assert_eq!(
        loaded
            .dimensions()
            .render(Dimension::Representation)
            .as_deref(),
        Some("InChIKey, salt-stripped and charge-neutralised")
    );
    let said = loaded
        .dimensions()
        .source(Dimension::Representation)
        .map(Source::describe)
        .unwrap();
    assert!(
        said.starts_with("from the representation line the standardiser wrote"),
        "{said}"
    );
    assert!(said.ends_with("molecules_library.tsv"), "{said}");
}

#[test]
fn a_set_with_keys_and_no_representation_line_leaves_the_axis_unknown() {
    // Never "as given". A ChEMBL download *is* standardised and a vendor
    // file may not be, so assuming the weakest state would be the engine
    // inventing the most dangerous of the possible answers.
    let loaded = molecules("molecules_no_representation.tsv");
    assert!(!loaded.dimensions().representation.is_known());
    let reason = loaded
        .dimensions()
        .reason(Dimension::Representation)
        .unwrap();
    assert!(reason.contains("declare representation"), "{reason}");
    assert!(
        !reason.contains("as given"),
        "the reason must not suggest the state it refused to assume: {reason}"
    );
}

#[test]
fn a_declared_representation_is_taken_as_given() {
    // D38's position on a declared namespace, on this axis: the engine has
    // no chemistry (D49), so it cannot check the claim, and a declaration
    // is a user telling it something it cannot read.
    let declared = Declared {
        representation: Some("inchikey; salt-stripped, tautomer-normalised".to_owned()),
        ..Declared::default()
    };
    let loaded = load::molecule_set(&fixture("molecules_library.tsv"), &declared).unwrap();
    assert_eq!(
        loaded
            .dimensions()
            .render(Dimension::Representation)
            .as_deref(),
        Some("InChIKey, salt-stripped and tautomer-normalised"),
        "a declaration wins over the file's own line"
    );
    assert_eq!(
        loaded
            .dimensions()
            .source(Dimension::Representation)
            .map(Source::describe)
            .as_deref(),
        Some("declared")
    );
}

#[test]
fn a_set_named_by_structures_alone_says_so() {
    // §3.2 of the design is why this is a weaker thing rather than an
    // equivalent one: RDKit's canonical SMILES for cisplatin changed
    // between 2024.09.6 and 2025.09.6 while its InChIKey did not, so a set
    // whose identity is a structure string has an identity its own toolkit
    // can move under it.
    let loaded = molecules("molecules_smiles_only.tsv");
    assert_eq!(
        loaded
            .dimensions()
            .render(Dimension::Representation)
            .as_deref(),
        Some("SMILES, as given")
    );
}

#[test]
fn a_table_with_neither_an_inchikey_nor_a_smiles_column_is_not_a_molecule_set() {
    let error = load::molecule_set(&fixture("molecules_no_identity.tsv"), &Declared::default())
        .expect_err("a table with no identity column is not a molecule set")
        .to_string();
    assert!(error.contains("inchikey"), "{error}");
    assert!(error.contains("smiles"), "{error}");
    // And it says what the file does have, as the named-list loader does.
    assert!(error.contains("activity_nm"), "{error}");
}

#[test]
fn an_operation_the_engine_does_not_know_is_not_a_representation() {
    // A closed set, on `CoordinateSystem`'s grounds: a value the engine
    // cannot render is a value it cannot explain, so a file naming an
    // operation it has never heard of is an error naming the four it knows
    // rather than a state carried along unread.
    let error = load::molecule_set(
        &fixture("molecules_unknown_operation.tsv"),
        &Declared::default(),
    )
    .expect_err("an unknown standardisation is not a representation")
    .to_string();
    assert!(error.contains("desalted-twice"), "{error}");
    assert!(error.contains("tautomer-normalised"), "{error}");
}

#[test]
fn a_molecule_set_does_not_record_its_organism_subject_or_release() {
    let loaded = molecules("molecules_library.tsv");
    for dimension in [Dimension::Organism, Dimension::Subject, Dimension::Release] {
        assert!(
            !loaded.dimensions().is_known(dimension),
            "{dimension} is not in a compound table"
        );
        assert!(
            loaded.dimensions().reason(dimension).is_some(),
            "{dimension} must say why it is unknown"
        );
    }
}

// --------------------------------------------------- canonical forms --

#[test]
fn the_order_of_a_molecule_sets_rows_says_nothing() {
    // The ordinary rule. The order compounds came off whatever assembled
    // the table is not a fact about the chemistry.
    assert_eq!(
        canonical(DataKind::MoleculeSet, &fixture("molecules_library.tsv")),
        canonical(
            DataKind::MoleculeSet,
            &fixture("molecules_library_reordered.tsv")
        )
    );
}

#[test]
fn the_order_of_a_molecule_ranking_is_its_result() {
    // D41 requires the contrast rather than the bare fact, and the reason
    // is about reading the code: a reader finding only an `assert_ne!`
    // between two orderings would reasonably take it for a bug and "fix"
    // `canon/molecule_ranking.rs` to sort like every other canonicaliser
    // in the module. So this hashes the *same two files* both ways.
    let (one, other) = (
        fixture("molecules_ranked.tsv"),
        fixture("molecules_ranked_shuffled.tsv"),
    );
    assert_ne!(
        canonical(DataKind::MoleculeRanking, &one),
        canonical(DataKind::MoleculeRanking, &other),
        "a ranking's order is the answer: two orders are two answers"
    );
    assert_eq!(
        canonical(DataKind::MoleculeSet, &one),
        canonical(DataKind::MoleculeSet, &other),
        "and the very same bytes read as a set are one answer"
    );
}

#[test]
fn changing_only_the_representation_line_changes_what_the_file_means() {
    // The clause that would be a correctness bug rather than a cold cache.
    // A step key is hash(code, parameters, input content hashes, tool
    // versions) -- dimensions are not in it. These two files hold a
    // byte-identical InChIKey column and differ only in what was done to
    // produce it, so without the line in the content hash the cache would
    // serve one for the other.
    let (standardised, as_given) = (
        fixture("molecules_library.tsv"),
        fixture("molecules_library_as_given.tsv"),
    );
    assert_ne!(
        canonical(DataKind::MoleculeSet, &standardised),
        canonical(DataKind::MoleculeSet, &as_given)
    );
}

#[test]
fn a_file_that_is_not_a_molecule_table_hashes_as_its_own_bytes() {
    // `canon`'s rule, and the conservative direction: a file that does not
    // parse as its kind falls back to its bytes, tagged so the two can
    // never collide.
    let (form, notes) = hashed(DataKind::MoleculeSet, &fixture("graph_targets.json"));
    assert_eq!(form, Form::Raw);
    assert!(notes.is_empty(), "{notes:#?}");
}

#[test]
fn one_file_read_as_two_kinds_gives_two_hashes() {
    // The kind is hashed into the stream, so a step annotated to return a
    // molecule set can never be served the cached output of one annotated
    // to return a gene list over the same bytes.
    assert_ne!(
        canonical(DataKind::MoleculeSet, &fixture("molecules_library.tsv")),
        canonical(DataKind::EntitySet, &fixture("molecules_library.tsv"))
    );
}

#[test]
fn the_molecule_kinds_say_what_their_canonical_forms_keep() {
    let set = canon::describe(DataKind::MoleculeSet).unwrap();
    assert!(set.contains("representation"), "{set}");
    let ranking = canon::describe(DataKind::MoleculeRanking).unwrap();
    assert!(ranking.contains("order"), "{ranking}");
}

// ------------------------------------------------------- the value type --

#[test]
fn a_representation_compares_its_identity_and_its_operations() {
    let keys = Representation::new(Identity::InChIKey, [Standardisation::SaltStripped]);
    let structures = Representation::new(Identity::Smiles, [Standardisation::SaltStripped]);
    assert_ne!(keys, structures, "the identity is part of the value");

    assert_eq!(
        Representation::new(
            Identity::InChIKey,
            [
                Standardisation::SaltStripped,
                Standardisation::ChargeNeutralised
            ]
        ),
        Representation::new(
            Identity::InChIKey,
            [
                Standardisation::ChargeNeutralised,
                Standardisation::SaltStripped
            ]
        ),
        "the operations are a set: the order they were listed in is not part \
         of the identity"
    );
    assert_ne!(
        Representation::new(Identity::InChIKey, [Standardisation::SaltStripped]),
        Representation::new(
            Identity::InChIKey,
            [
                Standardisation::SaltStripped,
                Standardisation::ChargeNeutralised
            ]
        ),
        "and salt-stripping alone is not salt-stripping and neutralisation: \
         `FragmentParent` leaves the carboxylate, so aspirin from a sodium \
         salt is `BSYNRYMUTXBXSQ-UHFFFAOYSA-M` where plain aspirin is `-N`"
    );
}

#[test]
fn a_representation_parses_what_a_file_spells() {
    assert_eq!(
        representation("inchikey; salt-stripped, charge-neutralised"),
        Representation::new(
            Identity::InChIKey,
            [
                Standardisation::SaltStripped,
                Standardisation::ChargeNeutralised
            ]
        )
    );
    assert_eq!(
        representation("inchikey; as given"),
        Representation::new(Identity::InChIKey, []),
        "`as given` is how a file spells no operations at all"
    );
    assert_eq!(
        representation("inchikey"),
        Representation::new(Identity::InChIKey, []),
        "and so is saying nothing after the identity"
    );
    assert_eq!(
        representation("inchikey; as given").to_string(),
        "InChIKey, as given"
    );
    assert!(
        Representation::parse("").is_err(),
        "a blank is not a representation: two blanks would compare equal"
    );
    assert!(
        Representation::parse("fingerprint; as given").is_err(),
        "and neither is an identity the engine cannot compute"
    );
}

// -------------------------------------------------------- the conversion --

#[test]
fn standardize_is_registered_across_representation() {
    // The third edge of the conversion graph, after `map_identifiers` and
    // before `liftover` and `rename_contigs`. It is registered because
    // `graph::propagate` guards its axis override on this list: a
    // `standardize` step missing from here would carry the *input's*
    // representation onto an output that has crossed.
    let entry = Conversion::by_name("standardize").expect("standardize is registered");
    assert_eq!(entry.axis, Dimension::Representation);
    for kind in MOLECULE_KINDS {
        assert!(entry.converts(kind), "{kind:?}");
    }
    assert_eq!(
        Conversion::across(Dimension::Representation).map(|found| found.name),
        Some("standardize")
    );

    // And its target is read from the one spelling every conversion reads.
    assert!(
        entry
            .target("inchikey; salt-stripped, charge-neutralised")
            .is_ok()
    );
    assert!(
        entry.target("").is_err(),
        "a conversion that accepted a blank would produce two outputs that \
         agree about being in no particular state"
    );
}

#[test]
fn stereo_discarded_is_the_only_difference_this_page_is_about() {
    // The fixture pair behind the silent-failures page, pinned so the page
    // and the engine cannot drift apart. Both libraries hold the same four
    // drugs, salt-stripped and neutralised; one of them was also tautomer
    // normalised, so ibuprofen's stereo layer is gone and its key is
    // `HEFNNWSXXWATRW-UHFFFAOYSA-N` instead of `...-JTQLQIEISA-N`.
    //
    // This is the dangerous direction: two enantiomers collapse onto one
    // name, so a join matches a pair that is not a pair.
    let errors = check_step(
        "score",
        &[
            bind("library", molecules("molecules_library.tsv")),
            bind("hits", molecules("molecules_library_stereo_lost.tsv")),
        ],
    );
    let found: Vec<_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::Representation))
        .collect();
    assert_eq!(found.len(), 1, "got: {errors:#?}");

    let said = found[0].to_string();
    assert!(
        said.contains("hits also lists stereo-discarded"),
        "the one operation that differs is the only part worth reading: {said}"
    );
}
