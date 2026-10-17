"""M2 acceptance: the golden example builds a graph and checks in one pass.

`docs/milestones.md` is specific about what M2 means: the golden example
builds a graph without executing anything, and `ligase check` reports all
type errors in one pass. The first half is asserted by a sentinel rather than
by inspection -- if a body ran, the test fails, whatever the graph looks like.
"""

from __future__ import annotations

import pytest

import ligase as lg
from ligase.types import (
    Alignment,
    Intervals,
    Reads,
    Reference,
    ResistanceReport,
    Variants,
)
from tests.conftest import FIXTURES, fixture

RAN: list[str] = []


@lg.step(tool="minimap2")
def align(reads: Reads, ref: Reference) -> Alignment:
    """Align reads against a reference."""
    RAN.append("align")
    return reads  # type: ignore[return-value]


@lg.step(tool="bcftools")
def call(aln: Alignment, ref: Reference, min_freq: float = 0.01) -> Variants:
    """Call variants from an alignment."""
    RAN.append("call")
    return aln  # type: ignore[return-value]


@lg.step
def resistance(v: Variants) -> ResistanceReport:
    """Interpret variants as drug resistance."""
    RAN.append("resistance")
    return v  # type: ignore[return-value]


@lg.step
def restrict(v: Variants, regions: Intervals) -> Variants:
    """Keep only the calls inside some regions."""
    RAN.append("restrict")
    return v


@pytest.fixture(autouse=True)
def _nothing_ran() -> None:
    RAN.clear()


def hxb2() -> Reference:
    return lg.reference(fixture("tiny_bare.fasta"), organism="HIV-1", build="tiny")


def one_sample() -> Reads:
    return lg.reads(fixture("reads_phred33_1.fastq"))


# ------------------------------------------------------- building the shape --


def test_the_golden_example_builds_a_graph_without_executing_anything() -> None:
    ref = hxb2()
    samples = lg.samples(str(FIXTURES / "reads_phred33_*.fastq"), paired=True)

    pipe = lg.Pipeline(samples.map(lambda s: resistance(call(align(s, ref), ref))))

    assert RAN == []
    assert pipe.steps == ("align", "call", "resistance")
    pipe.check()


def test_a_shared_input_becomes_one_node_not_several() -> None:
    ref = hxb2()
    reads = one_sample()

    # `ref` is passed to two steps. It is one file, so it is one node.
    pipe = lg.Pipeline(call(align(reads, ref), ref))

    assert len(pipe) == 4
    assert pipe.steps == ("align", "call")


def test_fan_out_builds_one_chain_per_sample() -> None:
    ref = hxb2()
    samples = lg.samples(str(FIXTURES / "reads_phred33_*.fastq"), paired=False)

    assert len(samples) == 2
    pipe = lg.Pipeline(samples.map(lambda s: call(align(s, ref), ref)))

    # Two samples, two chains: 2 reads + 1 reference + 2 aligns + 2 calls.
    assert len(pipe) == 7
    assert pipe.steps == ("align", "call", "align", "call")
    assert RAN == []


def test_a_handle_carries_the_dimensions_the_step_would_produce() -> None:
    ref = hxb2()
    aligned = align(one_sample(), ref)
    lg.Pipeline(aligned)

    produced = lg.handle(aligned).dataset
    assert produced is not None
    assert produced.kind == "Alignment"

    # The reference came from the FASTA, through a step that has not run.
    reference = produced.dimension("reference")
    assert reference is not None
    assert reference.value == "tiny"
    assert reference.source == "from step `align`"

    # And the coordinate system is the kind's own, not anything upstream's.
    coordinates = produced.dimension("coordinates")
    assert coordinates is not None
    assert coordinates.value == "1-based closed"


def test_a_non_dataset_argument_is_refused_when_the_graph_is_built() -> None:
    with pytest.raises(TypeError, match="ref"):
        lg.Pipeline(align(one_sample(), "refs/HXB2.fasta"))  # type: ignore[arg-type]


def test_a_pipeline_needs_at_least_one_output() -> None:
    with pytest.raises(ValueError, match="output"):
        lg.Pipeline([])


