//! The three axes M7 adds: release, subject and source set.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::check::{Binding, check_step};
use ligase::dim::{Dim, Dimension, Dimensions, Release, Source};
use ligase::types::{DataKind, Dataset};

fn reference_with(release: Dim<Release>) -> Dataset {
    Dataset::new(
        DataKind::Reference,
        Some("ref.fasta".into()),
        Dimensions {
            release,
            ..Dimensions::default()
        },
    )
}

fn bind(name: &str, dataset: Dataset) -> Binding {
    Binding {
        name: name.to_owned(),
        dataset,
    }
}

#[test]
fn the_three_axes_are_appended_so_earlier_reports_keep_their_order() {
    assert_eq!(
        Dimension::ALL.map(Dimension::name),
        [
            "organism",
            "reference",
            "contig_naming",
            "coordinates",
            "sample_set",
            "pairing",
            "quality_encoding",
            "release",
            "subject",
            "source_set",
        ]
    );
}

#[test]
fn every_dimension_round_trips_through_its_name_and_asks_a_question() {
    for dimension in Dimension::ALL {
        assert_eq!(Dimension::from_name(dimension.name()), Some(dimension));
        assert!(!dimension.leading_question().is_empty());
    }
}

#[test]
fn release_applies_to_a_reference_and_to_nothing_else() {
    assert!(DataKind::Reference.applies(Dimension::Release));
    for kind in DataKind::ALL {
        if kind != DataKind::Reference {
            assert!(!kind.applies(Dimension::Release), "{kind:?}");
        }
        assert!(!kind.applies(Dimension::Subject), "{kind:?}");
        assert!(!kind.applies(Dimension::SourceSet), "{kind:?}");
    }
}

#[test]
fn two_references_from_different_releases_disagree() {
    let errors = check_step(
        "score",
        &[
            bind(
                "a",
                reference_with(Dim::known(Release::new("GRCh38.p13"), Source::Declared)),
            ),
            bind(
                "b",
                reference_with(Dim::known(Release::new("GRCh38.p14"), Source::Declared)),
            ),
        ],
    );
    let found: Vec<_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::Release))
        .collect();
    assert_eq!(found.len(), 1, "got: {errors:#?}");
    assert!(found[0].to_string().contains("different release"));
}

#[test]
fn two_references_from_the_same_release_agree() {
    let errors = check_step(
        "score",
        &[
            bind(
                "a",
                reference_with(Dim::known(Release::new("GRCh38.p14"), Source::Declared)),
            ),
            bind(
                "b",
                reference_with(Dim::known(Release::new("GRCh38.p14"), Source::Declared)),
            ),
        ],
    );
    assert!(
        errors
            .iter()
            .all(|error| error.dimension() != Some(Dimension::Release)),
        "got: {errors:#?}"
    );
}

// Review Focus 4: a release on one side only is silence, not a demand.
#[test]
fn a_release_known_on_one_side_only_is_not_required() {
    let errors = check_step(
        "score",
        &[
            bind(
                "a",
                reference_with(Dim::known(Release::new("GRCh38.p14"), Source::Declared)),
            ),
            bind("b", reference_with(Dim::unknown("not declared"))),
        ],
    );
    assert!(
        errors
            .iter()
            .all(|error| error.dimension() != Some(Dimension::Release)),
        "got: {errors:#?}"
    );
}
