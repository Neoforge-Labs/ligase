"""M8: crossing an identifier space, and refusing to do it by accident."""

from __future__ import annotations

from pathlib import Path

import pytest

import ligase as lg
from tests.conftest import fixture


def hgnc(**declared: str) -> lg.EntitySet:
    """A list of HGNC symbols, which has to say so.

    Bare symbols carry no marker, so the file states no space of its own
    (D56) and a list meant to be joined or mapped declares one.
    """
    declared.setdefault("identifier_namespace", "HGNC symbol")
    return lg.entity_set(
        fixture("entities_hgnc.tsv"), organism="Homo sapiens", **declared
    )


def table() -> lg.EntitySet:
    return lg.entity_set(
        fixture("map_hgnc_to_ensembl.tsv"),
        organism="Homo sapiens",
        identifier_namespace="HGNC symbol",
    )


def ensembl() -> lg.EntitySet:
    return lg.entity_set(fixture("entities_ensembl.tsv"), organism="Homo sapiens")


def test_two_spaces_refuse_to_join_and_say_what_to_do_about_it() -> None:
    with pytest.raises(lg.DimensionError) as caught:
        lg.check_step("score", {"truth": hgnc(), "found": ensembl()})

    said = str(caught.value)
    assert "HGNC symbol" in said
    assert "Ensembl gene" in said
    assert "map_identifiers" in said
    assert "never renamed implicitly" in said


def test_a_mapped_list_declares_ensembl_and_joins_an_ensembl_list() -> None:
    mapped = lg.map_identifiers(
        hgnc(), table(), column="ensembl_gene_id", to="Ensembl gene"
    )

    @lg.step
    def score(left: lg.EntitySet, right: lg.EntitySet) -> lg.EntitySet:
        """Compare two gene lists."""
        return lg.sh("cat {left} {right} > {out}")

    pipe = lg.Pipeline(score(mapped, ensembl()))
    assert pipe.errors() == []

    dataset = lg.handle(mapped).dataset
    assert dataset is not None
    found = dataset.dimension("identifier_namespace")
    assert found is not None
    assert found.value == "Ensembl gene"
    assert found.source == "from step `map_identifiers`"


def test_a_conversion_carries_every_other_axis() -> None:
    genes = hgnc(subject="breast carcinoma", release="HGNC 2026-09")
    mapped = lg.map_identifiers(
        genes, table(), column="ensembl_gene_id", to="Ensembl gene"
    )
    lg.Pipeline(mapped).check()

    dataset = lg.handle(mapped).dataset
    assert dataset is not None
    for axis, expected in [
        ("subject", "breast carcinoma"),
        ("release", "HGNC 2026-09"),
        ("organism", "Homo sapiens"),
    ]:
        found = dataset.dimension(axis)
        assert found is not None, axis
        assert found.value == expected, axis


def test_a_table_in_the_wrong_space_is_refused_before_anything_runs() -> None:
    # An Entrez table handed to a list of HGNC symbols. The table is typed,
    # so this is a dimension disagreement rather than an empty result.
    entrez = lg.entity_set(fixture("entities_entrez.tsv"), organism="Homo sapiens")
    mapped = lg.map_identifiers(hgnc(), entrez, column="log2fc", to="Ensembl gene")

    errors = lg.Pipeline(mapped).errors()
    assert errors
    assert any(error.dimension == "identifier_namespace" for error in errors)


def test_running_it_names_every_symbol_that_did_not_cross(tmp_path: Path) -> None:
    mapped = lg.map_identifiers(
        hgnc(), table(), column="ensembl_gene_id", to="Ensembl gene"
    )
    report = lg.Pipeline(mapped).run(cache=tmp_path / "cache")

    notes = [note for run in report.steps for note in run.notes]
    assert any("SEPT7" in note for note in notes), notes
    assert any("NKX2-1" in note for note in notes), notes


def test_the_mapped_output_holds_only_identifiers_that_crossed(
    tmp_path: Path,
) -> None:
    mapped = lg.map_identifiers(
        hgnc(), table(), column="ensembl_gene_id", to="Ensembl gene"
    )
    report = lg.Pipeline(mapped).run(cache=tmp_path / "cache")

    written = Path(report.steps[-1].output).read_text()
    assert "ENSG00000141510" in written
    # Carried over unchanged, an HGNC symbol would sit in a set declaring
    # Ensembl -- the implicit rename D38 forbids.
    assert "SEPT7" not in written
    assert "TP53" not in written


def test_a_step_claiming_an_unknown_conversion_is_refused_at_decoration() -> None:
    with pytest.raises(lg.StepDefinitionError, match="liftover"):

        @lg.step(conversion="liftover")
        def lift(x: lg.EntitySet, *, to: str) -> lg.EntitySet:
            """Lift a gene list over to another build."""
            return lg.sh("cp {x} {out}")


def test_a_conversion_with_no_target_parameter_is_refused_at_decoration() -> None:
    with pytest.raises(lg.StepDefinitionError, match="`to`"):

        @lg.step(conversion="map_identifiers")
        def mapper(x: lg.EntitySet) -> lg.EntitySet:
            """Map a gene list's identifiers, without saying where to."""
            return lg.sh("cp {x} {out}")


def test_a_blank_target_is_refused_when_the_graph_is_built() -> None:
    mapped = lg.map_identifiers(hgnc(), table(), column="ensembl_gene_id", to="   ")
    with pytest.raises(ValueError, match="blank"):
        lg.Pipeline(mapped)
