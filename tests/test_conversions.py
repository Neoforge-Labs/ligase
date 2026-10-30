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


# ---------------------------------- D60: a declared one-to-many crossing --


def test_an_ambiguous_identifier_is_dropped_unless_a_fan_out_is_asked_for(
    tmp_path: Path,
) -> None:
    """The default is unchanged: NKX2-1 is in two rows and crosses nowhere."""
    mapped = lg.map_identifiers(
        hgnc(), table(), column="ensembl_gene_id", to="Ensembl gene"
    )
    report = lg.Pipeline(mapped).run(cache=tmp_path / "cache")
    run = next(step for step in report.steps if step.step == "map_identifiers")

    written = Path(run.output).read_text()
    assert "ENSG00000136352" not in written
    assert any("more than one target" in note for note in run.notes), run.notes


def test_a_declared_fan_out_puts_a_row_at_every_target(tmp_path: Path) -> None:
    """D60: some identifiers denote more than one entity.

    *P. falciparum* `DHFR-TS` is one polypeptide with two catalytic
    domains, answering to human `DHFR` and `TYMS`. "Decide which target you
    mean" has no correct answer for it, so the caller declares a fan-out
    and the note records that the choice was theirs.
    """
    mapped = lg.map_identifiers(
        hgnc(),
        table(),
        column="ensembl_gene_id",
        to="Ensembl gene",
        ambiguous="fan-out",
    )
    report = lg.Pipeline(mapped).run(cache=tmp_path / "cache")
    run = next(step for step in report.steps if step.step == "map_identifiers")

    written = Path(run.output).read_text()
    assert "ENSG00000136352" in written
    assert "ENSG00000268173" in written

    said = " ".join(run.notes)
    assert "asked for a fan-out" in said, run.notes
    assert "larger than the input" in said, run.notes


def test_the_fan_out_choice_is_a_parameter_so_it_reruns(tmp_path: Path) -> None:
    """Changing what an ambiguous identifier does changes the output.

    So it has to reach the cache key, which it does by being an ordinary
    step parameter — the thing a conversion's `to=` already relies on.
    """
    cache = tmp_path / "cache"
    dropped = lg.map_identifiers(
        hgnc(), table(), column="ensembl_gene_id", to="Ensembl gene"
    )
    lg.Pipeline(dropped).run(cache=cache)

    fanned = lg.map_identifiers(
        hgnc(),
        table(),
        column="ensembl_gene_id",
        to="Ensembl gene",
        ambiguous="fan-out",
    )
    report = lg.Pipeline(fanned).run(cache=cache)
    run = next(step for step in report.steps if step.step == "map_identifiers")

    assert run.outcome == "ran", (
        "a different answer about the biology cannot come from the cache "
        "entry of the other answer"
    )


def test_a_spelling_that_is_neither_is_refused(tmp_path: Path) -> None:
    """A typo must not fall back to the option that loses rows."""
    mapped = lg.map_identifiers(
        hgnc(),
        table(),
        column="ensembl_gene_id",
        to="Ensembl gene",
        ambiguous="fanout",
    )
    with pytest.raises(Exception, match="fan-out"):
        lg.Pipeline(mapped).run(cache=tmp_path / "cache")


def test_fanning_a_ranking_returns_a_set_because_the_ranking_is_gone(
    tmp_path: Path,
) -> None:
    """D41 and D60 together: one row becoming two has no rank.

    So the step that fans a ranking says the ranking did not survive, by
    returning an `EntitySet`. Inventing tie ranks, or renumbering, would
    hand back something that looks like a result nobody computed.
    """
    ranked = lg.ranked_list(
        fixture("ranked_degs.tsv"),
        organism="Homo sapiens",
        identifier_namespace="HGNC symbol",
    )
    fanned = lg.fan_out_ranked_identifiers(
        ranked, table(), column="ensembl_gene_id", to="Ensembl gene"
    )
    lg.Pipeline(fanned).check()

    dataset = lg.handle(fanned).dataset
    assert dataset is not None
    assert dataset.kind == "EntitySet", (
        "a fanned ranking is not a ranking: it has gained rows, and rank is "
        "the row position (D41)"
    )
    found = dataset.dimension("identifier_namespace")
    assert found is not None
    assert found.value == "Ensembl gene"


def test_a_fanned_ranking_says_what_fanned_and_not_that_ranks_moved(
    tmp_path: Path,
) -> None:
    """There are no ranks to have moved, which is the point of the type."""
    ranked = lg.ranked_list(
        fixture("ranked_degs.tsv"),
        organism="Homo sapiens",
        identifier_namespace="HGNC symbol",
    )
    fanned = lg.fan_out_ranked_identifiers(
        ranked, table(), column="ensembl_gene_id", to="Ensembl gene"
    )
    report = lg.Pipeline(fanned).run(cache=tmp_path / "cache")
    run = next(
        step for step in report.steps if step.step == "fan_out_ranked_identifiers"
    )

    said = " ".join(run.notes)
    assert "asked for a fan-out" in said, run.notes
    assert "renumbered" not in said, run.notes
