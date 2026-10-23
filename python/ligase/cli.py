"""The `ligase` command.

`ligase inspect FILE...` is the one command that needs no pipeline. It loads
each file, says what kind it read it as, and prints every dimension the
loader worked out along with where it read it from — and, given two or more
files, compares them the way a step's inputs are compared and prints the
diagnostic. Nothing is executed and nothing is cached: it is the question
"does Ligase think these two files agree?" asked of files you already have.

`ligase check` imports a pipeline file and type-checks every step in it. The
module *is* executed, the way a Snakefile is: that is how a Python pipeline
declares itself. The step bodies are not, because `@lg.step` defers them — so
nothing reads sequence, nothing shells out to a tool, and a mistake surfaces
in milliseconds instead of after an overnight run.

`ligase run` does the same and then computes whatever is not already known:
a step runs when something that could change its result has changed, and
not otherwise. Nothing runs until the whole pipeline checks clean.

`ligase audit <step>` runs one step twice and says whether it is
reproducible, which is the one question a cache is in the business of never
asking. It distinguishes a step whose *bytes* move from one whose *result*
does: the first is every tool that records its own command line, and the
second is a finding.

`ligase run` also resolves the pipeline's remote inputs, which happens
before the graph is built: a remote whose digest does not match the one the
pipeline pins is a failure before any step has run, and on a second run
nothing is fetched because the bytes and their digest are already in the
store.

`ligase methods` says how the results were produced: tool versions,
parameters, every reference by accession and checksum, and the references to
cite. It runs the pipeline first — normally and through the cache — because
a provenance record is filed under a step's key, and computing keys is what
a run does. `--bundle` writes everything another machine needs in order to
check the results, and `--verify` is that check.
"""

from __future__ import annotations

import argparse
import importlib.util
import sys
from pathlib import Path
from types import MappingProxyType
from typing import TYPE_CHECKING

from . import (
    _core,
    alignment,
    dimension_errors,
    entity_set,
    intervals,
    reads,
    reference,
    remotes,
    variants,
)
from .errors import CheckError
from .graph import CACHE_DIR, Pipeline
from .run import RunError

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping, Sequence
    from types import ModuleType

    from .types import Dataset

__all__ = ["main"]

#: What `ligase check` reads when no path is given.
DEFAULT_PIPELINE = "pipeline.py"

#: Exit codes. 1 means the pipeline is wrong, 2 means we could not look.
FOUND_ERRORS = 1
COULD_NOT_CHECK = 2

#: Which loader `inspect` reads which extension with.
#:
#: Private to the command, and deliberately neither in the engine nor in the
#: public API: which suffix means a FASTA is a question about filenames
#: rather than about biology, and a pipeline answers it by naming the loader
#: it wants. `inspect` is the one surface with nobody to ask.
#:
#: A compression suffix is stripped before the lookup, because every loader
#: opens a gzipped file.
LOADERS: Mapping[str, Callable[[str], Dataset]] = MappingProxyType(
    {
        ".fasta": reference,
        ".fa": reference,
        ".fna": reference,
        ".fastq": reads,
        ".fq": reads,
        ".sam": alignment,
        ".bam": alignment,
        ".vcf": variants,
        ".bed": intervals,
        ".gff": intervals,
        ".gff3": intervals,
        ".gtf": intervals,
        ".tsv": entity_set,
    }
)


