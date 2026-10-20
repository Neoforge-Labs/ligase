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
//! That leaves one thing worth stating, because it looks like a hole and
//! is not. If some rule named the *wild-type* amino acid at its position,
//! Stanford — who has the whole sequence — would score it on nearly every
//! sample, while this engine, seeing only the listed mutations, would
//! score nothing. Those two numbers would differ. Across 3,348
//! expectations they never do, so no such rule fires for any of these
//! thirty samples.
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

/// How many of the published expectations are a score other than zero.
///
/// A fixture in which everything is susceptible would be reproduced by an
/// engine that always answered zero, so this is what makes the comparison
/// above worth making.
fn assessed(ruleset: &Ruleset, cases: &BTreeMap<String, Observed>) -> usize {
    cases
        .values()
        .filter_map(|observed| interpret(ruleset, observed).ok())
        .flatten()
        .filter(|assessment| assessment.score.is_some_and(|score| score != 0))
        .count()
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
    // A fixture of all-zero scores would pass whatever the engine did.
    let scored = assessed(&ruleset, &cases);
    assert!(
        scored > 100,
        "only {scored} expectations are non-zero; this fixture proves little"
    );
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

#[test]
fn the_engine_carries_its_own_rules() {
    // An installed wheel interprets without reading anything from disk,
    // so a run cannot depend on where a file was unpacked.
    for (name, _) in ligase::hivdb::ruleset::VENDORED {
        let ruleset = Ruleset::vendored(name).unwrap();
        assert!(
            name.ends_with(&ruleset.version),
            "{name} vs {}",
            ruleset.version
        );
    }

    let unknown = Ruleset::vendored("HIVDB_7.0").unwrap_err();
    assert!(unknown.contains("no vendored algorithm"), "{unknown}");
    // The error lists what there is, because a typo is the likely cause.
    assert!(unknown.contains("HIVDB_9.8"), "{unknown}");
    assert!(unknown.contains("HIVDB_10.2"), "{unknown}");
}

#[test]
fn an_embedded_ruleset_is_the_file_it_was_derived_from() {
    // The embedded copy and the committed one must not drift: the Python
    // suite re-derives the committed `.ruleset` from its XML, and this
    // ties what the engine actually uses to that same file.
    for (name, embedded) in ligase::hivdb::ruleset::VENDORED {
        let committed = std::fs::read_to_string(rulesets().join(format!("{name}.ruleset")))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            *embedded, committed,
            "{name} drifted from its committed file"
        );
    }
}

#[test]
fn the_rules_enter_a_cache_key_by_content_rather_than_by_name() {
    let fingerprint = ligase::hivdb::ruleset::fingerprint().unwrap();

    // Both algorithms are named, because the probe runs before any step
    // has said which it wants. Over-conservative on purpose (D20).
    assert!(fingerprint.contains("9.8 2025-01-05"), "{fingerprint}");
    assert!(fingerprint.contains("10.2 2026-04-26"), "{fingerprint}");
    // It says what is shipped, not what was used: this string lands in a
    // methods paragraph after "ran with", and a step that named 9.8 must
    // not appear to have run with 10.2.
    assert!(
        fingerprint.starts_with("HIVdb rules as shipped:"),
        "{fingerprint}"
    );

    // And each by the digest of its bytes. Upstream publishes corrections
    // under near-identical names -- HIVDB_10.0 and HIVDB_10.0-1 both
    // exist -- so a key that moved only with the name would serve a
    // result computed under rules that have since changed.
    assert_eq!(fingerprint.matches("blake3:").count(), 2, "{fingerprint}");

    // This is what a step declaring `tool="hivdb"` records, with no
    // binary of that name anywhere.
    assert_eq!(ligase::exec::tool_version("hivdb").unwrap(), fingerprint);
}

#[test]
fn the_algorithm_is_cited_like_any_other_software() {
    // A methods section names the software that produced a result. An
    // interpretation algorithm is software, and D24 forbids both guessing
    // a reference and quietly omitting one.
    let citation = ligase::prov::cite::KNOWN
        .iter()
        .find(|citation| citation.tool == "hivdb")
        .expect("the resistance algorithm needs a reference");

    assert!(citation.text.contains("Rhee SY"), "{}", citation.text);
    assert_eq!(citation.doi, "10.1093/nar/gkg100");
}
