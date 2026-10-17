"""Ligase: type-safe, reproducible computational biology pipelines.

Biological data types carry dimensions (organism, reference build, coordinate
system, sample set), and the engine refuses to connect steps whose dimensions
do not match -- like units in physics.

    >>> import ligase as lg
    >>> hxb2 = lg.reference("refs/HXB2.fasta", organism="HIV-1", build="HXB2")
    >>> aln = lg.alignment("out/sample.bam")
    >>> lg.check_step("call", {"aln": aln, "ref": hxb2})
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from . import _core
from .types import (
    Alignment,
    Contig,
    Dataset,
    DimensionValue,
    Reads,
    Reference,
    ResistanceReport,
    Variants,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

__all__ = [
    "Alignment",
    "Contig",
    "ContigNamingError",
    "ContigOrderError",
    "Dataset",
    "DimensionError",
    "DimensionValue",
    "LoadError",
    "Reads",
    "Reference",
    "ResistanceReport",
    "UndeclaredDimensionError",
    "Variants",
    "__version__",
    "alignment",
    "check_step",
    "dimension_errors",
    "reads",
    "reference",
    "variants",
    "version",
]

#: Version of the compiled core engine. Reported by Rust, never hardcoded.
__version__: str = _core.version()

#: Raised when a file cannot be read or does not parse as its format.
LoadError = _core.LoadError


def version() -> str:
    """Return the version of the compiled Ligase core engine."""
    return _core.version()


# --------------------------------------------------------------- errors --


class DimensionError(Exception):
    """Two inputs to a step disagree about a dimension.

    The message names the step, both conflicting values, where each value
    came from, and asks a leading question. The provenance is the part that
    makes it actionable: knowing two references disagree sends you hunting,
    while knowing one came from a BAM `@SQ` header and the other from the
    FASTA you passed tells you what to change.

    Attributes:
        step: The step the disagreement was found in.
        dimension: The name of the dimension that disagrees.
        errors: Every disagreement found in the same pass, this one included.
            Checking reports all of them at once rather than one per run.
    """

    def __init__(self, message: str, step: str, dimension: str) -> None:
        """Record a disagreement, its step and the dimension involved."""
        super().__init__(message)
        self.step = step
        self.dimension = dimension
        self.errors: list[DimensionError] = []


class ContigNamingError(DimensionError):
    """The same contigs under different names, such as `chr1` against `1`.

    Distinct from a plain `DimensionError` because the fix is distinct: a
    renaming step, not a different reference. Still a `DimensionError`, so
    code that wants to catch any dimension problem can.
    """


class ContigOrderError(DimensionError):
    """The same contigs in a different order.

    htslib matches contigs by name, so `bcftools` does not mind. GATK
    requires sequence dictionaries in matching order and will reject this.
    """


class UndeclaredDimensionError(DimensionError):
    """A dimension the step requires is unknown and must be declared."""


_ERROR_CLASSES: dict[str, type[DimensionError]] = {
    "dimension": DimensionError,
    "contig_naming": ContigNamingError,
    "contig_order": ContigOrderError,
    "undeclared": UndeclaredDimensionError,
}


# -------------------------------------------------------------- loaders --


def reference(
    path: str,
    *,
    organism: str | None = None,
    build: str | None = None,
) -> Reference:
    """Load a reference FASTA, inferring its contigs.

    Each contig's MD5 is computed here, over the uppercased sequence, as the
    SAM specification defines `@SQ M5`. Computing it ourselves is what lets
    us check an aligner's claim about which reference it used.

    Args:
        path: The FASTA, optionally gzipped.
        organism: The organism, e.g. `HIV-1`. A FASTA cannot record this.
        build: A human label such as `HXB2` or `hg38`. Display only --
            reference identity never rests on a label, so two files both
            labelled `hg38` still fail to match if their contigs disagree.

    Returns:
        The loaded reference.

    Raises:
        LoadError: If the file cannot be read or is not a FASTA.
    """
    return Reference(_core.load_reference(path, organism, build))


def alignment(
    path: str,
    *,
    organism: str | None = None,
    build: str | None = None,
    samples: list[str] | None = None,
) -> Alignment:
    """Load a SAM or BAM, inferring dimensions from its header.

    Both containers yield identical dimensions: a lossless
    ``samtools view -b`` changes nothing biological.

    Args:
        path: The alignment file.
        organism: The organism. SAM and BAM headers do not record it.
        build: A human label for the reference build. Display only.
        samples: The sample identifiers, overriding the `@RG SM` tags.

    Returns:
        The loaded alignment.

    Raises:
        LoadError: If the file cannot be read or is not a SAM/BAM.
    """
    return Alignment(_core.load_alignment(path, organism, build, samples))


def variants(
    path: str,
    *,
    organism: str | None = None,
    build: str | None = None,
    samples: list[str] | None = None,
) -> Variants:
    """Load a VCF, inferring dimensions from its header.

    Identity comes from `##contig` alone. `##reference` is deliberately not
    trusted: it holds a path, and two runs pointing at `/refs/genome.fa` can
    be pointing at different genomes.

    Args:
        path: The VCF, optionally gzipped or BGZF-compressed.
        organism: The organism. A VCF does not record it.
        build: A human label for the reference build. Display only.
        samples: The sample identifiers, overriding the header columns.

    Returns:
        The loaded variant calls.

    Raises:
        LoadError: If the file cannot be read or is not a VCF.
    """
    return Variants(_core.load_variants(path, organism, build, samples))


def reads(
    r1: str,
    r2: str | None = None,
    *,
    organism: str | None = None,
    sample: str | None = None,
) -> Reads:
    """Load sequencing reads, inferring what a FASTQ can tell us.

    The quality encoding comes from the observed score range, and comes back
    unknown rather than guessed when the scores fall in the range Phred+33
    and Phred+64 share. Guessing would shift every quality value by 31,
    which no downstream tool would notice.

    Args:
        r1: The reads, or the first mate of a pair. Optionally gzipped.
        r2: The second mate, if the reads are paired.
        organism: The organism. A FASTQ does not record it.
        sample: The sample identifier, overriding the one taken from the
            filename.

    Returns:
        The loaded reads.

    Raises:
        LoadError: If a file cannot be read or is not a FASTQ.
    """
    samples = None if sample is None else [sample]
    return Reads(_core.load_reads(r1, r2, organism, samples))


# -------------------------------------------------------------- checking --


def dimension_errors(
    step: str, bindings: Mapping[str, Dataset]
) -> list[DimensionError]:
    """Return every dimension disagreement among a step's inputs.

    Args:
        step: The step's name, used in the messages.
        bindings: The step's inputs, by the name the step knows each by.

    Returns:
        One error per disagreement, most general first. Empty if the step's
        inputs are consistent. Every error carries the full list in its
        `errors` attribute.
    """
    inputs = [(name, data._core) for name, data in bindings.items()]
    raw = _core.check_step(step, inputs)

    errors = [
        _ERROR_CLASSES[kind](message, found_step, dimension)
        for kind, found_step, dimension, message in raw
    ]
    for error in errors:
        error.errors = errors
    return errors


def check_step(step: str, bindings: Mapping[str, Dataset]) -> None:
    """Check a step's inputs, raising on the first disagreement.

    Every disagreement is found before anything is raised, so the raised
    error's `errors` attribute holds all of them. Nothing is computed.

    Args:
        step: The step's name, used in the messages.
        bindings: The step's inputs, by the name the step knows each by.

    Raises:
        DimensionError: If any two inputs disagree. The subclass says what
            kind of disagreement it is: `ContigNamingError`,
            `ContigOrderError` or `UndeclaredDimensionError`.
    """
    errors = dimension_errors(step, bindings)
    if errors:
        raise errors[0]