def build_parser() -> argparse.ArgumentParser:
    """Build the argument parser, with every command and flag on it.

    Separate from `main` so that the parser can be read without running
    anything: the documentation site generates its command-line reference
    from this, which is what stops that page drifting from `--help`.

    Returns:
        The parser, with its four subcommands attached.
    """
    parser = argparse.ArgumentParser(
        prog="ligase",
        description="Type-safe, reproducible computational biology pipelines.",
    )
    commands = parser.add_subparsers(dest="command", required=True)
    # First, because it is the first thing someone does: it needs no
    # pipeline, no installation of anyone's tools and no commitment, and the
    # generated reference lists commands in the order they are added here.
    inspect = commands.add_parser(
        "inspect",
        help="say what Ligase makes of files you already have",
        description=(
            "Load each file and print every dimension the loader inferred, "
            "with where it read each one from. Given two or more files, "
            "compare them the way a step's inputs are compared and print "
            "the diagnostic. No pipeline, no cache, nothing executed."
        ),
    )
    inspect.add_argument(
        "files",
        nargs="+",
        type=Path,
        metavar="FILE",
        help=(
            "the files to inspect, read by extension; two or more are "
            "compared against each other"
        ),
    )

    check = commands.add_parser(
        "check",
        help="type-check a pipeline without running it",
        description=(
            "Build the pipeline's graph and check every step's dimensions. "
            "The pipeline file is imported; the step bodies are not run. "
            "A remote input resolves here, because its dimensions cannot "
            "be read until its bytes are in hand -- so this is also where "
            "a digest that does not match is reported."
        ),
    )
    check.add_argument(
        "pipeline",
        nargs="?",
        default=DEFAULT_PIPELINE,
        type=Path,
        help=f"the pipeline to check (default: {DEFAULT_PIPELINE})",
    )
    check.add_argument(
        "--cache",
        type=Path,
        metavar="DIR",
        help=(
            f"where to look for the pipeline's remote inputs (default: ./{CACHE_DIR})"
        ),
    )

    run = commands.add_parser(
        "run",
        help="check a pipeline, then compute what is not already known",
        description=(
            "Check the pipeline and run it. A step runs when its code, its "
            "parameters, its inputs' contents or a tool's version have "
            "changed, and not otherwise; independent steps run at once."
        ),
    )
    run.add_argument(
        "pipeline",
        nargs="?",
        default=DEFAULT_PIPELINE,
        type=Path,
        help=f"the pipeline to run (default: {DEFAULT_PIPELINE})",
    )
    run.add_argument(
        "-j",
        "--jobs",
        type=int,
        metavar="N",
        help="how many steps may run at once (default: one per core)",
    )
    run.add_argument(
        "--force",
        action="store_true",
        help="run every step, cache or no cache",
    )
    run.add_argument(
        "--cache",
        type=Path,
        metavar="DIR",
        help=f"where to keep the cache (default: ./{CACHE_DIR})",
    )
    # Mutually exclusive, so `--replay --refresh` is an error rather than a
    # precedence puzzle: one says never ask the source and the other says
    # always ask it, and a run that did both would be reporting a result
    # nobody could repeat.
    queries = run.add_mutually_exclusive_group()
    queries.add_argument(
        "--replay",
        action="store_true",
        help=(
            "serve every recorded query from its record and reach no "
            "network; a query with no record is an error"
        ),
    )
    queries.add_argument(
        "--refresh",
        action="store_true",
        help=(
            "ask every recorded query's source again and report what "
            "changed, keeping the existing records"
        ),
    )

    audit = commands.add_parser(
        "audit",
        help="run one step twice and say whether it is reproducible",
        description=(
            "Run the pipeline, then run one step twice more and compare "
            "what came out — both as bytes and as what those bytes mean. A "
            "step whose bytes move while its result does not is "
            "reproducible, which is the ordinary case for a tool that "
            "records its own command line."
        ),
    )
    audit.add_argument("step", help="the step to audit, by name")
    audit.add_argument(
        "pipeline",
        nargs="?",
        default=DEFAULT_PIPELINE,
        type=Path,
        help=f"the pipeline it is in (default: {DEFAULT_PIPELINE})",
    )
    audit.add_argument(
        "--sample",
        metavar="ID",
        help="audit only the node for this sample (default: all of them)",
    )
    audit.add_argument(
        "-j",
        "--jobs",
        type=int,
        metavar="N",
        help="how many steps may run at once in the run beforehand",
    )
    audit.add_argument(
        "--cache",
        type=Path,
        metavar="DIR",
        help=f"where to keep the cache (default: ./{CACHE_DIR})",
    )

    methods = commands.add_parser(
        "methods",
        help="say how the results were produced, and bundle it up",
        description=(
            "Run the pipeline, then write a methods paragraph naming every "
            "tool's version, every parameter, every reference by accession "
            "and checksum, and the references to cite. `--bundle` writes a "
            "reproducibility bundle; `--verify` checks one against what "
            "this machine produces."
        ),
    )
    methods.add_argument(
        "pipeline",
        nargs="?",
        default=DEFAULT_PIPELINE,
        type=Path,
        help=f"the pipeline to write up (default: {DEFAULT_PIPELINE})",
    )
    methods.add_argument(
        "--bundle",
        type=Path,
        metavar="DIR",
        help="write a reproducibility bundle into this directory",
    )
    methods.add_argument(
        "--verify",
        type=Path,
        metavar="DIR",
        help="check the bundle in this directory against this machine",
    )
    methods.add_argument(
        "-j",
        "--jobs",
        type=int,
        metavar="N",
        help="how many steps may run at once in the run beforehand",
    )
    methods.add_argument(
        "--cache",
        type=Path,
        metavar="DIR",
        help=f"where to keep the cache (default: ./{CACHE_DIR})",
    )

    return parser


