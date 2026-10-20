//! M6: from variant calls to the amino acids a protein holds.
//!
//! The scoring engine is checked against Stanford's own answers in
//! `hivdb_scoring.rs`. What is checked here is everything before it — the
//! step where a position in a VCF becomes a codon, and a codon becomes an
//! amino acid, and a gap in coverage becomes something a report must not
//! paper over.
//!
//! These are the judgements that decide whether a resistance report is
//! honest, so each gets a test that would fail if it were made the other
//! way round: a codon assembled from one record rather than all of them, a
//! mixture reduced to its majority, or an uncovered position read as
//! wild-type.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::path::{Path, PathBuf};

use ligase::hivdb::codon::{DEPTH, GENES, Gene, MIXTURE, read};
use tempfile::TempDir;

/// The gene a test is about, by name.
fn gene(name: &str) -> Gene {
    *GENES.iter().find(|gene| gene.name == name).unwrap()
}

/// A reference long enough to hold every gene, with a chosen codon
/// planted in one of them.
///
/// Everything else is `AAA` — lysine — which is a real amino acid and so
/// exercises the same path as any other. What matters is that a planted
/// codon lands exactly where the gene's coordinates say it should.
fn reference(directory: &Path, planted: &[(&str, u32, &str)]) -> PathBuf {
    let end = GENES.iter().map(|gene| gene.end).max().unwrap() as usize;
    let mut bases = vec![b'A'; end + 200];

    for (name, codon, triplet) in planted {
        let first = gene(name).base_of(*codon) as usize - 1;
        bases[first..first + 3].copy_from_slice(triplet.as_bytes());
    }

    let path = directory.join("reference.fasta");
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(file, ">K03455.1 planted").unwrap();
    for line in bases.chunks(60) {
        file.write_all(line).unwrap();
        writeln!(file).unwrap();
    }
    path
}

/// A VCF of the given records, each `(position, reference, alternate,
/// reference depth, alternate depth)`.
fn variants(directory: &Path, records: &[(u32, &str, &str, u32, u32)]) -> PathBuf {
    let path = directory.join("calls.vcf");
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(file, "##fileformat=VCFv4.2").unwrap();
    writeln!(file, "##contig=<ID=K03455.1>").unwrap();
    writeln!(
        file,
        "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tsample"
    )
    .unwrap();
    for (position, reference, alternate, reference_depth, alternate_depth) in records {
        writeln!(
            file,
            "K03455.1\t{position}\t.\t{reference}\t{alternate}\t225\t.\t.\tGT:AD\t1:{reference_depth},{alternate_depth}"
        )
        .unwrap();
    }
    path
}

/// What amino acids a codon came out holding.
fn acids(
    directory: &TempDir,
    planted: &[(&str, u32, &str)],
    records: &[(u32, &str, &str, u32, u32)],
    name: &str,
    codon: u32,
) -> String {
    let reading = read(
        &reference(directory.path(), planted),
        &variants(directory.path(), records),
        None,
        MIXTURE,
        DEPTH,
    )
    .unwrap();
    reading
        .observed
        .protein(name)
        .unwrap()
        .at(codon)
        .map(|acids| acids.to_string())
        .unwrap_or_default()
}

#[test]
fn a_codon_is_translated_where_the_gene_says_it_is() {
    let directory = TempDir::new().unwrap();

    // ATG is methionine, at reverse transcriptase codon 184 — the
    // position of the commonest resistance mutation there is.
    assert_eq!(
        acids(&directory, &[("RT", 184, "ATG")], &[], "RT", 184),
        "M"
    );
    // And the standard HXB2 landmarks put protease 1 at 2253.
    assert_eq!(gene("PR").base_of(1), 2253);
    assert_eq!(gene("RT").base_of(1), 2550);
    assert_eq!(gene("IN").base_of(1), 4230);
    assert_eq!(gene("CA").base_of(1), 1186);
}

