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
//! mixture reduced to its majority, an uncovered position read as
//! wild-type, or a combination of bases no read carries reported as an
//! amino acid.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use ligase::hivdb::codon::{DEPTH, GENES, Gene, MIXTURE, read};
use ligase::hivdb::{Reading, Ruleset};
use ligase::note::Note;
use noodles::sam::alignment::io::Write as _;
use noodles::{bam, sam};
use tempfile::TempDir;

/// Assess a gene however little of it was read.
///
/// These tests are about what one codon comes out holding, and several of
/// them cover a single codon deliberately. The share of a gene that has to
/// be read before it is assessed at all is a separate judgement, tested in
/// `hivdb_frame.rs`; here it would only throw away the gene under test.
const ANY: f64 = 0.0;

/// The algorithm these tests read against.
///
/// Which one barely matters here: with [`ANY`] for the share, nothing in
/// these tests turns on which positions it scores. A reading needs one
/// because the share it is given is a share of them.
fn ruleset() -> Ruleset {
    Ruleset::vendored("HIVDB_9.8").unwrap()
}

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

/// A BAM of the given reads, each `(first reference position, the bases
/// it carries from there, how many copies of it to write)`.
///
/// Written through a SAM, for the reason `tests/common/mod.rs` gives: what
/// a test says about its reads is then text, rather than a binary this
/// test wrote and read back to itself.
fn alignment(directory: &Path, reads: &[(u32, &str, u32)]) -> PathBuf {
    let length = GENES.iter().map(|gene| gene.end).max().unwrap() + 200;
    let sam = directory.join("alignment.sam");
    let mut text = String::from("@HD\tVN:1.6\n");
    text.push_str(&format!("@SQ\tSN:K03455.1\tLN:{length}\n"));
    let mut serial = 0_u32;
    for (position, bases, copies) in reads {
        for _ in 0..*copies {
            serial += 1;
            text.push_str(&format!(
                "read{serial}\t0\tK03455.1\t{position}\t60\t{}M\t*\t0\t0\t{bases}\t{}\n",
                bases.len(),
                "I".repeat(bases.len()),
            ));
        }
    }
    std::fs::write(&sam, text).unwrap();

    let path = directory.join("alignment.bam");
    let mut reader = sam::io::Reader::new(BufReader::new(std::fs::File::open(&sam).unwrap()));
    let header = reader.read_header().unwrap();
    let mut writer = bam::io::Writer::new(std::fs::File::create(&path).unwrap());
    writer.write_alignment_header(&header).unwrap();
    for record in reader.records() {
        writer
            .write_alignment_record(&header, &record.unwrap())
            .unwrap();
    }
    writer.try_finish().unwrap();
    path
}

/// One reading of a planted reference, its calls and, where given, its
/// reads.
fn reading(
    directory: &TempDir,
    planted: &[(&str, u32, &str)],
    records: &[(u32, &str, &str, u32, u32)],
    reads: Option<&[(u32, &str, u32)]>,
    mixture: f64,
) -> Reading {
    read(
        &reference(directory.path(), planted),
        &variants(directory.path(), records),
        reads
            .map(|reads| alignment(directory.path(), reads))
            .as_deref(),
        &ruleset(),
        mixture,
        DEPTH,
        ANY,
    )
    .unwrap()
}

/// What amino acids a codon came out holding.
fn held(reading: &Reading, name: &str, codon: u32) -> String {
    reading
        .observed
        .protein(name)
        .unwrap()
        .at(codon)
        .map(|acids| acids.to_string())
        .unwrap_or_default()
}

