"""Biological data types, each carrying the dimensions of its data.

These are the types a step annotates its parameters and its return with. The
annotations are not decoration: they are the only thing that says what a
step consumes and produces, because building a graph never runs a step.
"""

from __future__ import annotations

from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Mapping

    from ligase import _core

__all__ = [
    "KINDS",
    "Alignment",
    "Contig",
    "Dataset",
    "DimensionValue",
    "EntitySet",
    "Intervals",
    "Reads",
    "Reference",
    "RemoteOrigin",
    "ResistanceReport",
    "Variants",
]


@dataclass(frozen=True, slots=True)
class Contig:
    """One sequence in a reference.

    Attributes:
        name: The name as written: `chr1`, `1`, `K03455.1`.
        length: Length in bases, or None when the file records no length.
        md5: MD5 of the uppercased sequence, as the SAM specification defines
            `@SQ M5`, or None when neither recorded nor computable.
    """

    name: str
    length: int | None
    md5: str | None


@dataclass(frozen=True, slots=True)
class DimensionValue:
    """A dimension's value, and where it came from.

    Attributes:
        name: The dimension's name, e.g. `reference`.
        value: The value, rendered for a human.
        source: Where it came from, e.g. `from BAM @SQ header: data/x.bam`.
            This is the part that makes an error actionable.
    """

    name: str
    value: str
    source: str


@dataclass(frozen=True, slots=True)
class RemoteOrigin:
    """Where a dataset was fetched from.

    On `dataset.origin` for anything `lg.remote` produced, and `None` for a
    file somebody already had. `digest` is the value to write into
    `lg.remote(digest=...)` to make every later run a checked one.

    Attributes:
        locator: What the pipeline asked for, e.g. `ncbi:K03455.1`. Not the
            URL that answered, which can differ and is recorded separately.
        digest: The digest of what the files *mean*, which is what the pin
            is compared against.
        bytes: The digest of their bytes. It can move while `digest` does
            not: a source that reflows a FASTA has changed no biology.
        fetched_at: When they were fetched, as seconds since the epoch.
        verdict: What this input's reproducibility rests on. `pinned` means
            the source promises the same bytes forever, so anyone can
            obtain this input and check it.
    """

    locator: str
    digest: str
    bytes: str
    fetched_at: int
    verdict: str


