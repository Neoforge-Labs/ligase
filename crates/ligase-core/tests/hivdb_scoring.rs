//! M6: the scoring engine, against Stanford's own answers.
//!
//! The engine reads `rulesets/HIVDB_9.8.ruleset`, which is derived from
//! `HIVDB_9.8.xml`. Expectations derived from that same XML would test the
//! parser against itself and prove nothing, so the oracle here comes from
//! somewhere else entirely: the Stanford HIVdb reports published as
//! supplementary data by the HIVGenoPipe paper, produced by Sierra 3.5.3 —
//! a different implementation, in a different language, by the algorithm's
//! own authors.
//!
//! `tests/fixtures/hivdb/` holds 124 cases: thirty clinical samples at
//! four ambiguity thresholds, plus controls. Each case is a list of amino
//! acids present at each position, and 27 expected (score, level) pairs.
//! 3,348 expectations in all.
//!
//! # What the fixture does and does not say
//!
//! The published mutation lists name only positions that differ from the
//! subtype-B consensus. Every other position was sequenced and found
//! wild-type, but the fixture does not say so, and this engine treats a
//! position it was not told about as *unread*.
//!
//! That distinction cannot move a score. An unread position makes an item
//! undetermined, which contributes nothing; a read wild-type position
//! refutes the item, which also contributes nothing. So the scores and
//! levels below are exact comparisons. What it does change is the
//! `unread` list, which is why these tests do not assert on it — the
//! tests that do are the ones that construct coverage deliberately.
//!
//! The one way that could hide a bug is a rule naming the wild-type amino
//! acid at some position: every sample would then match it, and the
//! fixture could not tell us. `hivdb_codons.rs` closes that by checking no
//! such rule exists.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ligase::hivdb::ruleset::Acids;
use ligase::hivdb::{Observed, Protein, Ruleset, interpret};

/// Where the vendored algorithms live.
fn rulesets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("rulesets")
}

/// Where the published expectations live.
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("hivdb")
}

/// Read a fixture, dropping its comment header.
fn rows(name: &str) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(fixtures().join(name)).unwrap();
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .skip(1)
        .map(|line| line.split('\t').map(ToOwned::to_owned).collect())
        .collect()
}

/// Every published case, as an observation the engine can score.
///
/// Every gene is supplied for every case, whether or not the case has a
/// mutation in it. Each of these samples was sequenced across protease,
/// reverse transcriptase, integrase and capsid; a gene with no scored
/// mutation is a gene that was read and found wild-type, which is not the
/// same as a gene nobody sequenced. Leaving it out would have the engine
/// report "not assessed" — correctly, for an observation the fixture had
/// misdescribed.
fn cases(genes: &[&str]) -> BTreeMap<String, Observed> {
    let mut proteins: BTreeMap<(String, String), Protein> = BTreeMap::new();
    for row in rows("cases.tsv") {
        let key = (row[0].clone(), row[1].clone());
        let protein = proteins.entry(key).or_default();
        protein.saw(row[2].parse().unwrap(), Acids::parse(&row[3]).unwrap());
    }

    let mut cases: BTreeMap<String, Observed> = BTreeMap::new();
    for ((case, gene), protein) in proteins {
        let observed = cases.entry(case).or_insert_with(|| {
            let mut supplied = Observed::new();
            for gene in genes {
                supplied.with(gene, Protein::new());
            }
            supplied
        });
        observed.with(&gene, protein);
    }
    cases
}

#[test]
fn the_vendored_algorithm_reads() {
    let ruleset = Ruleset::read(&rulesets().join("HIVDB_9.8.ruleset")).unwrap();

    assert_eq!(ruleset.algorithm, "HIVDB");
    assert_eq!(ruleset.version, "9.8");
    assert_eq!(ruleset.date, "2025-01-05");
    assert_eq!(ruleset.describe(), "HIVDB 9.8 (2025-01-05)");
    assert_eq!(ruleset.drugs.len(), 27);
    assert_eq!(ruleset.genes(), vec!["RT", "PR", "IN", "CA"]);
}

