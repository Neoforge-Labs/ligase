"""M2: `@lg.step` reads a signature and defers the body.

The decorator never runs what it decorates. Calling a step records a node in
a graph and hands back a handle, which is what makes `pipe.check()` possible
at all: a pipeline is type-checked before a single byte of sequence is read.
"""

from __future__ import annotations

import pytest

import ligase as lg
from ligase.types import Alignment, Intervals, Reads, Reference, Variants
from tests.conftest import fixture

# A sentinel every step body sets. Nothing in M2 may ever flip it.
RAN: list[str] = []


@lg.step(tool="minimap2")
def align(reads: Reads, ref: Reference) -> Alignment:
    """Align reads, if anything ever ran it."""
    RAN.append("align")
    raise AssertionError("a step body must never run while building a graph")


@lg.step
def call(aln: Alignment, ref: Reference, min_freq: float = 0.01) -> Variants:
    """Call variants, if anything ever ran it."""
    RAN.append("call")
    raise AssertionError("a step body must never run while building a graph")


@pytest.fixture(autouse=True)
def _nothing_ran() -> None:
    RAN.clear()


def test_calling_a_step_does_not_run_its_body() -> None:
    reads = lg.reads(fixture("reads_phred33_1.fastq"))
    ref = lg.reference(fixture("tiny_bare.fasta"), organism="HIV-1")

    produced = lg.handle(align(reads, ref))

    assert RAN == []
    assert produced.step.name == "align"
    assert produced.kind == "Alignment"


def test_a_step_keeps_the_tool_it_was_given() -> None:
    # Recorded now, used by the cache in M3: a tool's version belongs in the
    # key, so a minimap2 upgrade invalidates the alignments it produced.
    assert align.tool == "minimap2"
    assert call.tool is None


def test_a_step_separates_dataset_parameters_from_plain_ones() -> None:
    assert [p.name for p in call.dataset_parameters] == ["aln", "ref"]
    assert [p.kind for p in call.dataset_parameters] == ["Alignment", "Reference"]
    assert [p.name for p in call.plain_parameters] == ["min_freq"]


def test_steps_compose_without_running_anything() -> None:
    reads = lg.reads(fixture("reads_phred33_1.fastq"))
    ref = lg.reference(fixture("tiny_bare.fasta"), organism="HIV-1")

    produced = lg.handle(call(align(reads, ref), ref, min_freq=0.05))

    assert RAN == []
    assert produced.kind == "Variants"
    assert produced.arguments["min_freq"] == 0.05


def test_a_step_can_still_be_called_for_real() -> None:
    # The body is not lost: `step.function` is the undecorated callable, which
    # is what execution will run in M3.
    assert call.function.__name__ == "call"
    with pytest.raises(AssertionError):
        call.function(None, None)  # type: ignore[arg-type]
    assert RAN == ["call"]


def test_the_wrong_arguments_fail_at_the_call_like_any_function() -> None:
    reads = lg.reads(fixture("reads_phred33_1.fastq"))

    with pytest.raises(TypeError):
        align(reads)  # type: ignore[call-arg]


# ----------------------------------------------------- bad step definitions --


def test_a_step_must_annotate_what_it_returns() -> None:
    with pytest.raises(lg.StepDefinitionError) as caught:

        @lg.step
        def nameless(reads: Reads):  # type: ignore[no-untyped-def]  # noqa: ANN202
            return None

    assert "return" in str(caught.value)
    assert "nameless" in str(caught.value)


def test_a_step_must_return_a_ligase_data_type() -> None:
    with pytest.raises(lg.StepDefinitionError) as caught:

        @lg.step  # type: ignore[type-var]
        def wrong(reads: Reads) -> str:
            return ""

    assert "str" in str(caught.value)


def test_every_parameter_must_be_annotated() -> None:
    # An unannotated parameter might be a dataset, and a dataset the graph
    # does not know about is an unchecked edge -- which is the whole class of
    # bug this exists to catch.
    with pytest.raises(lg.StepDefinitionError) as caught:

        @lg.step
        def loose(reads: Reads, ref) -> Alignment:  # type: ignore[no-untyped-def]  # noqa: ANN001
            return reads  # type: ignore[return-value]

    assert "ref" in str(caught.value)


def test_a_step_cannot_take_var_args() -> None:
    with pytest.raises(lg.StepDefinitionError) as caught:

        @lg.step
        def many(*parts: Variants) -> Variants:
            return parts[0]

    assert "parts" in str(caught.value)


def test_an_interval_step_may_declare_the_coordinates_it_writes() -> None:
    @lg.step(coordinates="0-based half-open")
    def regions(v: Variants) -> Intervals:
        return v  # type: ignore[return-value]

    assert regions.coordinates == "0-based half-open"

    with pytest.raises(ValueError, match="coordinate system"):

        @lg.step(coordinates="nearly 1-based")
        def confused(v: Variants) -> Intervals:
            return v  # type: ignore[return-value]


