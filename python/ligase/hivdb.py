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

`docs/golden-example.md` writes this step as `lg.hivdb.interpret(v)`, and
that turned out to be one argument short. A VCF holds only the positions
that *differ* from the reference, and two of the three bases of a codon
are usually not among them -- so there is nothing to translate without the
sequence those positions sit in. The reference is not a convenience here;
without it the step cannot name a single amino acid.

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
and where it is not the report says in a note that it assumed the other
thing.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from . import _core
from .recipes import write

if TYPE_CHECKING:
    from pathlib import Path

    from .types import Alignment, Reference, ResistanceReport, Variants

__all__ = ["ALGORITHMS", "DEFAULT", "fingerprint", "interpret"]

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


def fingerprint() -> str:
    """What the vendored algorithms are, by version, date and content.

    This is what a step declaring `tool="hivdb"` records as its tool
    version, so it is what puts the rules into the cache key. It names
    every vendored algorithm rather than only the one in use, because the
    engine probes versions before any step body has said which it wants.

    Returns:
        A line naming each algorithm, its version, its date and the BLAKE3
        of its rules.
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
) -> ResistanceReport:
    """Score a sample's variants against a vendored resistance algorithm.

    Args:
        variants: The calls to interpret.
        reference: The reference they were called against. Required:
            codons cannot be assembled without it.
        coverage: The alignment the calls came from. Without it no
            position can be known to be unread and no codon can be
            phased, and the reading notes both assumptions.
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

    Returns:
        A recipe, typed as the report the step declares.

    Raises:
        ValueError: If `algorithm` is not one the engine ships. The error
            names the ones it does.
    """
    if algorithm not in ALGORITHMS:
        raise ValueError(
            f"no vendored algorithm called {algorithm!r}; this engine ships "
            f"{' and '.join(ALGORITHMS)}"
        )

    def produce(out: Path) -> None:
        """Interpret and write, once the steps above have produced."""
        _core.interpret_resistance(
            reference.file,
            variants.file,
            str(out),
            None if coverage is None else coverage.file,
            algorithm,
            mixture,
            depth,
        )

    return write(produce)
