"""Aligning short reads with minimap2.

One step, and the only one most pipelines need from this tool.
"""

from __future__ import annotations

from ..graph import step
from ..recipes import sh
from ..types import Alignment, Reads, Reference

__all__ = ["align"]


@step(tool=("minimap2", "samtools"))
def align(
    reads: Reads,
    ref: Reference,
    sample_id: str,
    threads: int = 4,
) -> Alignment:
    """Align short reads to a reference, coordinate-sorted and read-grouped.

    This is `minimap2 -ax sr` followed by the two `samtools` invocations
    that make its output usable, and the reason it is one step rather than
    three is that the three are not independently useful: an alignment
    without a read group has no sample, and an unsorted one cannot be piled
    up or indexed. Both are declared in `tool=`, so both versions join the
    cache key.

    ### Why the read group is written here

    `minimap2` writes no `@RG`, and a BAM with no read group has no sample
    name. `bcftools` then falls back to naming the VCF's sample column after
    the *input file's path* — which in a Ligase run is a file inside the
    content-addressed store, so the calls would carry a cache path where a
    sample identifier belongs. That is wrong three times over: wrong in a
    methods paragraph, wrong for the sample-set dimension, and wrong for
    reproducibility, because the canonical form of a VCF includes its sample
    names and a cache path is not the same in two places.

    So the sample is not optional and is not inferred. It is the one thing
    this step asks of you that the files cannot answer.

    ### Paired-end reads

    This step aligns a pair. Handed single-end reads it fails before any
    compute, with the engine saying so: *`{reads.r2}`: `reads` is a
    single-end sample, so it has no second mate*. That the requirement is
    not in the signature is a gap rather than a choice — `Reads` is one kind
    covering both shapes, and nothing lets a step *require* a value of the
    `pairing` dimension the way `coordinates=` lets it declare one. The
    [architecture page](https://ligase.neoforgelabs.tech/project/architecture/step-library/)
    has the rest of that story; it is the same missing mechanism that keeps
    a paired-end trimmer out of this library.

    ### Why `threads` is a parameter like any other

    Changing it reruns this step, because the engine cannot know in advance
    that a thread count will not change the answer. What it *can* know,
    once the step has rerun, is whether the result changed: an alignment is
    hashed by what it means rather than by its bytes, and the `@PG` line
    where minimap2 records `-t 8` is not part of what it means. So the
    alignment comes out with the same canonical hash and everything below
    this step is skipped.

    Args:
        reads: The reads to align, as a pair of mates.
        ref: What to align them to. Its dimensions are checked against the
            reads' before anything runs, so reads from one organism against
            another organism's reference is a refusal rather than a result.
        sample_id: The sample these reads are of, written into the BAM's
            `@RG ID` and `@RG SM`. See above: nothing else can supply it.
        threads: How many cores the aligner and the sort may use.

    Returns:
        A recipe, typed as the coordinate-sorted alignment the step
        produces.
    """
    # One line of `&&`, rather than the pipeline this was or the `set -e`
    # script it briefly became. Two constraints meet here.
    #
    # `sh -c 'a | b'` reports only b's status, so a dead aligner left
    # `samtools sort` writing a valid, empty BAM and the step *succeeded* --
    # which downstream reads as a sample with no variants, and for the
    # reference demo as a patient susceptible to everything. POSIX `sh` has
    # no `pipefail` to reach for, so the pipeline had to go.
    #
    # And a multi-line script reads as a run-on in the methods paragraph,
    # where newlines collapse to spaces and `set -e work=$(mktemp -d)` looks
    # like one command with arguments. A reviewer has to be able to read that
    # paragraph, so the whole step is one line and `&&` is the separator.
    #
    # Intermediates sit beside `{out}` -- in the cache's own staging
    # directory, not in `$TMPDIR`, which is the right filesystem for them:
    # it is where the user put their storage, and an unsorted SAM for a
    # whole-genome sample does not fit in /tmp. A failed step leaves them
    # there until it next succeeds, which truncates them.
    return sh(
        "minimap2 -t {threads} -ax sr {ref} {reads.r1} {reads.r2} > {out}.sam"
        " && samtools addreplacerg -r ID:{sample_id} -r SM:{sample_id} -u"
        " -o {out}.rg.bam {out}.sam"
        " && samtools sort -@ {threads} -o {out} {out}.rg.bam"
        " && rm -f {out}.sam {out}.rg.bam"
    )
