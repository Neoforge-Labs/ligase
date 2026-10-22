"""M7: a dataset may be about more than one organism, and name its release."""

from __future__ import annotations

import pytest

import ligase as lg
from tests.conftest import fixture


def test_an_organism_can_be_declared_as_a_list() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), organism=["HIV-1", "Homo sapiens"])

    organism = ref.dimension("organism")
    assert organism is not None
    assert organism.value == "HIV-1 and Homo sapiens"


# The failure that would be silent: a str is a Python sequence, so a
# `Vec<String>` parameter would extract it as its characters.
def test_a_string_organism_is_not_split_into_characters() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), organism="HIV-1")

    organism = ref.dimension("organism")
    assert organism is not None
    assert organism.value == "HIV-1"


def test_a_tuple_of_organisms_is_accepted() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), organism=("HIV-1", "Homo sapiens"))

    organism = ref.dimension("organism")
    assert organism is not None
    assert organism.value == "HIV-1 and Homo sapiens"


# An empty list, refused at the boundary where the message is read.
def test_an_empty_organism_list_is_refused() -> None:
    with pytest.raises(ValueError, match="organism"):
        lg.reference(fixture("tiny_bare.fasta"), organism=[])


def test_an_organism_that_is_neither_a_string_nor_a_sequence_is_refused() -> None:
    with pytest.raises(TypeError):
        lg.reference(fixture("tiny_bare.fasta"), organism=1)  # type: ignore[arg-type]


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
