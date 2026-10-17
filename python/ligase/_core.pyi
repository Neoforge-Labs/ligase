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

# (kind, step, dimension, message) per disagreement.
def check_step(
    step: str,
    bindings: list[tuple[str, Dataset]],
) -> list[tuple[str, str, str, str]]: ...
