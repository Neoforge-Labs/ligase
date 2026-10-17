"""M4: the Python surface of `pipe.audit`.

The Rust tests own the verdicts themselves — what counts as reproducible,
and that an audit records nothing. These own what Python sees: that a report
arrives with a verdict per node and a sample on each, that a step or a
sample nothing is named after is a `ValueError` rather than a verdict, and
that the `reproducible` shortcuts read the way a user would expect.
"""

from __future__ import annotations

from pathlib import Path

import pytest

import ligase as lg
from ligase.types import Alignment, Reference
from tests.conftest import fixture


@lg.step
def steady(ref: Reference) -> Alignment:
    """A step that writes the same bytes every time."""
    return lg.sh("cat {ref} > {out}")


@lg.step
def wobbling(ref: Reference) -> Reference:
    """A step whose bytes move while its result does not.

    The output is a FASTA whose description records where it was written,
    which is what `minimap2` does in its `@PG CL` line — so the bytes carry
    the staging path and the sequence carries the biology.
    """
    return lg.sh("printf '>1 written to %s\\nACGT\\n' {out} > {out}")


@lg.step
def unsteady(ref: Reference) -> Reference:
    """A step whose result moves: a sequence nobody can predict."""
    return lg.sh("printf '>1\\nACG%s\\n' \"$$\" > {out}")


@lg.step
def below(aln: Alignment) -> Reference:
    """A step wanting an alignment, for the pipeline that must not run."""
    return lg.sh("cat {aln} > {out}")


def reference(name: str = "tiny_bare.fasta") -> Reference:
    """A loaded reference, with the dimensions a FASTA cannot state."""
    return lg.reference(fixture(name), organism="HIV-1", build="tiny")


# ------------------------------------------------------------ verdicts --


def test_a_steady_step_is_deterministic(tmp_path: Path) -> None:
    pipe = lg.Pipeline(steady(reference()))

    report = pipe.audit("steady", cache=tmp_path)

    assert [step.verdict for step in report.steps] == ["deterministic"]
    assert report.verdict == "deterministic"
    assert report.reproducible
    assert report.unreproducible == []


def test_a_step_whose_bytes_move_is_still_reproducible(tmp_path: Path) -> None:
    # `wobbling` writes its own output path into the file, so its bytes
    # cannot repeat -- and its sequence always does.
    pipe = lg.Pipeline(wobbling(reference()))

    report = pipe.audit("wobbling", cache=tmp_path)

    [audited] = report.steps
    assert audited.verdict == "reproducible"
    assert audited.first != audited.second
    assert audited.first_canonical == audited.second_canonical
    assert audited.reproducible
    assert report.reproducible


def test_a_step_whose_result_moves_is_not_reproducible(tmp_path: Path) -> None:
    pipe = lg.Pipeline(unsteady(reference()))

    report = pipe.audit("unsteady", cache=tmp_path)

    [audited] = report.steps
    assert audited.verdict == "not reproducible"
    assert audited.first_canonical != audited.second_canonical
    assert not audited.reproducible
    assert not report.reproducible
    assert report.unreproducible == [audited]


# -------------------------------------------------------------- report --


def test_the_report_is_worded_by_the_engine(tmp_path: Path) -> None:
    pipe = lg.Pipeline(wobbling(reference()))

    report = pipe.audit("wobbling", cache=tmp_path)

    assert "reproducible" in report.text
    assert "wobbling" in report.text
    assert report.summary == "1 step: 1 reproducible."
    assert repr(report) == "<AuditReport 1 step: 1 reproducible.>"


def test_every_node_of_a_step_is_audited(tmp_path: Path) -> None:
    samples = [reference("tiny_bare.fasta"), reference("tiny_other.fasta")]
    pipe = lg.Pipeline([steady(sample) for sample in samples])

    report = pipe.audit("steady", cache=tmp_path)

    assert len(report.steps) == 2
    assert report.summary == "2 steps: 2 deterministic."


def test_a_sample_narrows_the_audit(tmp_path: Path) -> None:
    # The references here carry no sample set, so the graph tells the two
    # nodes apart by the file each was built from -- which is the value
    # `--sample` matches, and the value a report would print.
    samples = [reference("tiny_bare.fasta"), reference("tiny_other.fasta")]
    pipe = lg.Pipeline([steady(sample) for sample in samples])
    whole = pipe.audit("steady", cache=tmp_path)
    scopes = [step.scope for step in whole.steps if "tiny_bare" in str(step.scope)]
    [scope] = [found for found in scopes if found is not None]

    narrowed = pipe.audit("steady", sample=scope.split(" ", 1)[1], cache=tmp_path)

    assert len(narrowed.steps) == 1
    assert narrowed.steps[0].scope == scope


# -------------------------------------------------------- what is wrong --


def test_a_step_that_is_not_there_is_refused(tmp_path: Path) -> None:
    pipe = lg.Pipeline(steady(reference()))

    with pytest.raises(ValueError, match="no step named `stedy`") as raised:
        pipe.audit("stedy", cache=tmp_path)

    # The names that do exist, since a misspelling is the likely cause.
    assert "`steady`" in str(raised.value)


def test_a_sample_that_is_not_there_is_refused(tmp_path: Path) -> None:
    pipe = lg.Pipeline(steady(reference()))

    with pytest.raises(ValueError, match="SRR0000000"):
        pipe.audit("steady", sample="SRR0000000", cache=tmp_path)


def test_nothing_is_audited_until_the_pipeline_checks(tmp_path: Path) -> None:
    # The same rule as `run`: an unchecked pipeline is how an overnight run
    # ends in a dimension error, and an audit runs the pipeline. `below`
    # wants an Alignment and is handed a Reference.
    pipe = lg.Pipeline(below(reference()))  # type: ignore[arg-type]

    with pytest.raises(lg.StepTypeError):
        pipe.audit("below", cache=tmp_path)

    assert not (tmp_path / "store").exists()
