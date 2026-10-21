//! M4: what a file means, as against what bytes it happens to hold.
//!
//! Every test here is the same shape: two files that differ, a claim about
//! whether the difference is biological, and the canonical hash agreeing. A
//! difference that is not biological must not move the hash, and one that is
//! must.
//!
//! The volatile fields are not hypothetical. `minimap2` writes its whole
//! command line into `@PG CL`, which includes the output path — and the
//! output path is the cache's staging file, named after the step's own key.
//! `bcftools` writes `##bcftools_callCommand=...; Date=Sat Oct 17 21:14:12
//! 2026`. Both are in the fixtures below as they come out of the tools.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use ligase::canon::{self, Form, Note, Tolerance};
use ligase::hash::ContentHash;
use ligase::types::DataKind;
use tempfile::TempDir;

mod common;

// ------------------------------------------------------------ scaffold --

/// A directory of files written for one test.
struct Files(TempDir);

impl Files {
    fn new() -> Self {
        Self(TempDir::new().unwrap())
    }

    /// A file holding exactly `contents`.
    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.path().join(name);
        fs::write(&path, contents).unwrap();
        path
    }
}

/// The canonical hash of a file read as `kind`.
fn canonical(kind: DataKind, path: &Path) -> ContentHash {
    let raw = ContentHash::of_file(path).unwrap();
    canon::hash(kind, path, raw).unwrap().hash
}

/// The canonical hash, and how it was arrived at.
fn form(kind: DataKind, path: &Path) -> Form {
    let raw = ContentHash::of_file(path).unwrap();
    canon::hash(kind, path, raw).unwrap().form
}

/// What canonicalising a file noticed, beyond its hash.
fn notes(kind: DataKind, path: &Path) -> Vec<Note> {
    let raw = ContentHash::of_file(path).unwrap();
    canon::hash(kind, path, raw).unwrap().notes
}

/// Assert that two files mean the same thing while holding different bytes.
///
/// Both halves matter: a canonicaliser that made every file hash the same
/// would pass the first half of every test here.
fn same_meaning(kind: DataKind, left: &Path, right: &Path) {
    assert_ne!(
        ContentHash::of_file(left).unwrap(),
        ContentHash::of_file(right).unwrap(),
        "these files hold identical bytes, so they test nothing: {} and {}",
        left.display(),
        right.display()
    );
    assert_eq!(
        canonical(kind, left),
        canonical(kind, right),
        "{} and {} should mean the same thing",
        left.display(),
        right.display()
    );
}

fn different_meaning(kind: DataKind, left: &Path, right: &Path) {
    assert_ne!(
        canonical(kind, left),
        canonical(kind, right),
        "{} and {} should not mean the same thing",
        left.display(),
        right.display()
    );
}

// ----------------------------------------------------------- alignment --

/// A header and two records, as the parts a test varies.
fn sam(header: &str, records: &[&str]) -> String {
    let mut text = header.to_owned();
    for record in records {
        text.push_str(record);
        text.push('\n');
    }
    text
}

const SQ: &str = "@HD\tVN:1.6\tSO:coordinate\n\
                  @SQ\tSN:1\tLN:20\tM5:a965a71aa3690f605935c54d320905ab\n\
                  @SQ\tSN:2\tLN:20\tM5:89c2d6e069f386bafb1a0c811ba821f4\n\
                  @RG\tID:rg0\tSM:sample_a\tPL:ILLUMINA\n";

const READ1: &str = "read1\t0\t1\t1\t60\t10M\t*\t0\t0\tACGTACGTAC\tIIIIIIIIII\tRG:Z:rg0";
const READ2: &str = "read2\t0\t1\t11\t60\t10M\t*\t0\t0\tTTTTCCCCGG\tIIIIIIIIII\tRG:Z:rg0";

#[test]
fn the_command_line_an_aligner_recorded_is_not_part_of_an_alignment() {
    // This is the acceptance criterion in miniature: the only thing a
    // different `-t` changes in a minimap2 BAM is this line.
    let files = Files::new();
    let one = files.write(
        "t1.sam",
        &sam(
            &format!(
                "{SQ}@PG\tID:minimap2\tPN:minimap2\tVN:2.31-r1302\tCL:minimap2 -t 1 -ax sr ref.fa reads.fq\n"
            ),
            &[READ1, READ2],
        ),
    );
    let eight = files.write(
        "t8.sam",
        &sam(
            &format!(
                "{SQ}@PG\tID:minimap2\tPN:minimap2\tVN:2.31-r1302\tCL:minimap2 -t 8 -ax sr ref.fa reads.fq\n"
            ),
            &[READ1, READ2],
        ),
    );

    same_meaning(DataKind::Alignment, &one, &eight);
}

