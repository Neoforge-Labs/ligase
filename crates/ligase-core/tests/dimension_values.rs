//! The value types the dimension axes are built from.
//!
//! The set type is generic over how two members are compared and has no
//! default, so every test here names the axis whose rule it is testing:
//! `Organism` folds ASCII case and `SourceSet` does not (D36). The two
//! halves of that split are pinned in `knowledge_graph.rs`, which is the
//! milestone that applied the second axis; what is here is the algebra both
//! share.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::dim::{Label, Organism, Release, SourceSet};

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
        Organism::of(["HIV-1", "Homo sapiens"]),
        Organism::of(["Homo sapiens", "hiv-1"])
    );
}

#[test]
fn a_repeated_member_is_recorded_once_in_its_first_spelling() {
    let set = Organism::of(["HIV-1", "hiv-1"]);
    assert_eq!(set.as_slice(), ["HIV-1"]);
}

#[test]
fn a_blank_member_is_dropped() {
    assert_eq!(Organism::of(["HIV-1", "   ", ""]).as_slice(), ["HIV-1"]);
    assert!(Organism::of([] as [&str; 0]).is_empty());
}

#[test]
fn a_set_renders_as_an_english_list() {
    assert_eq!(
        Organism::of(["HIV-1", "Homo sapiens"]).to_string(),
        "HIV-1 and Homo sapiens"
    );
    assert_eq!(Organism::of(["a", "b", "c"]).to_string(), "a, b and c");
}

#[test]
fn difference_names_what_the_other_set_lacks() {
    let both = Organism::of(["HIV-1", "Homo sapiens"]);
    let one = Organism::of(["hiv-1"]);
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
        Organism::of(["HIV-1"]),
        Organism::of(["HIV-1", "Homo sapiens"])
    );
    assert_ne!(Organism::of(["HIV-1"]), Organism::of(["HIV-2"]));
    assert_eq!(Organism::of([] as [&str; 0]).to_string(), "");
}

#[test]
fn the_two_set_axes_share_their_algebra_and_not_their_rule() {
    // Both sets trim, drop blanks, keep insertion order and compare
    // without regard to order. What they do not share is what makes two
    // members one member, which is the whole of the split.
    assert_eq!(
        SourceSet::of(["  open-targets ", "", "disgenet"]).as_slice(),
        ["open-targets", "disgenet"]
    );
    assert_eq!(
        SourceSet::of(["open-targets", "disgenet"]),
        SourceSet::of(["disgenet", "open-targets"])
    );
    assert_eq!(
        SourceSet::of(["open-targets", "disgenet"]).to_string(),
        "open-targets and disgenet"
    );
    assert_eq!(
        SourceSet::of(["open-targets", "disgenet"]).difference(&SourceSet::of(["open-targets"])),
        ["disgenet"]
    );

    assert_eq!(Organism::of(["HIV-1"]), Organism::of(["hiv-1"]));
    assert_ne!(SourceSet::of(["GSE12345"]), SourceSet::of(["gse12345"]));
}