#[test]
fn the_current_algorithm_reads_too() {
    let ruleset = Ruleset::read(&rulesets().join("HIVDB_10.2.ruleset")).unwrap();

    assert_eq!(ruleset.version, "10.2");
    assert_eq!(ruleset.drugs.len(), 28);
}

#[test]
fn the_global_range_maps_scores_to_levels() {
    let ruleset = Ruleset::read(&rulesets().join("HIVDB_9.8.ruleset")).unwrap();

    // The boundaries, and a negative score, which is a real HIVdb result
    // and must not be clamped: M184V alone scores -10 for zidovudine.
    for (score, order, wording) in [
        (-10, 1, "Susceptible"),
        (9, 1, "Susceptible"),
        (10, 2, "Potential Low-Level Resistance"),
        (14, 2, "Potential Low-Level Resistance"),
        (15, 3, "Low-Level Resistance"),
        (29, 3, "Low-Level Resistance"),
        (30, 4, "Intermediate Resistance"),
        (59, 4, "Intermediate Resistance"),
        (60, 5, "High-Level Resistance"),
        (500, 5, "High-Level Resistance"),
    ] {
        let level = ruleset.level(score).unwrap();
        assert_eq!(level.order, order, "score {score}");
        assert_eq!(level.name, wording, "score {score}");
    }
}

