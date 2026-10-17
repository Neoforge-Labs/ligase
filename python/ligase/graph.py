"""Steps that defer, and the pipeline that connects them.

`@lg.step` reads a function's signature once, when the module is imported,
and replaces the function with a `Step`. Calling that `Step` does not run the
body: it records what was passed and hands back a `StepOutput`, a handle on
data that does not exist yet. `lg.Pipeline` walks those handles into a graph
the engine can check.

This is why a pipeline can be type-checked in milliseconds rather than after
an overnight run. It is also why the annotations matter: the return
annotation is the only thing that says what a step produces, since nothing
ever asks the body.

A `Step` is typed as `Step[P, R]`, so a type checker sees `align` as
returning an `Alignment` and catches a mis-wired pipeline while you are still
editing it. At runtime the value is a `StepOutput`; `check()` is what catches
the same mistake for code that is not type-checked.
"""

from __future__ import annotations

import inspect
from collections.abc import Iterable
from dataclasses import dataclass
from typing import (
    TYPE_CHECKING,
    Any,
    Generic,
    ParamSpec,
    TypeVar,
    cast,
    get_type_hints,
    overload,
)

from . import _core
from .errors import CheckError, StepDefinitionError, build
from .types import KINDS, Dataset

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping

__all__ = [
    "Parameter",
    "Pipeline",
    "Report",
    "Step",
    "StepOutput",
    "handle",
    "step",
]

P = ParamSpec("P")
R = TypeVar("R", bound=Dataset)

#: The coordinate conventions `@lg.step(coordinates=...)` accepts.
COORDINATES = ("0-based half-open", "1-based closed")


@dataclass(frozen=True, slots=True)
class Parameter:
    """One parameter of a step.

    Attributes:
        name: The parameter's name, as the step declares it.
        kind: The Ligase data type it takes, or `None` when it is a plain
            value such as a threshold. Plain parameters are recorded but
            carry no dimensions; they join the cache key in M3.
    """

    name: str
    kind: str | None


class Step(Generic[P, R]):
    """A function that records a call instead of running.

    Attributes:
        function: The undecorated callable. Execution will run this; nothing
            in M2 does.
        tool: The external tool the step shells out to, if it was given one.
        coordinates: The coordinate convention the step writes, for the one
            output kind that does not imply one.
    """

    __slots__ = (
        "_signature",
        "coordinates",
        "function",
        "output",
        "parameters",
        "tool",
    )

    def __init__(
        self,
        function: Callable[P, R],
        *,
        tool: str | None = None,
        coordinates: str | None = None,
    ) -> None:
        """Read a function's signature, refusing one a graph cannot use."""
        self.function = function
        self.tool = tool
        self.coordinates = coordinates
        self._signature = inspect.signature(function)
        self.output = _output_kind(function)
        self.parameters = _parameters(function, self._signature)

    @property
    def name(self) -> str:
        """The step's name, as it appears in messages."""
        return self.function.__name__

    @property
    def dataset_parameters(self) -> tuple[Parameter, ...]:
        """The parameters that take data, in declaration order.

        Order matters: a dimension is inherited from the first argument that
        states it.
        """
        return tuple(p for p in self.parameters if p.kind is not None)

    @property
    def plain_parameters(self) -> tuple[Parameter, ...]:
        """The parameters that take plain values, in declaration order."""
        return tuple(p for p in self.parameters if p.kind is None)

    def __call__(self, *args: P.args, **kwargs: P.kwargs) -> R:
        """Record a call and return a handle on what it would produce.

        The body is not run. Missing or surplus arguments still fail here,
        exactly as they would for the undecorated function.
        """
        bound = self._signature.bind(*args, **kwargs)
        bound.apply_defaults()
        # The handle is not really an `R`. It stands in for one, which is
        # what lets a type checker follow a pipeline it will never run.
        return cast("R", StepOutput(self, dict(bound.arguments)))

    def __repr__(self) -> str:
        """Identify the step by name and what it produces."""
        return f"<Step {self.name} -> {self.output}>"


class StepOutput:
    """A handle on data a step would produce.

    Returned by calling a `Step`. It holds no data and has never touched a
    file; what it knows is which step would make it, and from what. Once a
    `Pipeline` has been built from it, `dataset` holds the dimensions the
    engine propagated — the dimensions the output *will* have, worked out
    without running anything.

    Attributes:
        step: The step that would produce it.
        arguments: Everything the step was called with, by parameter name.
    """

    __slots__ = ("_graph", "_node", "arguments", "step")

    def __init__(self, step: Step[Any, Any], arguments: dict[str, object]) -> None:
        """Record a deferred call."""
        self.step = step
        self.arguments = arguments
        self._graph: _core.Graph | None = None
        self._node: int | None = None

    @property
    def kind(self) -> str:
        """The kind of data the step declares it produces."""
        return self.step.output

    @property
    def dataset(self) -> Dataset | None:
        """The propagated output dataset, once a `Pipeline` has been built.

        `None` before that: propagation is the graph's work, and there is no
        graph until a pipeline asks for one.
        """
        if self._graph is None or self._node is None:
            return None
        found = self._graph.dataset(self._node)
        if found is None:
            return None
        return KINDS.get(self.kind, Dataset)(found)

    def inputs(self) -> tuple[tuple[Parameter, object], ...]:
        """The data arguments, paired with the parameters they filled."""
        return tuple(
            (parameter, self.arguments[parameter.name])
            for parameter in self.step.dataset_parameters
        )

    def _attach(self, graph: _core.Graph, node: int) -> None:
        self._graph = graph
        self._node = node

    def __repr__(self) -> str:
        """Identify the handle by the step that would produce it."""
        return f"<StepOutput {self.kind} from step `{self.step.name}`>"