/// What amino acids a codon came out holding, with no alignment.
fn acids(
    directory: &TempDir,
    planted: &[(&str, u32, &str)],
    records: &[(u32, &str, &str, u32, u32)],
    name: &str,
    codon: u32,
) -> String {
    held(
        &reading(directory, planted, records, None, MIXTURE),
        name,
        codon,
    )
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
        &ruleset(),
        MIXTURE,
        DEPTH,
        ANY,
    )
    .unwrap();

    // `bcftools call -mv` writes only the sites it called a variant at,
    // so without coverage every uncalled position is being taken as
    // reference. That is a real assumption with a clinical direction —
    // it manufactures susceptible results out of dropouts — and the one
    // thing it must not be is silent.
    assert!(
        reading.notes.contains(&Note::ReadingWithoutCoverage),
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

/// The reads behind the discrepancy that motivated phasing, in miniature.
///
/// RT codon 219 begins at HXB2 3204. On SRR32217726 the caller credits G
/// there in 43% of the reads and at 3205 in 2.4%; both pass at 2%.
const K219: [(u32, &str, u32); 4] = [
    (3204, "AAA", 540),
    (3204, "GAA", 450),
    (3204, "AGA", 5),
    (3204, "GGA", 5),
];

/// The calls those reads produce: two ambiguous bases in one codon.
const TWO_AMBIGUOUS: [(u32, &str, &str, u32, u32); 2] =
    [(3204, "A", "G", 55, 45), (3205, "A", "G", 97, 3)];

#[test]
fn a_codon_is_read_from_the_combinations_reads_actually_carry() {
    let directory = TempDir::new().unwrap();

    // Without the reads there is nothing to say which substitution
    // travels with which, so every combination of the two ambiguous bases
    // is a candidate: AAA, AGA, GAA, GGA -- lysine, arginine, glutamate
    // and glycine. Two of the four are carried by no read at all, as the
    // alignment below says.
    assert_eq!(
        held(
            &reading(&directory, &[], &TWO_AMBIGUOUS, None, 0.02),
            "RT",
            219,
        ),
        "EGKR"
    );

    // With them, the codon holds what the virus holds. 54% of the reads
    // spanning it read AAA and 45% read GAA; AGA and GGA are half a per
    // cent each, which is a long way below the 2% an allele must reach.
    assert_eq!(
        held(
            &reading(&directory, &[], &TWO_AMBIGUOUS, Some(&K219), 0.02),
            "RT",
            219,
        ),
        "EK"
    );
}

#[test]
fn a_codon_no_read_spans_keeps_its_per_base_reading_and_says_so() {
    let directory = TempDir::new().unwrap();

    // Every base of codon 219 is covered a hundred and fifty times over,
    // and no single read spans all three of them -- which is what
    // paired-end reads do to a codon that falls between the mates.
    // Phasing is unavailable for exactly this codon.
    let split = [(3203, "AAA", 150), (3205, "AAA", 150)];
    let reading = reading(&directory, &[], &TWO_AMBIGUOUS, Some(&split), 0.02);

    // So the per-base reading stands. Dropping the codon instead would
    // discard evidence; narrowing it by guessing a phase would invent
    // one. D31 keeps the wider answer, which is the direction D29 commits
    // to -- and says so.
    assert_eq!(held(&reading, "RT", 219), "EGKR");
    assert!(
        reading
            .notes
            .iter()
            .any(|note| matches!(note, Note::CodonsUnphased { gene, .. } if gene == "RT")),
        "{:?}",
        reading.notes
    );
}

#[test]
fn phasing_narrows_a_reading_and_never_widens_one() {
    let directory = TempDir::new().unwrap();

    // The reads carry GCA -- alanine -- in a tenth of the reads spanning
    // the codon, and the caller credited no such allele at 3204. It is
    // not reported, because what counts as *present* at a position is
    // still the caller's own allele depths: they are quality-aware, and
    // raw read bases are not.
    //
    // This is Sample 30's V75A in miniature, and the reason that
    // discrepancy survives this change: at HXB2 2773 the reads carry the
    // alanine base in 3.2% of themselves, where the caller credits 1.7%.
    // Phasing decides which combinations of called alleles are real; it
    // is not a second opinion about which alleles there are.
    let reads = [(3204, "AAA", 900), (3204, "GCA", 100)];
    assert_eq!(
        held(
            &reading(&directory, &[], &[], Some(&reads), 0.02),
            "RT",
            219
        ),
        "K"
    );
}

#[test]
fn an_amino_acid_split_across_two_haplotypes_is_still_present() {
    let directory = TempDir::new().unwrap();

    // 3204 is 80% G and 3206 is 20% G, so the candidates are AAA and AAG
    // -- both lysine -- and GAA and GAG, both glutamate. The reads split
    // the lysine between its two synonymous codons: a tenth each.
    //
    // Lysine is a fifth of this virus. Thresholding each combination on
    // its own would lose it at 15%, and losing an amino acid that is
    // there is the one direction D29 forbids, so the threshold applies to
    // the amino acid rather than to the spelling of it.
    let records = [(3204, "A", "G", 20, 80), (3206, "A", "G", 80, 20)];
    let reads = [(3204, "AAA", 100), (3204, "AAG", 100), (3204, "GAA", 800)];
    assert_eq!(
        held(
            &reading(&directory, &[], &records, Some(&reads), MIXTURE),
            "RT",
            219,
        ),
        "EK"
    );
}
