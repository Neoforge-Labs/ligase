"""Putting an alignment in coordinate order with samtools.

One step. `samtools index` is not here, and the reason is worth reading:
see `sort`.
"""

from __future__ import annotations

from ..graph import step
from ..recipes import sh
from ..types import Alignment

__all__ = ["sort"]


@step(tool="samtools")
def sort(aln: Alignment, threads: int = 4) -> Alignment:
    """Put an alignment in coordinate order.

    Both aligners in this library sort what they produce, so this step is
    for an alignment that came from somewhere else: a BAM a collaborator
    sent, one an older pipeline wrote, one a step of your own produced.
    Nearly everything downstream wants coordinate order — `bcftools
    mpileup` does, and so does any form of indexing.

    ### Sorting changes the bytes and not the meaning

    Which is a stronger statement than it sounds, and it is the engine's
    rather than this step's. An alignment's canonical form sorts its
    records before hashing (D4), because the order records sit in a file is
    not what the file is about. So a sorted alignment and its unsorted
    original have the *same* canonical hash — and inserting this step into
    a pipeline therefore reruns the sort and skips everything below it, by
    early cutoff, with no special case anywhere.

    The practical reading: a sort costs you one step's work once, and never
    costs you the pipeline below it.

    ### Why `samtools index` is not in this library

    Because it cannot say what it produces, and D42 admits nothing that
    cannot. `samtools index` writes a `.bai` beside its input. Declared
    `Alignment → Alignment` it would be false twice: the output file is an
    index rather than an alignment, and the engine would not catch it —
    a file that does not parse as its kind falls back to having its *bytes*
    hashed, so an index's bytes would stand in for an alignment's meaning
    and every step below would accept it as an alignment.

    Nor is the answer a new kind. An index brings no dimension, and under
    the scope rule a kind that brings no dimension does not earn a place
    here (D35). What an index actually wants is to travel *beside* the
    alignment it indexes, and a step's output is one file, so there is
    nowhere for it to travel. That is an engine gap, recorded as one on the
    [architecture page](https://ligase.neoforgelabs.tech/project/architecture/step-library/),
    rather than a step shipped with a false type on it.

    Args:
        aln: The alignment to sort. Its dimensions travel through
            unchanged, because sorting is not a change of what the file is
            about.
        threads: How many cores the sort may use. A parameter like any
            other, so changing it reruns this step — and, by the paragraph
            above, nothing below it.

    Returns:
        A recipe, typed as the coordinate-sorted alignment the step
        produces.
    """
    return sh("samtools sort -@ {threads} -o {out} {aln}")
