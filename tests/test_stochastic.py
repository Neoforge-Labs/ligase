"""M11: a step whose tool draws on randomness declares itself.

`web/content/project/decisions.mdx` D40 is the argument. These tests are the
teeth, on a real pipeline through the real cache.

The steps here shuffle a ranked list. That is a deliberate choice of vehicle.
§5.4 of the v0.2 spec offers "IQ-TREE on a small alignment, or a seeded Python
step where the tool is absent", and the Python step is taken for two reasons:
IQ-TREE is installed on no machine this project builds on -- it is not among
the `demo` job's three tools -- so an IQ-TREE test would skip everywhere, and
`test_demo.py` already says why a property that only holds where a tool is
installed is worth less than one that always runs. The other reason is that
§5.4's example returns a `Tree`, and D35 refuses that kind: a tree brings no
dimension axis that `EntitySet` and `RankedList` have not already claimed, so
what is left of it is Newick with a schema, which is a parser.

A `RankedList` is the right stand-in because its canonical form keeps row
order (D41). The seed's effect is therefore visible in the engine's own
comparison rather than hidden behind a sort, which is what lets these tests
assert on verdicts and hashes instead of on the step's internals.
"""

from __future__ import annotations

import random
from pathlib import Path

import pytest

import ligase as lg
from ligase.types import RankedList
from tests.conftest import fixture


def _shuffle(source: str, out: Path, seed: int) -> None:
    """Write the rows of a ranked list in a seeded random order."""
    header, *rows = Path(source).read_text().splitlines()
    random.Random(seed).shuffle(rows)
    out.write_text("\n".join([header, *rows]) + "\n")


def _shuffle_unseeded(source: str, out: Path) -> None:
    """Shuffle on system randomness, ignoring the seed the step declared."""
    header, *rows = Path(source).read_text().splitlines()
    random.SystemRandom().shuffle(rows)
    out.write_text("\n".join([header, *rows]) + "\n")


@lg.step(stochastic=True, seed="seed")
def permute(ranking: RankedList, seed: int = 42) -> RankedList:
    """Reorder a ranking, reproducibly, from its seed."""
    return lg.write(lambda out: _shuffle(ranking.file, out, seed))


@lg.step(stochastic=True, seed="seed")
def permute_ignoring_the_seed(ranking: RankedList, seed: int = 42) -> RankedList:
    """Declare a seed and draw on something else: a false declaration."""
    return lg.write(lambda out: _shuffle_unseeded(ranking.file, out))


@lg.step
def permute_undeclared(ranking: RankedList, seed: int = 42) -> RankedList:
    """The same work, undeclared, for the contrast the verdicts turn on."""
    return lg.write(lambda out: _shuffle(ranking.file, out, seed))


def flowed(text: str) -> str:
    """A methods paragraph with its line wrapping undone.

    The generator wraps at 76 columns, so a phrase these tests are about can
    land across two lines. What is asserted is the prose, not where it broke.
    """
    return " ".join(text.split())


def ranking() -> RankedList:
    """A ranked list of genes, with the dimensions a TSV cannot state."""
    return lg.ranked_list(
        fixture("ranked_degs.tsv"),
        organism="Homo sapiens",
        identifier_namespace="HGNC symbol",
        subject="lung adenocarcinoma",
        release="HGNC 2026-09",
    )


# ------------------------------------------------------------------ audit --


def test_a_seeded_stochastic_step_audits_as_reproducible(tmp_path: Path) -> None:
    pipe = lg.Pipeline(permute(ranking(), seed=7))

    report = pipe.audit("permute", cache=tmp_path / ".ligase")

    assert report.verdict == "reproducible", report.text
    assert report.reproducible