@dataclass(frozen=True, slots=True)
class Report:
    """What a check found.

    Attributes:
        errors: One error per problem, in the order the steps run.
        summary: One line summarising the lot, worded by the engine so that
            every surface reports a check the same way.
    """

    errors: list[CheckError]
    summary: str


class Pipeline:
    """A graph of steps, built from its outputs and checked before it runs.

    Construction walks back from the outputs through the handles each step
    was given, so a file passed to two steps becomes one node rather than
    two. Nothing is executed and nothing is read: the graph is built from
    signatures and from the dimensions the inputs were loaded with.
    """

    __slots__ = ("_graph", "_outputs", "_roots")

    def __init__(self, outputs: Dataset | Iterable[Dataset]) -> None:
        """Build the graph that produces these outputs.

        Args:
            outputs: One handle, or an iterable of them — what
                `samples.map(...)` returns. A type checker sees handles as
                the data types their steps return, which is how a mis-wired
                pipeline is caught before `check()` ever runs.

        Raises:
            ValueError: If no outputs were given.
            TypeError: If a step was passed something that is neither
                loaded data nor another step's output.
        """
        self._graph = _core.Graph()
        self._outputs: list[object] = list(_as_outputs(outputs))
        if not self._outputs:
            raise ValueError(
                "a Pipeline needs at least one output; pass the handle a step "
                "returned, or the list `samples.map(...)` gives you"
            )
        # Keyed by identity: two datasets loaded from the same file are two
        # nodes, because the user wrote them as two.
        placed: dict[int, int] = {}
        pinned: list[object] = []
        self._roots = [self._place(out, placed, pinned) for out in self._outputs]

    def _place(
        self, value: object, placed: dict[int, int], pinned: list[object]
    ) -> int:
        existing = placed.get(id(value))
        if existing is not None:
            return existing

        if isinstance(value, StepOutput):
            arguments = [
                (parameter.name, parameter.kind, self._place(given, placed, pinned))
                for parameter, given in _checked_inputs(value)
            ]
            node = self._graph.add_step(
                value.step.name,
                value.step.output,
                arguments,
                value.step.coordinates,
            )
            value._attach(self._graph, node)
        elif isinstance(value, Dataset):
            node = self._graph.add_input(value.kind.lower(), value._core)
        else:
            raise TypeError(
                f"a step was given {value!r}, which is neither loaded data nor "
                "another step's output"
            )

        placed[id(value)] = node
        # `id` is only unique while the object lives, so keep them all alive.
        pinned.append(value)
        return node

    @property
    def steps(self) -> tuple[str, ...]:
        """The steps this pipeline runs, in dependency order."""
        return tuple(self._graph.step_names(self._roots))

    def report(self) -> Report:
        """Check every step and return everything found, without raising."""
        raw, summary = self._graph.check(self._roots)
        return Report(build(raw), summary)

    def errors(self) -> list[CheckError]:
        """Return every problem in this pipeline, without raising.

        Returns:
            One error per problem, in the order the steps run. Empty when
            the pipeline is consistent. Nothing is computed.
        """
        return self.report().errors

    def check(self) -> None:
        """Type-check the whole pipeline. Nothing is computed.

        Every problem is found before anything is raised, so the raised
        error's `errors` attribute holds all of them.

        Raises:
            CheckError: If anything is wrong. The subclass says what:
                `DimensionError` and its own subclasses for data that
                disagrees, `StepTypeError` for a step wired to the wrong
                kind of data.
        """
        errors = self.errors()
        if errors:
            raise errors[0]

    def __len__(self) -> int:
        """How many nodes the graph holds, inputs included."""
        return len(self._graph)

    def __repr__(self) -> str:
        """Identify the pipeline by its size."""
        return f"<Pipeline {len(self.steps)} steps, {len(self)} nodes>"


@overload
def step(function: Callable[P, R], /) -> Step[P, R]: ...


@overload
def step(
    *,
    tool: str | None = None,
    coordinates: str | None = None,
) -> Callable[[Callable[P, R]], Step[P, R]]: ...


