#!/usr/bin/env python3
"""Regenerate the committed test fixtures for dimension inference.

The fixtures are tiny, synthetic and deliberately readable: two 20 bp contigs
rather than a real genome, so a reviewer can check an ``@SQ M5`` by hand. They
are committed rather than generated at test time because the tests assert on
exact checksums, and a fixture that regenerates itself can quietly agree with
a broken implementation.

Run from the repository root::

    python3 crates/ligase-core/tests/fixtures/generate.py

The output is deterministic: running it on an unchanged tree rewrites the same
bytes. ``git diff --exit-code`` after running it is the check that the
committed fixtures match this generator.
"""

from __future__ import annotations

import hashlib
from pathlib import Path

HERE = Path(__file__).resolve().parent

# Two 20 bp contigs, plus variations that isolate one dimension each.
C1 = "ACGTACGTACGTACGTACGT"
C2 = "TTTTCCCCGGGGAAAATTTT"
C3 = "CCCCAAAATTTTGGGGCCCC"
# Same length as C1 under the same name, different bases. This is the case
# filename-based reference identity gets wrong and only the MD5 catches.
C1_OTHER = "GGGGCCCCAAAATTTTGGGG"
# A contig that appears in no reference, for the containment failure case.
C9 = "AAAAAAAAAACCCCCCCCCC"
# A pol-relative HIV-1 reference: 3,012 bases, which is the length of K03455
# 2085..5096 and the shape of the file SAWA was interpreting from. What makes
# this fixture what it is, is its length and the frame nobody declared, not
# its sequence, so the bases are synthetic like every other fixture here.
POL = "ACGT" * 753

# ------------------------------------------------- the step library's set --
#
# `ligase.steps` needs fixtures a real aligner will take, and the ones above
# are not that: 20 bp contigs and 10 bp reads are below every aligner's seed
# length, so minimap2's `sr` preset finds no minimizer in them and `bwa mem`
# no seed. These are the smallest thing the five tools of M13 can actually be
# run on.
#
# Deliberately *not* biological. The sequence comes from a linear congruential
# generator with a fixed seed below, because what these fixtures have to be is
# long enough to index, varied enough to align uniquely, and identical on
# every machine for ever. A real virus would be none of the first two by
# accident and the third only by luck.

#: The step-library reference's length, in bases.
STEPS_LENGTH = 2000
#: The contig it holds. Bare, so no contig-naming question arises here.
STEPS_CONTIG = "steps1"
#: Where the planted substitution is, 1-based, as a caller would report it.
#:
#: Mid-contig, so reads cover it from both mates at full depth rather than
#: at an end where coverage tails off and a threshold test would be about
#: the edge instead of about the variant.
STEPS_VARIANT_AT = 1001
#: What the reads carry there, where the reference carries something else.
#: Resolved against the generated sequence in `steps_sequence`.
STEPS_VARIANT_TO = "G"
#: How long each read is, and how far apart the mates sit.
STEPS_READ = 100
STEPS_FRAGMENT = 300
#: The stride between fragments. 20 bases gives about elevenfold coverage at
#: the variant, which is enough for an allele fraction to mean something and
#: small enough to keep the files a few hundred kilobytes.
STEPS_STRIDE = 20


def steps_sequence() -> str:
    """The step-library reference, generated the same way every time.

    A linear congruential generator with a fixed seed. Not random and not
    meant to be: it has to be reproducible on every machine, because the
    committed fixtures are compared against it by `git diff --exit-code`.
    """
    bases = "ACGT"
    state = 0x2545F491
    out = []
    for _ in range(STEPS_LENGTH):
        # Numerical Recipes' constants; any full-period LCG would do.
        state = (1664525 * state + 1013904223) % (2**32)
        out.append(bases[(state >> 16) % 4])
    return "".join(out)


def revcomp(sequence: str) -> str:
    """The reverse complement, for the second mate of a pair."""
    pairs = {"A": "T", "C": "G", "G": "C", "T": "A"}
    return "".join(pairs[base] for base in reversed(sequence))


def m5(sequence: str) -> str:
    """Return the MD5 the SAM specification prescribes for an ``@SQ M5`` tag.

    The digest is over the sequence alone: uppercased, with all whitespace
    removed. Not a security choice -- this is the digest we must reproduce to
    compare against what an aligner wrote.
    """
    return hashlib.md5(sequence.upper().encode(), usedforsecurity=False).hexdigest()


def write(path: Path, text: str) -> None:
    """Write ``text`` to ``path`` with Unix line endings, and report it."""
    path.write_text(text, newline="\n")
    print(f"  {path.name:30s} {len(text.encode()):5d} B")


