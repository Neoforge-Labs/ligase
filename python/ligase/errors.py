"""The errors a check reports, and how the engine's findings become them.

Every message is built in Rust, so the Rust tests own the exact wording and
a caller in any language reads the same report. What lives here is the class
hierarchy, because that is what Python code catches on, and the shape of the
hierarchy is a promise about what each error means you should do:

- a `ContigNamingError` means add a renaming step
- a `ContigOrderError` means reorder a sequence dictionary
- an `UndeclaredDimensionError` means declare something where you load data
- a `StepTypeError` means pass a different value, or fix an annotation

Catching `DimensionError` gets every disagreement about data; catching
`CheckError` gets those plus the step-type errors, which are about the
pipeline rather than about the data.
"""

from __future__ import annotations

__all__ = [
    "CheckError",
    "ContigNamingError",
    "ContigOrderError",
    "DimensionError",
    "StepDefinitionError",
    "StepTypeError",
    "UndeclaredDimensionError",
]


class CheckError(Exception):
    """A pipeline will not work, found before anything ran.

    Attributes:
        step: The step the problem was found in.
        dimension: The name of the dimension involved, where one is. `None`
            for a `StepTypeError`, which blames no dimension.
        scope: What distinguishes that step from the others of the same name,
            as a `(kind, value)` pair — `("sample", "SRR32217714")` under
            per-sample fan-out. `None` for a step that stands for no single
            sample, such as one that spans a whole cohort. Collecting these
            is how you ask which samples a problem affects:

                affected = {e.scope[1] for e in pipe.errors() if e.scope}

        errors: Every problem found in the same pass, this one included.
            Checking reports all of them at once rather than one per run,
            because fixing errors one per overnight run is the problem
            Ligase exists to remove.
    """

    def __init__(
        self,
        message: str,
        step: str,
        dimension: str | None,
        scope: tuple[str, str] | None = None,
    ) -> None:
        """Record a problem, its step, its dimension and which step it was."""
        super().__init__(message)
        self.step = step
        self.dimension = dimension
        self.scope = scope
        self.errors: list[CheckError] = []


class DimensionError(CheckError):
    """Two inputs to a step disagree about a dimension.

    The message names the step, both conflicting values, where each value
    came from, and asks a leading question. The provenance is the part that
    makes it actionable: knowing two references disagree sends you hunting,
    while knowing one came from a BAM `@SQ` header and the other from the
    FASTA you passed tells you what to change.
    """


class ContigNamingError(DimensionError):
    """The same contigs under different names, such as `chr1` against `1`.

    Distinct from a plain `DimensionError` because the fix is distinct: a
    renaming step, not a different reference. Still a `DimensionError`, so
    code that wants to catch any dimension problem can.
    """


class ContigOrderError(DimensionError):
    """The same contigs in a different order.

    htslib matches contigs by name, so `bcftools` does not mind. GATK
    requires sequence dictionaries in matching order and will reject this.
    """


class UndeclaredDimensionError(DimensionError):
    """A dimension some step requires is unknown and must be declared.

    Raised only because a step actually joins on it: an unknown dimension
    nothing downstream requires is allowed to stay unknown. The message names
    the data that has to declare it and the route by which it reaches the
    step that needs it, which for a value propagated through the graph are
    two different places.
    """


class StepTypeError(CheckError):
    """A step was handed a kind of data it does not declare.

    Not a `DimensionError`: nothing disagrees about the data, the pipeline is
    simply wired up wrong. Reported instead of that step's dimension errors
    rather than alongside them, because comparing the dimensions of bindings
    the step never declared would describe a pipeline you did not write.
    """


class StepDefinitionError(TypeError):
    """A decorated function cannot be used as a step.

    Raised by `@lg.step` as the module is imported, not at check time: a
    missing annotation is a mistake in the pipeline's source, and the sooner
    it is said the less there is to read. A `TypeError`, because that is what
    a badly typed definition is.
    """


#: The exception class for each kind of finding the engine reports.
CLASSES: dict[str, type[CheckError]] = {
    "dimension": DimensionError,
    "contig_naming": ContigNamingError,
    "contig_order": ContigOrderError,
    "undeclared": UndeclaredDimensionError,
    "step_type": StepTypeError,
}


def build(
    raw: list[tuple[str, str, str | None, tuple[str, str] | None, str]],
) -> list[CheckError]:
    """Turn the engine's findings into exceptions that know about each other.

    Args:
        raw: One `(kind, step, dimension, scope, message)` tuple per finding.

    Returns:
        One exception per finding — one per failing step, so a problem that
        hits four samples is four errors — each carrying the full list in its
        `errors` attribute so that catching one shows them all.
    """
    errors = [
        CLASSES[kind](message, step, dimension, scope)
        for kind, step, dimension, scope, message in raw
    ]
    for error in errors:
        error.errors = errors
    return errors
