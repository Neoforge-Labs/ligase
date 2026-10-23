"""Reporting what the reads say at each position, with bcftools.

One step, and its name is the argument: `mpileup_call` is a pileup and a
threshold, not `bcftools call`. See `mpileup_call`.
"""

from __future__ import annotations

from ..graph import step
from ..recipes import sh
from ..types import Alignment, Reference, Variants

__all__ = ["mpileup_call"]


@step(tool="bcftools")
def mpileup_call(
    aln: Alignment,
    ref: Reference,
    min_freq: float = 0.01,
) -> Variants:
    """Report every allele seen in at least `min_freq` of the reads.

    There is no `bcftools call` in this step, and the name says
    `mpileup_call` because that is what people call this operation rather
    than because a caller runs. The pileup's own allele depths are the
    evidence, and `min_freq` is applied straight to them.

    ### Why the caller is gone

    `bcftools call -mv` is a *consensus* caller: it asks what the sample's
    genotype is, and a threshold below one half rarely removes anything,
    because an allele the majority does not carry is not a call it will
    make. For anything where the minority population is the question —
    drug resistance, subclonal variants, a mixed infection — that is the
    wrong question, and the reference demo is where the cost was measured.

    On one real sample the reads at a resistance position were 238
    reference and 192 alternate: a 45% mutation, clinically the whole
    answer, and `bcftools call -mv` emitted no record for it at all, because
    the majority allele agreed with the reference. The pipeline reported
    the sample susceptible to every drug. It was *right* given that input,
    and it matched the published majority-consensus report exactly — and
    the published 5% report for the same sample read high-level resistance
    to four drugs. A pipeline whose answer flips between "susceptible" and
    "resistant to most of a class" on a threshold it never exposed is not
    doing what it says it does.

    So the threshold is exposed and means what its name says. It is a
    typed, hashed parameter, so changing it reruns exactly the steps it
    could have changed.

    ### The index it does not leave behind

    `bcftools mpileup -f` needs a `.fai` for the reference and writes one
    beside the path it was given if there is none. In a Ligase run that
    path is an object in the content-addressed store, where every name is
    the hash of its own bytes — so writing an index there would be putting
    an unaddressed file into an addressed store, and on a store that is
    read-only or remote it would not be writing anything at all, it would
    be failing. So the step indexes a link in a directory of its own and
    throws both away.

    ### What it does not decide

    `min_freq` is about what the reads *show*. What counts as present for
    the purpose of acting on it is a separate question with a separate
    parameter, wherever the interpretation happens —
    `ligase.hivdb.interpret`'s `mixture`, for the demo. Conflating the two
    is how a threshold ends up unexposed.

    And a position this step reports nothing about is ambiguous between
    agreeing with the reference and nothing having aligned there. The two
    have opposite meanings and a VCF cannot tell them apart, so whatever
    reads these calls needs the alignment too; `ligase.hivdb.interpret`
    takes it for exactly that reason.

    Args:
        aln: The alignment to pile up. Coordinate-sorted — both aligners in
            this library produce one, and `ligase.steps.samtools.sort` is
            there for an alignment that came from elsewhere.
        ref: The reference it was aligned to, checked against the
            alignment's own dimensions before anything runs. Calling one
            sample's alignment against another build's reference is a
            refusal rather than a plausible VCF.
        min_freq: The least fraction of the reads at a position an allele
            must reach to be reported.

    Returns:
        A recipe, typed as the variant calls the step produces.
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
    #
    # The reference is reached through a link beside `{out}`, and that is not
    # tidiness. `bcftools mpileup -f x.fasta` writes `x.fasta.fai` beside
    # whatever path it was handed, and in a run that path is an object in the
    # content-addressed store, where every name is the hash of its own bytes
    # and an index is not. The link's target is resolved to an absolute path
    # first, because a pipeline's inputs are store paths relative to the
    # working directory and a relative link beside `{out}` would point
    # nowhere.
    return sh(
        'ln -sf "$(cd "$(dirname {ref})" && pwd)/$(basename {ref})"'
        " {out}.ref.fasta"
        " && bcftools mpileup -a AD -f {out}.ref.fasta"
        " -o {out}.pileup.vcf {aln}"
        " && bcftools view -i '(AD[0:1])/(AD[0:0]+AD[0:1]) >= {min_freq}'"
        " -Oz -o {out} {out}.pileup.vcf"
        " && rm -f {out}.ref.fasta {out}.ref.fasta.fai {out}.pileup.vcf"
    )