def _mode_of(arguments: argparse.Namespace) -> str | None:
    """Which query mode the flags ask for, or `None` for the default.

    `None` rather than `"auto"` so that a command without the flags leaves
    whatever a script already configured alone, which is how every other
    per-run setting behaves.
    """
    if getattr(arguments, "replay", False):
        return "replay"
    if getattr(arguments, "refresh", False):
        return "refresh"
    return None


def main(argv: Sequence[str] | None = None) -> int:
    """Run the command line interface.

    Args:
        argv: The arguments, excluding the program name. Taken from
            `sys.argv` when not given.

    Returns:
        0 when everything worked, 1 when a pipeline has errors or a step
        failed or inspected files disagree, 2 when the pipeline or a file
        could not be read at all.
    """
    arguments = build_parser().parse_args(argv)

    # Before anything imports a pipeline file. A remote resolves when the
    # pipeline declares it -- that is what makes a wrong digest fail before
    # any step runs -- so by the time `pipeline.run(cache=...)` is reached
    # the fetching is already done, and `--cache` would have been ignored.
    remotes.configure(
        cache=getattr(arguments, "cache", None),
        mode=_mode_of(arguments),
    )

    # Also before the import, and for the same reason: a bundle's recorded
    # responses have to be in the store before the pipeline declares the
    # queries that replay them. This is what lets a reviewer check a bundle
    # on a clean machine with no network and no API key.
    verifying = getattr(arguments, "verify", None)
    if verifying is not None:
        try:
            remotes.seed(verifying)
        except _core.RemoteError as raised:
            # Not fatal. A directory that is not a bundle, or a bundle with
            # no records, is reported by `--verify` itself in terms a reader
            # can act on; failing here would replace that with a complaint
            # about seeding.
            print(f"could not read the bundle's records: {raised}", file=sys.stderr)

    if arguments.command == "inspect":
        return _inspect(arguments.files)
    if arguments.command == "run":
        return _run(
            arguments.pipeline, arguments.jobs, arguments.force, arguments.cache
        )
    if arguments.command == "methods":
        return _methods(
            arguments.pipeline,
            arguments.bundle,
            arguments.verify,
            arguments.jobs,
            arguments.cache,
        )
    if arguments.command == "audit":
        return _audit(
            arguments.pipeline,
            arguments.step,
            arguments.sample,
            arguments.jobs,
            arguments.cache,
        )
    return _check(arguments.pipeline)


def _suffix(path: Path) -> str:
    """The extension that decides the format, ignoring a compression one.

    Every loader opens a gzipped file, so `genome.fasta.gz` is a FASTA.
    """
    name = path.name.lower()
    for compression in (".gz", ".bgz"):
        name = name.removesuffix(compression)
    return Path(name).suffix


def _names(paths: Sequence[Path]) -> list[str]:
    """A short, distinct handle for each file, for a diagnostic to use by.

    The basename, because that is what makes a diagnostic read like the one
    a pipeline prints — where the handles are `ref` and `aln` — while the
    full path travels on the same line inside the dimension's source.

    Distinctness is not cosmetic. The handles are the keys the comparison is
    given, so two files that shared one would be *one* input and the second
    would be silently dropped. Files sharing a basename therefore fall back
    to the path as typed, and a path typed twice is numbered.
    """
    basenames = [path.name for path in paths]
    names = [
        name if basenames.count(name) == 1 else str(path)
        for name, path in zip(basenames, paths, strict=True)
    ]

    seen: dict[str, int] = {}
    distinct = []
    for name in names:
        seen[name] = seen.get(name, 0) + 1
        distinct.append(name if seen[name] == 1 else f"{name} ({seen[name]})")
    return distinct


def _read(path: Path) -> Dataset | None:
    """Load one file for `inspect`, or say on stderr why it could not be.

    Returns None rather than raising. A file that will not open or will not
    parse is a fact about the file, and reporting it as a traceback would
    make it look like a fault in the tool that went to look.
    """
    if not path.exists():
        _report(f"no such file: {path}")
    elif path.is_dir():
        _report(f"{path} is a directory; name the files in it")
    elif not path.is_file():
        _report(f"{path} is not a file")
    elif (loader := LOADERS.get(_suffix(path))) is None:
        # Naming what *is* read turns a refusal into an answer. The suffix
        # is quoted where there is one, since "nothing reads ``" would be a
        # riddle for a file called `README`.
        known = ", ".join(sorted(LOADERS))
        reads = f"`{_suffix(path)}` files" if _suffix(path) else "a file named that"
        _report(
            f"{path}: nothing here reads {reads}; the extensions this knows "
            f"are {known} (each also with .gz)"
        )
    else:
        try:
            return loader(str(path))
        except Exception as error:
            _report(f"{path} could not be read: {error}")
    return None


