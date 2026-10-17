"""Biological data types, each carrying the dimensions of its data.

These are the types the golden example annotates steps with. In M1 they are
loaded from files and carry dimensions; connecting them into a graph arrives
in M2.
"""

from __future__ import annotations

from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Mapping

    from ligase import _core

__all__ = [
    "Alignment",
    "Contig",
    "Dataset",
    "DimensionValue",
    "Reads",
    "Reference",
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


class ResistanceReport(Dataset):
    """An interpreted drug-resistance report."""

    __slots__ = ()
