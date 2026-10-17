"""Type stubs for the compiled Rust extension module `ligase._core`.

This surface is private. The public API is in `ligase/__init__.py`, which is
where the docstrings and the exception hierarchy live.
"""

from collections.abc import Callable
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

# One finding: (kind, step, dimension, scope, message). The dimension is
# absent for a step-type error, which blames no dimension; the scope is
# absent for a step that stands for no single sample.
Finding = tuple[str, str, str | None, tuple[str, str] | None, str]

def check_step(
    step: str,
    bindings: list[tuple[str, Dataset]],
) -> list[Finding]: ...

# One task: node, code, command, callback, tools, parameters. Exactly one of
# the command and the callback is set.
Task = tuple[int, str, str | None, int | None, list[str], list[tuple[str, str]]]

# One finished step: node, step, outcome, key, output hash, output,
# milliseconds.
RunResult = tuple[int, str, str, str, str, str, int]

# One failed step: node, step, message.
RunFailure = tuple[int, str, str]

# A `lg.write` body: where to write, and the inputs by parameter name.
Callback = Callable[[str, list[tuple[str, list[str]]]], None]

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
    # Every finding, the one-line summary, and the whole report as text.
    def check(self, roots: list[int]) -> tuple[list[Finding], str, str]: ...
    # Every finished step, every failure, the summary, the report as text,
    # and (files hashed, files the prefilter already knew).
    def run(
        self,
        roots: list[int],
        tasks: list[Task],
        cache_dir: str,
        callbacks: list[Callback],
        jobs: int | None = None,
        force: bool = False,
    ) -> tuple[list[RunResult], list[RunFailure], str, str, tuple[int, int]]: ...
    def __len__(self) -> int: ...
