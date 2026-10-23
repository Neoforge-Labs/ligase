"""The reference demo: HIV-1 reads to drug-resistance calls.

    ligase check examples/hiv-resistance/pipeline.py
    ligase run examples/hiv-resistance/pipeline.py

Reads are aligned to HXB2 (K03455.1), variants are called, and the calls
are interpreted as drug resistance against the Stanford HIVdb algorithm.
Every feature of Ligase has to work end to end on this pipeline, which is
why it is a real one: real reads, real tools, real reference, and
resistance calls that are checked against published ones.

The data is not in the repository and there is no script to fetch it. The
pipeline says where it comes from — `lg.remote("ncbi:K03455.1", ...)` and
`lg.remote("sra:SRR8525904", ...)` — so `ligase run` obtains it, checks each
input against the digest pinned below before any step runs, and keeps it in
the content-addressed store. The second run fetches nothing.

Until M10 this was `scripts/fetch_test_data.sh`: three hundred lines that
knew ENA's directory layout, computed checksums and verified them, all of it
outside anything Ligase promises. The engine had to take the script's word
for what had arrived. Now the archive is an input like any other, with its
digest in every cache key below it and its locator in the methods paragraph.

Run it twice to watch the cache work. Nothing is recomputed the second time,
because a step's key is built from its code, its parameters, its inputs'
contents and its tools' versions — and none of those moved.

## On the digests

Each is over what the file *means*, not over its bytes: the canonical form
of what arrives, read as its kind. NCBI reflows a FASTA and rewords a
defline from time to time, and the shell script pinned HXB2 by the checksum
of its *sequence* for exactly that reason. A pin that broke on a reformat
would teach you to bump pins without reading, and then the one that matters
gets bumped too.

## On where the steps come from

There are no shell commands in this file. Alignment and variant calling
come from `ligase.steps`, the project's own library, and the pipeline reads

    aligned = minimap2.align(sample, hxb2, sample_id=run, threads=threads)
    called = bcftools.mpileup_call(aligned, hxb2, min_freq=min_freq)

which is what it means. Until M13 each of those was a `lg.sh` command
written here, and the alignment one was three programs piped together with
a read group to arrange; the reasoning that made it those three programs
has not gone anywhere, it has moved into the docstrings of the steps that
own it. A demo that spells out its own `samtools addreplacerg` is not
showing you Ligase, it is showing you the shell.

What stays here is `resistance`, because interpreting calls against the
Stanford algorithm is this pipeline's own subject rather than a tool
anybody's pipeline begins with. It is `lg.hivdb.interpret` and not a shell
command either.

## On what `resistance` does

It interprets. Until M6 it localised each variant to a coding region and
named the codon, which was arithmetic on positions rather than meaning;
`lg.hivdb` now scores the mutations against a vendored Stanford HIVdb
algorithm and reports a level for each of twenty-seven drugs.

The step therefore takes three inputs where
`web/content/project/golden-example.mdx` wrote one. Its docstring says why;
neither addition is optional.

## On `min_freq`, and the measurement behind it

`ligase.steps.bcftools.mpileup_call` reports allele depths rather than
running `bcftools call`, and its docstring gives the general argument: a
consensus caller answers "what is this sample's genotype?", which is the
wrong question wherever the minority population *is* the question.

This is where the cost of the right-sounding default was measured, in M6,
and the numbers are about this data rather than about that tool. On
SRR32217726 — Sample 3 of PRJNA1218827 — the reads at
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

So the caller went and the pileup's own allele depths are the evidence.
`min_freq` means what its name says: an allele seen in that fraction of
the reads is reported. What counts as *present* for interpretation is
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
from ligase.steps import bcftools, minimap2
from ligase.types import Alignment, Reads, Reference, ResistanceReport, Variants

if TYPE_CHECKING:
    from collections.abc import Sequence

#: The repository root, so the demo runs from any directory.
REPO = Path(__file__).resolve().parents[2]

#: HXB2, by the accession NCBI serves it under.
#:
#: 9,719 bases, one contig named `K03455.1`. The digest is over the
#: canonical form of the FASTA — the sequence, uppercased, with the defline
#: read for its contig name — so a reflow at NCBI does not break it.
HXB2 = "ncbi:K03455.1"
HXB2_DIGEST = "1f460b25ab23b95345d579c234c43ca6d0abf7c1adf6aeffb43b974b2b1941ad"

#: Every run the demo and its validation use, by BioProject, with the digest
#: each one is pinned to.
#:
#: Paired Illumina AMPLICON runs, the smallest in each project so the demo
#: stays quick. A run's digest is a fold over its two mates' canonical forms
#: in order, so swapping the mates is a different digest — which it must be,
#: since it reverses every read's orientation.
#:
#: This table is the manifest. CI keys its cache on the hash of this file,
#: so editing a pin re-fetches and nothing else does.
RUNS: dict[str, dict[str, str]] = {
    "PRJNA517147": {
        "SRR8525904": (
            "a7800f3dace6464faa08c34ce765f0398ab883c76c5475102318dae6c3d21923"
        ),
        "SRR8525908": (
            "6a81bfc60cf4c232afc1cf383131ae1c8eadaee5aef33ebc4b44b530afa6f495"
        ),
        "SRR8525942": (
            "c7b1d37c0a0e3af510523134cbd3994a9c656ba14cd76631790ca354551fb155"
        ),
        "SRR8525920": (
            "0e06df1d6e83d9dcda1169c741fe12547210783ef8bce93d26f17d44f83f03df"
        ),
    },
    "PRJNA1218827": {
        "SRR32217714": (
            "fbaacf18c2499c8c42de57a10e3b41158ccaa0454d91f476aa0e91e6e99441bc"
        ),
        "SRR32217716": (
            "e53caf5bccec4eefe2bd7430246fddff454e156dbc27e39be050bde7fd017765"
        ),
        "SRR32217715": (
            "05439677b3f12bed37e778c4ac6ea19fdb8aa1a8e19b39eb0dc2a11e91417578"
        ),
        "SRR32217726": (
            "d4234265f76c66c21fb70f3080cc8e60270b0084842f4505017a6dea35787359"
        ),
    },
}

#: The project the demo reads from unless told otherwise.
#:
#: PRJNA1218827 is the other one, and it is what M6's validation uses: its
#: samples have published Stanford HIVdb calls to check against.
PROJECT = "PRJNA517147"


def reference() -> Reference:
    """HXB2, fetched and checked against its pin.

    Returns:
        The reference, with its contigs and their checksums read from the
        FASTA that arrived rather than taken on trust.
    """
    return lg.remote(
        HXB2,
        kind=Reference,
        digest=HXB2_DIGEST,
        organism="HIV-1",
        build="HXB2",
    )


def run_reads(run: str, project: str = PROJECT) -> Reads:
    """One run's paired reads, fetched and checked against their pin.

    Args:
        run: The run accession, e.g. `SRR8525904`.
        project: Which BioProject it belongs to.

    Returns:
        The reads, with the sample declared as the run accession.

    Raises:
        KeyError: If the project or the run is not in `RUNS`.
    """
    # The sample has to be declared. A local FASTQ's sample comes from its
    # filename and a fetched one is stored under the hash of its bytes,
    # which is no sample identifier at all.
    return lg.remote(
        f"sra:{run}",
        kind=Reads,
        digest=RUNS[project][run],
        organism="HIV-1",
        samples=[run],
    )


#: The coding regions are no longer the pipeline's business.
#:
#: They were, while `resistance` did arithmetic on positions. The engine
#: owns them now, in `ligase.hivdb`, because which codons exist is a fact
#: about HIV-1 rather than about this pipeline -- and because getting them
#: wrong is silent. Two of the three this file used to carry were: reverse
#: transcriptase ran to 440 codons where HIVdb numbers it 1-560, and
#: integrase to 5096, which includes pol's stop codon.


@lg.step(tool="hivdb")
def resistance(
    v: Variants,
    ref: Reference,
    aln: Alignment,
    algorithm: str = lg.hivdb.DEFAULT,
    mixture: float = lg.hivdb.MIXTURE,
) -> ResistanceReport:
    """Interpret the calls as drug resistance.

    Three inputs rather than the one
    `web/content/project/golden-example.mdx` wrote, and both additions are
    forced rather than convenient.

    The **reference** is needed because a VCF holds only the positions
    that differ, and two of the three bases of a codon are usually not
    among them. Without the sequence those positions sit in there is
    nothing to translate. Passing it also puts it under `check()`, so
    interpreting one sample's calls against another organism's reference
    is refused before anything runs.

    The **alignment** is needed twice over. A VCF holds only the
    positions the caller had something to say about, so a position
    missing from it is ambiguous between "agrees with HXB2" and "nothing
    aligned here", and resolving that silently towards the first
    manufactures susceptible results out of dropouts. With the alignment,
    an uncovered codon is reported as unread and the drugs whose rules
    mention it say their score is a floor.

    It is also what carries phasing. A VCF says what is at each position
    and not what travels together, so a codon with two ambiguous bases
    has combinations no read carries; the reads spanning a codon settle
    which of them are real (D31).

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
        runs: Which runs to include, e.g. `["SRR8525904"]`. Every run the
            project's manifest names, by default.
        min_freq: The minimum allele fraction a variant must reach.
        threads: How many cores the aligner may use. Changing it reruns
            `align` and nothing below it, which is what semantic hashing
            buys.
        project: Which BioProject to read from. M6's validation uses
            `PRJNA1218827`, whose samples have published Stanford HIVdb
            calls to compare against.
        algorithm: Which vendored resistance algorithm to interpret with.
        mixture: The fraction of reads an allele must reach to count as
            present when interpreting. The published results this is
            validated against use 2%, 5% and 15%.

    Returns:
        The pipeline, built and ready to check. The remotes have resolved —
        which is to say the inputs are in the store and every digest has
        been checked — and no step has run.

    Raises:
        RemoteError: If an input cannot be fetched or does not match its
            pin. This happens here rather than in `run()`, because a
            remote's dimensions cannot be known until its bytes are in
            hand and the engine checks dimensions before any compute.
    """
    hxb2 = reference()
    wanted = list(RUNS[project]) if runs is None else list(runs)
    chosen = [(run, run_reads(run, project)) for run in wanted]

    return lg.Pipeline(
        [
            resistance(
                bcftools.mpileup_call(aligned, hxb2, min_freq=min_freq),
                hxb2,
                aligned,
                algorithm=algorithm,
                mixture=mixture,
            )
            for run, sample in chosen
            # The alignment is used twice -- once by the pileup and once
            # for coverage -- so it is named rather than built twice. One
            # node, one key, one run of minimap2.
            for aligned in [
                minimap2.align(sample, hxb2, sample_id=run, threads=threads)
            ]
        ]
    )


#: The pipeline `ligase check` and `ligase run` pick up from this file.
pipe = pipeline()