def step(
    function: Callable[P, R] | None = None,
    /,
    *,
    tool: str | None = None,
    coordinates: str | None = None,
) -> Step[P, R] | Callable[[Callable[P, R]], Step[P, R]]:
    """Make a function a pipeline step, without ever running it.

    The signature is read once, here. Each parameter annotated with a Ligase
    data type becomes an edge in the graph; the rest are plain values. The
    return annotation is what says the step produces an `Alignment`, since
    nothing will ask the body.

    Args:
        function: The function to decorate, when used bare as `@lg.step`.
        tool: The external tool the step shells out to, e.g. `minimap2`.
            Recorded now; its version joins the cache key in M3.
        coordinates: The coordinate convention the step writes, one of
            `"0-based half-open"` or `"1-based closed"`. Only meaningful for
            a step that produces `Intervals`: alignments and variant calls
            are 1-based closed by specification, while a tool that writes
            BED from a VCF changes the convention and must say so.

    Returns:
        A `Step`, which a type checker sees as the function it decorated.

    Raises:
        StepDefinitionError: If the signature is one a graph cannot use —
            an unannotated parameter, a missing or non-Ligase return
            annotation, or `*args`/`**kwargs`.
        ValueError: If `coordinates` is not one of the two conventions.
    """
    if coordinates is not None and coordinates not in COORDINATES:
        raise ValueError(
            f"`{coordinates}` is not a coordinate system; expected "
            + " or ".join(COORDINATES)
        )

    def decorate(inner: Callable[P, R]) -> Step[P, R]:
        return Step(inner, tool=tool, coordinates=coordinates)

    if function is None:
        return decorate
    return decorate(function)


def handle(output: object) -> StepOutput:
    """The handle behind what a step returned.

    A type checker sees `align(reads, ref)` as returning an `Alignment`,
    which is what lets it follow a pipeline and catch a mis-wired one while
    you are still editing. At runtime the value is a `StepOutput` — data that
    does not exist yet — and this is how to reach it without writing a cast
    at every call site.

    Args:
        output: What a step returned.

    Returns:
        The handle, with its step, its arguments and (once a `Pipeline` has
        been built) the dimensions the output will carry.

    Raises:
        TypeError: If this is not a step's output — loaded data, say, which
            is already the thing itself.
    """
    if not isinstance(output, StepOutput):
        raise TypeError(
            f"{output!r} is not a step's output; only a value a step returned "
            "has a handle behind it"
        )
    return output


# ------------------------------------------------------------- signatures --


def _output_kind(function: Callable[..., Any]) -> str:
    hints = _hints(function)
    annotation = hints.get("return")
    if annotation is None:
        raise StepDefinitionError(
            f"step `{function.__name__}` has no return annotation, so nothing "
            "says what it produces; annotate it with a Ligase data type such "
            "as `-> Alignment`"
        )
    kind = _kind_of(annotation)
    if kind is None:
        raise StepDefinitionError(
            f"step `{function.__name__}` returns {_name(annotation)}, which is "
            f"not a Ligase data type; expected one of {', '.join(KINDS)}"
        )
    return kind


def _parameters(
    function: Callable[..., Any], signature: inspect.Signature
) -> tuple[Parameter, ...]:
    hints = _hints(function)
    found: list[Parameter] = []

    for parameter in signature.parameters.values():
        if parameter.kind in (
            inspect.Parameter.VAR_POSITIONAL,
            inspect.Parameter.VAR_KEYWORD,
        ):
            raise StepDefinitionError(
                f"step `{function.__name__}` takes `{parameter.name}` as a "
                "variable number of arguments, which a graph cannot bind; "
                "declare each input as its own parameter"
            )

        annotation = hints.get(parameter.name)
        if annotation is None:
            raise StepDefinitionError(
                f"step `{function.__name__}` does not annotate `{parameter.name}`. "
                "An unannotated parameter might be data, and data the graph "
                "does not know about is an unchecked connection"
            )
        found.append(Parameter(parameter.name, _kind_of(annotation)))

    return tuple(found)


def _hints(function: Callable[..., Any]) -> Mapping[str, Any]:
    try:
        return get_type_hints(function)
    except Exception as error:
        raise StepDefinitionError(
            f"step `{function.__name__}` has annotations that cannot be "
            f"resolved: {error}"
        ) from error


def _kind_of(annotation: object) -> str | None:
    if isinstance(annotation, type):
        for name, cls in KINDS.items():
            if annotation is cls:
                return name
    return None


def _name(annotation: object) -> str:
    return getattr(annotation, "__name__", repr(annotation))


def _as_outputs(outputs: object) -> list[object]:
    if isinstance(outputs, StepOutput | Dataset):
        return [outputs]
    if isinstance(outputs, Iterable) and not isinstance(outputs, str | bytes):
        return list(outputs)
    return [outputs]


def _checked_inputs(output: StepOutput) -> tuple[tuple[Parameter, object], ...]:
    for parameter, given in output.inputs():
        if not isinstance(given, StepOutput | Dataset):
            raise TypeError(
                f"step `{output.step.name}` was given {given!r} for "
                f"`{parameter.name}`, which is neither loaded data nor another "
                "step's output"
            )
    return output.inputs()
