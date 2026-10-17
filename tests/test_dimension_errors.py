"""M1 acceptance, from the Python side.

The milestone requires that a BAM aligned to a different reference fails
`check()` with a clear `DimensionError`, and that a `chr1` against `1`
mismatch raises a *distinct* error. "Distinct" here means catchable on its
own: `ContigNamingError` is a `DimensionError`, so code that wants either
catches the base class, and code that wants to add a renaming step catches
the specific one.
"""

from __future__ import annotations

import pytest

import ligase as lg
from tests.conftest import fixture


def test_a_different_reference_raises_dimension_error() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"))
    aln = lg.alignment(fixture("tiny_other.sam"))

    with pytest.raises(lg.DimensionError) as caught:
        lg.check_step("call", {"aln": aln, "ref": ref})

    error = caught.value
    assert error.step == "call"
    assert error.dimension == "reference"
    # Not a naming problem: a naming fix would not help here.
    assert not isinstance(error, lg.ContigNamingError)

    message = str(error)
    assert message.startswith("DimensionError in step `call`:")
    assert "aln.reference" in message
    assert "ref.reference" in message
    assert "SAM @SQ header" in message
    assert "FASTA contigs" in message
    assert "Did you pass the wrong reference?" in message


def test_a_chr_prefix_mismatch_raises_contig_naming_error() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"))
    aln = lg.alignment(fixture("tiny_prefixed.sam"))

    with pytest.raises(lg.ContigNamingError) as caught:
        lg.check_step("call", {"aln": aln, "ref": ref})

    error = caught.value
    assert error.dimension == "contig_naming"
    # Distinct, but still a DimensionError, so a caller can catch either.
    assert isinstance(error, lg.DimensionError)
    assert "renaming step" in str(error)


def test_a_reference_mismatch_is_not_caught_as_a_naming_error() -> None:
    # Catching ContigNamingError must not swallow a genuinely different
    # reference, because the two need different fixes.
    ref = lg.reference(fixture("tiny_bare.fasta"))
    aln = lg.alignment(fixture("tiny_other.sam"))

    errors = lg.dimension_errors("call", {"aln": aln, "ref": ref})
    assert errors
    assert not any(isinstance(error, lg.ContigNamingError) for error in errors)


def test_contig_order_is_its_own_error() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"))
    aln = lg.alignment(fixture("tiny_reordered.sam"))

    with pytest.raises(lg.ContigOrderError) as caught:
        lg.check_step("call", {"aln": aln, "ref": ref})

    assert "GATK" in str(caught.value)


def test_a_missing_required_dimension_asks_for_a_declaration() -> None:
    # Joining an alignment to variant calls means joining on samples, and
    # this BAM has no @RG lines to take them from.
    aln = lg.alignment(fixture("tiny_no_rg.sam"))
    calls = lg.variants(fixture("tiny_bare.vcf"))

    with pytest.raises(lg.UndeclaredDimensionError) as caught:
        lg.check_step("resistance", {"aln": aln, "v": calls})

    error = caught.value
    assert error.dimension == "sample_set"
    assert isinstance(error, lg.DimensionError)

    message = str(error)
    assert "aln.sample_set is unknown" in message
    assert "@RG" in message
    assert "Declare it where you load the data." in message


def test_declaring_the_missing_dimension_resolves_it() -> None:
    # And the advice works: declaring the sample makes the step check out.
    aln = lg.alignment(fixture("tiny_no_rg.sam"), samples=["sample_a"])
    calls = lg.variants(fixture("tiny_bare.vcf"))

    assert lg.dimension_errors("resistance", {"aln": aln, "v": calls}) == []


def test_sample_sets_must_match_in_order() -> None:
    # The shuffled-sample-ID bug: same identities, wrong order, plausible
    # numbers, wrong answers.
    one = lg.variants(fixture("tiny_two_samples.vcf"))
    two = lg.variants(fixture("tiny_swapped_samples.vcf"))

    errors = lg.dimension_errors("merge", {"one": one, "two": two})
    assert [error.dimension for error in errors] == ["sample_set"]
    assert "in order" in str(errors[0])
    assert "The same samples appear in both" in str(errors[0])


def test_every_error_is_reported_in_one_pass() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"))
    aln = lg.alignment(fixture("tiny_other.sam"))

    with pytest.raises(lg.DimensionError) as caught:
        lg.check_step("call", {"aln": aln, "ref": ref})

    # The raised error is the first; all of them are reachable.
    assert caught.value.errors
    assert caught.value in caught.value.errors


def test_dimension_errors_does_not_raise() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"))
    aln = lg.alignment(fixture("tiny_other.sam"))

    errors = lg.dimension_errors("call", {"aln": aln, "ref": ref})
    assert len(errors) == 1
    assert isinstance(errors[0], lg.DimensionError)


def test_matching_dimensions_pass_silently() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"))
    aln = lg.alignment(fixture("tiny_bare.sam"))

    assert lg.dimension_errors("call", {"aln": aln, "ref": ref}) == []
    lg.check_step("call", {"aln": aln, "ref": ref})


def test_a_vcf_naming_a_subset_of_contigs_is_accepted() -> None:
    # Correct input that strict equality would have rejected.
    ref = lg.reference(fixture("tiny_superset.fasta"))
    calls = lg.variants(fixture("tiny_subset.vcf"))

    assert lg.dimension_errors("resistance", {"v": calls, "ref": ref}) == []


def test_the_exception_hierarchy_is_catchable_as_a_whole() -> None:
    assert issubclass(lg.ContigNamingError, lg.DimensionError)
    assert issubclass(lg.ContigOrderError, lg.DimensionError)
    assert issubclass(lg.UndeclaredDimensionError, lg.DimensionError)
    assert issubclass(lg.DimensionError, Exception)
