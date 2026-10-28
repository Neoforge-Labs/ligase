"""What a run did.

`pipe.run()` raises if any step failed, with the whole report attached —
the same shape as `pipe.check()`, and for the same reason: a pipeline that
half-worked and said nothing is how a wrong result reaches a paper.

    try:
        report = pipe.run()
    except lg.RunError as error:
        print(error.report.text)

The report is worded by the engine, so `ligase run` and a script print the
same thing.
"""

from __future__ import annotations

from dataclasses import dataclass, field

__all__ = ["Divergence", "RunError", "RunReport", "StepFailure", "StepRun"]


@dataclass(frozen=True, slots=True)
class StepRun:
    """One step that produced an output.

    Attributes:
        step: The step's name.
        sample: The sample it was for, or `None` for a step that spans more
            than one — naming such a step after one of its samples would
            send you to the wrong data.
        outcome: `ran`, `cached`, or `skipped`. `skipped` is early cutoff:
            the step's key was already known although something above it
            reran, because that rerun produced the same output as before.
        output: Where the output is, in the content-addressed store.
        key: The cache key, as hex.
        hash: The hash of the output's bytes, as hex, which is also its name
            in the store.
        canonical: The hash of what the output *means*, as hex: its records
            sorted and its volatile headers stripped. This is what the steps
            below were keyed on, so two runs that agree here skip everything
            downstream however far their bytes diverged.
        duration_ms: How long it took, when it ran; zero otherwise.
        node: Which node of the graph it was.
        notes: What was noticed about this output and its hash cannot say —
            one sentence each, and usually none. A note is not a failure:
            the step ran and its result stands.

            Two things put them here. Canonicalising the output finds the
            first kind, of which there is one in v0.1: a sample identifier
            that is a file name rather than a sample, which makes this
            output's canonical hash depend on where that file was. The
            step's own body says the second kind — an assumption its work
            rested on, which the file it wrote need not show. A resistance
            reading made without coverage is taking every position the
            caller did not report to be reference, and says so here (D32).
    """

    step: str
    sample: str | None
    outcome: str
    output: str
    key: str
    hash: str
    canonical: str
    duration_ms: int
    node: int
    notes: tuple[str, ...] = ()

    @property
    def ran(self) -> bool:
        """Whether this step actually ran, rather than being served."""
        return self.outcome == "ran"


@dataclass(frozen=True, slots=True)
class StepFailure:
    """One step that did not produce anything.

    A step whose input failed is in neither list: one cause, one failure.

    Attributes:
        step: The step's name.
        sample: The sample it was for, where it stands for one.
        message: What went wrong, in the tool's own words where there are
            any.
        node: Which node of the graph it was.
    """

    step: str
    sample: str | None
    message: str
    node: int


@dataclass(frozen=True, slots=True)
class Divergence:
    """One step another worker computed to a different answer.

    Two workers that agree on a step key agreed on the code, the
    parameters, the inputs' contents and the tool versions, so they are
    doing the same work and a race between them is harmless — they almost
    always disagree about the output's *bytes*, because a tool writes its
    own command line or a timestamp into what it produces, and nothing
    downstream is keyed on bytes.

    This is the case where they disagree about what the output **means**,
    which nothing downstream can absorb. Either the kind's canonical form
    does not fix its result, or the step's tool is stochastic in something
    other than its seed — a thread count, most often, which two cluster
    nodes do not share.

    It is not a failure: the step ran and produced something. The store's
    answer is the one in force on every machine, so nothing below the step
    disagrees between workers. But the step does not produce one answer
    from one set of inputs, and `pipe.audit()` is how to find out which of
    the two reasons it is.

    Attributes:
        step: The step's name.
        sample: The sample it was for, or `None` for a step spanning more
            than one.
        ours: What this worker's output means, as hex.
        theirs: What the output already in the store means, as hex — the
            one in force.
        node: Which node of the graph it was.
    """

    step: str
    sample: str | None
    ours: str
    theirs: str
    node: int


@dataclass(frozen=True, slots=True)
class RunReport:
    """Everything a run did.

    Attributes:
        steps: Every step that produced an output, in graph order.
        failures: Every step that did not.
        summary: One line — `12 steps: 8 ran, 4 from cache.` — counting
            places rather than names, since one written step is many steps
            under fan-out.
        text: The whole report as the command line prints it, worded by the
            engine so that every surface reads the same.
        hashed: Input files that had to be read and hashed.
        prefiltered: Input files whose hash was already known from their
            size and mtime, and so were not read at all.
        canonicalised: Files that had to be parsed to find out what they
            mean. Like `hashed`, this is zero on a run that found everything
            it needed already known.
        worker: What this worker called itself, or `None` for an ordinary
            run.
        divergences: Steps another worker computed to a different answer.
            Empty for almost every run, and for every run on one machine.
    """

    steps: list[StepRun] = field(default_factory=list)
    failures: list[StepFailure] = field(default_factory=list)
    summary: str = ""
    text: str = ""
    hashed: int = 0
    prefiltered: int = 0
    canonicalised: int = 0
    worker: str | None = None
    divergences: list[Divergence] = field(default_factory=list)

    @property
    def ran(self) -> list[StepRun]:
        """The steps that actually ran, in graph order."""
        return [step for step in self.steps if step.outcome == "ran"]

    @property
    def cached(self) -> list[StepRun]:
        """The steps that were served from the cache."""
        return [step for step in self.steps if step.outcome == "cached"]

    @property
    def skipped(self) -> list[StepRun]:
        """The steps an upstream rerun turned out not to change."""
        return [step for step in self.steps if step.outcome == "skipped"]

    def outputs(self, step: str) -> list[str]:
        """Where one step's outputs are, in graph order.

        Args:
            step: The step's name. Under fan-out there is one output per
                sample.

        Returns:
            One path per node of that name.
        """
        return [run.output for run in self.steps if run.step == step]

    def __repr__(self) -> str:
        """Identify the report by its summary."""
        return f"<RunReport {self.summary}>"


class RunError(Exception):
    """A step failed.

    Attributes:
        report: The whole run, including everything that did succeed —
            which for a cohort is usually most of it.
        failures: The steps that failed, as a shortcut.
    """

    def __init__(self, report: RunReport) -> None:
        """Carry the whole report, not just the first failure."""
        self.report = report
        self.failures = report.failures
        first = report.failures[0] if report.failures else None
        where = "" if first is None else f"step `{first.step}`: {first.message}"
        super().__init__(f"{report.summary} {where}".strip())
