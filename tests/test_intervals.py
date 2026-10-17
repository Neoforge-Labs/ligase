"""M2: intervals and sample sets, from the Python side."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

import ligase as lg
from ligase.types import Intervals
from tests.conftest import FIXTURES, fixture

if TYPE_CHECKING:
    from pathlib import Path


def test_a_bed_is_zero_based_half_open() -> None:
    regions = lg.intervals(fixture("tiny_bare.bed"))

    assert isinstance(regions, Intervals)
    assert regions.kind == "Intervals"
    coordinates = regions.dimension("coordinates")
    assert coordinates is not None
    assert coordinates.value == "0-based half-open"
    assert "BED coordinate convention" in coordinates.source


def test_a_gff_is_one_based_closed() -> None:
    regions = lg.intervals(fixture("tiny_bare.gff"))

    coordinates = regions.dimension("coordinates")
    assert coordinates is not None
    assert coordinates.value == "1-based closed"


def test_interval_contigs_come_from_the_records() -> None:
    regions = lg.intervals(fixture("tiny_bare.bed"))

    contigs = regions.contigs
    assert contigs is not None
    assert [c.name for c in contigs] == ["1", "2"]
    # A BED records neither, and inventing them would be the lie the whole
    # system guards against.
    assert contigs[0].length is None
    assert contigs[0].md5 is None


def test_sample_set_does_not_apply_to_intervals() -> None:
    regions = lg.intervals(fixture("tiny_bare.bed"))

    assert "sample_set" not in regions.applicable
    assert regions.dimension("sample_set") is None
    # Not a gap to be filled in: an interval file describes regions.
    assert regions.unknown_reason("sample_set") is None


def test_the_format_must_be_given_when_the_name_does_not_say(
    tmp_path: Path,
) -> None:
    path = tmp_path / "regions.txt"
    path.write_text("1\t0\t10\n")

    with pytest.raises(lg.LoadError, match="bed"):
        lg.intervals(str(path))

    regions = lg.intervals(str(path), format="bed")
    coordinates = regions.dimension("coordinates")
    assert coordinates is not None
    assert coordinates.value == "0-based half-open"


def test_an_unknown_format_name_is_refused() -> None:
    with pytest.raises(ValueError, match="interval format"):
        lg.intervals(fixture("tiny_bare.bed"), format="wig")


# ------------------------------------------------------------------ samples --


def test_samples_pair_mates_by_filename() -> None:
    samples = lg.samples(str(FIXTURES / "reads_phred33_*.fastq"), paired=True)

    assert len(samples) == 1
    assert samples.ids == ("reads_phred33",)
    pairing = samples[0].dimension("pairing")
    assert pairing is not None
    assert pairing.value == "paired-end"


def test_single_end_samples_are_one_per_file() -> None:
    samples = lg.samples(str(FIXTURES / "reads_phred33_*.fastq"))

    assert len(samples) == 2
    assert [s.kind for s in samples] == ["Reads", "Reads"]


def test_a_lone_mate_is_an_error_rather_than_a_single_end_sample() -> None:
    with pytest.raises(lg.LoadError, match="paired=False"):
        lg.samples(str(FIXTURES / "reads_phred33_1.fastq"), paired=True)


def test_a_glob_that_matches_nothing_finds_no_samples() -> None:
    samples = lg.samples(str(FIXTURES / "nothing_matches_*.fastq"))

    assert len(samples) == 0
    assert samples.ids == ()


def test_samples_map_without_running_anything() -> None:
    samples = lg.samples(str(FIXTURES / "reads_phred33_*.fastq"))

    assert samples.map(lambda s: s.kind) == ["Reads", "Reads"]
    assert "2 samples" in repr(samples)