#[test]
fn the_output_path_a_tool_recorded_is_not_part_of_an_alignment() {
    // The staging path holds the step's own key, so a `@PG CL` that keeps it
    // makes a step's output bytes depend on its own key -- and then no
    // downstream cutoff can ever happen.
    let files = Files::new();
    let first = files.write(
        "a.sam",
        &sam(
            &format!(
                "{SQ}@PG\tID:samtools\tPN:samtools\tCL:samtools sort -o .ligase/tmp/aaaa.bam\n"
            ),
            &[READ1],
        ),
    );
    let second = files.write(
        "b.sam",
        &sam(
            &format!(
                "{SQ}@PG\tID:samtools\tPN:samtools\tCL:samtools sort -o .ligase/tmp/bbbb.bam\n"
            ),
            &[READ1],
        ),
    );

    same_meaning(DataKind::Alignment, &first, &second);
}

#[test]
fn the_order_records_arrived_in_is_not_part_of_an_alignment() {
    let files = Files::new();
    let forwards = files.write("forwards.sam", &sam(SQ, &[READ1, READ2]));
    let backwards = files.write("backwards.sam", &sam(SQ, &[READ2, READ1]));

    same_meaning(DataKind::Alignment, &forwards, &backwards);
}

#[test]
fn a_sort_order_is_not_part_of_an_alignment() {
    // `@HD SO:` describes the file's layout, and the layout is exactly what
    // canonicalising removes.
    let files = Files::new();
    let sorted = files.write("sorted.sam", &sam(SQ, &[READ1]));
    let unsorted = files.write(
        "unsorted.sam",
        &sam(&SQ.replace("SO:coordinate", "SO:unsorted"), &[READ1]),
    );

    same_meaning(DataKind::Alignment, &sorted, &unsorted);
}

#[test]
fn a_changed_base_is_a_different_alignment() {
    let files = Files::new();
    let original = files.write("original.sam", &sam(SQ, &[READ1]));
    let mutated = files.write(
        "mutated.sam",
        &sam(SQ, &[&READ1.replace("ACGTACGTAC", "ACGTACGTAG")]),
    );

    different_meaning(DataKind::Alignment, &original, &mutated);
}

#[test]
fn a_missing_record_is_a_different_alignment() {
    // Hashing a sorted list of record digests must count duplicates and
    // absences, not merely the set of distinct records.
    let files = Files::new();
    let both = files.write("both.sam", &sam(SQ, &[READ1, READ2]));
    let one = files.write("one.sam", &sam(SQ, &[READ1]));
    let twice = files.write("twice.sam", &sam(SQ, &[READ1, READ1]));

    different_meaning(DataKind::Alignment, &both, &one);
    different_meaning(DataKind::Alignment, &one, &twice);
}

#[test]
fn which_reference_an_alignment_is_against_is_part_of_it() {
    let files = Files::new();
    let hxb2 = files.write("hxb2.sam", &sam(SQ, &[READ1]));
    let other = files.write(
        "other.sam",
        &sam(
            &SQ.replace(
                "a965a71aa3690f605935c54d320905ab",
                "ffffffffffffffffffffffffffffffff",
            ),
            &[READ1],
        ),
    );

    different_meaning(DataKind::Alignment, &hxb2, &other);
}

#[test]
fn the_order_of_a_sequence_dictionary_is_part_of_an_alignment() {
    // Decision D14: GATK requires two sequence dictionaries to agree on
    // order, so `@SQ` order is semantic where a BED's record order is not.
    let files = Files::new();
    let forwards = files.write("forwards.sam", &sam(SQ, &[READ1]));

    let mut lines: Vec<&str> = SQ.lines().collect();
    lines.swap(1, 2);
    let swapped_header = lines.join("\n") + "\n";
    let backwards = files.write("backwards.sam", &sam(&swapped_header, &[READ1]));

    different_meaning(DataKind::Alignment, &forwards, &backwards);
}

