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
            "identifier_namespace",
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

// M7 added these three axes with a kind for `release` alone, and asserted
// here that nothing else claimed any of them. M8 gives `release` a second
// kind and `subject` its first: a gene list has a release (which HGNC
// download named these genes) and a subject (what the list is about). Only
// `source_set` is still unclaimed, and the obligation to re-examine its
// deduplication before a kind takes it stays with M10 and M12 (D36).
#[test]
fn release_applies_to_a_reference_and_to_a_gene_list() {
    for kind in DataKind::ALL {
        assert_eq!(
            kind.applies(Dimension::Release),
            matches!(kind, DataKind::Reference | DataKind::EntitySet),
            "{kind:?}"
        );
        assert_eq!(
            kind.applies(Dimension::Subject),
            kind == DataKind::EntitySet,
            "{kind:?}"
        );
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

// A release known on one side only is silence, not a demand: nothing in
// `required_pair` names it, and a FASTA that records no release must stay
// loadable.
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

#[test]
fn each_new_axis_reads_its_own_field() {
    use ligase::dim::{SourceSet, Subject};

    let dimensions = Dimensions {
        release: Dim::known(Release::new("GRCh38.p14"), Source::Declared),
        subject: Dim::known(
            Subject::new("HeLa"),
            Source::Derived {
                step: "load".to_owned(),
            },
        ),
        source_set: Dim::<SourceSet>::unknown("no sources were recorded"),
        ..Dimensions::default()
    };

    assert_eq!(
        dimensions.render(Dimension::Release).as_deref(),
        Some("GRCh38.p14")
    );
    assert_eq!(
        dimensions.render(Dimension::Subject).as_deref(),
        Some("HeLa")
    );
    assert_eq!(dimensions.render(Dimension::SourceSet), None);

    assert_eq!(
        dimensions.source(Dimension::Release),
        Some(&Source::Declared)
    );
    assert_eq!(
        dimensions
            .source(Dimension::Subject)
            .map(Source::describe)
            .as_deref(),
        Some("from step `load`")
    );
    assert_eq!(dimensions.source(Dimension::SourceSet), None);

    assert_eq!(dimensions.reason(Dimension::Release), None);
    assert_eq!(dimensions.reason(Dimension::Subject), None);
    assert_eq!(
        dimensions.reason(Dimension::SourceSet),
        Some("no sources were recorded")
    );
}