#[test]
fn every_published_expectation_is_reproduced() {
    let ruleset = Ruleset::read(&rulesets().join("HIVDB_9.8.ruleset")).unwrap();
    let cases = cases(&ruleset.genes());

    let mut checked = 0;
    let mut wrong = Vec::new();

    for row in rows("expected.tsv") {
        let (case, drug) = (&row[0], &row[1]);
        let expected_score: i32 = row[2].parse().unwrap();
        let expected_level: u8 = row[3].parse().unwrap();

        // A case with no mutation at any scored position is still a case:
        // every drug should come back susceptible with a score of zero.
        let observed = cases.get(case).cloned().unwrap_or_else(|| {
            let mut empty = Observed::new();
            for gene in ruleset.genes() {
                empty.with(gene, Protein::new());
            }
            empty
        });

        let assessments = interpret(&ruleset, &observed).unwrap();
        let assessment = assessments
            .iter()
            .find(|assessment| &assessment.drug == drug)
            .unwrap_or_else(|| panic!("{case}: no assessment for {drug}"));

        checked += 1;
        if assessment.score != Some(expected_score) || assessment.level != Some(expected_level) {
            wrong.push(format!(
                "{case} {drug}: got {:?}/{:?}, Stanford says {expected_score}/{expected_level}",
                assessment.score, assessment.level
            ));
        }
    }

    assert!(checked > 3000, "only {checked} expectations were checked");
    assert!(
        wrong.is_empty(),
        "{} of {checked} disagree with Stanford:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

#[test]
fn a_mixture_satisfies_a_rule_that_any_of_its_amino_acids_satisfies() {
    let ruleset = Ruleset::read(&rulesets().join("HIVDB_9.8.ruleset")).unwrap();

    // RT K219KE is a K/E mixture: the reference amino acid and a
    // resistance one together. HIVdb scores it as the resistance one,
    // because the virus carrying it is there whatever else is.
    let mut reverse_transcriptase = Protein::new();
    reverse_transcriptase.saw(219, Acids::parse("KE").unwrap());
    let mut observed = Observed::new();
    observed.with("RT", reverse_transcriptase);

    let assessments = interpret(&ruleset, &observed).unwrap();
    let zidovudine = assessments
        .iter()
        .find(|assessment| assessment.drug == "AZT")
        .unwrap();

    assert_eq!(zidovudine.score, Some(10));
    assert_eq!(zidovudine.interpretation, "Potential Low-Level Resistance");
}

#[test]
fn a_gene_that_was_not_supplied_is_not_assessed() {
    let ruleset = Ruleset::read(&rulesets().join("HIVDB_9.8.ruleset")).unwrap();

    // A pol pipeline supplies protease, reverse transcriptase and
    // integrase. HIVDB 9.8 also scores lenacapavir against capsid, which
    // is in gag. Reporting it susceptible would be a negative result
    // manufactured out of a gene nobody sequenced.
    let mut observed = Observed::new();
    for gene in ["PR", "RT", "IN"] {
        observed.with(gene, Protein::new());
    }

    let assessments = interpret(&ruleset, &observed).unwrap();
    let lenacapavir = assessments
        .iter()
        .find(|assessment| assessment.drug == "LEN")
        .unwrap();

    assert_eq!(lenacapavir.score, None);
    assert_eq!(lenacapavir.level, None);
    assert_eq!(lenacapavir.interpretation, "Not assessed");

    // Everything scored against a gene that was supplied still answers.
    let lamivudine = assessments
        .iter()
        .find(|assessment| assessment.drug == "3TC")
        .unwrap();
    assert_eq!(lamivudine.score, Some(0));
    assert_eq!(lamivudine.interpretation, "Susceptible");
}

#[test]
fn an_unread_position_is_reported_rather_than_assumed_wild_type() {
    let ruleset = Ruleset::read(&rulesets().join("HIVDB_9.8.ruleset")).unwrap();

    // Reverse transcriptase supplied, but position 184 never covered.
    // Nothing is claimed about M184V either way; the score stands as a
    // floor and says which position it could not see.
    let mut reverse_transcriptase = Protein::new();
    reverse_transcriptase.saw(103, Acids::parse("K").unwrap());
    let mut observed = Observed::new();
    observed.with("RT", reverse_transcriptase);

    let assessments = interpret(&ruleset, &observed).unwrap();
    let lamivudine = assessments
        .iter()
        .find(|assessment| assessment.drug == "3TC")
        .unwrap();

    assert_eq!(lamivudine.score, Some(0));
    assert!(!lamivudine.is_complete());
    assert!(
        lamivudine.unread.contains(&184),
        "184 should be reported unread, got {:?}",
        lamivudine.unread
    );
}

#[test]
fn a_position_read_and_wild_type_is_not_reported_unread() {
    let ruleset = Ruleset::read(&rulesets().join("HIVDB_9.8.ruleset")).unwrap();

    // Every position 3TC's rules mention, read and holding something the
    // rules do not name. Nothing is uncertain, so nothing is reported.
    let lamivudine_positions: Vec<u32> = {
        let ruleset = Ruleset::read(&rulesets().join("HIVDB_9.8.ruleset")).unwrap();
        let drug = ruleset
            .drugs
            .iter()
            .find(|drug| drug.abbreviation == "3TC")
            .unwrap();
        let mut positions: Vec<u32> = drug
            .groups
            .iter()
            .flatten()
            .flat_map(|item| item.residues.iter().map(|residue| residue.position))
            .collect();
        positions.sort_unstable();
        positions.dedup();
        positions
    };

    let mut reverse_transcriptase = Protein::new();
    for position in lamivudine_positions {
        // `X` is in no rule in either vendored algorithm: it is the
        // ambiguity code for "any amino acid", which a rule never names.
        reverse_transcriptase.saw(position, Acids::parse("X").unwrap());
    }
    let mut observed = Observed::new();
    observed.with("RT", reverse_transcriptase);

    let assessments = interpret(&ruleset, &observed).unwrap();
    let lamivudine = assessments
        .iter()
        .find(|assessment| assessment.drug == "3TC")
        .unwrap();

    assert_eq!(lamivudine.score, Some(0));
    assert!(
        lamivudine.is_complete(),
        "nothing was unread, but {:?} was reported",
        lamivudine.unread
    );
}

#[test]
fn a_ruleset_this_engine_does_not_understand_is_refused() {
    for (text, expected) in [
        (
            "algorithm\tHIVDB\t9.8\t2025-01-05\n",
            "no `#ligase-hivdb-ruleset`",
        ),
        ("#ligase-hivdb-ruleset 99\n", "ruleset format 99"),
        ("#ligase-hivdb-ruleset 1\nnonsense\tfoo\n", "unknown record"),
        ("#ligase-hivdb-ruleset 1\n", "no `algorithm` record"),
    ] {
        let error = Ruleset::parse(text).unwrap_err();
        assert!(
            error.contains(expected),
            "expected {expected:?} in {error:?}"
        );
    }
}