# ---------------------------------------------------------- stochastic steps --
#
# M11: a step whose tool draws on randomness declares itself and names the
# parameter that seeds it. The two keywords are one declaration in two parts,
# so each half alone is refused -- see D40. These are decoration-time
# refusals, like a missing return annotation: a mistake in the pipeline's
# source, said as soon as the source is read.


def test_a_seeded_stochastic_step_keeps_what_it_declared() -> None:
    @lg.step(tool="iqtree", stochastic=True, seed="seed")
    def tree(v: Variants, seed: int = 42) -> Intervals:
        return v  # type: ignore[return-value]

    assert tree.stochastic is True
    assert tree.seed == "seed"
    # The seed is an ordinary plain parameter besides, which is what puts it
    # in the cache key without anything special being done about it.
    assert [p.name for p in tree.plain_parameters] == ["seed"]


def test_an_ordinary_step_declares_neither() -> None:
    assert align.stochastic is False
    assert align.seed is None


def test_a_stochastic_step_must_name_its_seed() -> None:
    with pytest.raises(lg.StepDefinitionError) as caught:

        @lg.step(tool="iqtree", stochastic=True)
        def unseeded(v: Variants, iterations: int = 1000) -> Intervals:
            return v  # type: ignore[return-value]

    message = str(caught.value)
    assert "unseeded" in message
    assert "seed=" in message


def test_the_refusal_suggests_an_int_parameter_it_can_see() -> None:
    # The error is a deliverable, not a by-product: a step that takes
    # `seed: int` and forgot to name it is the overwhelmingly common case, so
    # the message says which parameter it thinks was meant.
    with pytest.raises(lg.StepDefinitionError) as caught:

        @lg.step(tool="iqtree", stochastic=True)
        def nearly(v: Variants, seed: int = 42) -> Intervals:
            return v  # type: ignore[return-value]

    assert 'seed="seed"' in str(caught.value)


def test_a_named_seed_must_be_a_parameter_the_step_takes() -> None:
    with pytest.raises(lg.StepDefinitionError) as caught:

        @lg.step(stochastic=True, seed="rng")
        def absent(v: Variants, seed: int = 42) -> Intervals:
            return v  # type: ignore[return-value]

    message = str(caught.value)
    assert "rng" in message
    # And it names what the step does take, so the fix is in front of the
    # reader rather than in the signature they have to go back to.
    assert "seed" in message


def test_a_seed_must_be_typed_int() -> None:
    with pytest.raises(lg.StepDefinitionError) as caught:

        @lg.step(stochastic=True, seed="seed")
        def stringly(v: Variants, seed: str = "42") -> Intervals:
            return v  # type: ignore[return-value]

    message = str(caught.value)
    assert "seed" in message
    assert "str" in message
    assert "int" in message


def test_a_seed_cannot_be_a_dataset_parameter() -> None:
    # Caught by the same annotation test, and worth its own case: a dataset
    # parameter is the one wrong answer that would otherwise typecheck as a
    # parameter the step really does take.
    with pytest.raises(lg.StepDefinitionError) as caught:

        @lg.step(stochastic=True, seed="v")
        def datawise(v: Variants, seed: int = 42) -> Intervals:
            return v  # type: ignore[return-value]

    assert "Variants" in str(caught.value)


def test_naming_a_seed_without_declaring_stochastic_is_refused() -> None:
    # The spec describes only the other direction. This one is refused rather
    # than read as a shorthand for it, because two spellings of one
    # declaration is how a second untested code path gets in -- and because
    # D40's content is that a stochastic step *declares itself*.
    with pytest.raises(lg.StepDefinitionError) as caught:

        @lg.step(seed="seed")
        def quiet(v: Variants, seed: int = 42) -> Intervals:
            return v  # type: ignore[return-value]

    message = str(caught.value)
    assert "quiet" in message
    assert "stochastic=True" in message


def test_the_refusal_does_not_guess_a_seed_it_cannot_know() -> None:
    # A wrong suggestion is worse than none. `iterations: int` is an `int`
    # parameter and is not a seed, so the message says what it can see and
    # leaves the choice to the person who knows the tool.
    with pytest.raises(lg.StepDefinitionError) as caught:

        @lg.step(tool="iqtree", stochastic=True)
        def guessy(v: Variants, iterations: int = 1000) -> Intervals:
            return v  # type: ignore[return-value]

    message = str(caught.value)
    assert "iterations" in message
    assert 'seed="iterations"' not in message


def test_the_refusal_says_when_there_is_no_int_parameter_at_all() -> None:
    with pytest.raises(lg.StepDefinitionError) as caught:

        @lg.step(tool="iqtree", stochastic=True)
        def nothing_to_name(v: Variants, cutoff: float = 0.1) -> Intervals:
            return v  # type: ignore[return-value]

    assert "no `int` parameter" in str(caught.value)
