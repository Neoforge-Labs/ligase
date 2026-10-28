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
import os
import time
from collections.abc import Callable, Iterable, Sequence
from dataclasses import dataclass
from pathlib import Path
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

from . import _core, config
from .audit import AuditReport, StepAudit
from .config import STORE_VARIABLE
from .errors import CheckError, StepDefinitionError, build
from .provenance import (
    Bundle,
    InputCheck,
    Methods,
    MethodsGap,
    PinnedReference,
    ResultCheck,
    StepMethods,
    Verification,
)
from .recipes import Noticed, Shell, Written
from .run import Divergence, RunError, RunReport, StepFailure, StepRun
from .types import KINDS, Dataset

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

__all__ = [
    "CACHE_DIR",
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

#: Where a run keeps its cache, under the working directory.
CACHE_DIR = ".ligase"

#: One task as the engine takes it: node, code, command, callback, tools,
#: parameters, citations, the seed parameter's name. Exactly one of the
#: command and the callback is set.
_Task = tuple[
    int,
    str,
    str | None,
    int | None,
    list[str],
    list[tuple[str, str]],
    list[str],
    str | None,
]

#: A `lg.write` body, as the engine calls it: where to write, and the
#: inputs by parameter name. It hands back what it noticed about its own
#: work, which is almost always nothing.
_Callback = Callable[[str, list[tuple[str, list[str]]]], list[Noticed]]

#: A bundle as the engine reports it: where it is, what is in it, how many
#: steps it accounts for, how many it could not.
_Bundle = tuple[str, list[str], int, int]

#: A bundle checked as the engine reports it: the results, the inputs,
#: whether everything was reproduced, the worst verdict, the summary, the
#: text.
_Verification = tuple[
    list[tuple[str, str | None, str, str, str | None, bool]],
    list[tuple[str, str, str, str]],
    bool,
    str,
    str,
    str,
]


@dataclass(frozen=True, slots=True)
class Parameter:
    """One parameter of a step.

    Attributes:
        name: The parameter's name, as the step declares it.
        kind: The Ligase data type it takes, or `None` when it is a plain
            value such as a threshold. Plain parameters are recorded but
            carry no dimensions, but they do join the cache key, so
            changing one reruns the steps below it.
    """

    name: str
    kind: str | None


class Step(Generic[P, R]):
    """A function that records a call instead of running.

    Attributes:
        function: The undecorated callable. A run calls it once, before
            anything executes, to ask how the output is produced.
        tool: The external tools the step shells out to, as they were given.
        tools: Those tools as a tuple, whose versions join the cache key.
        coordinates: The coordinate convention the step writes, for the one
            output kind that does not imply one.
        conversion: The registered conversion this step performs, if it
            crosses a dimension rather than inheriting it. The target comes
            from the call rather than from here, read off the parameter the
            engine names.
        citations: References the step asks to be cited by.
        stochastic: Whether the step declared that its tool draws on
            randomness.
        seed: The name of the parameter that seeds it, for a stochastic
            step. Never set without `stochastic`, and never unset with it:
            the two are one declaration in two parts, checked here.
        output: The kind of data the step's return annotation declares, read
            once when the step is defined.
        parameters: Every parameter the function takes, in declaration
            order, each already classified as data or a plain value.
    """

    __slots__ = (
        "_signature",
        "citations",
        "conversion",
        "coordinates",
        "function",
        "output",
        "parameters",
        "seed",
        "stochastic",
        "tool",
        "tools",
    )

    def __init__(
        self,
        function: Callable[P, R],
        *,
        tool: str | Sequence[str] | None = None,
        coordinates: str | None = None,
        cite: str | Sequence[str] | None = None,
        conversion: str | None = None,
        stochastic: bool = False,
        seed: str | None = None,
    ) -> None:
        """Read a function's signature, refusing one a graph cannot use."""
        self.function = function
        self.tool = tool
        self.tools = _tools(tool)
        self.coordinates = coordinates
        self.conversion = conversion
        self.citations = _citations(cite)
        self.stochastic = stochastic
        self.seed = seed
        self._signature = inspect.signature(function)
        self.output = _output_kind(function)
        self.parameters = _parameters(function, self._signature)
        if conversion is not None:
            _check_conversion(self.name, conversion, self._signature)
        _check_seed(function, self._signature, stochastic=stochastic, seed=seed)

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

    __slots__ = ("_graph", "_node", "_produced", "arguments", "step")

    def __init__(self, step: Step[Any, Any], arguments: dict[str, object]) -> None:
        """Record a deferred call."""
        self.step = step
        self.arguments = arguments
        self._graph: _core.Graph | None = None
        self._node: int | None = None
        self._produced: str | None = None

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

    @property
    def file(self) -> str:
        """Where the output is, once the step has produced it.

        The same name `Dataset.file` has, because a body cannot tell which
        it was handed: a step's input is loaded data for the first step of a
        pipeline and another step's handle after that. A `lg.write` body
        reads it inside the function it hands back, by which time the engine
        has run whatever comes above it:

            return lg.write(lambda out: interpret(v.file, out))

        Raises:
            RuntimeError: Before the step has run, when there is no file to
                name. A shell command should use `{placeholders}` instead,
                which the engine fills in for it.
        """
        if self._produced is None:
            raise RuntimeError(
                f"step `{self.step.name}` has not produced anything yet, so "
                "its output has no file. A shell command names its inputs "
                "with placeholders and lets the engine fill them in; a "
                "`lg.write` body reads `.file` inside the function it "
                "returns, which runs after the steps above it"
            )
        return self._produced

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
        errors: One error per *failing step*, in the order the steps run. A
            problem that hits four samples is four errors, each naming its
            own sample.
        summary: One line summarising the lot — `4 errors in 4 steps.` — and
            the steps it counts are places rather than names, since one
            written step is many steps under fan-out.
        text: The whole report as the command line prints it: each distinct
            problem stated once, with every sample it affects named, and the
            summary at the end. Worded by the engine so that every surface
            reports a check the same way.
    """

    errors: list[CheckError]
    summary: str
    text: str


def _worker_name(worker: str | bool) -> str | None:
    """What to call this worker, or `None` for an ordinary run.

    `True` means "be a worker and choose a name for me". The name is made
    here rather than in the engine, even though the engine can make one,
    because a worker's cache directory is named after it: a name invented
    behind the binding would leave this side with no directory to point at.

    A string is the name, and the engine reduces it to characters a path
    and a single field can both carry — but a path separator is worth
    refusing rather than quietly turning into a dash, because
    `--worker=a/b` is far more likely to be a mistake than a wish.
    """
    if worker is False:
        return None
    if worker is True:
        # Enough to tell two workers on one machine apart, and honest about
        # saying nothing else. A name worth reading is one the user gave.
        return f"w{os.getpid():x}-{time.time_ns() % 0x10000:04x}"
    name = worker.strip()
    if not name:
        raise ValueError(
            "a worker's name cannot be blank; pass worker=True to be given one"
        )
    if "/" in name or "\\" in name:
        raise ValueError(
            f"`{name}` cannot be a worker's name: it holds a path separator, "
            "and a worker is named in its own cache directory"
        )
    return name


class Pipeline:
    """A graph of steps, built from its outputs and checked before it runs.

    Construction walks back from the outputs through the handles each step
    was given, so a file passed to two steps becomes one node rather than
    two. Nothing is executed and nothing is read: the graph is built from
    signatures and from the dimensions the inputs were loaded with.
    """

    __slots__ = ("_graph", "_handles", "_outputs", "_roots")

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
        self._handles: dict[int, StepOutput] = {}
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
                _declared_conversion(value),
            )
            value._attach(self._graph, node)
            # A run asks each handle how its output is produced, so the
            # graph's nodes have to lead back to the handles that made them.
            self._handles[node] = value
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
        raw, summary, text = self._graph.check(self._roots)
        return Report(build(raw), summary, text)

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

    def run(
        self,
        *,
        jobs: int | None = None,
        force: bool = False,
        cache: str | Path | None = None,
        worker: str | bool = False,
        lease: int | None = None,
    ) -> RunReport:
        """Check the pipeline, then compute whatever is not already known.

        The pipeline is checked first, every time: running an unchecked
        pipeline is how an overnight run ends in a dimension error.

        A step runs when something that could change its result has changed
        — its code, a parameter, an input's contents, or the version of a
        tool it calls — and not otherwise. Independent steps run at once,
        and a step whose rerun produced the same output as before leaves
        everything below it alone.

        Args:
            jobs: How many steps may run at once. Defaults to the number of
                cores.
            force: Run every step, cache or no cache. For the one case the
                cache cannot see: a file rewritten in place, keeping its
                length and its timestamp.
            cache: Where to keep the cache. Defaults to `.ligase` under the
                working directory. A worker defaults to its own directory
                beneath that one, because two workers must not share an
                index.
            worker: Make this run one worker among several, dividing the
                pipeline with the others through the shared store. `True`
                for a name chosen for you, or a string to choose one —
                `worker=os.environ["SLURM_ARRAY_TASK_ID"]` is the usual
                thing. Needs a store every worker can see.

                This is not an alternative to `jobs`: `jobs` is how many
                steps this machine runs at once, and `worker` is how many
                machines there are.
            lease: How many seconds a worker may hold a step before another
                worker may compute it too. Defaults to an hour. Set it
                above your longest step: too short duplicates work, too
                long leaves others waiting after a crash, and neither is a
                wrong answer.

        Returns:
            What every step did, and where its output is.

        Raises:
            CheckError: If the pipeline does not type-check. Nothing runs.
            RunError: If any step failed. Everything that did succeed is on
                the error's `report`, which for a cohort is usually most of
                it.
            ValueError: If `worker` is set and no shared store is
                configured. Workers divide work by claiming it in a store
                they can all see, so one without a store has nobody to
                divide it with and would quietly compute the whole
                pipeline on its own.
        """
        # Before the check and before the bodies are asked for their
        # recipes: this is about how the run was invoked rather than about
        # the pipeline, so there is no reason to execute any of the user's
        # code to find out that the command line is wrong.
        name = _worker_name(worker)
        directory = Path(cache) if cache is not None else Path.cwd() / CACHE_DIR
        if name is not None:
            if config.store() is None:
                raise ValueError(
                    "a worker needs a store every worker can see. Set "
                    f"{STORE_VARIABLE}, or `[store] url` in ligase.toml, to "
                    "a bucket or a directory on a shared filesystem — "
                    "without one this worker would compute the whole "
                    "pipeline by itself and call it a cluster."
                )
            # Its own, unless the caller insisted. Two workers sharing a
            # `cache.sqlite` over a network filesystem is the failure D43
            # names, and an array job defaulting to one $HOME walks into it.
            if cache is None:
                directory = directory / "workers" / name

        self.check()
        tasks, callbacks = self._plan()

        finished, failed, summary, text, counted, diverged = self._graph.run(
            self._roots,
            tasks,
            str(directory),
            callbacks,
            jobs,
            force,
            name,
            lease,
        )

        steps = []
        for (
            node,
            name,
            outcome,
            key,
            digest,
            canonical,
            output,
            duration,
            notes,
        ) in finished:
            # Tell the handle where its output landed, so a later
            # `lg.handle(x).path` and the next run's bodies can read it.
            produced = self._handles.get(node)
            if produced is not None:
                produced._produced = output
            steps.append(
                StepRun(
                    step=name,
                    sample=self._sample(node),
                    outcome=outcome,
                    output=output,
                    key=key,
                    hash=digest,
                    canonical=canonical,
                    duration_ms=duration,
                    node=node,
                    notes=tuple(notes),
                )
            )

        hashed, prefiltered, canonicalised = counted
        report = RunReport(
            worker=name,
            divergences=[
                Divergence(
                    step=step,
                    sample=self._sample(node),
                    ours=ours,
                    theirs=theirs,
                    node=node,
                )
                for node, step, ours, theirs in diverged
            ],
            steps=steps,
            failures=[
                StepFailure(
                    step=name,
                    sample=self._sample(node),
                    message=message,
                    node=node,
                )
                for node, name, message in failed
            ],
            summary=summary,
            text=text,
            hashed=hashed,
            prefiltered=prefiltered,
            canonicalised=canonicalised,
        )
        if report.failures:
            raise RunError(report)
        return report

    def audit(
        self,
        step: str,
        *,
        sample: str | None = None,
        jobs: int | None = None,
        cache: str | Path | None = None,
    ) -> AuditReport:
        """Run one step twice and report whether it is reproducible.

        The pipeline is run first, normally and through the cache, so that
        the step being audited has its inputs. Then each node of that step
        is run twice more, into scratch files that are thrown away, and the
        two outputs are compared both as bytes and as meaning.

        Three verdicts, because two would not do. A step whose bytes move
        while its result does not — which is every tool that records its own
        command line — is `reproducible`, not a failure. Only a step whose
        *result* moves is `not reproducible`.

        Args:
            step: The step's name, as written. Under per-sample fan-out
                there is one node of it per sample, and all of them are
                audited.
            sample: Audit only the node for this sample.
            jobs: How many steps may run at once in the preliminary run.
                The audited steps are taken in turn whatever this says, so
                that the durations reported mean something.
            cache: Where to keep the cache. Defaults to `.ligase` under the
                working directory.

        Returns:
            What two runs of each node showed.

        Raises:
            CheckError: If the pipeline does not type-check. Nothing runs.
            ValueError: If no step of that name is in the pipeline, or no
                node of it is for that sample.
        """
        self.check()
        tasks, callbacks = self._plan()
        directory = Path(cache) if cache is not None else Path.cwd() / CACHE_DIR

        audited, failed, verdict, summary, text = self._graph.audit(
            self._roots,
            tasks,
            str(directory),
            callbacks,
            (step, sample),
            jobs,
        )

        return AuditReport(
            steps=[
                StepAudit(
                    step=name,
                    sample=self._sample(node),
                    scope=scope,
                    verdict=step_verdict,
                    first=first,
                    second=second,
                    first_canonical=first_canonical,
                    second_canonical=second_canonical,
                    first_duration_ms=first_ms,
                    second_duration_ms=second_ms,
                    node=node,
                    seed=seed,
                )
                for (
                    node,
                    name,
                    scope,
                    step_verdict,
                    first,
                    second,
                    first_canonical,
                    second_canonical,
                    first_ms,
                    second_ms,
                    seed,
                ) in audited
            ],
            failures=[
                StepFailure(
                    step=name,
                    sample=self._sample(node),
                    message=message,
                    node=node,
                )
                for node, name, message in failed
            ],
            verdict=verdict,
            summary=summary,
            text=text,
        )

    def methods(
        self,
        *,
        bundle: str | Path | None = None,
        thin: bool = False,
        verify: str | Path | None = None,
        include: Sequence[str | Path] = (),
        jobs: int | None = None,
        cache: str | Path | None = None,
    ) -> Methods:
        """Say how this pipeline's results were produced.

        The pipeline is run first, normally and through the cache, because a
        provenance record is filed under a step's key and computing keys is
        what a run does. A pipeline already computed costs a cache lookup
        per step, not a recomputation.

        What comes back names, for every step: the tools and the versions
        they ran at, the parameters as they were hashed, the command as
        written, and every reference by accession and checksum. A step whose
        output predates provenance is reported as a gap rather than left
        out, because nothing can reconstruct how it was produced — the
        command that would have said so is deliberately not in the hash.

        Args:
            bundle: Write a reproducibility bundle into this directory.
            thin: Leave the object bodies out of the bundle and name the
                store they are in. Smaller, and not self-contained: a thin
                bundle needs access to that store, including for the
                recorded query responses that a fat one carries so a
                reviewer can replay with no network and no API key. For a
                collaborator who can already reach the lab's store; write a
                fat one for a reviewer or an archive.
            verify: Check the bundle in this directory against what this
                machine just produced, and report where they differ.
            include: Files to copy into the bundle as they are, such as the
                pipeline itself.
            jobs: How many steps may run at once in the run beforehand.
            cache: Where to keep the cache. Defaults to `.ligase` under the
                working directory.

        Returns:
            How every step was produced, with the bundle written and the
            verification done, where either was asked for.

        Raises:
            CheckError: If the pipeline does not type-check. Nothing runs.
            RuntimeError: If a step failed, since a pipeline that did not
                finish has no methods section to write; or if a bundle
                cannot be written.
            ValueError: If the directory to verify against is not a bundle.
        """
        self.check()
        tasks, callbacks = self._plan()
        directory = Path(cache) if cache is not None else Path.cwd() / CACHE_DIR

        described, written, checked = self._graph.methods(
            self._roots,
            tasks,
            str(directory),
            callbacks,
            (
                None if bundle is None else str(bundle),
                None if verify is None else str(verify),
                [str(path) for path in include],
                thin,
            ),
            jobs,
        )

        (
            groups,
            gaps,
            references,
            citations,
            uncited,
            engines,
            platforms,
            summary,
            text,
        ) = described

        return Methods(
            steps=[
                StepMethods(
                    step=step,
                    nodes=nodes,
                    scopes=list(scopes),
                    scope_kind=scope_kind,
                    tools=list(tools),
                    parameters=list(parameters),
                    command=command,
                )
                for (
                    step,
                    nodes,
                    scopes,
                    scope_kind,
                    tools,
                    parameters,
                    command,
                ) in groups
            ],
            gaps=[
                MethodsGap(step=step, scopes=list(scopes), nodes=nodes)
                for step, scopes, nodes in gaps
            ],
            references=[
                PinnedReference(
                    param=param,
                    path=path,
                    build=build,
                    accession=accession,
                    contigs=contigs,
                    length=length,
                    checksum=checksum,
                    fingerprint=fingerprint,
                )
                for (
                    param,
                    path,
                    build,
                    accession,
                    contigs,
                    length,
                    checksum,
                    fingerprint,
                ) in references
            ],
            citations=list(citations),
            uncited=list(uncited),
            engines=list(engines),
            platforms=list(platforms),
            summary=summary,
            text=text,
            bundle=_bundle(written),
            verification=_verification(checked),
        )

    def _plan(self) -> tuple[list[_Task], list[_Callback]]:
        """Ask every step how its output is produced, before anything runs.

        This is the one moment a step body is called, and it is called for
        its *answer* rather than for its effect: a command to render, or a
        function to call later. Nothing has been computed, so a body that
        reaches for an input's contents here is a body that cannot work.
        """
        tasks: list[_Task] = []
        callbacks: list[_Callback] = []

        for node, produced in sorted(self._handles.items()):
            recipe = _recipe(produced)
            callback: int | None = None
            command: str | None = None
            if isinstance(recipe, Shell):
                command = recipe.command
            else:
                callback = len(callbacks)
                callbacks.append(_bind(recipe, produced))

            step = produced.step
            tasks.append(
                (
                    node,
                    _source(step.function),
                    command,
                    callback,
                    list(step.tools),
                    [
                        (parameter.name, repr(produced.arguments[parameter.name]))
                        for parameter in step.plain_parameters
                    ],
                    list(step.citations),
                    step.seed,
                )
            )

        return tasks, callbacks

    def _sample(self, node: int) -> str | None:
        """The one sample a node stands for, if it stands for exactly one."""
        found = self._graph.dataset(node)
        if found is None:
            return None
        samples = found.samples()
        if samples is None or len(samples) != 1:
            return None
        return samples[0]

    def __len__(self) -> int:
        """How many nodes the graph holds, inputs included."""
        return len(self._graph)

    def __repr__(self) -> str:
        """Identify the pipeline by its size."""
        return f"<Pipeline {len(self.steps)} steps, {len(self)} nodes>"


def _check_conversion(name: str, conversion: str, signature: inspect.Signature) -> None:
    """Refuse a step that claims a conversion it cannot perform.

    Two things can be wrong, and both are wrong when the step is *written*
    rather than when a pipeline runs: the conversion may not be one the
    engine has, and the step may not say where it converts to. The engine
    owns both answers -- which conversions exist, and what the target
    parameter is called -- so that the decorator and the graph cannot
    disagree about them.
    """
    try:
        _core.conversion_axis(conversion)
    except ValueError as error:
        raise StepDefinitionError(
            f"step `{name}` declares conversion={conversion!r}, but {error}"
        ) from error

    target = _core.conversion_target_parameter()
    if target not in signature.parameters:
        raise StepDefinitionError(
            f"step `{name}` performs the `{conversion}` conversion but takes "
            f"no `{target}` parameter, so nothing says where it converts to. "
            f"Add `{target}:` and name the value the output is in"
        )


def _seed_suggestion(function: Callable[..., Any], signature: inspect.Signature) -> str:
    """How to help a step that declared itself stochastic and named no seed.

    The commonest mistake by far is a step that already takes `seed: int`
    and forgot to name it, and for that one the message can say what was
    meant. For anything else it must not: `iterations: int` is an `int`
    parameter and is not a seed, and `seed="iterations"` would be a
    confident wrong answer that a reader might take. A guess is only worth
    printing where it cannot be wrong, so this suggests a spelling only when
    a parameter is actually called one, and otherwise says what it can see
    and leaves the choice where it belongs.
    """
    hints = _hints(function)
    ints = [
        parameter for parameter in signature.parameters if hints.get(parameter) is int
    ]
    named = [parameter for parameter in ints if "seed" in parameter.lower()]

    if named:
        return (
            f' It takes `{named[0]}: int`, so `seed="{named[0]}"` is '
            "probably what you meant."
        )
    if ints:
        listed = ", ".join(f"`{parameter}`" for parameter in ints)
        return (
            f" Its `int` parameters are {listed}; name the one the tool "
            "seeds from, or add one."
        )
    return " It takes no `int` parameter, so add one for the seed."


def _check_seed(
    function: Callable[..., Any],
    signature: inspect.Signature,
    *,
    stochastic: bool,
    seed: str | None,
) -> None:
    """Refuse a stochastic declaration that does not hold together.

    `stochastic=` and `seed=` are one declaration in two parts, so each half
    alone is refused. Everything here is wrong when the step is *written*,
    which is why it is said at decoration rather than at check time: a
    pipeline that cannot repeat is a mistake in its source, and the sooner it
    is said the less there is to read.
    """
    name = function.__name__

    if stochastic and seed is None:
        raise StepDefinitionError(
            f"step `{name}` declares itself stochastic but names no seed, so "
            "nothing says what makes it repeat. Add `seed=` naming the "
            "parameter the tool draws its randomness from."
            f"{_seed_suggestion(function, signature)}"
        )

    if seed is None:
        return

    if not stochastic:
        raise StepDefinitionError(
            f"step `{name}` names `{seed}` as its seed but does not declare "
            "itself stochastic, so the engine has been told how the step "
            "repeats without being told that it needs to. Add "
            "`stochastic=True` beside it, or drop `seed=` if the tool draws "
            "on no randomness"
        )

    if seed not in signature.parameters:
        taken = ", ".join(f"`{parameter}`" for parameter in signature.parameters)
        raise StepDefinitionError(
            f"step `{name}` names `{seed}` as its seed, but takes no such "
            f"parameter; it takes {taken}"
        )

    annotation = _hints(function).get(seed)
    if annotation is not int:
        raise StepDefinitionError(
            f"step `{name}` names `{seed}` as its seed, but `{seed}` is "
            f"{_name(annotation)} rather than `int`. A seed has to be a "
            "value the engine can hash into the cache key and a methods "
            "section can print, which is what makes two runs of a "
            "stochastic step comparable at all"
        )


def _declared_conversion(produced: StepOutput) -> tuple[str, str] | None:
    """The conversion a call performs, and the target it declared.

    `None` for an ordinary step. For a conversion, the target is read off
    the call rather than off the step, because one written
    `map_identifiers` converts to whichever space its `to=` named.
    """
    conversion = produced.step.conversion
    if conversion is None:
        return None
    target = _core.conversion_target_parameter()
    return conversion, str(produced.arguments[target])


@overload
def step(function: Callable[P, R], /) -> Step[P, R]: ...


@overload
def step(
    *,
    tool: str | Sequence[str] | None = None,
    coordinates: str | None = None,
    cite: str | Sequence[str] | None = None,
    conversion: str | None = None,
    stochastic: bool = False,
    seed: str | None = None,
) -> Callable[[Callable[P, R]], Step[P, R]]: ...


def step(
    function: Callable[P, R] | None = None,
    /,
    *,
    tool: str | Sequence[str] | None = None,
    coordinates: str | None = None,
    cite: str | Sequence[str] | None = None,
    conversion: str | None = None,
    stochastic: bool = False,
    seed: str | None = None,
) -> Step[P, R] | Callable[[Callable[P, R]], Step[P, R]]:
    """Make a function a pipeline step, without ever running it.

    The signature is read once, here. Each parameter annotated with a Ligase
    data type becomes an edge in the graph; the rest are plain values. The
    return annotation is what says the step produces an `Alignment`, since
    nothing will ask the body.

    Args:
        function: The function to decorate, when used bare as `@lg.step`.
        tool: The external tool the step shells out to, e.g. `minimap2`,
            or several of them: `tool=("minimap2", "samtools")` for a body
            that pipes one into the other. Each one's `--version` joins the
            cache key, so an upgrade that changes a result reruns the steps
            it could have changed.
        coordinates: The coordinate convention the step writes, one of
            `"0-based half-open"` or `"1-based closed"`. Only meaningful for
            a step that produces `Intervals`: alignments and variant calls
            are 1-based closed by specification, while a tool that writes
            BED from a VCF changes the convention and must say so.
        cite: How the step asks to be cited, as one reference or several.
            `ligase methods` knows the reference pipeline's own tools; a
            step using anything else names its reference here, or the
            methods section reports that tool as uncited. A citation never
            joins the cache key — it cannot change a result, so adding one
            must not rerun anything.
        conversion: The registered conversion this step performs, named as
            the engine knows it — `"map_identifiers"` is the only one
            today. A conversion step is the one kind that *crosses* a
            dimension instead of inheriting it: its output declares the
            value its call's `to=` names, and every other dimension travels
            through unchanged. The step must take a `to` parameter, or it
            has not said where it converts to.

            Ordinary steps leave this alone. It exists so that
            `lg.map_identifiers` is an entry in the engine's conversion
            registry rather than a special case in the graph, which is what
            lets a future `liftover` be another entry.
        stochastic: That the step's tool draws on randomness -- IQ-TREE,
            RAxML, BEAST, AutoDock Vina, any model training. Given with
            `seed`, never without it.

            What it buys is in the audit. `ligase audit` distinguishes
            *deterministic* from *reproducible*, and a step that declares
            this is never called deterministic, however its bytes fall: a
            seeded tool is byte-identical because its seed was held still,
            and saying *deterministic* of it would tell a reader there is no
            randomness in the step. "This pipeline is reproducible" is a far
            stronger claim once the engine refuses to let you write an
            irreproducible step by accident.
        seed: The name of the parameter that seeds the step. It must be a
            parameter the function takes and it must be annotated `int`;
            either failing is a `StepDefinitionError` as the module is
            imported, like a missing return annotation.

            Given with `stochastic`, never without it. Naming a seed
            without declaring the step stochastic is refused rather than
            read as a shorthand, because one declaration with two spellings
            is how a second untested path gets in -- and because what the
            decision asks for is a step that *declares itself*.

            The seed is an ordinary typed parameter besides, so its value is
            hashed into the cache key like any other and two seeds are two
            runs. Nothing special is done to arrange that.

    Returns:
        A `Step`, which a type checker sees as the function it decorated.

    Raises:
        StepDefinitionError: If the signature is one a graph cannot use —
            an unannotated parameter, a missing or non-Ligase return
            annotation, or `*args`/`**kwargs` — or if `conversion` names a
            conversion the engine does not have, or the step performs one
            and takes no `to` parameter, or `stochastic` and `seed` do not
            agree, or the named seed is not a parameter of the step typed
            `int`.
        ValueError: If `coordinates` is not one of the two conventions, or
            a tool was named as an empty string.
    """
    if coordinates is not None and coordinates not in COORDINATES:
        raise ValueError(
            f"`{coordinates}` is not a coordinate system; expected "
            + " or ".join(COORDINATES)
        )

    def decorate(inner: Callable[P, R]) -> Step[P, R]:
        return Step(
            inner,
            tool=tool,
            coordinates=coordinates,
            cite=cite,
            conversion=conversion,
            stochastic=stochastic,
            seed=seed,
        )

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


def _tools(tool: str | Sequence[str] | None) -> tuple[str, ...]:
    """The tools a step named, as a tuple, however they were given."""
    if tool is None:
        return ()
    names = (tool,) if isinstance(tool, str) else tuple(tool)
    for name in names:
        if not name:
            raise ValueError(
                "a step's `tool` must name a command to run, not an empty "
                "string; a step that calls no external tool needs no `tool`"
            )
    return names


def _citations(cite: str | Sequence[str] | None) -> tuple[str, ...]:
    """A step's references, as a tuple.

    Unlike a tool, a reference is never validated against anything: there is
    no authority here to check it against, and a wrong citation is a
    scientific mistake rather than an engine one. Blank entries are refused,
    because a blank reference in a reference list is a bug nobody would
    notice.
    """
    if cite is None:
        return ()
    references = (cite,) if isinstance(cite, str) else tuple(cite)
    for reference in references:
        if not reference.strip():
            message = "a citation cannot be blank"
            raise ValueError(message)
    return references


def _bundle(written: _Bundle | None) -> Bundle | None:
    """The bundle the engine wrote, if one was asked for."""
    if written is None:
        return None
    root, files, steps, gaps = written
    return Bundle(root=root, files=list(files), steps=steps, gaps=gaps)


def _verification(checked: _Verification | None) -> Verification | None:
    """What checking a bundle found, if one was asked for."""
    if checked is None:
        return None
    results, inputs, reproduced, verdict, summary, text = checked
    return Verification(
        results=[
            ResultCheck(
                step=step,
                scope=scope,
                verdict=step_verdict,
                expected=expected,
                found=found,
                same_key=same_key,
            )
            for step, scope, step_verdict, expected, found, same_key in results
        ],
        inputs=[
            InputCheck(step=step, param=param, path=path, presence=presence)
            for step, param, path, presence in inputs
        ],
        reproduced=reproduced,
        verdict=verdict,
        summary=summary,
        text=text,
    )


def _recipe(produced: StepOutput) -> Shell | Written:
    """Ask a step body how its output is produced.

    The body is called here and nowhere else, and it is called for its
    answer rather than for its effect.
    """
    name = produced.step.name
    try:
        answer = produced.step.function(**produced.arguments)
    # Any failure in the user's own body, including reaching for an input's
    # contents before anything has run.
    except Exception as error:
        raise StepDefinitionError(
            f"step `{name}` could not say how to produce its output: "
            f"{error!r}. A body is called before anything has been "
            "computed, so it can only describe the work -- `lg.sh(...)` or "
            "`lg.write(...)` -- not do it"
        ) from error

    if not isinstance(answer, Shell | Written):
        raise StepDefinitionError(
            f"step `{name}` returned {answer!r}, which does not say how to "
            'produce anything; return `lg.sh("...")` for a command, or '
            "`lg.write(f)` for a step that writes the file itself"
        )
    return answer


def _bind(recipe: Written, produced: StepOutput) -> _Callback:
    """A `lg.write` body, with its inputs' paths filled in when it runs."""

    def write(out: str, inputs: list[tuple[str, list[str]]]) -> list[Noticed]:
        for parameter, files in inputs:
            given = produced.arguments.get(parameter)
            if isinstance(given, StepOutput) and files:
                given._produced = files[0]
        noticed = recipe.function(Path(out))
        if noticed is None:
            return []
        # Said here rather than left to whatever iterating it raises: the
        # natural one-liner, `lg.write(lambda out: out.write_text(...))`,
        # hands back however many characters were written.
        try:
            return [(str(code), str(value)) for code, value in noticed]
        except (TypeError, ValueError) as error:
            raise TypeError(
                f"the body of step `{produced.step.name}` returned "
                f"{noticed!r}. A body returns `None`, or what it noticed as "
                "pairs of a note's code and what the note is about"
            ) from error

    return write


def _source(function: Callable[..., Any]) -> str:
    """A step's code, as the cache key sees it.

    The source as written, so that changing what a step does reruns it. A
    comment-only edit therefore reruns it too, and then early cutoff stops
    the rerun going any further: too cold is a cost, too warm is a wrong
    answer.
    """
    try:
        return inspect.getsource(function)
    # A function defined in a REPL or an `exec` has no source to read. Its
    # compiled form still changes when it does, which is what the key needs.
    except (OSError, TypeError):
        code = function.__code__
        return f"{code.co_code.hex()}:{code.co_consts!r}"


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
