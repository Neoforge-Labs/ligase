"""M7: a dataset may be about more than one organism, and name its release."""

from __future__ import annotations

import re
from collections.abc import Callable
from typing import Any

import pytest

import ligase as lg
from tests.conftest import FIXTURES, fixture

EMPTY_LIST = (
    "organism was given an empty list; name at least one organism, or leave it out"
)
ONLY_BLANKS = (
    "organism was given only blank names; name at least one organism, or leave it out"
)
BLANK_RELEASE = "release was given a blank value; name the release, or leave it out"

# Every loader that takes an organism, each loaded from a fixture of its own
# kind. `samples` returns a collection, so its first sample is what is read.
LOADERS: list[tuple[str, Callable[[Any], Any]]] = [
    ("reference", lambda o: lg.reference(fixture("tiny_bare.fasta"), organism=o)),
    ("alignment", lambda o: lg.alignment(fixture("tiny_bare.sam"), organism=o)),
    ("variants", lambda o: lg.variants(fixture("tiny_bare.vcf"), organism=o)),
    ("reads", lambda o: lg.reads(fixture("reads_phred33_1.fastq"), organism=o)),
    ("intervals", lambda o: lg.intervals(fixture("tiny_bare.bed"), organism=o)),
    (
        "samples",
        lambda o: lg.samples(
            str(FIXTURES / "reads_phred33_*.fastq"), paired=True, organism=o
        )[0],
    ),
]
EACH_LOADER = pytest.mark.parametrize(
    "load", [load for _, load in LOADERS], ids=[name for name, _ in LOADERS]
)


def test_an_organism_can_be_declared_as_a_list() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), organism=["HIV-1", "Homo sapiens"])

    organism = ref.dimension("organism")
    assert organism is not None
    assert organism.value == "HIV-1 and Homo sapiens"


def test_a_tuple_of_organisms_is_accepted() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), organism=("HIV-1", "Homo sapiens"))

    organism = ref.dimension("organism")
    assert organism is not None
    assert organism.value == "HIV-1 and Homo sapiens"


# The failure that would be silent: a str is a Python sequence, so a
# `Vec<String>` parameter would extract it as its characters.
@EACH_LOADER
def test_every_loader_reads_a_string_organism_whole(load: Callable[[Any], Any]) -> None:
    organism = load("HIV-1").dimension("organism")

    assert organism is not None
    assert organism.value == "HIV-1"


@EACH_LOADER
def test_every_loader_reads_a_list_of_organisms(load: Callable[[Any], Any]) -> None:
    organism = load(["HIV-1", "Homo sapiens"]).dimension("organism")

    assert organism is not None
    assert organism.value == "HIV-1 and Homo sapiens"


# An empty list, refused at the boundary where the message is read.
@EACH_LOADER
def test_every_loader_refuses_an_empty_organism_list(
    load: Callable[[Any], Any],
) -> None:
    with pytest.raises(ValueError, match=re.escape(EMPTY_LIST)):
        load([])


# `organism=config.get("organism", "")` is how a blank arrives, and it declares
# nothing; it is refused as plainly as an empty list.
@EACH_LOADER
@pytest.mark.parametrize(
    "blank",
    ["", "   ", [""], ["", " "]],
    ids=["empty-string", "spaces", "list-of-empty", "list-of-blanks"],
)
def test_every_loader_refuses_a_blank_organism(
    load: Callable[[Any], Any], blank: str | list[str]
) -> None:
    with pytest.raises(ValueError, match=re.escape(ONLY_BLANKS)):
        load(blank)


@EACH_LOADER
def test_every_loader_refuses_an_organism_that_is_not_a_name(
    load: Callable[[Any], Any],
) -> None:
    with pytest.raises(TypeError):
        load(1)


@pytest.mark.parametrize("blank", ["", "   "], ids=["empty-string", "spaces"])
def test_a_blank_release_is_refused(blank: str) -> None:
    with pytest.raises(ValueError, match=re.escape(BLANK_RELEASE)):
        lg.reference(fixture("tiny_bare.fasta"), release=blank)


def test_a_reference_can_declare_a_release() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), release="GRCh38.p14")

    assert "release" in ref.applicable
    release = ref.dimension("release")
    assert release is not None
    assert release.value == "GRCh38.p14"
    assert release.source == "declared"


def test_release_does_not_apply_to_an_alignment() -> None:
    aln = lg.alignment(fixture("tiny_bare.sam"))

    assert "release" not in aln.applicable
    assert aln.unknown_reason("release") is None


def test_a_wider_organism_set_is_reported_against_a_narrower_one() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), organism=["HIV-1", "Homo sapiens"])
    aln = lg.alignment(fixture("tiny_bare.sam"), organism="HIV-1")

    errors = lg.dimension_errors("call", {"aln": aln, "ref": ref})

    assert len(errors) == 1
    assert isinstance(errors[0], lg.DimensionError)
    assert "ref also lists Homo sapiens." in str(errors[0])