#[test]
fn which_sample_an_alignment_is_of_is_part_of_it() {
    let files = Files::new();
    let sample_a = files.write("a.sam", &sam(SQ, &[READ1]));
    let sample_b = files.write(
        "b.sam",
        &sam(&SQ.replace("SM:sample_a", "SM:sample_b"), &[READ1]),
    );

    different_meaning(DataKind::Alignment, &sample_a, &sample_b);
}

#[test]
fn a_bam_and_the_sam_it_came_from_are_the_same_alignment() {
    // Decision D9: re-encoding is not a biological change. The dimension
    // checker already takes that view; so must the hash, or `samtools view
    // -b` would invalidate a cache for nothing.
    let (_dir, bam) = common::bam_from_sam("tiny_bare.sam");

    assert_eq!(
        canonical(DataKind::Alignment, &common::fixture("tiny_bare.sam")),
        canonical(DataKind::Alignment, &bam),
    );
}

// ------------------------------------------------------------ variants --

fn vcf(header_extra: &str, records: &[&str]) -> String {
    vcf_of("sample_a", header_extra, records)
}

/// The same, with the sample column named by the caller.
fn vcf_of(sample: &str, header_extra: &str, records: &[&str]) -> String {
    let mut text = String::from("##fileformat=VCFv4.3\n");
    text.push_str(header_extra);
    text.push_str("##contig=<ID=1,length=20,md5=a965a71aa3690f605935c54d320905ab>\n");
    text.push_str(&format!(
        "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\t{sample}\n"
    ));
    for record in records {
        text.push_str(record);
        text.push('\n');
    }
    text
}

const VARIANT_5: &str = "1\t5\t.\tA\tG\t30\tPASS\t.\tGT\t0/1";
const VARIANT_9: &str = "1\t9\t.\tC\tT\t40\tPASS\t.\tGT\t0/1";

#[test]
fn a_command_line_and_its_date_are_not_part_of_a_vcf() {
    // Exactly what bcftools 1.24 writes, timestamp and staging path and all.
    let files = Files::new();
    let first = files.write(
        "first.vcf",
        &vcf(
            "##bcftools_callVersion=1.24+htslib-1.24\n\
             ##bcftools_callCommand=call -mv --ploidy 1 -Oz -o /tmp/aaaa.vcf.gz; Date=Sat Oct 17 21:14:12 2026\n",
            &[VARIANT_5],
        ),
    );
    let second = files.write(
        "second.vcf",
        &vcf(
            "##bcftools_callVersion=1.24+htslib-1.24\n\
             ##bcftools_callCommand=call -mv --ploidy 1 -Oz -o /tmp/bbbb.vcf.gz; Date=Sun Oct 18 09:02:55 2026\n",
            &[VARIANT_5],
        ),
    );

    same_meaning(DataKind::Variants, &first, &second);
}

#[test]
fn a_file_date_and_a_source_and_a_reference_path_are_not_part_of_a_vcf() {
    // `##reference` holds a path, and `web/content/reference/dimensions.mdx`
    // is explicit that reference identity never rests on a filename -- the
    // `##contig` lines carry it, and they are kept.
    let files = Files::new();
    let first = files.write(
        "first.vcf",
        &vcf(
            "##fileDate=20261017\n##source=freeBayes v1.3.6\n##reference=file:///refs/HXB2.fasta\n",
            &[VARIANT_5],
        ),
    );
    let second = files.write(
        "second.vcf",
        &vcf(
            "##fileDate=20261018\n##source=freeBayes v1.3.7\n##reference=file:///data/HXB2.fasta\n",
            &[VARIANT_5],
        ),
    );

    same_meaning(DataKind::Variants, &first, &second);
}

#[test]
fn the_order_records_arrived_in_is_not_part_of_a_vcf() {
    let files = Files::new();
    let forwards = files.write("forwards.vcf", &vcf("", &[VARIANT_5, VARIANT_9]));
    let backwards = files.write("backwards.vcf", &vcf("", &[VARIANT_9, VARIANT_5]));

    same_meaning(DataKind::Variants, &forwards, &backwards);
}