def test_a_stochastic_step_is_never_reported_deterministic(tmp_path: Path) -> None:
    # The clause D40 turns on. This step is byte-identical across two runs
    # with one seed -- which is what a correctly seeded stochastic tool does,
    # and the exact case where byte equality alone would have said
    # `deterministic`. It is not deterministic: the bytes match because the
    # seed was held still, and a reader told `deterministic` would conclude
    # there is no randomness in the step.
    pipe = lg.Pipeline(permute(ranking(), seed=7))

    report = pipe.audit("permute", cache=tmp_path / ".ligase")
    audited = report.steps[0]

    assert audited.first == audited.second, "the bytes should match here"
    assert audited.verdict != "deterministic", report.text
    assert audited.verdict == "reproducible", report.text


def test_the_same_work_undeclared_is_reported_deterministic(tmp_path: Path) -> None:
    # The contrast, over the same body and the same seed. The verdict turns
    # on the declaration and on nothing else, which is what makes the
    # declaration worth making.
    pipe = lg.Pipeline(permute_undeclared(ranking(), seed=7))

    report = pipe.audit("permute_undeclared", cache=tmp_path / ".ligase")

    assert report.steps[0].verdict == "deterministic", report.text


def test_an_audited_stochastic_step_reports_its_seed(tmp_path: Path) -> None:
    pipe = lg.Pipeline(permute(ranking(), seed=7))

    report = pipe.audit("permute", cache=tmp_path / ".ligase")

    assert report.steps[0].seed == ("seed", "7")


def test_a_stochastic_step_that_ignores_its_seed_is_not_reproducible(
    tmp_path: Path,
) -> None:
    # A step that names a seed and still disagrees with itself has a
    # declaration that is false. Four rows shuffle into one of 24 orders, so
    # two draws agree one time in 24; the step is audited until it disagrees
    # or the odds pass 1e-11, which is not flakiness but the only way to
    # observe a wobble that is sometimes invisible.
    pipe = lg.Pipeline(permute_ignoring_the_seed(ranking(), seed=7))

    for attempt in range(16):
        report = pipe.audit(
            "permute_ignoring_the_seed", cache=tmp_path / f"attempt-{attempt}"
        )
        if report.verdict == "not reproducible":
            break
    else:
        pytest.fail("sixteen audits of a system-random shuffle all agreed")

    assert not report.reproducible
    assert "does not control" in report.text, report.text


# ---------------------------------------------------------------- methods --


def test_the_methods_paragraph_names_the_seed(tmp_path: Path) -> None:
    pipe = lg.Pipeline(permute(ranking(), seed=7))

    methods = pipe.methods(cache=tmp_path / ".ligase")

    # Named *as a seed*, not merely printed as another parameter. A reader of
    # `with seed = 7` cannot tell the step is stochastic or that this is what
    # makes it repeat, which is what D40 asks the paragraph to say.
    assert "seeded with seed = 7" in flowed(methods.text), methods.text


def test_an_ordinary_steps_paragraph_says_nothing_about_seeding(
    tmp_path: Path,
) -> None:
    pipe = lg.Pipeline(permute_undeclared(ranking(), seed=7))

    methods = pipe.methods(cache=tmp_path / ".ligase")

    assert "seeded" not in flowed(methods.text), methods.text
    # The parameter is still reported: it is a parameter, declared or not.
    assert "seed = 7" in flowed(methods.text), methods.text


# ------------------------------------------------------------------ cache --


def test_changing_the_seed_reruns_the_step(tmp_path: Path) -> None:
    # `CLAUDE.md` requires seeds to be explicit, typed, hashed parameters.
    # If two seeds shared a key, the second run would be served the first
    # one's answer -- a different ranking, in silence, which is the whole
    # failure mode semantic hashing exists to be incapable of.
    cache = tmp_path / ".ligase"

    first = lg.Pipeline(permute(ranking(), seed=7)).run(cache=cache)
    again = lg.Pipeline(permute(ranking(), seed=7)).run(cache=cache)
    moved = lg.Pipeline(permute(ranking(), seed=8)).run(cache=cache)

    assert [step.outcome for step in first.steps] == ["ran"]
    assert [step.outcome for step in again.steps] == ["cached"]
    assert [step.outcome for step in moved.steps] == ["ran"]
    assert first.steps[0].key != moved.steps[0].key
