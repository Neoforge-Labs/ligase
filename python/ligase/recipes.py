"""What a step body returns: how to produce the output, not the output.

A step's body is never asked to compute anything. It is called once, before
the run starts, and what it hands back is a *recipe* — a command to render,
or a function to call. The engine then owns the whole run: it decides what
needs doing, runs it in parallel and caches the result, without stopping to
ask Python anything.

    @lg.step(tool="minimap2")
    def align(reads: Reads, ref: Reference) -> Alignment:
        return lg.sh(
            "minimap2 -ax sr {ref} {reads.r1} {reads.r2} | samtools sort -o {out}"
        )

`lg.sh` covers every step that shells out, which is most of them.
`lg.write` is for a step whose work is Python:

    @lg.step
    def resistance(v: Variants) -> ResistanceReport:
        return lg.write(lambda out: interpret(v.file, out))

The declared return type is the data the step produces, so that a type
checker can follow the pipeline (see `graph.py`). A recipe stands in for
that data, exactly as the handle a step call returns stands in for it — one
lie, told in one place, so that user pipelines type-check as written.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, TypeVar

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from .types import Dataset

__all__ = ["Recipe", "Shell", "Written", "sh", "write"]

#: What a step declares it produces. A recipe stands in for it.
R = TypeVar("R", bound="Dataset")


@dataclass(frozen=True, slots=True)
class Shell:
    """A shell command, with the step's inputs still to fill in.

    Attributes:
        command: The command as written, with `{placeholders}`.
    """

    command: str


@dataclass(frozen=True, slots=True)
class Written:
    """A step whose work is Python, and writes its own output.

    Attributes:
        function: Called with the path to write to, once the engine has
            decided what that path is.
    """

    function: Callable[[Path], None]


#: Either kind of recipe.
Recipe = Shell | Written


def sh(command: str) -> R:  # type: ignore[type-var]
    """Produce the output by running a shell command.

    The command is not run here and the string is not formatted here: the
    engine fills it in once it knows where the inputs are, and runs it at
    whatever width the machine allows.

    Placeholders name the step's own parameters:

    - `{ref}` — the file bound to the parameter `ref`.
    - `{reads.r1}`, `{reads.r2}` — the mates of a paired input.
    - `{min_freq}` — a plain parameter's value.
    - `{out}` — the file the step must write. A step that writes anywhere
      else produces nothing, as far as the engine is concerned.

    Every value is substituted as exactly one shell word, so a path with a
    space in it stays one path. For a literal brace — an `awk '{{print $1}}'`
    — double it.

    Args:
        command: The command, with placeholders.

    Returns:
        A recipe, typed as the data the step declares, so a body reads as
        `return lg.sh(...)` under a `-> Alignment` annotation.
    """
    # Typed as the step's own return so that user pipelines type-check as
    # written; at runtime it is a `Shell`, which is what the engine wants.
    return Shell(command)  # type: ignore[return-value]


def write(function: Callable[[Path], None]) -> R:  # type: ignore[type-var]
    """Produce the output in Python, by writing the file yourself.

    `function` is called with the path to write to. It is the one thing in a
    run that takes the GIL, so steps written this way do not run in parallel
    with each other — which is the right trade for an interpretation step
    and the wrong one for anything that does real work. Prefer `lg.sh`.

    Args:
        function: Called with the path to write to.

    Returns:
        A recipe, typed as the data the step declares.
    """
    return Written(function)  # type: ignore[return-value]
