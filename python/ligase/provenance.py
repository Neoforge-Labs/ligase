"""How results were produced, and the bundle that lets someone check.

    methods = pipe.methods()
    print(methods.text)

    written = pipe.methods(bundle="paper/bundle").bundle
    checked = pipe.methods(verify="paper/bundle").verification
    assert checked.reproduced

A methods section is generated rather than written: every sentence follows a
fixed form, because a generator free to phrase things would also be free to
leave something out, and that is the one thing a methods section must not
do. The wording will want tidying; the versions in it will not want
checking.

The reports are worded by the engine, so `ligase methods` and a script print
the same thing.
"""

from __future__ import annotations

from dataclasses import dataclass, field

__all__ = [
    "BoundTo",
    "Bundle",
    "Claim",
    "ClaimCheck",
    "InputCheck",
    "Methods",
    "MethodsGap",
    "PinnedReference",
    "ResultCheck",
    "StepMethods",
    "Verification",
]

#: The same key and the same result: nothing feeding the step moved.
IDENTICAL = "identical"

#: A different key and the same result: the environment moved, the science
#: did not. What a reviewer on another machine usually gets.
REPRODUCED = "reproduced"

#: A different result.
DIFFERS = "differs"

#: The bundle expected a step this pipeline no longer produces.
MISSING = "missing"

#: An input is the file the bundle was built from.
SAME = "same"

#: An input is there, with different contents.
CHANGED = "changed"

#: An input is not where the bundle said it was.
ABSENT = "absent"

#: A claim rests on a computation this write-up accounts for.
BOUND = "bound"

#: Nothing in the run stands behind a claim.
UNBACKED = "unbacked"

#: Something stands behind a claim and disagrees with it. The stronger
#: statement: the claim is bound, to a computation that contradicts it.
CONTRADICTED = "contradicted"


@dataclass(frozen=True, slots=True)
class PinnedReference:
    """A reference sequence, pinned as firmly as its file allows.

    Nothing here is declared or guessed. The accession is the sequence name
    where the name is one — `K03455.1` carries its own version — and the
    checksum is the MD5 the SAM specification defines for `@SQ M5`, over the
    uppercased sequence with whitespace removed.

    Attributes:
        param: The parameter it filled.
        path: Where it was when the step ran.
        build: The label it was declared under, e.g. `HXB2`. Display only.
        accession: The accession with its version, where the sequence name
            is one, and `None` where it is not — `chr1` is a name, not an
            accession, and a methods section must not cite it as one.
        contigs: How many sequences it holds.
        length: Their total length in bases.
        checksum: The MD5 of the sequence, for a single-sequence reference.
        fingerprint: A short digest of the whole identity, always present.
    """

    param: str
    path: str
    build: str | None
    accession: str | None
    contigs: int
    length: int
    checksum: str | None
    fingerprint: str


@dataclass(frozen=True, slots=True)
class StepMethods:
    """One way a step was run.

    One written step becomes one node per sample, and all of them are one
    entry here. Where two nodes of a step genuinely were run differently
    they are two entries, because then the difference is the thing worth
    reporting.

    Attributes:
        step: The step's name, as written.
        nodes: How many nodes this covers.
        scopes: What tells them apart, e.g. the sample identifiers.
        scope_kind: What kind of thing that is, e.g. `sample`.
        tools: Each tool and the whole version it printed.
        parameters: Each plain parameter and its value, as it was hashed.
        command: The command as written in the pipeline, placeholders and
            all, or `None` for a step whose body is Python.
    """

    step: str
    nodes: int
    scopes: list[str]
    scope_kind: str | None
    tools: list[tuple[str, str]]
    parameters: list[tuple[str, str]]
    command: str | None


@dataclass(frozen=True, slots=True)
class MethodsGap:
    """A step nothing is known about.

    Its output is in the cache from a run that recorded no provenance, and
    nothing can reconstruct how it was produced: the command that would have
    said so is not in the hash, by design. `ligase run --force` runs it
    again and records it.

    Attributes:
        step: The step's name.
        scopes: What tells its nodes apart.
        nodes: How many of them are unaccounted for.
    """

    step: str
    scopes: list[str]
    nodes: int


