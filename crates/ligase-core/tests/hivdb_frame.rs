//! What happens when the reference is not the one the gene windows mean,
//! and when a gene was barely read.
//!
//! `codon::GENES` states protease, reverse transcriptase, integrase and
//! capsid as nucleotide ranges of HXB2 K03455 — whole-genome coordinates,
//! counted from the first base of the genome. Nothing used to check that
//! the reference handed to a reading was in that frame.
//!
//! SAWA, interpreting the same virus from pol-relative sequence, found what
//! that costs. Its reference spans K03455 2085..5096, so every position in
//! it is 2084 lower than the engine assumes. The window the engine calls
//! protease then lands in integrase, and the reading came back with a
//! protease it never saw, scored, at a clinical level, with no error and no
//! note. The same evaluation found the other half of this file: a sequence
//! covering 32% of integrase was reported as five susceptible integrase
//! inhibitors, where the gene was simply not there to assess.
//!
//! Both are the one thing a resistance report must never do, which is to
//! manufacture a clean answer out of data that cannot support one.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};

use ligase::dim::{Contig, Frame, ReferenceId};
use ligase::hivdb::codon::{DEPTH, GENE_COVERAGE, GENES, Gene, MIXTURE, read};
use ligase::hivdb::{Reading, Ruleset, frame, interpret};
use ligase::note::Note;
use tempfile::TempDir;

/// Where pol starts in K03455, 1-based: the offset a pol-relative
/// reference is out by, and the padding that puts it back.
const POL: u32 = 2085;

/// The gene a test is about, by name.
fn gene(name: &str) -> Gene {
    *GENES.iter().find(|gene| gene.name == name).unwrap()
}

/// A reference of `bases` bases, named as HXB2.
///
/// The background is `A`, which translates to lysine — a real amino acid,
/// so nothing here turns on an unreadable base. `planted` puts a chosen
/// triplet at a chosen 1-based position; `masked` fills a range with `N`,
/// which is what a sequence that did not cover a region actually holds.
fn reference(
    directory: &Path,
    bases: u32,
    planted: &[(u32, &str)],
    masked: &[Range<usize>],
) -> PathBuf {
    let mut sequence = vec![b'A'; bases as usize];
    for range in masked {
        sequence[range.clone()].fill(b'N');
    }
    for (position, triplet) in planted {
        let first = *position as usize - 1;
        sequence[first..first + 3].copy_from_slice(triplet.as_bytes());
    }

    let path = directory.join("reference.fasta");
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(file, ">K03455 HIV-1").unwrap();
    for line in sequence.chunks(60) {
        file.write_all(line).unwrap();
        writeln!(file).unwrap();
    }
    path
}

/// A VCF with no records: every position agrees with the reference.
fn no_variants(directory: &Path) -> PathBuf {
    let path = directory.join("calls.vcf");
    std::fs::write(
        &path,
        "##fileformat=VCFv4.2\n\
         ##contig=<ID=K03455>\n\
         #CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tsample\n",
    )
    .unwrap();
    path
}

/// The algorithm these tests read against.
fn ruleset() -> Ruleset {
    Ruleset::vendored("HIVDB_9.8").unwrap()
}

/// Read a reference with no calls, no alignment and the default policy.
fn reading(path: &Path, directory: &Path) -> Result<Reading, String> {
    read(
        path,
        &no_variants(directory),
        None,
        &ruleset(),
        MIXTURE,
        DEPTH,
        GENE_COVERAGE,
    )
    .map_err(|error| error.to_string())
}

/// What the algorithm says about one drug.
fn call(reading: &Reading, drug: &str) -> String {
    let ruleset = ruleset();
    interpret(&ruleset, &reading.observed)
        .unwrap()
        .into_iter()
        .find(|assessment| assessment.drug == drug)
        .unwrap()
        .interpretation
}

/// A reference identity of one contig of `bases` bases, as a FASTA gives.
fn identity(bases: u64) -> ReferenceId {
    ReferenceId::new(
        None,
        vec![Contig {
            name: "K03455".to_owned(),
            length: Some(bases),
            md5: None,
        }],
    )
}

// ------------------------------------------------------- the frame -----

