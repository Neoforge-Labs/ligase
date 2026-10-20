"""The reference demo: HIV-1 reads to drug-resistance calls.

    ligase check examples/hiv-resistance/pipeline.py
    ligase run examples/hiv-resistance/pipeline.py

Reads are aligned to HXB2 (K03455.1), variants are called, and the calls
are interpreted as drug resistance against the Stanford HIVdb algorithm.
Every feature of Ligase has to work end to end on this pipeline, which is
why it is a real one: real reads, real tools, real reference, and
resistance calls that are checked against published ones.

The data is not in the repository. Fetch it first:

    scripts/fetch_test_data.sh --project PRJNA517147

Run it twice to watch the cache work. Nothing is recomputed the second time,
because a step's key is built from its code, its parameters, its inputs'
contents and its tools' versions — and none of those moved.

## On what `resistance` does

It interprets. Until M6 it localised each variant to a coding region and
named the codon, which was arithmetic on positions rather than meaning;
`lg.hivdb` now scores the mutations against a vendored Stanford HIVdb
algorithm and reports a level for each of twenty-seven drugs.

The step therefore takes three inputs where `docs/golden-example.md`
wrote one. Its docstring says why; neither addition is optional.

## On `min_freq`, and why the caller changed in M6

This step used to run `bcftools call -mv --ploidy 1`, and its docstring
said what is still true of that tool: `bcftools call` is a consensus
caller, so a frequency threshold below a half rarely removes anything,
and a low-frequency caller is a change of tool rather than of threshold.

M6 made the change, because validating against published results is what
showed the cost. On SRR32217726 — Sample 3 of PRJNA1218827 — the reads at
HXB2 3204 are A 238, G 192. That is a 45% K219E, a zidovudine resistance
mutation, and `bcftools call -mv` emitted no record for it at all: the
majority allele is the reference, so there was no variant to call. The
report came back susceptible to all twenty-seven drugs.

It was *right* to come back susceptible, given that input — it matches
Stanford's own majority-consensus report for the same sample exactly. But
Stanford's 5% report for that sample finds L100I as well and reads
high-level resistance to four NNRTIs. A pipeline whose answer flips from
"fully susceptible" to "resistant to most of the NNRTI class" depending on
a threshold it never exposed is not doing what it says it does.

So the caller is gone and the pileup's own allele depths are the evidence.
`min_freq` now means what its name says: an allele seen in that fraction
of the reads is reported. What counts as *present* for interpretation is
`mixture`, a separate parameter on `resistance`, because the two questions
are different — one is about what the reads show, the other is about what
you are willing to act on. The published thresholds are 2%, 5% and 15%.

Both parameters are typed and hashed, so changing either reruns exactly
the steps it could have changed and no others.
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

import ligase as lg
from ligase.types import Alignment, Reads, Reference, ResistanceReport, Variants

if TYPE_CHECKING:
    from collections.abc import Sequence

#: The repository root, so the demo runs from any directory.
REPO = Path(__file__).resolve().parents[2]

#: The HXB2 reference, as `scripts/fetch_test_data.sh` leaves it.
REFERENCE = REPO / "data" / "refs" / "HXB2.fasta"

#: Where the fetched reads live, by BioProject.
READS = REPO / "data" / "reads"

#: The project the demo reads from unless told otherwise.
#:
#: PRJNA1218827 is the other one fetched, and it is what M6's validation
#: uses: its samples have published Stanford HIVdb calls to check against.
PROJECT = "PRJNA517147"

#: The coding regions are no longer the pipeline's business.
#:
#: They were, while `resistance` did arithmetic on positions. The engine
#: owns them now, in `ligase.hivdb`, because which codons exist is a fact
#: about HIV-1 rather than about this pipeline -- and because getting them
#: wrong is silent. Two of the three this file used to carry were: reverse
#: transcriptase ran to 440 codons where HIVdb numbers it 1-560, and
#: integrase to 5096, which includes pol's stop codon.


@lg.step(tool=("minimap2", "samtools"))
def align(reads: Reads, ref: Reference, sample_id: str, threads: int = 4) -> Alignment:
    """Align one sample's reads against the reference.

    `threads` is how many cores the aligner may use. It is a parameter like
    any other, so changing it reruns this step — the engine cannot know in
    advance that it will not change the answer. What it does know, once the
    step has rerun, is whether the *result* changed: a BAM is hashed by what
    it means rather than by its bytes, and the `@PG` line where minimap2
    records `-t 8` is not part of what it means. So the alignments come out
    the same, and nothing below here is recomputed.

    ## Why the read group is written here

    `minimap2` writes no `@RG`, and a BAM with no read group has no sample
    name. `bcftools` then falls back to naming the VCF's sample column after
    the *input file's path* — which for a Ligase run is a file inside the
    content-addressed store, so the calls would carry a cache path as a
    sample identifier. That would be wrong in three ways at once: wrong in a
    methods paragraph, wrong for the sample-set dimension, and wrong for
    reproducibility, since the canonical form of a VCF includes its sample
    names and a cache path is not the same in two places.

    So the sample is declared here, as the run accession it is. The same
    reasoning as the `@PG` line in D22, with the opposite conclusion: a
    sample name cannot be dropped before hashing, because it is part of what
    the file is about.
    """
    return lg.sh(
        "minimap2 -t {threads} -ax sr {ref} {reads.r1} {reads.r2} "
        "| samtools addreplacerg -r ID:{sample_id} -r SM:{sample_id} -u -o - - "
        "| samtools sort -@ {threads} -o {out}"
    )


@lg.step(tool="bcftools")
def call(aln: Alignment, ref: Reference, min_freq: float = 0.01) -> Variants:
    """Report every allele above a minimum fraction of the reads.

    There is no `bcftools call` here any more, and M6 is why. See the
    module docstring: a consensus caller cannot see a minority virus, and
    for drug resistance the minority virus is the whole question.

    The pileup's own allele depths are the evidence, so `min_freq` is
    applied straight to them and the step reports what the reads say
    rather than what a caller concluded. The threshold now means what its
    name says.
    """
    return lg.sh(
        "bcftools mpileup -a AD -f {ref} {aln} "
        "| bcftools view -i '(AD[0:1])/(AD[0:0]+AD[0:1]) >= {min_freq}' "
        "-Oz -o {out}"
    )


@lg.step(tool="hivdb")
def resistance(
    v: Variants,
    ref: Reference,
    aln: Alignment,
    algorithm: str = lg.hivdb.DEFAULT,
    mixture: float = lg.hivdb.MIXTURE,
) -> ResistanceReport:
    """Interpret the calls as drug resistance.

    Three inputs rather than the one `docs/golden-example.md` wrote, and
    both additions are forced rather than convenient.

    The **reference** is needed because a VCF holds only the positions
    that differ, and two of the three bases of a codon are usually not
    among them. Without the sequence those positions sit in there is
    nothing to translate. Passing it also puts it under `check()`, so
    interpreting one sample's calls against another organism's reference
    is refused before anything runs.

    The **alignment** is needed because `bcftools call -mv` writes only
    the sites it called a variant at. A position missing from the VCF is
    ambiguous between "agrees with HXB2" and "nothing aligned here", and
    resolving that silently towards the first manufactures susceptible
    results out of dropouts. With the alignment, an uncovered codon is
    reported as unread and the drugs whose rules mention it say their
    score is a floor.

    `algorithm` is an ordinary parameter, so changing it reruns this step
    and shows up in the methods paragraph. What pins the *rules* is
    `tool="hivdb"`: it records every vendored algorithm by version, date
    and the digest of its bytes, because upstream publishes corrections
    under near-identical names and a key that moved only with the name
    would serve a result computed under rules that have since changed.
    """
    return lg.hivdb.interpret(
        v, ref, coverage=aln, algorithm=algorithm, mixture=mixture
    )


def pipeline(
    runs: Sequence[str] | None = None,
    min_freq: float = 0.01,
    threads: int = 4,
    project: str = PROJECT,
    algorithm: str = lg.hivdb.DEFAULT,
    mixture: float = lg.hivdb.MIXTURE,
) -> lg.Pipeline:
    """Build the demo over some or all of the fetched runs.

    Args:
        runs: Which runs to include, e.g. `["SRR8525904"]`. All of the
            fetched ones by default.
        min_freq: The minimum allele fraction a variant must reach.
        threads: How many cores the aligner may use. Changing it reruns
            `align` and nothing below it, which is what semantic hashing
            buys.
        project: Which fetched BioProject to read from. M6's validation
            uses `PRJNA1218827`, whose samples have published Stanford
            HIVdb calls to compare against.
        algorithm: Which vendored resistance algorithm to interpret with.
        mixture: The fraction of reads an allele must reach to count as
            present when interpreting. The published results this is
            validated against use 2%, 5% and 15%.

    Returns:
        The pipeline, built and ready to check. Nothing has run.
    """
    hxb2 = lg.reference(str(REFERENCE), organism="HIV-1", build="HXB2")
    reads = READS / project

    if runs is None:
        found = lg.samples(str(reads / "*.fastq.gz"), paired=True, organism="HIV-1")
        chosen = list(zip(found.ids, found, strict=True))
    else:
        chosen = [
            (
                run,
                lg.reads(
                    str(reads / f"{run}_1.fastq.gz"),
                    str(reads / f"{run}_2.fastq.gz"),
                    organism="HIV-1",
                ),
            )
            for run in runs
        ]

    return lg.Pipeline(
        [
            resistance(
                call(aligned, hxb2, min_freq=min_freq),
                hxb2,
                aligned,
                algorithm=algorithm,
                mixture=mixture,
            )
            for run, sample in chosen
            # The alignment is used twice -- once by the caller and once
            # for coverage -- so it is named rather than built twice. One
            # node, one key, one run of minimap2.
            for aligned in [align(sample, hxb2, sample_id=run, threads=threads)]
        ]
    )


#: The pipeline `ligase check` and `ligase run` pick up from this file.
pipe = pipeline()