#[test]
fn which_contigs_a_vcf_names_is_part_of_it() {
    let files = Files::new();
    let original = files.write("original.vcf", &vcf("", &[VARIANT_5]));
    let other = files.write(
        "other.vcf",
        &vcf("", &[VARIANT_5]).replace(
            "a965a71aa3690f605935c54d320905ab",
            "ffffffffffffffffffffffffffffffff",
        ),
    );

    different_meaning(DataKind::Variants, &original, &other);
}

#[test]
fn which_sample_a_vcf_is_of_is_part_of_it() {
    let files = Files::new();
    let sample_a = files.write("a.vcf", &vcf("", &[VARIANT_5]));
    let sample_b = files.write(
        "b.vcf",
        &vcf("", &[VARIANT_5]).replace("sample_a", "sample_b"),
    );

    different_meaning(DataKind::Variants, &sample_a, &sample_b);
}

#[test]
fn a_changed_call_is_a_different_vcf() {
    let files = Files::new();
    let reference = files.write("ref.vcf", &vcf("", &[VARIANT_5]));
    let alternate = files.write(
        "alt.vcf",
        &vcf("", &[&VARIANT_5.replace("\tA\tG\t", "\tA\tT\t")]),
    );

    different_meaning(DataKind::Variants, &reference, &alternate);
}

// ----------------------------------------------------------- tolerance --

#[test]
fn a_quality_that_wobbles_below_the_tolerance_is_the_same_vcf() {
    // QUAL is a floating-point accumulation. A tenth of a Phred unit is
    // below anything anyone filters on, so a wobble there is not a result.
    let files = Files::new();
    let first = files.write(
        "first.vcf",
        &vcf("", &[&VARIANT_5.replace("\t30\t", "\t215.376\t")]),
    );
    let second = files.write(
        "second.vcf",
        &vcf("", &[&VARIANT_5.replace("\t30\t", "\t215.375\t")]),
    );

    same_meaning(DataKind::Variants, &first, &second);
}

#[test]
fn a_quality_that_moves_further_than_the_tolerance_is_a_different_vcf() {
    let files = Files::new();
    let confident = files.write(
        "confident.vcf",
        &vcf("", &[&VARIANT_5.replace("\t30\t", "\t215.4\t")]),
    );
    let doubtful = files.write(
        "doubtful.vcf",
        &vcf("", &[&VARIANT_5.replace("\t30\t", "\t12.0\t")]),
    );

    different_meaning(DataKind::Variants, &confident, &doubtful);
}

#[test]
fn a_tolerance_puts_values_within_it_in_one_bucket() {
    let tenth = Tolerance::new(0.1).unwrap();

    assert_eq!(tenth.bucket(215.376), tenth.bucket(215.375));
    assert_ne!(tenth.bucket(215.4), tenth.bucket(215.5));
    // Exactness is the default everywhere it is not declared.
    assert_eq!(canon::tolerance(DataKind::Variants, "QUAL"), Some(tenth));
    assert_eq!(canon::tolerance(DataKind::Variants, "POS"), None);
    assert_eq!(canon::tolerance(DataKind::Alignment, "MAPQ"), None);
}

#[test]
fn a_tolerance_that_is_not_a_positive_number_is_refused() {
    // Zero would divide; a negative one would invert the ordering.
    assert!(Tolerance::new(0.0).is_none());
    assert!(Tolerance::new(-1.0).is_none());
    assert!(Tolerance::new(f64::NAN).is_none());
    assert!(Tolerance::new(f64::INFINITY).is_none());
}

// ---------------------------------------------------------- reference --

#[test]
fn line_wrapping_and_case_are_not_part_of_a_reference() {
    // The SAM specification's `M5` checksum is defined over the uppercased,
    // whitespace-stripped sequence; `ReferenceId` already agrees.
    let files = Files::new();
    let wrapped = files.write("wrapped.fasta", ">1\nACGTA\nCGTAC\n");
    let flat = files.write("flat.fasta", ">1\nacgtacgtac\n");

    same_meaning(DataKind::Reference, &wrapped, &flat);
}

#[test]
fn a_description_is_not_part_of_a_reference() {
    let files = Files::new();
    let described = files.write(
        "described.fasta",
        ">1 HIV-1 isolate HXB2, complete genome\nACGT\n",
    );
    let bare = files.write("bare.fasta", ">1\nACGT\n");

    same_meaning(DataKind::Reference, &described, &bare);
}