@dataclass(frozen=True, slots=True)
class Claim:
    """A number or a phrase a paper states, and where it came from.

        pipe = lg.Pipeline(
            outputs,
            claims=[
                lg.Claim(
                    "variants were called above 1% allele frequency",
                    step="call",
                    parameter="min_freq",
                    value=0.01,
                ),
            ],
        )

    `text` and `value` are separate on purpose. A paper writes **1%**; the
    pipeline holds **0.01**. Comparing the sentence against the parameter
    would be a guess about arithmetic in English, so the sentence is what a
    reader greps the write-up for and `value` is what the engine compares —
    written the way the pipeline holds it.

    Everything except `text` is checked against the run: the step is one the
    write-up accounts for, the sample is one it ran on, the parameter or tool
    is one the record holds, and the value agrees. A claim that survives all
    of that is `bound` to the step key and the output hash behind it; one
    that does not is reported rather than reprinted, because a declaration
    nothing can check is a comment.

    A claim never enters a cache key, so editing one reruns nothing.

    Attributes:
        text: The sentence as the paper states it. Never checked, and
            cannot be.
        step: The step that produced it, by name.
        scope: The sample it is about, where it is about one. `None` means
            the step across every sample it ran on, which is right for
            "every sample was interpreted" and wrong for a count from one of
            them. There is no inference.
        parameter: The parameter it restates, where it restates one.
        tool: The tool whose version it states, where it states one.
        value: What the paper says that parameter or version is, rendered
            the way the run recorded it.

    Raises:
        ValueError: If it names both a parameter and a tool, gives a value
            with neither to compare it against, or names either with no
            value — each of which is a declaration with nothing behind it.
    """

    text: str
    step: str
    scope: str | None = None
    parameter: str | None = None
    tool: str | None = None
    value: str | float | None = None

    def __post_init__(self) -> None:
        """Refuse a declaration the engine could not check.

        Here rather than at run time for the reason `stochastic=` and
        `seed=` are checked where the step is written: the two halves are
        one declaration, and the moment to say so is the moment it is made.
        """
        if not self.text.strip():
            raise ValueError(
                "a claim needs the sentence the paper states, so a reader can "
                "find it: lg.Claim('...', step=...)"
            )
        if not self.step.strip():
            raise ValueError(
                f"the claim {self.text!r} names no step, so there is nothing "
                "for it to rest on. Add `step=` naming the step that "
                "produces this number"
            )
        if self.parameter is not None and self.tool is not None:
            raise ValueError(
                f"the claim {self.text!r} restates both a parameter and a "
                "tool version. A claim restates one thing; write two claims"
            )
        named = self.parameter if self.parameter is not None else self.tool
        if named is not None and self.value is None:
            raise ValueError(
                f"the claim {self.text!r} names {named!r} and gives no "
                "`value=`, so there is nothing to compare it against"
            )
        if named is None and self.value is not None:
            raise ValueError(
                f"the claim {self.text!r} gives a value and names nothing it "
                "is the value of. Add `parameter=` or `tool=`, or drop "
                "`value=` and let the claim rest on the step"
            )

    @property
    def rendered(self) -> str | None:
        """The value as the engine compares it: text, however it was given.

        `value=0.01` is what a person writes and `0.01` is how the parameter
        was hashed, so one `str()` is the whole conversion. A number in a
        paper is text.
        """
        return None if self.value is None else str(self.value)

    def __repr__(self) -> str:
        """Identify the claim by its sentence and the step behind it."""
        return f"<Claim {self.text!r} on {self.step}>"


@dataclass(frozen=True, slots=True)
class BoundTo:
    """One computation a claim rests on.

    The step key, not the occasion. A key is the identity of a computation —
    the same code, parameters, inputs and tool versions — so a step served
    from the cache backs its claim completely, and what this adds is when
    that computation happened, which may be long before the run that wrote
    the claim up.

    Attributes:
        scope: The sample it was for, where anything tells the nodes apart.
        key: The step key, in full.
        canonical: The canonical hash of what it produced.
        recorded_at: When the computation happened, as seconds since the
            Unix epoch — not when this run fetched it.
        platform: The platform it happened on.
    """

    scope: str | None
    key: str
    canonical: str
    recorded_at: int
    platform: str


@dataclass(frozen=True, slots=True)
class ClaimCheck:
    """One claim, and what the run had to say about it.

    Attributes:
        claim: The claim as it was declared.
        verdict: `bound`, `unbacked` or `contradicted`.
        reason: The sentence the write-up prints for it, so a script
            reporting one claim says what `ligase methods` says.
        recorded: What the run recorded, where that disagrees with the
            claim, and `None` otherwise.
        bound: The computations it rests on. Empty for an unbacked claim;
            one entry for a claim naming a sample, and one per sample for a
            claim that names none.
    """

    claim: Claim
    verdict: str
    reason: str
    recorded: str | None
    bound: list[BoundTo] = field(default_factory=list)

    @property
    def is_bound(self) -> bool:
        """Whether the write-up can stand behind the claim."""
        return self.verdict == BOUND

    def __repr__(self) -> str:
        """Identify the check by its verdict and the claim it is about."""
        return f"<ClaimCheck {self.verdict} {self.claim.text!r}>"


@dataclass(frozen=True, slots=True)
class Bundle:
    """A reproducibility bundle that was written.

    Attributes:
        root: Where it is.
        files: Everything in it, relative to the root.
        steps: How many steps it accounts for.
        gaps: How many it could not.
    """

    root: str
    files: list[str]
    steps: int
    gaps: int

    def __repr__(self) -> str:
        """Identify the bundle by where it is and how much it covers."""
        return f"<Bundle {self.root}, {self.steps} steps, {len(self.files)} files>"


