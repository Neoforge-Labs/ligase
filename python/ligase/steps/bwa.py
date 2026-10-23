"""Aligning short reads with BWA-MEM.

The other aligner everyone uses, and the one that made the engine's
version capture honest. See `mem`.
"""

from __future__ import annotations

from ..graph import step
from ..recipes import sh
from ..types import Alignment, Reads, Reference

__all__ = ["mem"]


@step(tool=("bwa", "samtools"))
def mem(
    reads: Reads,
    ref: Reference,
    sample_id: str,
    threads: int = 4,
) -> Alignment:
    """Align short reads with BWA-MEM, coordinate-sorted and read-grouped.

    The same output as `ligase.steps.minimap2.align` and deliberately not
    the same step. Two aligners are not interchangeable in provenance: they
    are different programs with different versions and different papers, so
    swapping one for the other changes the cache key and changes the methods
    paragraph. Neither of those is arranged here — both fall out of `tool=`
    naming `bwa` where the other names `minimap2`.

    The read group is written by `samtools addreplacerg` rather than by
    bwa's own `-R`, for no deeper reason than that it is then written the
    same way in both steps, and one mechanism has one place to be wrong.
    Why it has to be written at all is in
    `ligase.steps.minimap2.align`'s docstring.

    ### The index, and what it costs

    `bwa mem` cannot read a bare FASTA: it needs the five files
    `bwa index` writes. In a Ligase run the reference is a file in the
    content-addressed store, where a step has no business writing anything
    — an object there is named after its own bytes, and a step that left
    five siblings beside one would be putting unaddressed files into an
    addressed store.

    So this step builds the index into a temporary directory of its own and
    throws it away afterwards. That is correct and it is slow: the index is
    rebuilt on every run of the step, which for a 2 kb reference costs
    nothing and for a human genome costs an hour. `minimap2` has no such
    cost because it indexes from the FASTA in memory.

    The honest fix is not a flag on this step. It is the same engine
    feature two other things want — a step that can produce more than one
    file — after which an index is something a step produces once and
    others consume. Until then: this step is the right one for a small
    reference, and for a large one, align with minimap2 or write a step of
    your own that points at an index you keep.

    ### Paired-end reads

    As with the other aligner, this one aligns a pair; see its docstring
    for why the requirement is not in the signature.

    Args:
        reads: The reads to align, as a pair of mates.
        ref: What to align them to. Indexed into a temporary directory, so
            nothing is written beside the reference itself.
        sample_id: The sample these reads are of, written into the BAM's
            `@RG ID` and `@RG SM`.
        threads: How many cores the aligner and the sort may use. `bwa
            index` is single-threaded and ignores it.

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
        "bwa index -p {out}.idx {ref}"
        " && bwa mem -t {threads} {out}.idx {reads.r1} {reads.r2} > {out}.sam"
        " && samtools addreplacerg -r ID:{sample_id} -r SM:{sample_id} -u"
        " -o {out}.rg.bam {out}.sam"
        " && samtools sort -@ {threads} -o {out} {out}.rg.bam"
        " && rm -f {out}.sam {out}.rg.bam {out}.idx.*"
    )
