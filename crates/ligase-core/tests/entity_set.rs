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
fn only_a_named_list_has_an_identifier_namespace() {
    // Two kinds, since M12: a gene list and the ranked list that is a gene
    // list which remembers its order. No sequence kind has a namespace,
    // because no sequence kind names entities.
    for kind in DataKind::ALL {
        assert_eq!(
            kind.applies(Dimension::IdentifierNamespace),
            matches!(kind, DataKind::EntitySet | DataKind::RankedList),
            "{kind:?}"
        );
    }
}

#[test]
fn only_a_named_list_has_a_subject() {
    for kind in DataKind::ALL {
        assert_eq!(
            kind.applies(Dimension::Subject),
            matches!(kind, DataKind::EntitySet | DataKind::RankedList),
            "{kind:?}"
        );
    }
}

#[test]
fn source_set_still_has_no_kind() {
    // D36 reserves the right to split the set type for `source_set` before
    // any kind applies it. M8 gave it none; M12 settled that a ranked list
    // gives it none either, since spec §4.2 grants it four dimensions and
    // that is not among them. So the shared `NameSet` dedup still cannot
    // bite anything, and the question passes on with the kind that needs it.
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
    // A ranked list is not a sequence kind: it is the other named list, and
    // the namespace is demanded between the two of them. See
    // `ranked_list.rs::two_named_lists_must_both_say_which_space_they_are_in`.
    for kind in DataKind::ALL {
        if !matches!(kind, DataKind::EntitySet | DataKind::RankedList) {
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

// ------------------------------------------------------------- loading --

mod common;

use common::fixture;
use ligase::dim::Source;
use ligase::load::{self, Declared};

/// A declaration naming the organism and nothing else.
fn declared_human() -> Declared {
    Declared {
        organism: Some(vec!["Homo sapiens".to_owned()]),
        ..Declared::default()
    }
}

#[test]
fn a_namespace_is_inferred_from_the_identifiers_themselves() {
    for (file, expected) in [
        ("entities_hgnc.tsv", "HGNC symbol"),
        ("entities_ensembl.tsv", "Ensembl gene"),
        ("entities_entrez.tsv", "Entrez gene"),
        ("entities_uniprot.tsv", "UniProt accession"),
    ] {
        let loaded = load::entity_set(&fixture(file), None, &declared_human()).unwrap();
        let dimensions = loaded.dimensions();
        assert_eq!(
            dimensions.render(Dimension::IdentifierNamespace).as_deref(),
            Some(expected),
            "{file}"
        );
        // And it says where it got that from, which is what makes the
        // inference checkable rather than magic.
        let source = dimensions
            .source(Dimension::IdentifierNamespace)
            .map(Source::describe)
            .unwrap_or_default();
        assert!(
            source.contains("the identifiers' own pattern"),
            "{file}: {source}"
        );
    }
}

#[test]
fn a_file_that_mixes_two_namespaces_declines_to_infer() {
    let loaded = load::entity_set(&fixture("entities_mixed.tsv"), None, &declared_human()).unwrap();
    assert!(!loaded.dimensions().identifier_namespace.is_known());
    assert!(
        loaded
            .dimensions()
            .reason(Dimension::IdentifierNamespace)
            .is_some()
    );
}

#[test]
fn a_header_with_no_rows_declines_to_infer() {
    // Every pattern fits all zero identifiers, so a vacuous answer here
    // would declare a space for a file that names nothing.
    let loaded = load::entity_set(
        &fixture("entities_header_only.tsv"),
        None,
        &declared_human(),
    )
    .unwrap();
    assert!(!loaded.dimensions().identifier_namespace.is_known());
}

#[test]
fn the_loader_notices_a_symbol_excel_turned_into_a_date() {
    let loaded = load::entity_set(
        &fixture("entities_excel_dates.tsv"),
        None,
        &declared_human(),
    )
    .unwrap();
    let said: Vec<String> = loaded.notes().iter().map(ToString::to_string).collect();
    assert_eq!(loaded.notes().len(), 2, "{said:?}");
    assert!(said.iter().any(|note| note.contains("2-Sep")), "{said:?}");
    assert!(said.iter().any(|note| note.contains("1-Mar")), "{said:?}");
}

#[test]
fn a_carriage_return_is_not_part_of_an_identifier() {
    // Excel writes CRLF, and `TP53\r` would match no namespace at all.
    let loaded = load::entity_set(&fixture("entities_crlf.tsv"), None, &declared_human()).unwrap();
    assert_eq!(
        loaded
            .dimensions()
            .render(Dimension::IdentifierNamespace)
            .as_deref(),
        Some("HGNC symbol")
    );
    assert!(loaded.notes().is_empty());
}

#[test]
fn a_declared_namespace_wins_over_what_the_identifiers_look_like() {
    // A declaration is recorded, not checked: a user who says their list is
    // an in-house spelling is telling the engine something it cannot read
    // off the file, and `map_identifiers` is how they find out otherwise.
    let declared = Declared {
        identifier_namespace: Some("my curated list".to_owned()),
        ..declared_human()
    };
    let loaded = load::entity_set(&fixture("entities_hgnc.tsv"), None, &declared).unwrap();
    assert_eq!(
        loaded
            .dimensions()
            .render(Dimension::IdentifierNamespace)
            .as_deref(),
        Some("my curated list")
    );
    assert_eq!(
        loaded.dimensions().source(Dimension::IdentifierNamespace),
        Some(&Source::Declared)
    );
}

#[test]
fn a_blank_declared_namespace_is_a_gap_not_a_blank_label() {
    // A blank that compares equal to another blank is a false agreement.
    for blank in ["", "   "] {
        let declared = Declared {
            identifier_namespace: Some(blank.to_owned()),
            ..declared_human()
        };
        let loaded = load::entity_set(&fixture("entities_hgnc.tsv"), None, &declared).unwrap();
        assert_eq!(
            loaded
                .dimensions()
                .render(Dimension::IdentifierNamespace)
                .as_deref(),
            Some("HGNC symbol"),
            "{blank:?}"
        );
    }
}

#[test]
fn a_declared_subject_and_release_are_carried() {
    let declared = Declared {
        subject: Some("breast carcinoma".to_owned()),
        release: Some("HGNC 2026-09".to_owned()),
        ..declared_human()
    };
    let loaded = load::entity_set(&fixture("entities_hgnc.tsv"), None, &declared).unwrap();
    assert_eq!(
        loaded.dimensions().render(Dimension::Subject).as_deref(),
        Some("breast carcinoma")
    );
    assert_eq!(
        loaded.dimensions().render(Dimension::Release).as_deref(),
        Some("HGNC 2026-09")
    );
}

#[test]
fn the_identifier_column_can_be_named() {
    // The mapping table's own identifiers live under `id`; a file that
    // spells it differently says so rather than being guessed at.
    let loaded = load::entity_set(
        &fixture("map_hgnc_to_ensembl.tsv"),
        Some("id"),
        &declared_human(),
    )
    .unwrap();
    assert_eq!(
        loaded
            .dimensions()
            .render(Dimension::IdentifierNamespace)
            .as_deref(),
        Some("HGNC symbol")
    );
}

#[test]
fn a_file_with_no_such_column_says_which_columns_it_has() {
    let error = load::entity_set(
        &fixture("entities_hgnc.tsv"),
        Some("symbol"),
        &declared_human(),
    )
    .unwrap_err();
    let said = error.to_string();
    assert!(said.contains("symbol"), "{said}");
    assert!(said.contains("log2fc"), "{said}");
}

#[test]
fn a_file_with_carriage_return_line_endings_is_read_as_its_rows() {
    // Excel 2011 and classic Mac tools write CR alone. Read as one line,
    // a 400-gene list loads as an empty gene list and says nothing -- which
    // is data loss that every later check would be happy with.
    let loaded =
        load::entity_set(&fixture("entities_cr_only.tsv"), None, &declared_human()).unwrap();
    assert_eq!(
        loaded
            .dimensions()
            .render(Dimension::IdentifierNamespace)
            .as_deref(),
        Some("HGNC symbol")
    );
}

#[test]
fn a_file_whose_symbols_are_dates_says_so_where_the_user_is_looking() {
    // The reason an axis is unknown is what a user acts on. "declare
    // identifier_namespace" is the one piece of advice that buries this
    // file's problem: declare it and `SEPT7` is missing from every join
    // for good. So the reason names the cause and sends them to the notes.
    let loaded = load::entity_set(
        &fixture("entities_excel_dates.tsv"),
        None,
        &declared_human(),
    )
    .unwrap();
    let reason = loaded
        .dimensions()
        .reason(Dimension::IdentifierNamespace)
        .unwrap();
    assert!(reason.contains("date"), "{reason}");
    assert!(reason.contains("notes"), "{reason}");

    // A file that merely mixes two spaces still gets the other reason.
    let mixed = load::entity_set(&fixture("entities_mixed.tsv"), None, &declared_human()).unwrap();
    let other = mixed
        .dimensions()
        .reason(Dimension::IdentifierNamespace)
        .unwrap();
    assert!(!other.contains("date"), "{other}");
    assert!(other.contains("declare"), "{other}");
}