@dataclass(frozen=True, slots=True)
class ResultCheck:
    """One result, as a bundle recorded it and as this machine produced it.

    Attributes:
        step: The step's name.
        scope: What tells this node apart, where anything does.
        verdict: `identical`, `reproduced`, `differs` or `missing`.
        expected: The canonical hash the bundle recorded.
        found: The canonical hash produced here, or `None` where this
            pipeline produced nothing for it.
        same_key: Whether the cache key matched too. False with a verdict of
            `reproduced` means something in the environment moved and the
            result did not, which is the interesting outcome.
    """

    step: str
    scope: str | None
    verdict: str
    expected: str
    found: str | None
    same_key: bool

    @property
    def reproduced(self) -> bool:
        """Whether the result came out the same, by either route."""
        return self.verdict in (IDENTICAL, REPRODUCED)


@dataclass(frozen=True, slots=True)
class InputCheck:
    """One input file, as a bundle recorded it and as it is here.

    Attributes:
        step: The step it fed.
        param: The parameter it filled.
        path: Where the bundle said it was.
        presence: `same`, `changed` or `absent`, by content rather than by
            name — which is the only way to tell the file a bundle was built
            from apart from a different file with the same name.
    """

    step: str
    param: str
    path: str
    presence: str


@dataclass(frozen=True, slots=True)
class Verification:
    """What checking a bundle against this machine found.

    Attributes:
        results: Every result the bundle recorded.
        inputs: Every input it recorded.
        reproduced: Whether every recorded result came out the same.
        verdict: The worst thing found.
        summary: One line — `12 results: 4 identical, 8 reproduced.`
        text: The whole report as the command line prints it.
    """

    results: list[ResultCheck] = field(default_factory=list)
    inputs: list[InputCheck] = field(default_factory=list)
    reproduced: bool = False
    verdict: str = MISSING
    summary: str = ""
    text: str = ""

    @property
    def differing(self) -> list[ResultCheck]:
        """The results that did not come out the same."""
        return [check for check in self.results if not check.reproduced]

    @property
    def moved(self) -> list[InputCheck]:
        """The inputs that are not the ones the bundle was built from.

        Worth reading before any `differs` verdict: a changed input explains
        a changed output, and most of a bundle's inputs are named rather
        than carried -- a local file by content hash, a pinned remote by
        locator and digest -- so they are the ones that can have moved. A
        recorded query cannot appear here, because its response body
        travels in the bundle and is replayed from it.
        """
        return [check for check in self.inputs if check.presence != SAME]

    def __repr__(self) -> str:
        """Identify the verification by its summary."""
        return f"<Verification {self.summary}>"


@dataclass(frozen=True, slots=True)
class Methods:
    """How a set of results was produced.

    Attributes:
        steps: Each distinct way a step was run, in the order the pipeline
            runs them.
        gaps: Steps nothing is known about.
        references: Every reference any step was given, pinned, with no
            duplicates.
        citations: The reference list, as a methods section prints it.
        uncited: Tools with nothing to cite — named rather than omitted, so
            a short reference list cannot pass for a complete one.
        claims: Each claim the pipeline declared, and what became of it.
            Empty for a pipeline that declared none, which is why nothing
            about an existing write-up changes.
        engines: Every version of Ligase that produced any of the results.
            One entry for a run on one machine; more for a distributed run
            where a worker was on an older wheel.
        platforms: Every platform any of the results were produced on. One
            entry for a run on one machine; more for a mixed cluster, or an
            ARM workstation beside x86 nodes.
        summary: One line — `12 steps, all accounted for.`
        text: The paragraph and the reference list, as the command line
            prints them.
        bundle: The bundle written, where one was asked for.
        verification: What checking a bundle found, where one was asked for.
    """

    steps: list[StepMethods] = field(default_factory=list)
    gaps: list[MethodsGap] = field(default_factory=list)
    references: list[PinnedReference] = field(default_factory=list)
    citations: list[str] = field(default_factory=list)
    uncited: list[str] = field(default_factory=list)
    claims: list[ClaimCheck] = field(default_factory=list)
    engines: list[str] = field(default_factory=list)
    platforms: list[str] = field(default_factory=list)
    summary: str = ""
    text: str = ""
    bundle: Bundle | None = None
    verification: Verification | None = None

    @property
    def accounted_for(self) -> bool:
        """Whether every step's production is recorded and every claim bound.

        False means the write-up has a hole in it and says so. A number the
        analysis does not support is such a hole: a property with this name
        that returned True for one would be making the engine's own promise
        falsely.
        """
        return not self.gaps and not self.unbacked

    @property
    def unbacked(self) -> list[ClaimCheck]:
        """The claims this write-up cannot stand behind.

        Contradicted ones included: a number the run positively disagrees
        with is not a number the write-up stands behind.
        """
        return [check for check in self.claims if not check.is_bound]

    @property
    def tools(self) -> list[tuple[str, str]]:
        """Every tool any step used, with the version it ran at."""
        seen: list[tuple[str, str]] = []
        for step in self.steps:
            for tool in step.tools:
                if tool not in seen:
                    seen.append(tool)
        return sorted(seen)

    def __repr__(self) -> str:
        """Identify the methods section by its summary."""
        return f"<Methods {self.summary}>"
