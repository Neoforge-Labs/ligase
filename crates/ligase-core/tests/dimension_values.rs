//! The value types the dimension axes are built from.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::dim::{Label, NameSet, Organism, Release};

#[test]
fn a_single_name_renders_as_itself_trimmed() {
    assert_eq!(Organism::new("  HIV-1 ").to_string(), "HIV-1");
}

#[test]
fn members_are_compared_ignoring_case_and_surrounding_space() {
    assert_eq!(Organism::new("  HIV-1 "), Organism::new("hiv-1"));
}

#[test]
fn order_does_not_affect_equality() {
    assert_eq!(
        NameSet::of(["HIV-1", "Homo sapiens"]),
        NameSet::of(["Homo sapiens", "hiv-1"])
    );
}

#[test]
fn a_repeated_member_is_recorded_once_in_its_first_spelling() {
    let set = NameSet::of(["HIV-1", "hiv-1"]);
    assert_eq!(set.as_slice(), ["HIV-1"]);
}

#[test]
fn a_blank_member_is_dropped() {
    assert_eq!(NameSet::of(["HIV-1", "   ", ""]).as_slice(), ["HIV-1"]);
    assert!(NameSet::of([] as [&str; 0]).is_empty());
}

#[test]
fn a_set_renders_as_an_english_list() {
    assert_eq!(
        NameSet::of(["HIV-1", "Homo sapiens"]).to_string(),
        "HIV-1 and Homo sapiens"
    );
    assert_eq!(NameSet::of(["a", "b", "c"]).to_string(), "a, b and c");
}

#[test]
fn difference_names_what_the_other_set_lacks() {
    let both = NameSet::of(["HIV-1", "Homo sapiens"]);
    let one = NameSet::of(["hiv-1"]);
    assert_eq!(both.difference(&one), ["Homo sapiens"]);
    assert!(one.difference(&both).is_empty());
}

#[test]
fn a_label_compares_exactly_after_trimming() {
    assert_eq!(Release::new(" HGNC 2026-09 "), Release::new("HGNC 2026-09"));
    assert_ne!(Release::new("hgnc 2026-09"), Release::new("HGNC 2026-09"));
}

#[test]
fn a_blank_label_is_empty() {
    assert!(Label::new("   ").is_empty());
    assert!(!Label::new("GRCh38.p14").is_empty());
}

#[test]
fn sets_of_different_sizes_are_not_equal() {
    assert_ne!(
        NameSet::of(["HIV-1"]),
        NameSet::of(["HIV-1", "Homo sapiens"])
    );
    assert_ne!(NameSet::of(["HIV-1"]), NameSet::of(["HIV-2"]));
    assert_eq!(NameSet::of([] as [&str; 0]).to_string(), "");
}
