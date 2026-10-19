"""The `ligase` command.

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
from typing import TYPE_CHECKING

from .errors import CheckError
from .graph import CACHE_DIR, Pipeline
from .run import RunError

if TYPE_CHECKING:
    from collections.abc import Sequence
    from types import ModuleType

__all__ = ["main"]

#: What `ligase check` reads when no path is given.
DEFAULT_PIPELINE = "pipeline.py"

#: Exit codes. 1 means the pipeline is wrong, 2 means we could not look.
FOUND_ERRORS = 1
COULD_NOT_CHECK = 2


def main(argv: Sequence[str] | None = None) -> int:
    """Run the command line interface.

    Args:
        argv: The arguments, excluding the program name. Taken from
            `sys.argv` when not given.

    Returns:
        0 when everything worked, 1 when a pipeline has errors or a step
        failed, 2 when the pipeline could not be read at all.
    """
    parser = argparse.ArgumentParser(
        prog="ligase",
        description="Type-safe, reproducible computational biology pipelines.",
    )
    commands = parser.add_subparsers(dest="command", required=True)
    check = commands.add_parser(
        "check",
        help="type-check a pipeline without running it",
        description=(
            "Build the pipeline's graph and check every step's dimensions. "
            "The pipeline file is imported; the step bodies are not run."
        ),
    )
    check.add_argument(
        "pipeline",
        nargs="?",
        default=DEFAULT_PIPELINE,
        type=Path,
        help=f"the pipeline to check (default: {DEFAULT_PIPELINE})",
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

    arguments = parser.parse_args(argv)
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
    print(f"ligase: {message}", file=sys.stderr)
    return COULD_NOT_CHECK


def _plural(count: int, word: str) -> str:
    return word if count == 1 else f"{word}s"
