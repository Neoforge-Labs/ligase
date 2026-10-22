"""M8: a gene list on the Python surface, and the space its names live in."""

from __future__ import annotations

import pytest

import ligase as lg
from ligase.types import KINDS, EntitySet
from tests.conftest import fixture


def test_a_gene_list_infers_the_space_its_identifiers_are_in() -> None:
    genes = lg.entity_set(fixture("entities_hgnc.tsv"), organism="Homo sapiens")

    found = genes.dimension("identifier_namespace")
    assert found is not None
    assert found.value == "HGNC symbol"
    # And it says where it got that from, so the inference is checkable.
    assert "pattern" in found.source


@pytest.mark.parametrize(
    ("file", "expected"),
    [
        ("entities_hgnc.tsv", "HGNC symbol"),
        ("entities_ensembl.tsv", "Ensembl gene"),
        ("entities_entrez.tsv", "Entrez gene"),
        ("entities_uniprot.tsv", "UniProt accession"),
    ],
)
def test_every_namespace_the_catalogue_knows_is_inferred(
    file: str, expected: str
) -> None:
    genes = lg.entity_set(fixture(file), organism="Homo sapiens")
    found = genes.dimension("identifier_namespace")
    assert found is not None
    assert found.value == expected


def test_a_file_that_mixes_two_namespaces_declines_to_infer() -> None:
    genes = lg.entity_set(fixture("entities_mixed.tsv"), organism="Homo sapiens")

    assert genes.dimension("identifier_namespace") is None
    reason = genes.unknown_reason("identifier_namespace")
    assert reason is not None
    assert "identifier_namespace" in reason


def test_a_gene_list_has_no_coordinate_system_to_ask_about() -> None:
    # Asking a gene list for its coordinates is a category error, not a gap.
    genes = lg.entity_set(fixture("entities_hgnc.tsv"), organism="Homo sapiens")

    assert set(genes.applicable) == {
        "organism",
        "release",
        "subject",
        "identifier_namespace",
    }
    for absent in ("coordinates", "sample_set", "reference", "contig_naming"):
        assert absent not in genes.applicable
        assert genes.dimension(absent) is None
        assert genes.unknown_reason(absent) is None


def test_the_loader_says_which_symbols_a_spreadsheet_ate() -> None:
    genes = lg.entity_set(fixture("entities_excel_dates.tsv"), organism="Homo sapiens")

    assert len(genes.notes) == 2
    assert any("2-Sep" in note for note in genes.notes)
    assert any("1-Mar" in note for note in genes.notes)
    assert all("date" in note for note in genes.notes)


def test_an_ordinary_dataset_notices_nothing() -> None:
    assert lg.reference(fixture("tiny_bare.fasta"), organism="test").notes == ()


def test_a_declared_namespace_is_recorded_as_declared() -> None:
    genes = lg.entity_set(
        fixture("entities_hgnc.tsv"),
        organism="Homo sapiens",
        identifier_namespace="my curated list",
    )
    found = genes.dimension("identifier_namespace")
    assert found is not None
    assert found.value == "my curated list"
    assert found.source == "declared"


def test_a_subject_and_a_release_are_carried() -> None:
    genes = lg.entity_set(
        fixture("entities_hgnc.tsv"),
        organism="Homo sapiens",
        subject="breast carcinoma",
        release="HGNC 2026-09",
    )
    subject = genes.dimension("subject")
    release = genes.dimension("release")
    assert subject is not None
    assert subject.value == "breast carcinoma"
    assert release is not None
    assert release.value == "HGNC 2026-09"


@pytest.mark.parametrize("blank", ["", "   "])
@pytest.mark.parametrize("field", ["identifier_namespace", "subject", "release"])
def test_a_blank_declaration_is_refused_where_it_is_written(
    blank: str, field: str
) -> None:
    # A blank that compares equal to another blank is a false agreement, and
    # the message belongs where it was written rather than three steps later.
    with pytest.raises(ValueError, match=field):
        lg.entity_set(
            fixture("entities_hgnc.tsv"),
            organism="Homo sapiens",
            **{field: blank},
        )


def test_a_named_identifier_column_is_read() -> None:
    table = lg.entity_set(
        fixture("map_hgnc_to_ensembl.tsv"),
        organism="Homo sapiens",
        id_column="id",
    )
    found = table.dimension("identifier_namespace")
    assert found is not None
    assert found.value == "HGNC symbol"


def test_a_file_with_no_such_column_says_which_columns_it_has() -> None:
    with pytest.raises(lg.LoadError) as caught:
        lg.entity_set(
            fixture("entities_hgnc.tsv"),
            organism="Homo sapiens",
            id_column="symbol",
        )
    assert "symbol" in str(caught.value)
    assert "log2fc" in str(caught.value)


def test_a_gene_list_is_an_entity_set() -> None:
    genes = lg.entity_set(fixture("entities_hgnc.tsv"), organism="Homo sapiens")

    assert isinstance(genes, EntitySet)
    assert genes.kind == "EntitySet"
    assert KINDS["EntitySet"] is EntitySet