def _inspect(paths: Sequence[Path]) -> int:
    """Describe each file, then compare them as a step's inputs are compared.

    One path with two sections rather than two paths: the descriptions are
    the evidence for the diagnostic, and the diagnostic's lines are the
    description's lines, so printing one without the other would withhold
    the half that explains it. A single file is the degenerate case — there
    are no pairs, so there is no comparison.
    """
    loaded: list[tuple[str, Dataset]] = []
    unreadable: list[Path] = []

    for name, path in zip(_names(paths), paths, strict=True):
        data = _read(path)
        if data is None:
            unreadable.append(path)
        else:
            loaded.append((name, data))

    for name, data in loaded:
        print(data._core.describe(name))
        print()

    files = f"{len(paths)} {_plural(len(paths), 'file')}"

    # A verdict over the files that happened to load reads as a verdict over
    # all of them, so there is no verdict at all.
    if unreadable:
        print(
            f"nothing to inspect: {files} could not be read."
            if len(unreadable) == len(paths)
            else (
                f"not comparing: {len(unreadable)} of {files} could not be "
                "read, and a file nobody could read has no verdict."
            )
        )
        return COULD_NOT_CHECK

    if len(loaded) < 2:
        print(
            f"{files}, so nothing to compare: two or more are compared the "
            "way one step's inputs are."
        )
        return 0

    # The same `check_step` a step's inputs go through. The step is named
    # `inspect` because that is what is doing the comparing; there is no
    # step, and the diagnostic is otherwise the one a pipeline would print.
    errors = dimension_errors("inspect", dict(loaded))
    for disagreement in errors:
        print(disagreement)
        print()

    compared = _core.comparable([data._core for _, data in loaded])
    on = f", compared on {_english(compared)}" if compared else ""

    if errors:
        # The count leads with the files, not with `compared`, because a
        # dimension nothing states can still be a disagreement: two gene
        # lists that each say nothing about their identifier space do not
        # agree, they fail to answer a question the comparison demands.
        found = f"{len(errors)} {_plural(len(errors), 'disagreement')}"
        print(f"{files}{on}: {found}.")
        return FOUND_ERRORS

    if not compared:
        # "They agree" over files sharing no stated dimension is false
        # comfort: a FASTQ and a BED agree the way two sentences in
        # different languages agree.
        print(
            f"{files}, but nothing was compared: no dimension is stated by two of them."
        )
        return 0

    print(f"{files}{on}: no disagreement.")
    return 0


def _check(path: Path) -> int:
    pipelines = _pipelines(path)
    if isinstance(pipelines, int):
        return pipelines

    found = 0
    for pipeline in pipelines:
        report = pipeline.report()
        found += len(report.errors)
        steps = len(pipeline.steps)
        print(f"checked {steps} {_plural(steps, 'step')} in {path}")
        print()
        # The engine lays the report out, so every surface reads the same and
        # one problem across four hundred samples is stated once.
        print(report.text)

    return FOUND_ERRORS if found else 0


def _run(path: Path, jobs: int | None, force: bool, cache: Path | None) -> int:
    pipelines = _pipelines(path)
    if isinstance(pipelines, int):
        return pipelines

    found = 0
    for pipeline in pipelines:
        steps = len(pipeline.steps)
        print(f"running {steps} {_plural(steps, 'step')} in {path}")
        print()
        try:
            report = pipeline.run(jobs=jobs, force=force, cache=cache)
        except CheckError as error:
            # A pipeline that does not type-check has not run, and the check
            # report is what the user needs, not a traceback.
            print(pipeline.report().text)
            found += len(error.errors)
            continue
        except RunError as error:
            report = error.report
            found += len(report.failures)
        # Worded by the engine, so a script and the command line agree.
        print(report.text)

    return FOUND_ERRORS if found else 0


