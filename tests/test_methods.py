"""M5: the Python surface of `pipe.methods`.

The Rust tests own what a methods section says and what a bundle claims.
These own what Python sees: that a run's production is described with a
sample on each entry, that `cite=` reaches the reference list, that a
bundle comes back as an object rather than a path, and that verifying one
against a pipeline that has changed is a finding rather than an exception.
"""

from __future__ import annotations

from pathlib import Path

import pytest

import ligase as lg
from ligase.types import Alignment, Reference
from tests.conftest import fixture


@lg.step
def copy(ref: Reference) -> Alignment:
    """A step that copies its input."""
    return lg.sh("cat {ref} > {out}")


@lg.step(tool="python3", cite="Van Rossum G (1995) Python reference manual.")
def cited(ref: Reference) -> Alignment:
    """A step that says what to cite it by."""
    return lg.sh("cat {ref} > {out}")


@lg.step(tool="python3")
def uncited(ref: Reference) -> Alignment:
    """A step using a tool the engine knows no reference for."""
    return lg.sh("cat {ref} > {out}")


@lg.step
def filtered(ref: Reference, min_freq: float = 0.01) -> Alignment:
    """A step with a parameter worth reporting."""
    return lg.sh("cat {ref} > {out} && echo {min_freq} > /dev/null")


@lg.step
def failing(ref: Reference) -> Alignment:
    """A step that does not produce its output."""
    return lg.sh("false")


def reference(name: str = "tiny_bare.fasta") -> Reference:
    """A loaded reference, with the dimensions a FASTA cannot state."""
    return lg.reference(fixture(name), organism="HIV-1", build="tiny")


# ------------------------------------------------------ what it reports --


def test_a_methods_section_describes_every_step(tmp_path: Path) -> None:
    pipe = lg.Pipeline(copy(reference()))

    methods = pipe.methods(cache=tmp_path)

    assert [step.step for step in methods.steps] == ["copy"]
    assert methods.steps[0].nodes == 1
    assert methods.steps[0].command == "cat {ref} > {out}"
    assert methods.accounted_for
    assert methods.gaps == []
    assert methods.engine == lg.version()
    assert methods.platform
    assert "copy" in methods.text


def test_a_parameter_is_reported_as_it_was_hashed(tmp_path: Path) -> None:
    pipe = lg.Pipeline(filtered(reference(), min_freq=0.05))

    methods = pipe.methods(cache=tmp_path)

    assert methods.steps[0].parameters == [("min_freq", "0.05")]
    assert "min_freq = 0.05" in methods.text


def test_a_reference_is_pinned_by_what_its_file_says(tmp_path: Path) -> None:
    pipe = lg.Pipeline(copy(reference()))

    methods = pipe.methods(cache=tmp_path)

    assert len(methods.references) == 1
    pinned = methods.references[0]
    assert pinned.param == "ref"
    assert pinned.build == "tiny"
    assert pinned.length > 0
    # This fixture holds two sequences, so there is no one accession to name
    # and no one checksum to give. Both are reported as absent rather than
    # as one of the two, and the fingerprint still tells it from any other
    # reference.
    assert pinned.contigs == 2
    assert pinned.accession is None
    assert pinned.checksum is None
    assert pinned.fingerprint


def test_a_single_sequence_reference_is_pinned_by_accession(
    tmp_path: Path, hxb2: str
) -> None:
    pipe = lg.Pipeline(copy(lg.reference(hxb2, organism="HIV-1", build="HXB2")))

    methods = pipe.methods(cache=tmp_path)

    # Read from the defline, not declared: HXB2's sequence name *is* its
    # accession with its version, and the checksum is the one the loader
    # computed from the sequence.
    pinned = methods.references[0]
    assert pinned.accession == "K03455.1"
    assert pinned.contigs == 1
    assert pinned.length == 9719
    assert pinned.checksum is not None
    assert "K03455.1" in methods.text


def test_a_tool_is_reported_at_the_version_that_ran(tmp_path: Path) -> None:
    pipe = lg.Pipeline(uncited(reference()))

    methods = pipe.methods(cache=tmp_path)

    assert [tool for tool, _ in methods.tools] == ["python3"]
    assert "Python" in dict(methods.tools)["python3"]


def test_a_step_can_say_what_to_cite_it_by(tmp_path: Path) -> None:
    pipe = lg.Pipeline(cited(reference()))

    methods = pipe.methods(cache=tmp_path)

    assert methods.citations == ["Van Rossum G (1995) Python reference manual."]
    assert methods.uncited == []


