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

from . import _core, hivdb
from .audit import AuditReport, StepAudit
from .errors import (
    CheckError,
    ContigNamingError,
    ContigOrderError,
    DimensionError,
    ReferenceFrameError,
    StepDefinitionError,
    StepTypeError,
    UndeclaredDimensionError,
)
from .errors import (
    build as _build_errors,
)
from .graph import Pipeline, Report, Step, StepOutput, handle, step
from .provenance import (
    Bundle,
    InputCheck,
    Methods,
    MethodsGap,
    PinnedReference,
    ResultCheck,
    StepMethods,
    Verification,
)
from .recipes import Shell, Written, sh, write
from .run import RunError, RunReport, StepFailure, StepRun
from .samples import Samples
from .types import (
    Alignment,
    Contig,
    Dataset,
    DimensionValue,
    EntitySet,
    Intervals,
    Reads,
    Reference,
    ResistanceReport,
    Variants,
)

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

__all__ = [
    "Alignment",
    "AuditReport",
    "Bundle",
    "CheckError",
    "Contig",
    "ContigNamingError",
    "ContigOrderError",
    "Dataset",
    "DimensionError",
    "DimensionValue",
    "EntitySet",
    "InputCheck",
    "Intervals",
    "LoadError",
    "Methods",
    "MethodsGap",
    "PinnedReference",
    "Pipeline",
    "Reads",
    "Reference",
    "ReferenceFrameError",
    "Report",
    "ResistanceReport",
    "ResultCheck",
    "RunError",
    "RunReport",
    "Samples",
    "Shell",
    "Step",
    "StepAudit",
    "StepDefinitionError",
    "StepFailure",
    "StepMethods",
    "StepOutput",
    "StepRun",
    "StepTypeError",
    "UndeclaredDimensionError",
    "Variants",
    "Verification",
    "Written",
    "__version__",
    "alignment",
    "check_step",
    "dimension_errors",
    "entity_set",
    "handle",
    "hivdb",
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
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
    frame: tuple[str, int] | None = None,
    release: str | None = None,
) -> Reference:
    """Load a reference FASTA, inferring its contigs.

    Each contig's MD5 is computed here, over the uppercased sequence, as the
    SAM specification defines `@SQ M5`. Computing it ourselves is what lets
    us check an aligner's claim about which reference it used.

    Args:
        path: The FASTA, optionally gzipped.
        organism: The organism, e.g. `HIV-1`. A FASTA cannot record this.
            Give a list for data that is about more than one organism, such as a
            host-pathogen dataset, a metagenome or a xenograft; two sets of
            organisms are equal when they name the same ones, whatever the
            order.
        build: A human label such as `HXB2` or `hg38`. Display only --
            reference identity never rests on a label, so two files both
            labelled `hg38` still fail to match if their contigs disagree.
        frame: Which coordinate frame this reference's positions are
            numbered along, as `(frame, first)`: base 1 of this file is
            base `first` of `frame`, 1-based, so a whole genome declares
            `("K03455", 1)` and the usual pol slice declares
            `("K03455", 2085)`.

            Declared because no FASTA records it. A 3,012-base slice of
            K03455 and a 3,012-base genome of something else are identical
            from the outside, and a step whose own coordinates are fixed --
            `lg.hivdb.interpret` holds the HIV-1 gene windows as positions
            in K03455 -- cannot tell them apart without being told. Leave
            it unset for a whole-genome reference such as HXB2 itself,
            which is recognised by its length; a step that needs a frame
            and cannot work one out asks for it by name.
        release: Which release of the reference this is, e.g.
            `GRCh38.p14` or `Ensembl 110`. A FASTA cannot record it, and
            two releases of one build can differ in sequence while sharing
            a name. It is compared by equality: two references declare the
            same release only if their labels are identical after
            trimming, so `GRCh38.p13` and `GRCh38.p14` do not match.

    Returns:
        The loaded reference.

    Raises:
        ValueError: If `organism` names no organism (an empty list, or only
            blank names), or `release` is blank.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read or is not a FASTA.
    """
    return Reference(_core.load_reference(path, organism, build, frame, release))


def alignment(
    path: str,
    *,
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
    samples: list[str] | None = None,
) -> Alignment:
    """Load a SAM or BAM, inferring dimensions from its header.

    Both containers yield identical dimensions: a lossless
    ``samtools view -b`` changes nothing biological.

    Args:
        path: The alignment file.
        organism: The organism. SAM and BAM headers do not record it.
            Give a list for data that is about more than one organism, such as a
            host-pathogen dataset, a metagenome or a xenograft; two sets of
            organisms are equal when they name the same ones, whatever the
            order.
        build: A human label for the reference build. Display only.
        samples: The sample identifiers, overriding the `@RG SM` tags.

    Returns:
        The loaded alignment.

    Raises:
        ValueError: If `organism` names no organism: an empty list, or only
            blank names.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read or is not a SAM/BAM.
    """
    return Alignment(_core.load_alignment(path, organism, build, samples))