def _audit(
    path: Path,
    step: str,
    sample: str | None,
    jobs: int | None,
    cache: Path | None,
) -> int:
    pipelines = _pipelines(path)
    if isinstance(pipelines, int):
        return pipelines

    # A file may hold more than one pipeline, and a step belongs to the ones
    # that run it. Auditing is about a step, so the pipelines that do not
    # have it are not failures — they are not what was asked about.
    having = [pipeline for pipeline in pipelines if step in pipeline.steps]
    if not having:
        known = sorted({name for pipeline in pipelines for name in pipeline.steps})
        return _cannot(
            f"no step named `{step}` in {path}; it has "
            + ", ".join(f"`{name}`" for name in known)
        )

    found = 0
    for pipeline in having:
        for_sample = "" if sample is None else f" for {sample}"
        print(f"auditing `{step}`{for_sample} in {path}")
        print()
        try:
            report = pipeline.audit(step, sample=sample, jobs=jobs, cache=cache)
        except CheckError as error:
            # Nothing is audited in a pipeline that does not type-check, and
            # the check report is what the user needs.
            print(pipeline.report().text)
            found += len(error.errors)
            continue
        except ValueError as error:
            # A sample no node of the step is for — the step itself is known
            # by now. Either way it is a mistake in the command rather than a
            # finding about the pipeline, so it is not a finding's exit code.
            return _cannot(str(error))
        print(report.text)

        # A step that does not agree with itself is a finding, and a finding
        # is what a non-zero exit code is for.
        found += len(report.unreproducible) + len(report.failures)

    return FOUND_ERRORS if found else 0


def _methods(
    path: Path,
    bundle: Path | None,
    verify: Path | None,
    jobs: int | None,
    cache: Path | None,
) -> int:
    pipelines = _pipelines(path)
    if isinstance(pipelines, int):
        return pipelines

    found = 0
    for pipeline in pipelines:
        steps = len(pipeline.steps)
        print(f"writing up {steps} {_plural(steps, 'step')} in {path}")
        print()
        try:
            methods = pipeline.methods(
                bundle=bundle,
                verify=verify,
                # The pipeline file goes into the bundle, so a bundle holds
                # the pipeline that produced it and not only its steps.
                include=[path],
                jobs=jobs,
                cache=cache,
            )
        except CheckError as error:
            # Nothing is written up for a pipeline that does not type-check.
            print(pipeline.report().text)
            found += len(error.errors)
            continue
        except ValueError as error:
            # The directory given to `--verify` is not a bundle: a mistake
            # in the command rather than a finding about the pipeline.
            return _cannot(str(error))
        except RuntimeError as error:
            # A step failed, or a bundle could not be written. Either way
            # there is nothing to write up.
            return _cannot(str(error))

        # Worded by the engine, so a script and the command line agree.
        print(methods.text)

        if methods.bundle is not None:
            print()
            files = len(methods.bundle.files)
            print(f"wrote {files} {_plural(files, 'file')} to {methods.bundle.root}")

        if methods.verification is not None:
            print()
            print(methods.verification.text)
            if not methods.verification.reproduced:
                found += len(methods.verification.differing)

        # A paragraph with a hole in it is a finding: something in this
        # pipeline cannot be accounted for.
        found += len(methods.gaps)

    return FOUND_ERRORS if found else 0


def _pipelines(path: Path) -> list[Pipeline] | int:
    """Every pipeline in a file, or the exit code for not being able to look."""
    if not path.is_file():
        return _cannot(f"no such pipeline: {path}")

    try:
        module = _import(path)
    # Any failure in the user's own file: report it, do not traceback.
    except Exception as error:
        return _cannot(f"{path} could not be imported: {error!r}")

    pipelines = [
        value for value in vars(module).values() if isinstance(value, Pipeline)
    ]
    if not pipelines:
        return _cannot(
            f"no lg.Pipeline found in {path}; assign one at the top level of "
            "the file, as `pipe = lg.Pipeline(...)`"
        )
    return pipelines


def _import(path: Path) -> ModuleType:
    """Import a pipeline file by path, as its own module.

    Its directory goes on the path first, so a pipeline can import the
    helpers that sit beside it.
    """
    resolved = path.resolve()
    specification = importlib.util.spec_from_file_location(
        f"ligase_pipeline_{resolved.stem}", resolved
    )
    if specification is None or specification.loader is None:
        message = f"{path} is not an importable Python file"
        raise ImportError(message)

    module = importlib.util.module_from_spec(specification)
    directory = str(resolved.parent)
    added = directory not in sys.path
    if added:
        sys.path.insert(0, directory)
    try:
        specification.loader.exec_module(module)
    finally:
        if added:
            sys.path.remove(directory)
    return module


def _cannot(message: str) -> int:
    _report(message)
    return COULD_NOT_CHECK


def _report(message: str) -> None:
    """Say something went wrong, on the stream for saying so."""
    print(f"ligase: {message}", file=sys.stderr)


def _plural(count: int, word: str) -> str:
    return word if count == 1 else f"{word}s"


def _english(words: Sequence[str]) -> str:
    """Join as English does: `a`, `a and b`, `a, b and c`.

    The engine punctuates its own lists this way; this is the one list the
    command composes itself.
    """
    if len(words) < 2:
        return "".join(words)
    return f"{', '.join(words[:-1])} and {words[-1]}"
