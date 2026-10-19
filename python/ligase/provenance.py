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
    "Bundle",
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
        a changed output, and a bundle cannot fetch its own data.
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
        engine: The version of Ligase that produced the results.
        platform: The platform they were produced on.
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
    engine: str = ""
    platform: str = ""
    summary: str = ""
    text: str = ""
    bundle: Bundle | None = None
    verification: Verification | None = None

    @property
    def accounted_for(self) -> bool:
        """Whether every step's production is recorded.

        False means the paragraph has a hole in it and says so.
        """
        return not self.gaps

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
