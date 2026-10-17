"""Ligase: type-safe, reproducible computational biology pipelines.

Biological data types carry dimensions (organism, reference build, coordinate
system, sample set), and the engine refuses to connect steps whose dimensions
do not match -- like units in physics.

    >>> import ligase as lg
    >>> from ligase.types import Alignment, Reads, Reference
    >>> hxb2 = lg.reference("refs/HXB2.fasta", organism="HIV-1", build="HXB2")
    >>> samples = lg.samples("data/*.fastq.gz", paired=True)
    >>> @lg.step(tool="minimap2")
    ... def align(reads: Reads, ref: Reference) -> Alignment: ...
    >>> pipe = lg.Pipeline(samples.map(lambda s: align(s, hxb2)))
    >>> pipe.check()   # type-checks every step; nothing runs
    >>> pipe.run()     # parallel, cached, per-sample fan-out
"""

from __future__ import annotations

from glob import glob
from typing import TYPE_CHECKING

from . import _core
from .audit import AuditReport, StepAudit
from .errors import (
    CheckError,
    ContigNamingError,
    ContigOrderError,
    DimensionError,
    StepDefinitionError,
    StepTypeError,
    UndeclaredDimensionError,
)
from .errors import (
    build as _build_errors,
)
from .graph import Pipeline, Report, Step, StepOutput, handle, step
from .recipes import Shell, Written, sh, write
from .run import RunError, RunReport, StepFailure, StepRun
from .samples import Samples
from .types import (
    Alignment,
    Contig,
    Dataset,
    DimensionValue,
    Intervals,
    Reads,
    Reference,
    ResistanceReport,
    Variants,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

__all__ = [
    "Alignment",
    "AuditReport",
    "CheckError",
    "Contig",
    "ContigNamingError",
    "ContigOrderError",
    "Dataset",
    "DimensionError",
    "DimensionValue",
    "Intervals",
    "LoadError",
    "Pipeline",
    "Reads",
    "Reference",
    "Report",
    "ResistanceReport",
    "RunError",
    "RunReport",
    "Samples",
    "Shell",
    "Step",
    "StepAudit",
    "StepDefinitionError",
    "StepFailure",
    "StepOutput",
    "StepRun",
    "StepTypeError",
    "UndeclaredDimensionError",
    "Variants",
    "Written",
    "__version__",
    "alignment",
    "check_step",
    "dimension_errors",
    "handle",
    "intervals",
    "reads",
    "reference",
    "samples",
    "sh",
    "step",
    "variants",
    "version",
    "write",
]

#: Version of the compiled core engine. Reported by Rust, never hardcoded.
__version__: str = _core.version()

#: Raised when a file cannot be read or does not parse as its format.
LoadError = _core.LoadError


def version() -> str:
    """Return the version of the compiled Ligase core engine."""
    return _core.version()


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


def intervals(
    path: str,
    *,
    format: str | None = None,  # noqa: A002 - the file's format, not a builtin
    organism: str | None = None,
    build: str | None = None,
) -> Intervals:
    """Load genomic intervals from a BED or GFF, inferring what they say.

    The coordinate convention comes from the format — BED is 0-based
    half-open, GFF and GTF are 1-based closed — and that is the point: a
    tool handed the wrong one produces results that are off by one base and
    look fine.

    Contigs come from the records, so they carry no length and no checksum.
    They are still compared against a reference by name, because comparing
    what is present is what makes these files checkable at all.

    Args:
        path: The BED, GFF3 or GTF, optionally gzipped.
        format: `bed`, `gff`, `gff3` or `gtf`, when the filename does not
            say. A format is never guessed: guessing it wrong shifts every
            interval by a base.
        organism: The organism. An interval file does not record it.
        build: A human label for the reference build. Display only.

    Returns:
        The loaded intervals.

    Raises:
        LoadError: If the file cannot be read, is not in the format given,
            or its format cannot be told from its name.
    """
    return Intervals(_core.load_intervals(path, format, organism, build))


def samples(
    pattern: str,
    *,
    paired: bool = False,
    organism: str | None = None,
) -> Samples:
    """Find sequencing runs and group them into samples.

    Args:
        pattern: A glob, e.g. `data/*.fastq.gz`. Matches are taken in sorted
            order, so the samples come back in a stable order on every
            machine.
        paired: Whether the files are mates of a pair. When true, they are
            paired by filename — `<sample>_1`/`<sample>_2` or
            `<sample>_R1`/`<sample>_R2` — and a file whose mate cannot be
            identified is an error rather than a guess. Mis-pairing two
            samples' files would produce results for a mixture of two
            people without anything crashing.
        organism: The organism. A FASTQ does not record it.

    Returns:
        The samples, in the order their files appeared. Use `.map()` to
        apply a chain of steps to every one of them.

    Raises:
        LoadError: If a file cannot be read, or the files cannot be grouped
            into whole samples.
    """
    found = sorted(glob(pattern))
    return Samples(
        [Reads(core) for core in _core.load_samples(found, paired, organism)]
    )


# -------------------------------------------------------------- checking --


def dimension_errors(step: str, bindings: Mapping[str, Dataset]) -> list[CheckError]:
    """Return every dimension disagreement among a step's inputs.

    One step on its own, for when you are not building a pipeline. A whole
    pipeline is checked with `lg.Pipeline(...).check()`, which also knows
    what each step requires of the ones before it.

    Args:
        step: The step's name, used in the messages.
        bindings: The step's inputs, by the name the step knows each by.

    Returns:
        One error per disagreement, most general first. Empty if the step's
        inputs are consistent. Every error carries the full list in its
        `errors` attribute.
    """
    inputs = [(name, data._core) for name, data in bindings.items()]
    return _build_errors(_core.check_step(step, inputs))


def check_step(step: str, bindings: Mapping[str, Dataset]) -> None:
    """Check a step's inputs, raising on the first disagreement.

    Every disagreement is found before anything is raised, so the raised
    error's `errors` attribute holds all of them. Nothing is computed.

    Args:
        step: The step's name, used in the messages.
        bindings: The step's inputs, by the name the step knows each by.

    Raises:
        CheckError: If any two inputs disagree. The subclass says what kind
            of disagreement it is: `ContigNamingError`, `ContigOrderError`
            or `UndeclaredDimensionError`, all of which are
            `DimensionError`s.
    """
    errors = dimension_errors(step, bindings)
    if errors:
        raise errors[0]