def fasta(name: str, contigs: list[tuple[str, str, str]]) -> None:
    """Write a FASTA whose deflines carry a name and a description."""
    body = "".join(f">{cname} {desc}\n{seq}\n" for cname, desc, seq in contigs)
    write(HERE / name, body)


def sam(name: str, sq: list[tuple[str, str]], samples: list[str]) -> None:
    """Write a SAM with ``@SQ`` lines, optional ``@RG`` lines and one record."""
    lines = ["@HD\tVN:1.6\tSO:coordinate"]
    for cname, seq in sq:
        lines.append(f"@SQ\tSN:{cname}\tLN:{len(seq)}\tM5:{m5(seq)}\tSP:synthetic")
    for index, sample in enumerate(samples):
        lines.append(f"@RG\tID:rg{index}\tSM:{sample}\tPL:ILLUMINA")
    lines.append("@PG\tID:ligase-fixture\tPN:ligase-fixture\tVN:0.0.0")
    if sq:
        rg = "\tRG:Z:rg0" if samples else ""
        cname, seq = sq[0]
        lines.append(
            f"read1\t0\t{cname}\t1\t60\t10M\t*\t0\t0\t{seq[:10]}\tIIIIIIIIII{rg}"
        )
    else:
        # An unaligned SAM: a record with no reference, flag 4 (unmapped).
        rg = "\tRG:Z:rg0" if samples else ""
        lines.append(f"read1\t4\t*\t0\t0\t*\t*\t0\t0\t{C1[:10]}\tIIIIIIIIII{rg}")
    write(HERE / name, "\n".join(lines) + "\n")


def vcf(
    name: str, contigs: list[tuple[str, str]], samples: list[str], ref: str
) -> None:
    """Write a VCF with ``##reference``, ``##contig`` lines and sample columns."""
    lines = ["##fileformat=VCFv4.3", f"##reference={ref}"]
    for cname, seq in contigs:
        lines.append(f"##contig=<ID={cname},length={len(seq)},md5={m5(seq)}>")
    columns = ["#CHROM", "POS", "ID", "REF", "ALT", "QUAL", "FILTER", "INFO"]
    record = [contigs[0][0], "5", ".", "A", "G", "30", "PASS", "."]
    if samples:
        columns += ["FORMAT", *samples]
        record += ["GT", *["0/1"] * len(samples)]
    lines.append("\t".join(columns))
    lines.append("\t".join(record))
    write(HERE / name, "\n".join(lines) + "\n")


def fastq(name: str, qualities: list[str]) -> None:
    """Write a FASTQ whose quality strings drive encoding inference."""
    lines: list[str] = []
    for index, quality in enumerate(qualities, start=1):
        lines += [f"@read{index}", C1[: len(quality)], "+", quality]
    write(HERE / name, "\n".join(lines) + "\n")


def steps_fixtures() -> None:
    """Write the reference, read pairs and unsorted SAM `ligase.steps` uses.

    One reference, one pair of FASTQs tiling it with a substitution planted
    at `STEPS_VARIANT_AT`, and one deliberately out-of-order SAM for
    `steps.samtools.sort` to have something to sort.
    """
    reference = steps_sequence()
    fasta("steps_ref.fasta", [(STEPS_CONTIG, "step library reference", reference)])

    # The reads carry the substitution; the reference does not. Everything
    # the caller should find at that position comes from this one line.
    at = STEPS_VARIANT_AT - 1
    assert reference[at] != STEPS_VARIANT_TO, (
        f"the planted allele is the reference base at {STEPS_VARIANT_AT}; "
        "change STEPS_VARIANT_TO"
    )
    altered = reference[:at] + STEPS_VARIANT_TO + reference[at + 1 :]

    # Q40 throughout bar the two ends, which are Q20. The point of the Q20
    # bases is the encoding: 53 is below anything Phred+64 can write, so the
    # loader resolves the encoding rather than returning unknown, and Q20 is
    # comfortably above the base quality `bcftools mpileup` discards at.
    quality = "5" + "I" * (STEPS_READ - 2) + "5"

    forward: list[str] = []
    reverse: list[str] = []
    starts = range(0, STEPS_LENGTH - STEPS_FRAGMENT + 1, STEPS_STRIDE)
    for index, start in enumerate(starts, start=1):
        fragment = altered[start : start + STEPS_FRAGMENT]
        forward += [f"@fx{index}", fragment[:STEPS_READ], "+", quality]
        reverse += [
            f"@fx{index}",
            revcomp(fragment[STEPS_FRAGMENT - STEPS_READ :]),
            "+",
            quality,
        ]

    write(HERE / "steps_reads_1.fastq", "\n".join(forward) + "\n")
    write(HERE / "steps_reads_2.fastq", "\n".join(reverse) + "\n")

    # Six records over the same reference, written in no coordinate order
    # at all. `steps.samtools.sort` must put them in order -- and the
    # alignment canonicaliser sorts records before hashing, so sorting
    # changes the file's bytes and not its meaning. That contrast is what
    # `tests/test_steps.py` asserts, and it is why a sort is cheap to
    # insert into a pipeline.
    lines = [
        "@HD\tVN:1.6\tSO:unknown",
        f"@SQ\tSN:{STEPS_CONTIG}\tLN:{STEPS_LENGTH}\tM5:{m5(reference)}",
        "@RG\tID:rg0\tSM:steps_sample\tPL:ILLUMINA",
    ]
    for index, start in enumerate([1200, 40, 900, 300, 1700, 600], start=1):
        lines.append(
            f"out{index}\t0\t{STEPS_CONTIG}\t{start}\t60\t{STEPS_READ}M\t*\t0\t0\t"
            f"{reference[start - 1 : start - 1 + STEPS_READ]}\t{quality}\tRG:Z:rg0"
        )
    write(HERE / "steps_unsorted.sam", "\n".join(lines) + "\n")


