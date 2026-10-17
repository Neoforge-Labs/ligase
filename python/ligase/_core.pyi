"""Type stubs for the compiled Rust extension module `ligase._core`.

This surface is private. The public API is in `ligase/__init__.py`, which is
where the docstrings and the exception hierarchy live.
"""

from typing import final

class LoadError(Exception):
    """A file could not be read or did not parse as the expected format."""

@final
class Dataset:
    @property
    def kind(self) -> str: ...
    @property
    def path(self) -> str | None: ...
    def applicable(self) -> list[str]: ...
    def dimension(self, name: str) -> tuple[str, str] | None: ...
    def unknown_reason(self, name: str) -> str | None: ...
    def contigs(self) -> list[tuple[str, int | None, str | None]] | None: ...
    def samples(self) -> list[str] | None: ...

def version() -> str: ...
def load_reference(
    path: str,
    organism: str | None = None,
    build: str | None = None,
) -> Dataset: ...
def load_alignment(
    path: str,
    organism: str | None = None,
    build: str | None = None,
    samples: list[str] | None = None,
) -> Dataset: ...
def load_variants(
    path: str,
    organism: str | None = None,
    build: str | None = None,
    samples: list[str] | None = None,
) -> Dataset: ...
def load_reads(
    r1: str,
    r2: str | None = None,
    organism: str | None = None,
    samples: list[str] | None = None,
) -> Dataset: ...
def load_intervals(
    path: str,
    format: str | None = None,  # noqa: A002 - the file's format, not a builtin
    organism: str | None = None,
    build: str | None = None,
) -> Dataset: ...
def load_samples(
    paths: list[str],
    paired: bool,
    organism: str | None = None,
) -> list[Dataset]: ...

# One finding: (kind, step, dimension, message). The dimension is absent for
# a step-type error, which blames no dimension.
Finding = tuple[str, str, str | None, str]

def check_step(
    step: str,
    bindings: list[tuple[str, Dataset]],
) -> list[Finding]: ...

@final
class Graph:
    def __init__(self) -> None: ...
    def add_input(self, label: str, dataset: Dataset) -> int: ...
    def add_step(
        self,
        name: str,
        output: str,
        # (parameter, declared kind, node) per dataset argument.
        arguments: list[tuple[str, str | None, int]],
        coordinates: str | None = None,
    ) -> int: ...
    def dataset(self, node: int) -> Dataset | None: ...
    def step_names(self, roots: list[int]) -> list[str]: ...
    # Every finding, plus the one-line summary.
    def check(self, roots: list[int]) -> tuple[list[Finding], str]: ...
    def __len__(self) -> int: ...