#[test]
fn the_order_of_contigs_is_part_of_a_reference() {
    // Decision D14 again, from the other side: a reference is a sequence
    // dictionary, and its order is one of the things GATK insists on.
    different_meaning(
        DataKind::Reference,
        &common::fixture("tiny_bare.fasta"),
        &common::fixture("tiny_reordered.fasta"),
    );
}

#[test]
fn a_changed_base_is_a_different_reference() {
    let files = Files::new();
    let original = files.write("original.fasta", ">1\nACGT\n");
    let mutated = files.write("mutated.fasta", ">1\nACGA\n");
    let renamed = files.write("renamed.fasta", ">chr1\nACGT\n");

    different_meaning(DataKind::Reference, &original, &mutated);
    different_meaning(DataKind::Reference, &original, &renamed);
}

// -------------------------------------------------------------- reads --

#[test]
fn the_order_reads_arrived_in_is_not_part_of_a_sample() {
    // A re-fetched SRA run comes out in a different order often enough to be
    // worth catching: the reads are a bag, not a list.
    let files = Files::new();
    let forwards = files.write(
        "forwards.fastq",
        "@read1\nACGT\n+\nIIII\n@read2\nTTTT\n+\nIIII\n",
    );
    let backwards = files.write(
        "backwards.fastq",
        "@read2\nTTTT\n+\nIIII\n@read1\nACGT\n+\nIIII\n",
    );

    same_meaning(DataKind::Reads, &forwards, &backwards);
}

#[test]
fn a_description_is_not_part_of_a_read() {
    let files = Files::new();
    let described = files.write("described.fastq", "@read1 length=4\nACGT\n+\nIIII\n");
    let bare = files.write("bare.fastq", "@read1\nACGT\n+read1\nIIII\n");

    same_meaning(DataKind::Reads, &described, &bare);
}

#[test]
fn a_quality_score_is_part_of_a_read() {
    different_meaning(
        DataKind::Reads,
        &common::fixture("reads_phred33_1.fastq"),
        &common::fixture("reads_phred64_1.fastq"),
    );
}

// ---------------------------------------------------------- intervals --

#[test]
fn browser_lines_and_comments_are_not_part_of_an_interval_set() {
    let files = Files::new();
    let decorated = files.write(
        "decorated.bed",
        "track name=\"one\"\nbrowser position 1:1-20\n# a comment\n1\t0\t10\tregion1\n",
    );
    let bare = files.write("bare.bed", "1\t0\t10\tregion1\n");

    same_meaning(DataKind::Intervals, &decorated, &bare);
}

#[test]
fn the_order_of_intervals_is_not_part_of_an_interval_set() {
    // Decision D14: a BED's record order is whatever order the intervals
    // were sorted in, which is why it is not compared as contig order.
    let files = Files::new();
    let forwards = files.write("forwards.bed", "1\t0\t10\tregion1\n2\t5\t15\tregion3\n");
    let backwards = files.write("backwards.bed", "2\t5\t15\tregion3\n1\t0\t10\tregion1\n");

    same_meaning(DataKind::Intervals, &forwards, &backwards);
}

#[test]
fn a_moved_interval_is_a_different_interval_set() {
    let files = Files::new();
    let original = files.write("original.bed", "1\t0\t10\tregion1\n");
    let moved = files.write("moved.bed", "1\t1\t10\tregion1\n");

    different_meaning(DataKind::Intervals, &original, &moved);
}

// ------------------------------------------------------------- report --

#[test]
fn the_order_of_rows_is_not_part_of_a_report() {
    let files = Files::new();
    let header = "contig\tposition\tref\talt\tgene\tcodon\n";
    let forwards = files.write(
        "forwards.tsv",
        &format!("{header}1\t2253\tA\tG\tPR\t1\n1\t2550\tC\tT\tRT\t1\n"),
    );
    let backwards = files.write(
        "backwards.tsv",
        &format!("{header}1\t2550\tC\tT\tRT\t1\n1\t2253\tA\tG\tPR\t1\n"),
    );

    same_meaning(DataKind::ResistanceReport, &forwards, &backwards);
}

#[test]
fn the_columns_a_report_has_are_part_of_it() {
    let files = Files::new();
    let named = files.write("named.tsv", "contig\tposition\n1\t2253\n");
    let renamed = files.write("renamed.tsv", "chrom\tposition\n1\t2253\n");

    different_meaning(DataKind::ResistanceReport, &named, &renamed);
}