def test_a_tool_with_nothing_to_cite_is_named(tmp_path: Path) -> None:
    pipe = lg.Pipeline(uncited(reference()))

    methods = pipe.methods(cache=tmp_path)

    # Named rather than omitted: a reference list that quietly dropped a
    # tool would let an incomplete methods section look complete.
    assert methods.uncited == ["python3"]
    assert "No reference is recorded for python3" in methods.text


def test_a_blank_citation_is_refused() -> None:
    with pytest.raises(ValueError, match="citation cannot be blank"):

        @lg.step(cite="   ")
        def blank(ref: Reference) -> Alignment:
            return lg.sh("cat {ref} > {out}")


def test_a_citation_does_not_rerun_anything(tmp_path: Path) -> None:
    # A citation cannot change a result, so it must not be a key ingredient:
    # adding one to a step is not a reason to recompute a cohort.
    first = lg.Pipeline(copy(reference())).run(cache=tmp_path)
    assert [step.outcome for step in first.steps] == ["ran"]

    copy.citations = ("Someone A (2026) A paper.",)
    try:
        again = lg.Pipeline(copy(reference())).run(cache=tmp_path)
    finally:
        copy.citations = ()

    assert [step.outcome for step in again.steps] == ["cached"]


def test_a_failed_step_has_no_methods_section(tmp_path: Path) -> None:
    pipe = lg.Pipeline(failing(reference()))

    # A pipeline that did not finish has nothing to write up, and a
    # paragraph describing the rest would read as a complete one.
    with pytest.raises(RuntimeError, match="nothing to write up"):
        pipe.methods(cache=tmp_path)


# -------------------------------------------------------- the bundle --


def test_a_bundle_comes_back_as_an_object(tmp_path: Path) -> None:
    pipe = lg.Pipeline(copy(reference()))
    into = tmp_path / "bundle"

    methods = pipe.methods(bundle=into, cache=tmp_path / "cache")

    assert methods.bundle is not None
    assert Path(methods.bundle.root) == into
    assert methods.bundle.steps == 1
    assert methods.bundle.gaps == 0
    assert "results.tsv" in methods.bundle.files
    assert (into / "METHODS.md").is_file()
    assert (into / "MANIFEST").is_file()


def test_a_bundle_carries_the_files_it_was_given(tmp_path: Path) -> None:
    pipe = lg.Pipeline(copy(reference()))
    carried = tmp_path / "pipeline.py"
    carried.write_text("pipe = lg.Pipeline(...)\n")
    into = tmp_path / "bundle"

    pipe.methods(bundle=into, include=[carried], cache=tmp_path / "cache")

    assert (into / "pipeline" / "pipeline.py").is_file()


def test_a_bundle_verifies_against_the_pipeline_it_came_from(
    tmp_path: Path,
) -> None:
    pipe = lg.Pipeline(copy(reference()))
    into = tmp_path / "bundle"
    pipe.methods(bundle=into, cache=tmp_path / "cache")

    methods = pipe.methods(verify=into, cache=tmp_path / "cache")

    assert methods.verification is not None
    assert methods.verification.reproduced
    assert methods.verification.verdict == "identical"
    assert methods.verification.differing == []
    assert methods.verification.moved == []


def test_a_bundle_verifies_from_an_empty_cache(tmp_path: Path) -> None:
    pipe = lg.Pipeline(copy(reference()))
    into = tmp_path / "bundle"
    pipe.methods(bundle=into, cache=tmp_path / "first")

    # What a clean machine is, to the engine: nothing computed and nothing
    # remembered. The step runs again from nothing and must land in the
    # same place.
    methods = pipe.methods(verify=into, cache=tmp_path / "clean")

    assert methods.verification is not None
    assert methods.verification.reproduced, methods.verification.text


def test_a_result_that_moved_is_reported_rather_than_raised(
    tmp_path: Path,
) -> None:
    pipe = lg.Pipeline(copy(reference()))
    into = tmp_path / "bundle"
    pipe.methods(bundle=into, cache=tmp_path / "cache")

    # The same step name over a different reference: the result moves.
    changed = lg.Pipeline(copy(reference("tiny_other.fasta")))
    methods = changed.methods(verify=into, cache=tmp_path / "cache")

    assert methods.verification is not None
    assert not methods.verification.reproduced
    assert methods.verification.verdict == "differs"
    assert [check.step for check in methods.verification.differing] == ["copy"]


def test_a_directory_that_is_not_a_bundle_is_refused(tmp_path: Path) -> None:
    pipe = lg.Pipeline(copy(reference()))
    empty = tmp_path / "empty"
    empty.mkdir()

    with pytest.raises(ValueError, match=r"results\.tsv"):
        pipe.methods(verify=empty, cache=tmp_path / "cache")
