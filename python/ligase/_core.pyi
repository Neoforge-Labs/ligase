"""Type stubs for the compiled Rust extension module `ligase._core`.

This surface is private. The public API is in `ligase/__init__.py`, which is
where the docstrings and the exception hierarchy live.
"""

from collections.abc import Callable, Sequence
from typing import final

class LoadError(Exception):
    """A file could not be read or did not parse as the expected format."""

class RemoteError(Exception):
    """A remote input could not be fetched, or is not what its digest says."""

class FrameError(ValueError):
    """A reference is not numbered along the frame a step's coordinates mean.

    Private plumbing: `ligase.hivdb.interpret` turns this into the public
    `ReferenceFrameError`, which is a `DimensionError` like every other
    disagreement about data.
    """

@final
class Dataset:
    @property
    def kind(self) -> str: ...
    @property
    def path(self) -> str | None: ...
    def applicable(self) -> list[str]: ...
    def dimension(self, name: str) -> tuple[str, str] | None: ...
    def unknown_reason(self, name: str) -> str | None: ...
    def notes(self) -> list[str]: ...
    def contigs(self) -> list[tuple[str, int | None, str | None]] | None: ...
    def files(self) -> list[str]: ...
    def origin(self) -> tuple[str, str, str, int, str] | None: ...
    def samples(self) -> list[str] | None: ...

def version() -> str: ...
def conversion_axis(name: str) -> str: ...
def conversion_target_parameter() -> str: ...
def map_identifiers(
    entities: str,
    id_column: str,
    table: str,
    table_id_column: str,
    column: str,
    out: str,
) -> list[tuple[str, str]]: ...
def load_reference(
    path: str,
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
    frame: tuple[str, int] | None = None,
    release: str | None = None,
) -> Dataset: ...
def resolve_remote(
    locator: str,
    kind: str,
    digest: str | None = None,
    cache: str | None = None,
    offline: bool = False,
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
    frame: tuple[str, int] | None = None,
    release: str | None = None,
    samples: list[str] | None = None,
    namespace: str | None = None,
    subject: str | None = None,
) -> Dataset: ...
def load_entity_set(
    path: str,
    id_column: str | None = None,
    organism: str | Sequence[str] | None = None,
    identifier_namespace: str | None = None,
    subject: str | None = None,
    release: str | None = None,
) -> Dataset: ...
def load_alignment(
    path: str,
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
    samples: list[str] | None = None,
) -> Dataset: ...
def load_variants(
    path: str,
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
    samples: list[str] | None = None,
) -> Dataset: ...
def load_reads(
    r1: str,
    r2: str | None = None,
    organism: str | Sequence[str] | None = None,
    samples: list[str] | None = None,
) -> Dataset: ...
def load_intervals(
    path: str,
    format: str | None = None,  # noqa: A002 - the file's format, not a builtin
    organism: str | Sequence[str] | None = None,
    build: str | None = None,
) -> Dataset: ...
def load_samples(
    paths: list[str],
    paired: bool,
    organism: str | Sequence[str] | None = None,
) -> list[Dataset]: ...

# One finding: (kind, step, dimension, scope, message). The dimension is
# absent for a step-type error, which blames no dimension; the scope is
# absent for a step that stands for no single sample.
Finding = tuple[str, str, str | None, tuple[str, str] | None, str]

def check_step(
    step: str,
    bindings: list[tuple[str, Dataset]],
) -> list[Finding]: ...

# -------------------------------------------------- resistance (M6) --

def interpret_resistance(
    reference: str,
    variants: str,
    out: str,
    alignment: str | None = None,
    algorithm: str | None = None,
    mixture: float | None = None,
    depth: int | None = None,
    gene_coverage: float | None = None,
) -> list[tuple[str, str]]: ...
def resistance_algorithms() -> list[str]: ...
def default_resistance_algorithm() -> str: ...
def resistance_fingerprint() -> str: ...

# One task: node, code, command, callback, tools, parameters, citations.
# Exactly one of the command and the callback is set.
Task = tuple[
    int,
    str,
    str | None,
    int | None,
    list[str],
    list[tuple[str, str]],
    list[str],
]

