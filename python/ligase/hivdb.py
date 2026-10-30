"""Interpreting variant calls as HIV-1 drug resistance.

    @lg.step(tool="hivdb")
    def resistance(
        v: Variants, ref: Reference, aln: Alignment
    ) -> ResistanceReport:
        return lg.hivdb.interpret(v, ref, coverage=aln)

The rules are Stanford's HIVdb algorithm, vendored into the engine and
pinned by checksum rather than fetched. Stanford runs a web service that
does this interpretation; calling it would make a result depend on the day
it was asked for, which no cache key can see and no reproducibility bundle
can reproduce.

## Why the reference is a parameter

`web/content/project/golden-example.mdx` writes this step as
`lg.hivdb.interpret(v)`, and that turned out to be one argument short. A VCF
holds only the positions that *differ* from the reference, and two of the
three bases of a codon are usually not among them -- so there is nothing to
translate without the sequence those positions sit in. The reference is not
a convenience here; without it the step cannot name a single amino acid.

Passing it has a second effect worth having. The reference becomes an
input to the step, so `check()` compares it against the calls before
anything runs, and interpreting one sample's variants against another
organism's reference is refused rather than reported.

## Why coverage is a parameter

`bcftools call -mv` writes only the sites it called a variant at. A
position missing from a VCF is therefore ambiguous between "agrees with
the reference" and "nothing aligned here", and those have opposite
clinical meanings: the first is a susceptible result and the second is no
result at all.

Resolving that silently towards wild-type manufactures susceptible calls
out of dropouts. So the alignment is read for coverage where it is given,
and where it is not the reading says in a note that it assumed the other
thing.

## Where a note goes

A reading's notes are hedges with a clinical direction, and the report they
are about cannot state them: a fully covered run and a run with no coverage
at all can write the same bytes. So the step hands them back to the engine,
which says them wherever it says what a run did -- the run report, the
step's `notes` on `pipe.run()`, provenance, a reproducibility bundle and
the methods section (D32).

## Why the reference has to be in the engine's own frame

The gene windows are positions in HXB2 K03455 counted from the first base
of the genome, and they are built into the engine. A pol-relative
reference -- the obvious thing to have when the amplicon is pol -- is a
slice of K03455 starting at 2085, so every position in it is 2084 lower
than the window means, and the window named protease lands in integrase.

That used to be read without complaint and reported as
protease-inhibitor resistance. It is now a `ReferenceFrameError`, raised
by `check()` before anything runs where the pipeline declared a frame, and
by this function otherwise.

## Why a gene barely read is not assessed

A few unread codons a report survives honestly: the rules that name them
go undetermined, the score is a floor and the unread positions are listed.
A gene mostly *unread* it does not. Nothing there refutes resistance and
nothing shows it, so every drug lands at zero, which the algorithm's own
ranges read as susceptible -- a clean clinical negative from a gene the run
never covered. So a gene read below `gene_coverage` of its codons is
reported as not assessed, which is what it is.

The share is counted over the codons the chosen algorithm *scores*, not
over the gene. A position no rule names cannot change an answer however it
was read, so reading it is no evidence the gene was read well enough to
score -- and counting it put the default out of reach of the records
clinical labs hold. HIVDB 9.8 scores 40 of reverse transcriptase's 560
codons and the highest is 348, so a partial-pol genotype covering every
position the rules consult had still read 62% of the gene, and was refused
for it.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from . import _core
from .errors import ReferenceFrameError
from .recipes import write

if TYPE_CHECKING:
    from pathlib import Path

    from .recipes import Noticed
    from .types import Alignment, Reference, ResistanceReport, Variants

__all__ = [
    "ALGORITHMS",
    "DEFAULT",
    "DEPTH",
    "GENE_COVERAGE",
    "MIXTURE",
    "fingerprint",
    "interpret",
]

#: Every resistance algorithm the engine ships, newest first.
ALGORITHMS: tuple[str, ...] = tuple(_core.resistance_algorithms())

#: The algorithm used when a pipeline does not name one.
#:
#: The older of the two shipped, deliberately: it is the version the
#: published results Ligase is validated against were produced with, and a
#: default that disagreed with the suite's own reference answers would
#: make every validation run an argument about algorithm drift. A pipeline
#: that wants the current science names the newer one.
DEFAULT: str = _core.default_resistance_algorithm()

#: What a position must reach, as a fraction of the reads, to be counted
#: as present. 0.15 is the `Amb15` convention of the published results.
MIXTURE: float = 0.15

#: What depth a position must reach to count as read at all.
DEPTH: int = 100

#: What share of a gene's scored codons must be read for the gene to be
#: assessed.
#:
#: A policy, not a fact, which is why it is a parameter: how much of a gene
#: a surveillance programme will call susceptible on is its own decision,
#: and a judgement belongs in a cache key rather than in a constant nobody
#: can see. Four fifths by default, because the failure it guards against
#: is a susceptible call made from a gene that was largely not sequenced.
#:
#: Counted over the codons the algorithm scores -- the positions some
#: drug's rules name -- rather than over the gene: a position no rule
#: names cannot change an answer, so reading it is no evidence the gene
#: was read well enough to score.
GENE_COVERAGE: float = 0.8


def fingerprint() -> str:
    """What the vendored algorithms are, and which engine reads them.

    This is what a step declaring `tool="hivdb"` records as its tool
    version, so it is what puts the rules into the cache key. It names
    every vendored algorithm rather than only the one in use, because the
    engine probes versions before any step body has said which it wants.

    It ends with this engine's own version, because the rules are only
    half of what the step is: the other half is the engine's judgements
    about reading them -- which codons count as read, how much of a gene
    has to be -- and those move between releases.

    Returns:
        A line naming each algorithm, its version, its date and the BLAKE3
        of its rules, then the version of the engine reading them.
    """
    return _core.resistance_fingerprint()


def interpret(
    variants: Variants,
    reference: Reference,
    *,
    coverage: Alignment | None = None,
    algorithm: str = DEFAULT,
    mixture: float = MIXTURE,
    depth: int = DEPTH,
    gene_coverage: float = GENE_COVERAGE,
) -> ResistanceReport:
    """Score a sample's variants against a vendored resistance algorithm.

    Args:
        variants: The calls to interpret.
        reference: The reference they were called against. Required:
            codons cannot be assembled without it.
        coverage: The alignment the calls came from. Without it no
            position can be known to be unread and no codon can be
            phased, and the reading notes both assumptions -- which the
            run report then says and `pipe.run()` carries on the step's
            `notes`.
        algorithm: Which vendored algorithm to use, from `ALGORITHMS`.
        mixture: The fraction of reads an allele must reach to count as
            present. Where both the reference and an alternate reach it
            the codon holds both amino acids, exactly as an ambiguous
            base does to Stanford's own interpreter -- taking the majority
            instead would discard the minority resistance that deep
            sequencing exists to find.

            With an alignment it also decides which *combinations* of
            those alleles are real: a VCF says what is at each position
            and not what travels together, so reads spanning a codon are
            counted as the triplets they carry and the codon holds the
            amino acids reaching this fraction of them.
        depth: The depth a position must reach to count as read. A codon
            whose bases do not all reach it is reported as unread rather
            than as reference.
        gene_coverage: The share of a gene's *scored* codons -- the ones
            `algorithm`'s rules name -- that must be read for the gene to be
            assessed at all. A gene below it is reported as not assessed
            rather than scored against the little that was seen, because a
            gene whose scored positions are mostly unread scores zero, and
            zero reads as susceptible. Counted over the scored codons
            rather than the gene because a position no rule names cannot
            change an answer.

    Returns:
        A recipe, typed as the report the step declares.

    Raises:
        ValueError: If `algorithm` is not one the engine ships. The error
            names the ones it does.
        ReferenceFrameError: When the recipe runs, if `reference` cannot be
            the frame the gene windows are stated in. A pipeline is refused
            earlier than this, by `check()`, where the reference's declared
            frame is known.
    """
    if algorithm not in ALGORITHMS:
        raise ValueError(
            f"no vendored algorithm called {algorithm!r}; this engine ships "
            f"{' and '.join(ALGORITHMS)}"
        )

    def produce(out: Path) -> list[Noticed]:
        """Interpret and write, once the steps above have produced.

        Returns what the reading noticed, which the engine then says: a
        hedge dropped here is a hedge with a clinical direction that
        nobody is ever told about.
        """
        try:
            return _core.interpret_resistance(
                reference.file,
                variants.file,
                str(out),
                None if coverage is None else coverage.file,
                algorithm,
                mixture,
                depth,
                gene_coverage,
            )
        except _core.FrameError as error:
            # Named here rather than in the engine: the hierarchy is what
            # Python code catches on, and this belongs beside the other
            # disagreements about data. No step to name -- a check that ran
            # before this would have said which.
            raise ReferenceFrameError(str(error), dimension="reference") from error

    return write(produce)