#[test]
fn a_pol_relative_reference_is_refused_rather_than_read_as_protease() {
    let directory = TempDir::new().unwrap();

    // SAWA's reference: K03455 2085..5096, so 3012 bases, named for the
    // accession it is a slice of.
    let length = 5096 - POL + 1;
    assert_eq!(length, 3012);

    // Methionine where the engine thinks protease codon 90 is. In a
    // pol-relative reference those three bases are K03455 4604..4606,
    // inside integrase; L90M is a protease-inhibitor mutation worth 20 to
    // 60 points on its own, and reading it as protease is how a report of
    // protease-inhibitor resistance came out of integrase.
    let ninety = gene("PR").base_of(90);
    assert_eq!(ninety + POL - 1, 4604);

    let path = reference(directory.path(), length, &[(ninety, "ATG")], &[]);
    let error = reading(&path, directory.path()).unwrap_err();

    // Both lengths, so the reader can see what is wrong without guessing.
    assert!(error.contains("3012 bases"), "{error}");
    assert!(error.contains("base 5093"), "{error}");
    // And what to do about it, including the padding that fixes it.
    assert!(error.contains("pol-relative"), "{error}");
    assert!(error.contains(r#""N" * 2084 + pol"#), "{error}");
    assert!(error.contains(r#"frame=("K03455", 1)"#), "{error}");
}

#[test]
fn the_padding_workaround_is_read() {
    let directory = TempDir::new().unwrap();

    // `"N" * 2084 + hxb2_pol`, named K03455: the sequence put back where
    // its own numbers mean what the gene windows say.
    let padding = 0..(POL as usize - 1);
    let padded = reference(directory.path(), 5096, &[], std::slice::from_ref(&padding));
    let reading = reading(&padded, directory.path()).unwrap();

    // Protease, reverse transcriptase and integrase are all in the pol
    // part, so all three are read.
    for name in ["PR", "RT", "IN"] {
        assert!(reading.observed.protein(name).is_some(), "{name}");
    }
    // Capsid is inside the padding, so there is nothing there to assess —
    // which is the honest answer for a pol amplicon, not a susceptible one.
    assert!(reading.observed.protein("CA").is_none());
    assert_eq!(call(&reading, "LEN"), "Not assessed");
}

#[test]
fn a_whole_genome_reference_needs_no_declaration() {
    // 9,719 bases is K03455, or something deliberately numbered along it.
    // Nothing to declare, so the demo's own reference keeps working.
    assert_eq!(frame::mismatch(&identity(frame::BASES)), None);
}

#[test]
fn a_declared_slice_is_refused_with_its_offset_named() {
    let sliced = identity(3012).with_frame(Some(Frame::new("K03455", POL.into())));
    let mismatch = frame::mismatch(&sliced).unwrap();

    assert_eq!(
        mismatch,
        frame::Mismatch::Sliced {
            frame: Frame::new("K03455", 2085),
        }
    );
    let detail = mismatch.detail();
    assert!(detail.contains("base 2085 of K03455"), "{detail}");
    assert!(detail.contains("2084 lower"), "{detail}");
}

#[test]
fn a_frame_that_is_named_differently_is_still_this_frame() {
    // A declaration is read, not used to establish identity: `HXB2` and
    // `K03455.1` say the same thing as `K03455`, and refusing them would
    // be pedantry rather than safety.
    for name in ["K03455", "K03455.1", "HXB2", "hxb2"] {
        let declared = identity(3012).with_frame(Some(Frame::new(name, 1)));
        assert_eq!(frame::mismatch(&declared), None, "{name}");
    }

    let elsewhere = identity(3012).with_frame(Some(Frame::new("NC_001802", 1)));
    assert!(matches!(
        frame::mismatch(&elsewhere),
        Some(frame::Mismatch::Elsewhere { .. })
    ));
}

#[test]
fn a_reference_that_is_neither_cannot_be_guessed_at() {
    // Long enough to hold the windows, but not the whole frame. A slice of
    // K03455 and a genome of something else look identical from outside,
    // so the engine asks.
    assert_eq!(
        frame::mismatch(&identity(6000)),
        Some(frame::Mismatch::Undeclared { bases: Some(6000) })
    );
}

// ---------------------------------------------------- the coverage -----

#[test]
fn a_gene_barely_read_is_not_assessed_rather_than_susceptible() {
    let directory = TempDir::new().unwrap();

    // 32% of integrase, which is what SAWA's OQ686066 covered. The rest is
    // `N`: a sequence that did not reach there, which is a different thing
    // from a sequence that agrees with the reference.
    let integrase = gene("IN");
    let covered = (f64::from(integrase.codons()) * 0.32) as u32;
    let unread = (integrase.base_of(covered + 1) as usize - 1)..(integrase.end as usize);

    let path = reference(directory.path(), frame::BASES as u32, &[], &[unread]);
    let reading = reading(&path, directory.path()).unwrap();

    // The gene is left out of the reading, so every integrase inhibitor
    // comes back unassessed rather than at a level.
    assert!(reading.observed.protein("IN").is_none());
    for drug in ["DTG", "RAL", "EVG", "BIC", "CAB"] {
        assert_eq!(call(&reading, drug), "Not assessed", "{drug}");
    }

    // And the reading says why, as a note the engine carries into the run
    // report, provenance, a bundle and the methods section (D32): what it
    // read, and what it would have had to read.
    // Five of the twenty-six integrase positions HIVDB 9.8 scores fall in
    // the 32% that was covered, where the floor asks for twenty-one.
    let note = Note::GeneBarelyRead {
        gene: "IN".to_owned(),
        read: 5,
        scored: 26,
        needed: 21,
    };
    assert!(reading.notes.contains(&note), "{:?}", reading.notes);
    assert!(
        note.to_string()
            .contains("was not assessed rather than scored"),
        "{note}"
    );
}

#[test]
fn a_gene_read_almost_in_full_is_still_scored() {
    let directory = TempDir::new().unwrap();

    // One dropped codon is not a reason to refuse a gene: the score is
    // reported as a floor and the unread position is listed, which is more
    // use than silence. Only a gene mostly unseen is refused.
    let integrase = gene("IN");
    let one = (integrase.base_of(7) as usize - 1)..(integrase.base_of(8) as usize - 1);

    let path = reference(directory.path(), frame::BASES as u32, &[], &[one]);
    let reading = reading(&path, directory.path()).unwrap();

    let protein = reading.observed.protein("IN").unwrap();
    assert_eq!(protein.read(), integrase.codons() as usize - 1);
    // Assessed, at whatever level this background earns. What matters is
    // that the gene was not withheld for one unread codon.
    assert_ne!(call(&reading, "DTG"), "Not assessed");
}

#[test]
fn an_algorithm_scores_only_a_fraction_of_a_genes_codons() {
    let ruleset = ruleset();

    // A gene's codons and the codons an algorithm scores are not the same
    // number, and for reverse transcriptase they are not close: 560 codons,
    // 40 of which any rule in HIVDB 9.8 names. The highest is 348, so a
    // reading that covered every position the rules look at would still
    // have read 62% of the gene.
    assert_eq!(ruleset.scored("RT").len(), 40);
    assert_eq!(ruleset.scored("RT").last().copied(), Some(348));
    assert_eq!(ruleset.scored("PR").len(), 25);
    assert_eq!(ruleset.scored("IN").len(), 26);
    assert_eq!(ruleset.scored("CA").len(), 8);

    // A gene no drug is scored against has nothing to be read: the floor
    // over it is zero rather than a share of a gene nothing would consult.
    assert!(ruleset.scored("gag").is_empty());
}

#[test]
fn a_partial_pol_record_is_assessed_rather_than_withheld() {
    let directory = TempDir::new().unwrap();

    // What a clinical genotype actually covers: protease, and reverse
    // transcriptase as far as the rules reach. Past RT 348 nothing in
    // HIVDB 9.8 is scored, so a record that stops there has read every
    // position any drug would consult — and 348 of 560 codons, which a
    // floor over the whole gene reads as a gene barely sequenced.
    let rt = gene("RT");
    let beyond = (rt.base_of(349) as usize - 1)..(rt.end as usize);

    let path = reference(directory.path(), frame::BASES as u32, &[], &[beyond]);
    let reading = reading(&path, directory.path()).unwrap();

    let protein = reading.observed.protein("RT").unwrap();
    assert_eq!(protein.read(), 348);
    for drug in ["3TC", "AZT", "TDF", "EFV", "NVP", "DOR"] {
        assert_ne!(call(&reading, drug), "Not assessed", "{drug}");
    }
    assert!(
        !reading
            .notes
            .iter()
            .any(|note| matches!(note, Note::GeneBarelyRead { gene, .. } if gene == "RT")),
        "{:?}",
        reading.notes
    );

    // Unread codons are still unread: the gene is assessed, and the report
    // says what it did not see rather than passing it off as covered.
    assert!(
        reading.notes.contains(&Note::CodonsUnread {
            gene: "RT".to_owned(),
            read: 348,
            codons: rt.codons(),
            depth: DEPTH,
        }),
        "{:?}",
        reading.notes
    );
}

#[test]
fn a_gene_whose_scored_codons_went_unread_is_not_assessed() {
    let directory = TempDir::new().unwrap();

    // The other direction, and the reason the floor is not simply lower:
    // 479 of reverse transcriptase's 560 codons is 86% of the gene, which
    // any share over the whole gene waves through. What the dropout took
    // is RT 40 to 120, where 21 of the 40 positions HIVDB 9.8 scores
    // live, leaving 19 against a floor of 32: the drugs that matter were
    // being scored against under half the rules that could have refuted
    // them.
    let rt = gene("RT");
    let middle = (rt.base_of(40) as usize - 1)..(rt.base_of(121) as usize - 1);

    let path = reference(directory.path(), frame::BASES as u32, &[], &[middle]);
    let reading = reading(&path, directory.path()).unwrap();

    assert!(reading.observed.protein("RT").is_none());
    for drug in ["3TC", "AZT", "TDF", "EFV", "NVP"] {
        assert_eq!(call(&reading, drug), "Not assessed", "{drug}");
    }
    assert!(
        reading.notes.contains(&Note::GeneBarelyRead {
            gene: "RT".to_owned(),
            read: 19,
            scored: 40,
            needed: 32,
        }),
        "{:?}",
        reading.notes
    );
}
