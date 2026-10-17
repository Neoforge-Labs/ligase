"""A set of samples, and mapping a pipeline over each of them.

`lg.samples("data/*.fastq.gz", paired=True)` globs the files, groups mates
into samples and loads each one. `samples.map(...)` then applies a chain of
steps to every sample, which is how the golden example builds one pipeline
and gets one graph per sample out of it.

The grouping happens in Rust, where it is tested: mis-pairing two samples'
files produces results for a mixture of two people and nothing crashes, so a
file whose mate cannot be identified is an error rather than a guess.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, TypeVar, overload

if TYPE_CHECKING:
    from collections.abc import Callable, Iterator

    from .types import Reads

__all__ = ["Samples"]

T = TypeVar("T")


class Samples:
    """The samples a glob matched, in the order their files appeared.

    Iterable, indexable and sized, so it can be used as an ordinary sequence
    of `Reads`. `map` is the part the golden example uses.
    """

    __slots__ = ("_samples",)

    def __init__(self, samples: list[Reads]) -> None:
        """Hold loaded reads, one entry per sample."""
        self._samples = samples

    @property
    def ids(self) -> tuple[str, ...]:
        """Each sample's identifier, in order.

        Taken from the filenames, with any mate marker removed, unless the
        reads were loaded with the identifier declared.
        """
        return tuple(_identify(sample) for sample in self._samples)

    def map(self, build: Callable[[Reads], T]) -> list[T]:
        """Apply a chain of steps to every sample.

        Nothing is executed. `build` is called once per sample and returns
        whatever the last step in the chain returns — a handle, not data —
        so the result is one output per sample, ready for `lg.Pipeline`.

        Args:
            build: What to do with one sample's reads.

        Returns:
            One result per sample, in order.
        """
        return [build(sample) for sample in self._samples]

    def __len__(self) -> int:
        """How many samples were found."""
        return len(self._samples)

    def __iter__(self) -> Iterator[Reads]:
        """Iterate the samples in order."""
        return iter(self._samples)

    @overload
    def __getitem__(self, index: int) -> Reads: ...
    @overload
    def __getitem__(self, index: slice) -> list[Reads]: ...

    def __getitem__(self, index: int | slice) -> Reads | list[Reads]:
        """One sample, or a slice of them."""
        return self._samples[index]

    def __repr__(self) -> str:
        """Identify the set by how many samples it holds."""
        return f"<Samples {len(self)} samples: {', '.join(self.ids)}>"


def _identify(sample: Reads) -> str:
    names = sample.samples
    if not names:
        return "<unnamed>"
    return names[0]