def bed(name: str, intervals: list[tuple[str, int, int]], header: bool) -> None:
    """Write a BED whose intervals are 0-based half-open.

    An optional ``track`` line and a ``#`` comment are included so the loader
    is tested against the lines a real BED carries but a parser must skip.
    """
    lines: list[str] = []
    if header:
        lines += ['track name="fixture" description="ligase fixture"', "# a comment"]
    for index, (cname, start, end) in enumerate(intervals, start=1):
        lines.append(f"{cname}\t{start}\t{end}\tregion{index}\t0\t+")
    write(HERE / name, "\n".join(lines) + "\n" if lines else "")


def gff(
    name: str,
    regions: list[tuple[str, str]],
    features: list[tuple[str, int, int]],
    fasta_section: bool,
) -> None:
    """Write a GFF3 whose features are 1-based closed.

    ``##sequence-region`` is the only place a GFF records a contig length, and
    it is only trustworthy as a length when the region starts at 1.
    """
    lines = ["##gff-version 3"]
    for cname, seq in regions:
        lines.append(f"##sequence-region {cname} 1 {len(seq)}")
    for index, (cname, start, end) in enumerate(features, start=1):
        lines.append(
            f"{cname}\tligase-fixture\tgene\t{start}\t{end}\t.\t+\t.\tID=gene{index}"
        )
    if fasta_section:
        # Everything after this directive is sequence, not features. A parser
        # that keeps reading would take `>1` for a contig named `>1`.
        lines += ["##FASTA", ">1 contig one", C1]
    write(HERE / name, "\n".join(lines) + "\n")