#[test]
fn a_changed_row_is_a_different_report() {
    let files = Files::new();
    let original = files.write("original.tsv", "contig\tposition\n1\t2253\n");
    let changed = files.write("changed.tsv", "contig\tposition\n1\t2254\n");

    different_meaning(DataKind::ResistanceReport, &original, &changed);
}

// ----------------------------------------------------------- fallback --

#[test]
fn a_file_that_does_not_parse_falls_back_to_its_bytes() {
    // Everything in the engine has to keep working on a file that is not
    // what its name claims. Falling back to the bytes is the conservative
    // direction: a stricter test can only cost time, never correctness.
    let files = Files::new();
    let text = files.write("not-really.bam", "ACGT\n");
    let other = files.write("also-not.bam", "TGCA\n");

    assert_eq!(form(DataKind::Alignment, &text), Form::Raw);
    different_meaning(DataKind::Alignment, &text, &other);

    let copy = files.write("copy.bam", "ACGT\n");
    assert_eq!(
        canonical(DataKind::Alignment, &text),
        canonical(DataKind::Alignment, &copy),
    );
}

#[test]
fn a_file_that_parses_says_so() {
    let files = Files::new();
    let real = files.write("real.sam", &sam(SQ, &[READ1]));

    assert_eq!(form(DataKind::Alignment, &real), Form::Canonical);
}

#[test]
fn a_fallback_hash_is_never_a_plain_content_hash() {
    // If the fallback were the raw hash itself, a file whose bytes happened
    // to be some other file's canonical digest could collide with it. The
    // policy is part of what is hashed, so it cannot.
    let files = Files::new();
    let text = files.write("not-really.bam", "ACGT\n");

    assert_ne!(
        canonical(DataKind::Alignment, &text),
        ContentHash::of_file(&text).unwrap(),
    );
}

#[test]
fn a_truncated_file_does_not_parse_as_a_whole_one() {
    // A canonicaliser that accepted what it could read and ignored the rest
    // would be the dangerous kind of lenient: two different files agreeing.
    let files = Files::new();
    let whole = files.write("whole.vcf", &vcf("", &[VARIANT_5, VARIANT_9]));
    let truncated = files.write(
        "truncated.vcf",
        &vcf("", &[VARIANT_5, VARIANT_9])
            .replace("1\t9\t.\tC\tT\t40\tPASS\t.\tGT\t0/1\n", "1\t9\t.\tC"),
    );

    assert_eq!(form(DataKind::Variants, &truncated), Form::Raw);
    different_meaning(DataKind::Variants, &whole, &truncated);
}

// -------------------------------------------------------- the policies --

#[test]
fn every_kind_has_a_canonical_form() {
    // A kind added later must be given one deliberately rather than
    // defaulting to its bytes without anyone deciding that.
    for kind in DataKind::ALL {
        assert!(
            canon::describe(kind).is_some(),
            "{} has no canonical form",
            kind.name()
        );
    }
}

#[test]
fn two_kinds_never_share_a_hash_for_one_file() {
    // The kind is part of what is hashed, so the same bytes read as two
    // kinds are two different claims.
    let files = Files::new();
    let text = files.write("ambiguous.txt", "ACGT\n");

    assert_ne!(
        canonical(DataKind::Alignment, &text),
        canonical(DataKind::Variants, &text),
    );
}

#[test]
fn a_read_whose_sequence_and_qualities_disagree_in_length_is_refused() {
    // The one FASTQ error worth naming: four lines that look like a record
    // but describe a read with more bases than measurements.
    let files = Files::new();
    let ragged = files.write("ragged.fastq", "@read1\nACGTACGT\n+\nIIII\n");

    assert_eq!(form(DataKind::Reads, &ragged), Form::Raw);
}

// --------------------------------------------------------------- notes --
//
// A sample set is a dimension, so a sample identifier cannot be dropped the
// way `@PG` was and cannot be quietly rewritten either (D26). What the
// engine can do is notice. The store path in these fixtures is the one
// `bcftools` wrote when the reference pipeline's alignments carried no
// `@RG SM`, which is the failure that put these tests here.