class Dataset:
    """Data of a known kind, carrying the dimensions inferred from it."""

    __slots__ = ("_core",)

    def __init__(self, core: _core.Dataset) -> None:
        """Wrap a dataset loaded by the core engine."""
        self._core = core

    @property
    def kind(self) -> str:
        """The name of the kind of data this holds."""
        return self._core.kind

    @property
    def path(self) -> str | None:
        """The file this was loaded from."""
        return self._core.path

    @property
    def file(self) -> str:
        """The file this data is, for code that has to open it.

        `path` is the honest answer to "where did this come from, if
        anywhere?", so it can be `None` — a step's output carries dimensions
        worked out before anything ran, and has no file of its own until it
        does. This is the answer for the other question, the one a
        `lg.write` body asks:

            return lg.write(lambda out: interpret(v.file, out))

        Inside that function the steps above have run, so there is a file,
        and saying so in the type is what keeps a body from having to prove
        it.

        Raises:
            ValueError: If this data has no file behind it at all.
        """
        found = self._core.path
        if found is None:
            raise ValueError(
                f"this {self.kind} has no file behind it, so there is "
                "nothing to open. A step's output has none until the step "
                "has run; `path` is the accessor that admits to that"
            )
        return found

    @property
    def files(self) -> tuple[str, ...]:
        """Every file this data is, in order.

        One for most kinds and two for a pair of FASTQ mates, in mate
        order. `file` names the primary one, which is what a message
        quotes; this is what code that has to open all of them wants.
        """
        return tuple(self._core.files())

    @property
    def origin(self) -> RemoteOrigin | None:
        """Where this was fetched from, or `None` if it was not fetched.

        The overwhelming majority of inputs are files somebody has, and for
        those this says nothing by being `None`.
        """
        found = self._core.origin()
        if found is None:
            return None
        locator, digest, raw, fetched_at, verdict = found
        return RemoteOrigin(
            locator=locator,
            digest=digest,
            bytes=raw,
            fetched_at=fetched_at,
            verdict=verdict,
        )

    @property
    def applicable(self) -> tuple[str, ...]:
        """The dimensions that mean anything for this kind of data.

        A dimension outside this set is not a gap to be filled in: asking a
        FASTA for its quality encoding is a category error.
        """
        return tuple(self._core.applicable())

    @property
    def dimensions(self) -> Mapping[str, DimensionValue]:
        """Every dimension that has a known value, by name."""
        found = {}
        for name in self._core.applicable():
            pair = self._core.dimension(name)
            if pair is not None:
                found[name] = DimensionValue(name, pair[0], pair[1])
        return MappingProxyType(found)

    def dimension(self, name: str) -> DimensionValue | None:
        """One dimension's value, or None if it is unknown or inapplicable."""
        pair = self._core.dimension(name)
        if pair is None:
            return None
        return DimensionValue(name, pair[0], pair[1])

    def unknown_reason(self, name: str) -> str | None:
        """Why an applicable dimension has no value.

        None when the dimension is known, or when it does not apply to this
        kind of data at all.
        """
        return self._core.unknown_reason(name)

    @property
    def notes(self) -> tuple[str, ...]:
        """What loading this noticed, as one sentence each.

        Usually empty. A note is not a failure and never stops anything: it
        is something true of the data that you cannot act on without being
        told. A gene list is the file this exists for — its symbols can
        arrive already corrupted by a spreadsheet, which is a fact about the
        input rather than about any step, so no step's output could report
        it.
        """
        return tuple(self._core.notes())

    @property
    def contigs(self) -> list[Contig] | None:
        """The contigs of the reference this data is on, in file order."""
        contigs = self._core.contigs()
        if contigs is None:
            return None
        return [Contig(name, length, md5) for name, length, md5 in contigs]

    @property
    def samples(self) -> list[str] | None:
        """The samples this data covers, in order."""
        return self._core.samples()

    def __repr__(self) -> str:
        """Identify the dataset by kind and origin."""
        where = self.path or "<in memory>"
        return f"<{type(self).__name__} {self.kind} {where}>"


class Reads(Dataset):
    """Unaligned sequencing reads (FASTQ)."""

    __slots__ = ()


class Reference(Dataset):
    """A reference sequence (FASTA)."""

    __slots__ = ()


class Alignment(Dataset):
    """Reads placed on a reference (SAM/BAM)."""

    __slots__ = ()


class Variants(Dataset):
    """Variant calls (VCF)."""

    __slots__ = ()


class Intervals(Dataset):
    """Genomic intervals (BED/GFF).

    The one type whose `coordinates` dimension varies by format: BED is
    0-based half-open, GFF is 1-based closed. Feeding one to a tool
    expecting the other produces results that are plausible and off by one.
    """

    __slots__ = ()


class ResistanceReport(Dataset):
    """An interpreted drug-resistance report."""

    __slots__ = ()


class EntitySet(Dataset):
    """A set of genes or proteins, named in one identifier space.

    The first type here that holds no sequence, and the one whose dimension
    is the commonest silent bug in biology: a list of HGNC symbols joined to
    a table of Ensembl IDs overlaps in nothing, and nothing says so.

    It has no coordinate system, no contigs, no read pairing and no quality
    scores. Those are not gaps to be filled in -- a list of gene names has
    no such properties to have -- so asking for one gets `None` rather than
    a reason.
    """

    __slots__ = ()


#: Every type a step may annotate, by the name the engine knows it as.
#:
#: Annotating a step with anything else is refused when it is decorated,
#: rather than when it runs: the graph has to know what flows along an edge,
#: and a connection it cannot type is a connection it cannot check.
KINDS: Mapping[str, type[Dataset]] = MappingProxyType(
    {
        "Reads": Reads,
        "Reference": Reference,
        "Alignment": Alignment,
        "Variants": Variants,
        "Intervals": Intervals,
        "ResistanceReport": ResistanceReport,
        "EntitySet": EntitySet,
    }
)
