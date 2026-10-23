"""What running a step twice showed.

    report = pipe.audit("align")
    print(report.text)

    if report.verdict == "not reproducible":
        for step in report.unreproducible:
            print(step.step, step.sample)

Three verdicts, not two. A step whose bytes move while its result does not
is `reproducible` — which is the ordinary case for any tool that records its
own command line, and not a problem. Only a step whose *result* moves is
`not reproducible`.

The report is worded by the engine, so `ligase audit` and a script print the
same thing.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from .run import StepFailure

__all__ = ["AuditReport", "StepAudit"]

#: The bytes matched, and the step declared no seed.
DETERMINISTIC = "deterministic"

#: The two runs agreed on the result. Either the bytes differ and what
#: they mean does not, or the step declared itself stochastic, which can
#: reach no better verdict than this.
REPRODUCIBLE = "reproducible"

#: The two runs disagree about the result.
NOT_REPRODUCIBLE = "not reproducible"


@dataclass(frozen=True, slots=True)
class StepAudit:
    """What two runs of one step showed.

    Attributes:
        step: The step's name.
        sample: The sample it was for, or `None` for a step that spans more
            than one.
        scope: How a report names this node where its step's name is shared
            — `sample SRR8525904` — or `None` where the name is already
            unambiguous.
        verdict: `deterministic`, `reproducible`, or `not reproducible`.
        first: The first run's output hash.
        second: The second run's output hash.
        first_canonical: What the first run's output means.
        second_canonical: What the second run's output means.
        first_duration_ms: How long the first run took.
        second_duration_ms: How long the second run took.
        node: Which node of the graph it was.
        seed: The parameter this step declared as its seed and the value it
            was held at, as `("seed", "42")`. `None` for a step that
            declared none.

            A step with a seed is never `deterministic`, however its bytes
            fall: byte equality under a fixed seed says the seed held, not
            that the step has no randomness in it. The `reproducible` it
            gets instead is the best verdict available to it.
    """

    step: str
    sample: str | None
    scope: str | None
    verdict: str
    first: str
    second: str
    first_canonical: str
    second_canonical: str
    first_duration_ms: int
    second_duration_ms: int
    node: int
    seed: tuple[str, str] | None = None

    @property
    def reproducible(self) -> bool:
        """Whether the two runs agreed on the result.

        True for a step whose bytes moved and whose meaning did not: that is
        what reproducible means here.
        """
        return self.verdict != NOT_REPRODUCIBLE


@dataclass(frozen=True, slots=True)
class AuditReport:
    """What an audit found.

    Attributes:
        steps: Every node audited, in graph order.
        failures: Every node that could not be audited because it failed.
        verdict: The worst verdict any node got — the worst rather than the
            commonest, since one irreproducible sample in a cohort is an
            irreproducible step.
        summary: One line — `4 steps: 1 deterministic, 3 reproducible.`
        text: The whole report as the command line prints it, worded by the
            engine so that every surface reads the same.
    """

    steps: list[StepAudit] = field(default_factory=list)
    failures: list[StepFailure] = field(default_factory=list)
    verdict: str = DETERMINISTIC
    summary: str = ""
    text: str = ""

    @property
    def reproducible(self) -> bool:
        """Whether every node audited agreed with itself."""
        return all(step.reproducible for step in self.steps) and not self.failures

    @property
    def unreproducible(self) -> list[StepAudit]:
        """The nodes whose two runs disagreed about the result."""
        return [step for step in self.steps if not step.reproducible]

    def __repr__(self) -> str:
        """Identify the report by its summary."""
        return f"<AuditReport {self.summary}>"