const STORE_PATH: &str = "/Users/kelyn/ligase/.ligase/store/62/7db9fa0cbac.bam";

#[test]
fn a_vcf_whose_sample_is_a_store_path_says_so() {
    let files = Files::new();
    let calls = files.write("calls.vcf", &vcf_of(STORE_PATH, "", &[VARIANT_5]));

    assert_eq!(
        notes(DataKind::Variants, &calls),
        vec![Note::SampleIsAFile {
            sample: STORE_PATH.to_owned()
        }],
    );
}

#[test]
fn a_vcf_whose_sample_is_a_sample_says_nothing() {
    // The common case, and the one that has to stay silent: a checker that
    // remarks on every file is a checker a user stops reading.
    let files = Files::new();
    let calls = files.write("calls.vcf", &vcf_of("SRR8525904", "", &[VARIANT_5]));

    assert!(notes(DataKind::Variants, &calls).is_empty());
    assert_eq!(form(DataKind::Variants, &calls), Form::Canonical);
}

#[test]
fn two_vcfs_differing_only_in_a_path_shaped_sample_differ_and_say_why() {
    // The original failure, in miniature. The same calls, written by two
    // runs whose caches are in different places, so the only difference is
    // the `#CHROM` line's last column. The hashes must differ — a sample
    // set is a dimension and the engine does not invent agreement — and
    // both files must say what is wrong, because that difference is the
    // whole reason a bundle written from one cache reported `differs`
    // against another.
    let files = Files::new();
    let here = files.write(
        "here.vcf",
        &vcf_of("/home/a/.ligase/store/62/7db9fa0cbac.bam", "", &[VARIANT_5]),
    );
    let there = files.write(
        "there.vcf",
        &vcf_of("/mnt/b/.ligase/store/91/0c4e18baf21.bam", "", &[VARIANT_5]),
    );

    assert_ne!(
        canonical(DataKind::Variants, &here),
        canonical(DataKind::Variants, &there),
        "a sample set is a dimension: these are different claims",
    );
    for calls in [&here, &there] {
        let found = notes(DataKind::Variants, calls);
        assert_eq!(found.len(), 1, "{}", calls.display());
        assert!(found[0].to_string().contains("not a sample"));
        assert!(found[0].advice().contains("addreplacerg"));
    }
}

#[test]
fn a_read_group_naming_a_file_says_so() {
    // Where it starts: a BAM with no sample is what makes `bcftools` write
    // a path, and an aligner given `-R` with a file name writes one here.
    let files = Files::new();
    let aligned = files.write(
        "aligned.sam",
        &sam(
            "@HD\tVN:1.6\tSO:coordinate\n\
             @SQ\tSN:1\tLN:20\tM5:a965a71aa3690f605935c54d320905ab\n\
             @RG\tID:rg0\tSM:aligned.bam\n",
            &[READ1],
        ),
    );

    assert_eq!(
        notes(DataKind::Alignment, &aligned),
        vec![Note::SampleIsAFile {
            sample: "aligned.bam".to_owned()
        }],
    );
}

#[test]
fn one_file_named_by_two_read_groups_is_one_note() {
    // One thing is wrong with this alignment, not two.
    let files = Files::new();
    let aligned = files.write(
        "aligned.sam",
        &sam(
            &format!(
                "@HD\tVN:1.6\tSO:coordinate\n\
                 @SQ\tSN:1\tLN:20\tM5:a965a71aa3690f605935c54d320905ab\n\
                 @RG\tID:rg0\tSM:{STORE_PATH}\n\
                 @RG\tID:rg1\tSM:{STORE_PATH}\n"
            ),
            &[READ1],
        ),
    );

    assert_eq!(notes(DataKind::Alignment, &aligned).len(), 1);
}

#[test]
fn a_file_that_falls_back_to_its_bytes_notices_nothing() {
    // A note about a file that did not parse would be a remark about the
    // half of it that was read (D21).
    let files = Files::new();
    let truncated = files.write(
        "truncated.vcf",
        &format!(
            "##fileformat=VCFv4.3\n\
             #CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\t{STORE_PATH}\n\
             1\t5\t.\tA\n"
        ),
    );

    assert_eq!(form(DataKind::Variants, &truncated), Form::Raw);
    assert!(notes(DataKind::Variants, &truncated).is_empty());
}
