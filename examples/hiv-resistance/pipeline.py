"""The reference demo: HIV-1 reads to drug-resistance calls.

    ligase check examples/hiv-resistance/pipeline.py
    ligase run examples/hiv-resistance/pipeline.py

Reads from PRJNA517147 are aligned to HXB2 (K03455.1), variants are called,
and each variant is placed in the gene and codon it falls in. Every feature
of Ligase has to work end to end on this pipeline, which is why it is a real
one: real reads, real tools, real reference.

The data is not in the repository. Fetch it first:

    scripts/fetch_test_data.sh --project PRJNA517147

Run it twice to watch the cache work. Nothing is recomputed the second time,
because a step's key is built from its code, its parameters, its inputs'
contents and its tools' versions — and none of those moved.

## On what `resistance` does today

It localises each variant to the HXB2 protease, reverse transcriptase or
integrase coding region and names the codon. That is arithmetic on
positions, not interpretation: matching mutations against the Stanford HIVdb
rules is milestone M6, and `lg.hivdb` does not exist yet. The step is here
in full because the *pipeline* is what M3 is signed off on, and a pipeline
that stops at variant calls would not exercise fan-out through to a report.

## On `min_freq`

`bcftools call` is a consensus caller: it reports the variants it is
confident in, so a frequency threshold below a half rarely removes any. It
is applied here as a post-filter on allele depth, which is honest about what
bcftools gives us. A low-frequency caller is a change of tool, not of
threshold — and the point of the parameter being typed and hashed is that
changing it reruns exactly the steps it could have changed.
"""

from __future__ import annotations

import gzip
from pathlib import Path
from typing import TYPE_CHECKING

import ligase as lg
from ligase.types import Alignment, Reads, Reference, ResistanceReport, Variants

if TYPE_CHECKING:
    from collections.abc import Sequence

#: The repository root, so the demo runs from any directory.
REPO = Path(__file__).resolve().parents[2]

#: The HXB2 reference and the reads, as `scripts/fetch_test_data.sh` leaves
#: them.
REFERENCE = REPO / "data" / "refs" / "HXB2.fasta"
READS = REPO / "data" / "reads" / "PRJNA517147"

#: The pol coding regions of HXB2, as nucleotide ranges (1-based, closed).
#:
#: These are the standard HXB2 landmarks: protease is 99 codons, reverse
#: transcriptase 440, integrase 288. Resistance mutations are named by codon
#: within one of these, which is why a report has to know them.
GENES = (
    ("PR", 2253, 2549),
    ("RT", 2550, 3869),
    ("IN", 4230, 5096),
)


@lg.step(tool=("minimap2", "samtools"))
def align(reads: Reads, ref: Reference, threads: int = 4) -> Alignment:
    """Align one sample's reads against the reference.

    `threads` is how many cores the aligner may use. It is a parameter like
    any other, so changing it reruns this step — the engine cannot know in
    advance that it will not change the answer. What it does know, once the
    step has rerun, is whether the *result* changed: a BAM is hashed by what
    it means rather than by its bytes, and the `@PG` line where minimap2
    records `-t 8` is not part of what it means. So the alignments come out
    the same, and nothing below here is recomputed.
    """
    return lg.sh(
        "minimap2 -t {threads} -ax sr {ref} {reads.r1} {reads.r2} "
        "| samtools sort -@ {threads} -o {out}"
    )


@lg.step(tool="bcftools")
def call(aln: Alignment, ref: Reference, min_freq: float = 0.01) -> Variants:
    """Call variants, keeping those above a minimum allele fraction."""
    return lg.sh(
        "bcftools mpileup -a AD -f {ref} {aln} "
        "| bcftools call -mv --ploidy 1 -Oz "
        "| bcftools view -i '(AD[0:1])/(AD[0:0]+AD[0:1]) >= {min_freq}' "
        "-Oz -o {out}"
    )


@lg.step
def resistance(v: Variants) -> ResistanceReport:
    """Place each variant in the pol gene and codon it falls in."""
    return lg.write(lambda out: _localise(v.file, out))


def _localise(variants: str, out: Path) -> None:
    """Write one row per variant, with the gene and codon it falls in."""
    rows = ["contig\tposition\tref\talt\tgene\tcodon"]
    with gzip.open(variants, "rt") as calls:
        for line in calls:
            if line.startswith("#"):
                continue
            contig, position, _, reference, alternate = line.split("\t")[:5]
            gene, codon = _codon_of(int(position))
            rows.append(
                f"{contig}\t{position}\t{reference}\t{alternate}\t"
                f"{gene or '-'}\t{codon or '-'}"
            )
    out.write_text("\n".join(rows) + "\n")


def _codon_of(position: int) -> tuple[str | None, int | None]:
    """Which gene and codon an HXB2 position falls in, if any."""
    for gene, start, end in GENES:
        if start <= position <= end:
            return gene, (position - start) // 3 + 1
    return None, None


def pipeline(
    runs: Sequence[str] | None = None,
    min_freq: float = 0.01,
    threads: int = 4,
) -> lg.Pipeline:
    """Build the demo over some or all of the fetched runs.

    Args:
        runs: Which runs to include, e.g. `["SRR8525904"]`. All of the
            fetched ones by default.
        min_freq: The minimum allele fraction a variant must reach.
        threads: How many cores the aligner may use. Changing it reruns
            `align` and nothing below it, which is what semantic hashing
            buys.

    Returns:
        The pipeline, built and ready to check. Nothing has run.
    """
    hxb2 = lg.reference(str(REFERENCE), organism="HIV-1", build="HXB2")

    if runs is None:
        chosen = list(
            lg.samples(str(READS / "*.fastq.gz"), paired=True, organism="HIV-1")
        )
    else:
        chosen = [
            lg.reads(
                str(READS / f"{run}_1.fastq.gz"),
                str(READS / f"{run}_2.fastq.gz"),
                organism="HIV-1",
            )
            for run in runs
        ]

    return lg.Pipeline(
        [
            resistance(
                call(align(sample, hxb2, threads=threads), hxb2, min_freq=min_freq)
            )
            for sample in chosen
        ]
    )


#: The pipeline `ligase check` and `ligase run` pick up from this file.
pipe = pipeline()