# ------------------------------------------------------------- the checking --


def test_the_wrong_reference_fails_the_whole_pipeline() -> None:
    right = hxb2()
    wrong = lg.reference(fixture("tiny_other.fasta"), organism="HIV-1", build="other")

    pipe = lg.Pipeline(call(align(one_sample(), right), wrong))

    with pytest.raises(lg.DimensionError) as caught:
        pipe.check()

    error = caught.value
    assert error.step == "call"
    assert error.dimension == "reference"
    assert "Did you pass the wrong reference?" in str(error)


def test_every_error_is_reported_in_one_pass() -> None:
    right = hxb2()
    wrong = lg.reference(fixture("tiny_other.fasta"), organism="HIV-1", build="other")
    regions = lg.intervals(fixture("tiny_bare.bed"))

    pipe = lg.Pipeline(restrict(call(align(one_sample(), right), wrong), regions))

    errors = pipe.errors()
    assert [e.step for e in errors] == ["call", "restrict"]
    assert [e.dimension for e in errors] == ["reference", "coordinates"]
    # Every error carries the whole list, so catching one shows them all.
    assert errors[0].errors == errors


def test_a_coordinate_mismatch_is_what_intervals_are_for() -> None:
    ref = hxb2()
    regions = lg.intervals(fixture("tiny_bare.bed"))

    pipe = lg.Pipeline(restrict(call(align(one_sample(), ref), ref), regions))

    with pytest.raises(lg.DimensionError) as caught:
        pipe.check()

    message = str(caught.value)
    assert "0-based half-open" in message
    assert "1-based closed" in message
    assert "convert between coordinate systems" in message


def test_passing_the_wrong_kind_of_data_is_a_step_type_error() -> None:
    calls = lg.variants(fixture("tiny_bare.vcf"))

    # `align` wants a Reference and is handed a VCF.
    pipe = lg.Pipeline(align(one_sample(), calls))  # type: ignore[arg-type]

    with pytest.raises(lg.StepTypeError) as caught:
        pipe.check()

    error = caught.value
    assert error.step == "align"
    assert error.dimension is None
    # A step-type error is a check error but not a dimension error: the fix
    # is a different argument, not a declaration.
    assert isinstance(error, lg.CheckError)
    assert not isinstance(error, lg.DimensionError)
    assert "Reference" in str(error)
    assert "Variants" in str(error)


def test_an_unknown_dimension_nothing_requires_is_left_alone() -> None:
    # The alignment has no @RG lines, so its sample set is unknown. Nothing
    # here joins two sample sets, so there is nothing to declare.
    ref = hxb2()
    aln = lg.alignment(fixture("tiny_no_rg.sam"))

    pipe = lg.Pipeline(resistance(call(aln, ref)))

    assert pipe.errors() == []


def test_an_unknown_dimension_a_later_step_needs_names_the_file_to_fix() -> None:
    aln = lg.alignment(fixture("tiny_no_rg.sam"))
    other = lg.variants(fixture("tiny_bare.vcf"))

    @lg.step
    def merge(a: Variants, b: Variants) -> Variants:
        return a

    pipe = lg.Pipeline(merge(call(aln, hxb2()), other))

    with pytest.raises(lg.UndeclaredDimensionError) as caught:
        pipe.check()

    message = str(caught.value)
    assert caught.value.dimension == "sample_set"
    # The step that needs it, the file that has to declare it, and the route
    # between them -- the user is looking at `merge` and must edit the BAM.
    assert "merge" in message
    assert "tiny_no_rg.sam" in message
    assert "through step `call`" in message


def test_a_report_summarises_what_it_found() -> None:
    ref = hxb2()
    wrong = lg.reference(fixture("tiny_other.fasta"), organism="HIV-1", build="other")

    clean = lg.Pipeline(call(align(one_sample(), ref), ref))
    assert clean.report().summary == "no problems found."

    broken = lg.Pipeline(call(align(one_sample(), ref), wrong))
    assert broken.report().summary == "1 error in 1 step."