#[test]
fn a_codon_is_assembled_from_every_record_that_falls_in_it() {
    let directory = TempDir::new().unwrap();
    let first = gene("RT").base_of(151);

    // Reference CAG is glutamine. Two records in one codon, changing the
    // first and second bases together, make ATG — methionine, which is
    // Q151M, a multi-drug resistance mutation.
    //
    // Translating each record on its own would give two amino acids where
    // the virus has one, and neither of them M.
    let records = [(first, "C", "A", 0, 200), (first + 1, "A", "T", 0, 200)];
    assert_eq!(
        acids(&directory, &[("RT", 151, "CAG")], &records, "RT", 151),
        "M"
    );
}

#[test]
fn a_mixture_is_every_amino_acid_it_could_be() {
    let directory = TempDir::new().unwrap();
    let first = gene("RT").base_of(184);

    // Reference ATG is methionine. A to G in the second base gives GTG,
    // valine — M184V. At 60/40 both viruses are there, so the codon holds
    // both amino acids, which is what an ambiguous base means to
    // Stanford's own interpreter.
    let records = [(first + 1, "T", "T", 0, 0), (first, "A", "G", 60, 40)];
    assert_eq!(
        acids(&directory, &[("RT", 184, "ATG")], &records[1..], "RT", 184),
        "MV"
    );

    // Below the mixture threshold the minority base is not there at all.
    let faint = [(first, "A", "G", 97, 3)];
    assert_eq!(
        acids(&directory, &[("RT", 184, "ATG")], &faint, "RT", 184),
        "M"
    );
}

#[test]
fn a_stop_codon_is_a_stop_codon_rather_than_an_amino_acid() {
    let directory = TempDir::new().unwrap();

    // TAA in the middle of protease is a defective virus, not a resistant
    // one. No rule names `*`, so it can never satisfy one — which is the
    // right answer, reached by saying what is there rather than by
    // guessing something that is not.
    assert_eq!(acids(&directory, &[("PR", 30, "TAA")], &[], "PR", 30), "*");
}

#[test]
fn without_an_alignment_the_reading_says_what_it_assumed() {
    let directory = TempDir::new().unwrap();
    let reading = read(
        &reference(directory.path(), &[]),
        &variants(directory.path(), &[]),
        None,
        MIXTURE,
        DEPTH,
    )
    .unwrap();

    // `bcftools call -mv` writes only the sites it called a variant at,
    // so without coverage every uncalled position is being taken as
    // reference. That is a real assumption with a clinical direction —
    // it manufactures susceptible results out of dropouts — and the one
    // thing it must not be is silent.
    assert!(
        reading
            .notes
            .iter()
            .any(|note| note.contains("coverage is unknown")),
        "{:?}",
        reading.notes
    );

    // Every gene is still read, so every drug can still be assessed.
    for gene in GENES {
        assert!(
            reading.observed.protein(gene.name).is_some(),
            "{} was not read",
            gene.name
        );
    }
}

#[test]
fn the_genes_cover_every_position_the_rules_name() {
    // A rule about a position outside the window this engine reads would
    // never fire, and would do so silently — a susceptible result arrived
    // at by not looking. The highest positions HIVDB names are PR 90,
    // RT 348 and IN 263, and capsid is included because HIVDB 9.8 scores
    // lenacapavir against it.
    for (name, highest) in [("PR", 90), ("RT", 348), ("IN", 263), ("CA", 107)] {
        let gene = gene(name);
        assert!(
            gene.codons() >= highest,
            "{name} reads {} codons but a rule names {highest}",
            gene.codons()
        );
    }

    // Reverse transcriptase runs the full 560 codons rather than the 440
    // of the p51 subunit, because HIVdb numbers RT 1-560 and its
    // published mutation lists reach 558.
    assert_eq!(gene("RT").codons(), 560);
    assert_eq!(gene("PR").codons(), 99);
    // 288, not the 289 that 4230-5096 would give: pol's last
    // three bases are a stop codon, not an amino acid.
    assert_eq!(gene("IN").codons(), 288);
    assert_eq!(gene("CA").codons(), 231);
}
