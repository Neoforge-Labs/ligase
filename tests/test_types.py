"""M1: the Python surface loads data and exposes its inferred dimensions."""

from __future__ import annotations

import pytest

import ligase as lg
from ligase.types import Alignment, Reads, Reference, Variants
from tests.conftest import fixture


def test_reference_loads_with_inferred_contigs() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), organism="HIV-1", build="tiny")

    assert isinstance(ref, Reference)
    assert ref.kind == "Reference"
    contigs = ref.contigs
    assert contigs is not None
    assert [c.name for c in contigs] == ["1", "2"]
    assert contigs[0].length == 20
    assert contigs[0].md5 == "a965a71aa3690f605935c54d320905ab"


def test_dimensions_carry_their_provenance() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), organism="HIV-1", build="tiny")

    reference = ref.dimension("reference")
    assert reference is not None
    assert reference.value == "tiny"
    # The provenance is the point: it says what to go and look at.
    assert "FASTA contigs" in reference.source
    assert "tiny_bare.fasta" in reference.source

    organism = ref.dimension("organism")
    assert organism is not None
    assert organism.value == "HIV-1"
    assert organism.source == "declared"


def test_an_undeclared_dimension_reports_why_it_is_unknown() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"))

    assert ref.dimension("organism") is None
    reason = ref.unknown_reason("organism")
    assert reason is not None
    assert "declare it" in reason


def test_a_dimension_that_does_not_apply_differs_from_one_that_is_unknown() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"))

    # A reference has no quality encoding at all: a category error, not a
    # gap to be filled in, so there is nothing to declare and no reason to
    # give.
    assert "quality_encoding" not in ref.applicable
    assert ref.unknown_reason("quality_encoding") is None

    # Organism does apply, and here it simply was not declared -- so it has
    # a reason, and the reason says what to do.
    assert "organism" in ref.applicable
    assert "organism" not in ref.dimensions
    assert ref.unknown_reason("organism") is not None

    # And a dimension that was inferred is present with a value.
    assert "reference" in ref.applicable
    assert "reference" in ref.dimensions


def test_alignment_loads_with_sample_set_from_read_groups() -> None:
    aln = lg.alignment(fixture("tiny_two_rg.sam"))

    assert isinstance(aln, Alignment)
    assert aln.samples == ["sample_a", "sample_b"]
    assert aln.dimension("coordinates") is not None
    assert aln.dimensions["coordinates"].value == "1-based closed"


def test_variants_load_with_sample_columns() -> None:
    calls = lg.variants(fixture("tiny_two_samples.vcf"))

    assert isinstance(calls, Variants)
    assert calls.samples == ["sample_a", "sample_b"]


def test_reads_load_with_pairing_and_quality_encoding() -> None:
    reads = lg.reads(
        fixture("reads_phred33_1.fastq"),
        fixture("reads_phred33_2.fastq"),
    )

    assert isinstance(reads, Reads)
    assert reads.dimensions["pairing"].value == "paired-end"
    assert reads.dimensions["quality_encoding"].value == "Phred+33"
    assert reads.samples == ["reads_phred33"]


def test_a_missing_file_raises_load_error() -> None:
    with pytest.raises(lg.LoadError, match=r"nope\.fasta"):
        lg.reference("nope.fasta")


def test_a_file_that_is_not_a_fasta_raises_load_error() -> None:
    with pytest.raises(lg.LoadError):
        lg.reference(fixture("tiny_bare.vcf"))


def test_repr_is_useful_in_a_notebook() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), build="tiny")
    assert "Reference" in repr(ref)
    assert "tiny_bare.fasta" in repr(ref)


def test_real_hxb2_loads(hxb2: str) -> None:
    ref = lg.reference(hxb2, organism="HIV-1", build="HXB2")

    contigs = ref.contigs
    assert contigs is not None
    assert len(contigs) == 1
    assert contigs[0].name == "K03455.1"
    assert contigs[0].length == 9719
    assert contigs[0].md5 == "c66838498aab02f786a6ec2f2d62cdc1"


def test_real_demo_alignment_agrees_with_real_hxb2(hxb2: str, demo_bam: str) -> None:
    """The reference demo from CLAUDE.md, end to end on real data."""
    ref = lg.reference(hxb2, organism="HIV-1", build="HXB2")
    aln = lg.alignment(demo_bam)

    # minimap2 wrote the @SQ header; we computed the checksum from the FASTA.
    assert aln.contigs is not None
    assert aln.contigs[0].md5 == "c66838498aab02f786a6ec2f2d62cdc1"

    lg.check_step("call", {"aln": aln, "ref": ref})