# One finished step: node, step, outcome, key, output hash, canonical hash,
# output, milliseconds, and what was noticed about the output -- by its
# canonicalisation, and by the body that wrote it.
RunResult = tuple[int, str, str, str, str, str, str, int, list[str]]

# One failed step: node, step, message.
RunFailure = tuple[int, str, str]

# One audited step: node, step, scope, verdict, the two output hashes, the
# two canonical hashes, and the two durations in milliseconds.
StepAudit = tuple[int, str, str | None, str, str, str, str, str, int, int]

# One reference, pinned: parameter, path, build, accession, sequences,
# bases, checksum, fingerprint.
Reference = tuple[
    str,
    str,
    str | None,
    str | None,
    int,
    int,
    str | None,
    str,
]

# One way a step was run: step, nodes, what tells them apart, what kind of
# thing that is, tools, parameters, the command as written.
MethodsGroup = tuple[
    str,
    int,
    list[str],
    str | None,
    list[tuple[str, str]],
    list[tuple[str, str]],
    str | None,
]

# A step nothing is known about: step, what tells its nodes apart, how many.
MethodsGap = tuple[str, list[str], int]

# A methods section: the ways steps were run, the gaps, the references, the
# reference list, the tools with nothing to cite, the engine, the platform,
# the summary, the text.
MethodsReport = tuple[
    list[MethodsGroup],
    list[MethodsGap],
    list[Reference],
    list[str],
    list[str],
    str,
    str,
    str,
    str,
]

# A bundle written: root, files, steps accounted for, steps not.
Bundle = tuple[str, list[str], int, int]

# One result checked: step, scope, verdict, expected, found, same key.
ResultCheck = tuple[str, str | None, str, str, str | None, bool]

# One input checked: step, parameter, path, what was found.
InputCheck = tuple[str, str, str, str]

# A bundle checked: the results, the inputs, whether all were reproduced,
# the worst verdict, the summary, the text.
Verification = tuple[
    list[ResultCheck],
    list[InputCheck],
    bool,
    str,
    str,
    str,
]

# A `lg.write` body: where to write, and the inputs by parameter name. It
# hands back what it noticed about its own work, as a note's code and value.
Callback = Callable[[str, list[tuple[str, list[str]]]], list[tuple[str, str]]]

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
        # The registered conversion this step performs, and the target its
        # call declared.
        conversion: tuple[str, str] | None = None,
    ) -> int: ...
    def dataset(self, node: int) -> Dataset | None: ...
    def step_names(self, roots: list[int]) -> list[str]: ...
    # Every finding, the one-line summary, and the whole report as text.
    def check(self, roots: list[int]) -> tuple[list[Finding], str, str]: ...
    # Every finished step, every failure, the summary, the report as text,
    # and (files hashed, files the prefilter already knew, files parsed to
    # find what they mean).
    def run(
        self,
        roots: list[int],
        tasks: list[Task],
        cache_dir: str,
        callbacks: list[Callback],
        jobs: int | None = None,
        force: bool = False,
    ) -> tuple[list[RunResult], list[RunFailure], str, str, tuple[int, int, int]]: ...
    # Every audited step, every failure, the overall verdict, the summary,
    # and the report as text.
    def audit(
        self,
        roots: list[int],
        tasks: list[Task],
        cache_dir: str,
        callbacks: list[Callback],
        # The step's name, and the one sample to narrow to.
        subject: tuple[str, str | None],
        jobs: int | None = None,
    ) -> tuple[list[StepAudit], list[RunFailure], str, str, str]: ...
    # How every result was produced, the bundle written where one was asked
    # for, and what checking a bundle found where one was.
    def methods(
        self,
        roots: list[int],
        tasks: list[Task],
        cache_dir: str,
        callbacks: list[Callback],
        # Where to write a bundle, a bundle to check against, and files to
        # carry into the bundle as they are.
        write_up: tuple[str | None, str | None, list[str]],
        jobs: int | None = None,
    ) -> tuple[MethodsReport, Bundle | None, Verification | None]: ...
    def __len__(self) -> int: ...