def main() -> None:
    """Regenerate every fixture."""
    print("references:")
    fasta("tiny_bare.fasta", [("1", "contig one", C1), ("2", "contig two", C2)])
    # The same sequences under chr-prefixed names: reference identity must see
    # one reference, and the difference must surface as a naming error.
    fasta(
        "tiny_prefixed.fasta", [("chr1", "contig one", C1), ("chr2", "contig two", C2)]
    )
    # Same names and lengths as tiny_bare, different bases for contig one.
    fasta(
        "tiny_other.fasta",
        [("1", "a different contig one", C1_OTHER), ("2", "contig two", C2)],
    )
    # The same two contigs in the opposite order.
    fasta("tiny_reordered.fasta", [("2", "contig two", C2), ("1", "contig one", C1)])
    # A reference too short to hold the HIV-1 gene windows, which is what a
    # pol-relative reference is. Declaring nothing about its frame is the
    # point: a FASTA cannot record where its first base sits in anything
    # else, so the engine has to refuse rather than guess.
    fasta("hivdb_pol.fasta", [("K03455", "HIV-1 pol, K03455 2085..5096", POL)])
    # tiny_bare plus a third contig: data on 1 and 2 is contained in this.
    fasta(
        "tiny_superset.fasta",
        [("1", "contig one", C1), ("2", "contig two", C2), ("3", "contig three", C3)],
    )

    print("alignments:")
    sam("tiny_bare.sam", [("1", C1), ("2", C2)], ["sample_a"])
    sam("tiny_prefixed.sam", [("chr1", C1), ("chr2", C2)], ["sample_a"])
    sam("tiny_other.sam", [("1", C1_OTHER), ("2", C2)], ["sample_a"])
    sam("tiny_reordered.sam", [("2", C2), ("1", C1)], ["sample_a"])
    # A contig in no reference: the containment failure case.
    sam("tiny_unknown_contig.sam", [("1", C1), ("9", C9)], ["sample_a"])
    # No @RG: the sample set is unknown, which is not the same as empty.
    sam("tiny_no_rg.sam", [("1", C1), ("2", C2)], [])
    # No @SQ: an unaligned BAM carries no reference at all.
    sam("tiny_unaligned.sam", [], ["sample_a"])
    sam("tiny_two_rg.sam", [("1", C1), ("2", C2)], ["sample_a", "sample_b"])

    print("variants:")
    vcf("tiny_bare.vcf", [("1", C1), ("2", C2)], ["sample_a"], "tiny_bare.fasta")
    # Only contig one: correct against tiny_bare, and must not be an error.
    vcf("tiny_subset.vcf", [("1", C1)], ["sample_a"], "tiny_bare.fasta")
    # Calls against the pol-relative reference, agreeing with it about the
    # contig, so a frame error is the only thing a check can find.
    vcf("hivdb_pol.vcf", [("K03455", POL)], ["sample_a"], "hivdb_pol.fasta")
    vcf("tiny_other.vcf", [("1", C1_OTHER)], ["sample_a"], "tiny_other.fasta")
    vcf(
        "tiny_two_samples.vcf",
        [("1", C1), ("2", C2)],
        ["sample_a", "sample_b"],
        "tiny_bare.fasta",
    )
    # The same two samples in the opposite order: the shuffled-ID bug.
    vcf(
        "tiny_swapped_samples.vcf",
        [("1", C1), ("2", C2)],
        ["sample_b", "sample_a"],
        "tiny_bare.fasta",
    )

    print("reads:")
    # Phred+33: '!' is 0, below anything Phred+64 can encode.
    fastq("reads_phred33_1.fastq", ["IIIIIIIIII", "!!!!!!!!!!", "IIIIIIIIII"])
    fastq("reads_phred33_2.fastq", ["IIIIIIIIII", "###+++++++", "IIIIIIIIII"])
    # Phred+64: 'h' is 40, above anything Phred+33 can encode.
    fastq("reads_phred64_1.fastq", ["hhhhhhhhhh", "aaaaaaaaaa", "hhhhhhhhhh"])
    # Mid-range only: consistent with either encoding, so inference must
    # return unknown rather than guess.
    fastq("reads_ambiguous_1.fastq", ["AAAAAAAAAA", "BBBBBBBBBB"])

    print("intervals:")
    # Bare names, 0-based half-open: contained in tiny_bare.fasta.
    bed("tiny_bare.bed", [("1", 0, 10), ("1", 10, 20), ("2", 5, 15)], header=True)
    # The same regions under chr-prefixed names: a naming error, not a
    # different reference.
    bed("tiny_prefixed.bed", [("chr1", 0, 10), ("chr2", 5, 15)], header=False)
    # Contig two before contig one. Records have no sequence dictionary, so
    # this must NOT be reported as a contig order mismatch.
    bed("tiny_reordered.bed", [("2", 5, 15), ("1", 0, 10)], header=False)
    # A contig in no reference: the containment failure case.
    bed("tiny_unknown_contig.bed", [("1", 0, 10), ("9", 0, 10)], header=False)
    # No records at all: the reference is unknown, not empty.
    bed("tiny_empty.bed", [], header=True)
    # 1-based closed, with lengths from ##sequence-region.
    gff(
        "tiny_bare.gff",
        [("1", C1), ("2", C2)],
        [("1", 1, 10), ("2", 6, 15)],
        fasta_section=False,
    )
    # No directives, so names come from records and no length is known. The
    # trailing ##FASTA section must not be read as features.
    gff("tiny_no_region.gff", [], [("1", 1, 10)], fasta_section=True)
    # A GTF: same coordinate convention, different extension.
    gff("tiny_bare.gtf", [], [("1", 1, 10), ("2", 6, 15)], fasta_section=False)

    print("the step library's set:")
    steps_fixtures()

    print()
    print("checksums (SAM @SQ M5 / VCF ##contig md5):")
    for label, sequence in [
        ("contig 1", C1),
        ("contig 2", C2),
        ("contig 3", C3),
        ("contig 1 (other)", C1_OTHER),
        ("contig 9", C9),
    ]:
        print(f"  {label:18s} {m5(sequence)}")


if __name__ == "__main__":
    main()
