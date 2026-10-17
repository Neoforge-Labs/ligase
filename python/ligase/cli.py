"""The `ligase` command.

`ligase check` imports a pipeline file and type-checks every step in it. The
module *is* executed, the way a Snakefile is: that is how a Python pipeline
declares itself. The step bodies are not, because `@lg.step` defers them — so
nothing reads sequence, nothing shells out to a tool, and a mistake surfaces
in milliseconds instead of after an overnight run.

`ligase run` does the same and then computes whatever is not already known:
a step runs when something that could change its result has changed, and
not otherwise. Nothing runs until the whole pipeline checks clean.

`methods` and `audit` are in `docs/golden-example.md` and arrive with the
milestones that implement them; this parser offers only what works.
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

    arguments = parser.parse_args(argv)
    if arguments.command == "run":
        return _run(
            arguments.pipeline, arguments.jobs, arguments.force, arguments.cache
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