def variants(
    path: str,
    *,
    organism: str | Sequence[str] | None = None,
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
            Give a list for data that is about more than one organism, such as a
            host-pathogen dataset, a metagenome or a xenograft; two sets of
            organisms are equal when they name the same ones, whatever the
            order.
        build: A human label for the reference build. Display only.
        samples: The sample identifiers, overriding the header columns.

    Returns:
        The loaded variant calls.

    Raises:
        ValueError: If `organism` names no organism: an empty list, or only
            blank names.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read or is not a VCF.
    """
    return Variants(_core.load_variants(path, organism, build, samples))


def reads(
    r1: str,
    r2: str | None = None,
    *,
    organism: str | Sequence[str] | None = None,
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
            Give a list for data that is about more than one organism, such as a
            host-pathogen dataset, a metagenome or a xenograft; two sets of
            organisms are equal when they name the same ones, whatever the
            order.
        sample: The sample identifier, overriding the one taken from the
            filename.

    Returns:
        The loaded reads.

    Raises:
        ValueError: If `organism` names no organism: an empty list, or only
            blank names.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If a file cannot be read or is not a FASTQ.
    """
    samples = None if sample is None else [sample]
    return Reads(_core.load_reads(r1, r2, organism, samples))


def intervals(
    path: str,
    *,
    format: str | None = None,  # noqa: A002 - the file's format, not a builtin
    organism: str | Sequence[str] | None = None,
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
            Give a list for data that is about more than one organism, such as a
            host-pathogen dataset, a metagenome or a xenograft; two sets of
            organisms are equal when they name the same ones, whatever the
            order.
        build: A human label for the reference build. Display only.

    Returns:
        The loaded intervals.

    Raises:
        ValueError: If `organism` names no organism: an empty list, or only
            blank names.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read, is not in the format given,
            or its format cannot be told from its name.
    """
    return Intervals(_core.load_intervals(path, format, organism, build))


def entity_set(
    path: str,
    *,
    id_column: str = "id",
    organism: str | Sequence[str] | None = None,
    identifier_namespace: str | None = None,
    subject: str | None = None,
    release: str | None = None,
) -> EntitySet:
    """Load a set of genes or proteins from a TSV with a header.

    The first type here that holds no sequence. What it carries instead is
    the identifier space its names live in, which is the dimension it exists
    for: a list of HGNC symbols joined to a table of Ensembl IDs overlaps in
    nothing, and until now nothing said so.

    Args:
        path: The TSV, optionally gzipped. It must have a header row.
        id_column: The column holding the identifiers. Every other column is
            an attribute, carried without being read.
        organism: The organism these entities belong to. A gene list does
            not record it. Give a list for data about more than one
            organism; two sets of organisms are equal when they name the
            same ones, whatever the order.
        identifier_namespace: Which space the identifiers are in, such as
            `HGNC symbol`, `Ensembl gene`, `Entrez gene` or
            `UniProt accession` -- or whatever an in-house list calls its
            own spelling, since a curated set's names are a space too.

            Leave it out and the space is read off the identifiers
            themselves, but only where every one of them matches a single
            known pattern: a file mixing two spaces, a file with no rows,
            and a file holding one name the engine cannot place all come
            back with the dimension unknown rather than guessed. Declared,
            it is *recorded* rather than checked -- the engine knows seven
            spaces and there are more than seven, so it cannot tell a space
            it has never heard of from a wrong declaration.
        subject: What the list is about when that is one named thing: a
            disease, a cell line, a cohort. Compared by equality.
        release: Which release named these entities, such as
            `HGNC 2026-09` or `Ensembl 110`. Compared by equality, so
            `HGNC 2026-03` and `HGNC 2026-09` do not match -- which is the
            point, because a symbol can mean different genes in two
            releases.

    Returns:
        The loaded set. Anything the loader noticed about the identifiers --
        a symbol a spreadsheet has turned into a date, say -- is on `notes`.

    Raises:
        ValueError: If `organism` names no organism (an empty list, or only
            blank names), or if `identifier_namespace`, `subject` or
            `release` is blank.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
        LoadError: If the file cannot be read, has no header row, has no
            column named `id_column`, or holds a row its header cannot
            account for.
    """
    return EntitySet(
        _core.load_entity_set(
            path, id_column, organism, identifier_namespace, subject, release
        )
    )


def samples(
    pattern: str,
    *,
    paired: bool = False,
    organism: str | Sequence[str] | None = None,
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
            Give a list for data that is about more than one organism, such as a
            host-pathogen dataset, a metagenome or a xenograft; two sets of
            organisms are equal when they name the same ones, whatever the
            order.

    Returns:
        The samples, in the order their files appeared. Use `.map()` to
        apply a chain of steps to every one of them.

    Raises:
        ValueError: If `organism` names no organism: an empty list, or only
            blank names.
        TypeError: If `organism` is neither a string nor a sequence of
            strings.
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
