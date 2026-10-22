//! The `EntitySet` kind: the first that holds no sequence.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::check;
use ligase::dim::Dimension;
use ligase::types::DataKind;

#[test]
fn a_gene_list_has_no_coordinate_system() {
    // The category error `types.rs` warns about: a gene list has no
    // coordinates, no contigs, no pairing and no quality scores, and
    // demanding any of them would be a gap nobody can fill.
    for dimension in [
        Dimension::ContigNaming,
        Dimension::Coordinates,
        Dimension::Pairing,
        Dimension::QualityEncoding,
        Dimension::Reference,
        Dimension::SampleSet,
    ] {
        assert!(!DataKind::EntitySet.applies(dimension), "{dimension}");
    }
}

#[test]
fn a_gene_list_carries_organism_namespace_subject_and_release() {
    for dimension in [
        Dimension::Organism,
        Dimension::IdentifierNamespace,
        Dimension::Subject,
        Dimension::Release,
    ] {
        assert!(DataKind::EntitySet.applies(dimension), "{dimension}");
    }
}

#[test]
fn no_kind_but_an_entity_set_has_an_identifier_namespace() {
    for kind in DataKind::ALL {
        assert_eq!(
            kind.applies(Dimension::IdentifierNamespace),
            kind == DataKind::EntitySet,
            "{kind:?}"
        );
    }
}

#[test]
fn an_entity_set_is_the_only_kind_with_a_subject() {
    for kind in DataKind::ALL {
        assert_eq!(
            kind.applies(Dimension::Subject),
            kind == DataKind::EntitySet,
            "{kind:?}"
        );
    }
}

#[test]
fn source_set_still_has_no_kind() {
    // D36 reserves the right to split the set type for `source_set`, and
    // that obligation stays with M10 and M12: this milestone gives it no
    // kind, so the shared `NameSet` dedup cannot bite anything yet.
    for kind in DataKind::ALL {
        assert!(!kind.applies(Dimension::SourceSet), "{kind:?}");
    }
}

#[test]
fn two_gene_lists_must_both_say_which_space_they_are_in() {
    // A demand a user can always meet: declare it, or let the loader infer
    // it. Unknown on both sides would be two gene lists agreeing by silence.
    assert!(
        check::required_pair(DataKind::EntitySet, DataKind::EntitySet)
            .contains(&Dimension::IdentifierNamespace)
    );
}

#[test]
fn a_gene_list_demands_nothing_of_a_sequence_kind() {
    for kind in DataKind::ALL {
        if kind != DataKind::EntitySet {
            assert!(
                check::required_pair(DataKind::EntitySet, kind).is_empty(),
                "{kind:?}"
            );
            assert!(
                check::required_pair(kind, DataKind::EntitySet).is_empty(),
                "{kind:?}"
            );
        }
    }
}

#[test]
fn a_gene_list_round_trips_through_its_name() {
    assert_eq!(DataKind::EntitySet.name(), "EntitySet");
    assert_eq!(DataKind::from_name("EntitySet"), Some(DataKind::EntitySet));
}

#[test]
fn a_gene_list_is_written_as_a_table() {
    // Tools read a format off the name, so the suffix is not cosmetic.
    assert_eq!(ligase::exec::output_suffix(DataKind::EntitySet), ".tsv");
}

#[test]
fn a_dataset_carries_what_loading_it_noticed() {
    use ligase::dim::Dimensions;
    use ligase::note::Note;
    use ligase::types::Dataset;

    let noticed = Dataset::new(DataKind::EntitySet, None, Dimensions::default()).with_notes(vec![
        Note::IdentifierIsADate {
            identifier: "2-Sep".to_owned(),
        },
    ]);
    assert_eq!(noticed.notes().len(), 1);

    // And an ordinary dataset says nothing, so this channel stays silent
    // for every kind that has nothing to report.
    let quiet = Dataset::new(DataKind::Reference, None, Dimensions::default());
    assert!(quiet.notes().is_empty());
}
