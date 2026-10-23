"""M12: a ranked list on the Python surface, where order is the result.

A `RankedList` is an `EntitySet` and one more thing: its canonical form
preserves order, because for a ranking the order *is* the output (D41). The
canonical hashes themselves are pinned in Rust, where they are reachable;
what is checked here is the surface — that the kind exists, carries the four
dimensions a gene list carries and no others, infers the space its names live
in, and brings the same two refusals to a ranked result.
"""

from __future__ import annotations

import pytest

import ligase as lg
from ligase.types import KINDS, RankedList
from tests.conftest import fixture


def ranked(**declared: object) -> lg.RankedList:
    """A ranking of HGNC symbols, with anything else the caller declares."""
    return lg.ranked_list(
        fixture("ranked_degs.tsv"),
        organism="Homo sapiens",
        **declared,  # type: ignore[arg-type]
    )


def truth_set(**declared: object) -> lg.EntitySet:
    """A curated truth set of HGNC symbols, as a gene list."""
    return lg.entity_set(
        fixture("entities_hgnc.tsv"),
        organism="Homo sapiens",
        **declared,  # type: ignore[arg-type]
    )


# ------------------------------------------------------------- the kind --


def test_a_ranked_list_is_a_kind_a_step_may_annotate() -> None:
    assert KINDS["RankedList"] is RankedList
    assert issubclass(RankedList, lg.Dataset)


def test_a_ranked_list_infers_the_space_its_identifiers_are_in() -> None:
    found = ranked().dimension("identifier_namespace")
    assert found is not None
    assert found.value == "HGNC symbol"
    # And it says where it got that from, so the inference is checkable.
    assert "pattern" in found.source


def test_a_ranked_list_carries_a_ranking_s_own_identifier_column() -> None:
    # A ranked table usually has a rank column, so `id` is rarely the one.
    listed = lg.ranked_list(
        fixture("ranked_named_column.tsv"),
        id_column="gene",
        organism="Homo sapiens",
    )
    found = listed.dimension("identifier_namespace")
    assert found is not None
    assert found.value == "HGNC symbol"


def test_a_ranked_list_applies_exactly_what_a_gene_list_applies() -> None:
    # The whole of D41 as a surface assertion: the two kinds differ in
    # their canonical form and in nothing else, so a dimension on one and
    # not the other would mean the difference had spread.
    assert set(ranked().applicable) == set(truth_set().applicable)
    assert set(ranked().applicable) == {
        "organism",
        "release",
        "subject",
        "identifier_namespace",
    }


def test_a_ranked_list_has_no_coordinate_system_to_ask_about() -> None:
    listed = ranked()
    for absent in ("coordinates", "sample_set", "reference", "contig_naming"):
        assert absent not in listed.applicable
        assert listed.dimension(absent) is None
        # Not a gap with a reason, either: the question does not arise.
        assert listed.unknown_reason(absent) is None


def test_a_ranked_list_says_what_it_is_in_its_repr() -> None:
    assert "RankedList" in repr(ranked())


def test_the_loader_says_which_ranks_a_spreadsheet_ate() -> None:
    listed = lg.ranked_list(fixture("ranked_excel_dates.tsv"), organism="Homo sapiens")
    said = " ".join(listed.notes)
    assert "2-Sep" in said
    assert "1-Mar" in said


def test_a_blank_declaration_is_refused_rather_than_recorded() -> None:
    # A blank that compares equal to another blank is a false agreement.
    for axis in ("identifier_namespace", "subject", "release"):
        with pytest.raises(ValueError, match=axis):
            ranked(**{axis: "   "})


# --------------------------------------------------------- the refusals --


def test_a_ranking_and_a_truth_set_in_different_spaces_refuse_to_join() -> None:
    with pytest.raises(lg.DimensionError) as caught:
        lg.check_step(
            "score",
            {
                "ranked": ranked(identifier_namespace="HGNC symbol"),
                "truth": truth_set(identifier_namespace="Ensembl gene"),
            },
        )

    said = str(caught.value)
    assert "HGNC symbol" in said
    assert "Ensembl gene" in said
    assert "map_identifiers" in said


def test_a_ranking_refuses_to_score_against_another_release() -> None:
    with pytest.raises(lg.DimensionError) as caught:
        lg.check_step(
            "score",
            {
                "ranked": ranked(release="HGNC 2026-09"),
                "truth": truth_set(release="HGNC 2026-03"),
            },
        )

    said = str(caught.value)
    assert "HGNC 2026-09" in said
    assert "HGNC 2026-03" in said


def test_two_silent_lists_are_refused_rather_than_agreeing() -> None:
    # The mixed pair has to demand the namespace as the matching pairs do,
    # or two lists that each say nothing join happily and overlap in
    # nothing -- which is the failure in its purest form.
    errors = lg.dimension_errors(
        "score",
        {
            "ranked": lg.ranked_list(fixture("ranked_excel_dates.tsv")),
            "truth": lg.entity_set(fixture("entities_mixed.tsv")),
        },
    )
    assert any(
        isinstance(error, lg.UndeclaredDimensionError)
        and "identifier_namespace" in str(error)
        for error in errors
    ), errors


def test_a_ranking_joins_a_truth_set_that_agrees_with_it() -> None:
    @lg.step
    def score(ranked: lg.RankedList, truth: lg.EntitySet) -> lg.RankedList:
        """Score a ranking against a curated truth set."""
        return lg.sh("cat {ranked} {truth} > {out}")

    pipe = lg.Pipeline(
        score(
            ranked(release="HGNC 2026-09", subject="breast carcinoma"),
            truth_set(release="HGNC 2026-09", subject="breast carcinoma"),
        )
    )
    assert pipe.errors() == []


# ----------------------------------- M13: crossing an identifier space --
#
# `lg.map_identifiers` is typed for an `EntitySet`, and M12 left the
# conversion registry covering a `RankedList` with no shipped step to use
# the edge. `lg.map_ranked_identifiers` is that step, and what is tested
# here is the half that needed deciding: what it *says* about an output
# whose ranks have moved.


def mapping() -> lg.EntitySet:
    """HGNC symbols to Ensembl IDs, with one of each failure in it.

    `TP53` and `BRCA1` cross cleanly, `NKX2-1` goes to two targets, and
    `EGFR` has no row at all. So any ranking mapped through it loses rows,
    which is the case that matters.
    """
    return lg.entity_set(fixture("map_hgnc_to_ensembl.tsv"), organism="Homo sapiens")


def test_a_mapped_ranking_declares_the_space_it_crossed_into() -> None:
    """The conversion crosses the axis for a ranking as it does for a set.

    M12 found and fixed the registry row this rests on: `graph::propagate`
    guarded a conversion's axis override on the kinds the conversion was
    registered for, and with only `EntitySet` named, a step annotated to
    return a ranking would have carried the *input's* namespace onto data
    that had crossed. This is the shipped step over that fixed row.
    """
    mapped = lg.map_ranked_identifiers(
        ranked(), mapping(), column="ensembl_gene_id", to="Ensembl gene"
    )
    lg.Pipeline(mapped).check()

    dataset = lg.handle(mapped).dataset
    assert dataset is not None
    found = dataset.dimension("identifier_namespace")
    assert found is not None
    assert found.value == "Ensembl gene"
    assert dataset.kind == "RankedList", (
        "a mapped ranking is still a ranking: its rows are in rank order and "
        "its canonical form keeps them that way"
    )


def test_a_mapped_ranking_keeps_its_order_and_says_the_ranks_moved(
    tmp_path: object,
) -> None:
    """The rows come out in rank order, shorter, and the step says so.

    Two assertions and they are the whole decision. The output is a
    well-formed ranking of what crossed — ranks 1..n, nothing in the file
    to show that n is smaller than it was — so if the step did not say the
    ranks had been renumbered, nothing would.

    `ranked_degs.tsv` is TP53, BRCA1, EGFR, NKX2-1 in that order. EGFR has
    no row in the table and NKX2-1 has two, so both are left out and what
    remains is TP53 then BRCA1: the top of the ranking crossed, and the
    ranks below the hole still closed up.
    """
    from pathlib import Path

    mapped = lg.map_ranked_identifiers(
        ranked(), mapping(), column="ensembl_gene_id", to="Ensembl gene"
    )
    report = lg.Pipeline(mapped).run(cache=Path(str(tmp_path)) / "cache")
    run = next(step for step in report.steps if step.step == "map_ranked_identifiers")

    rows = Path(run.output).read_text().splitlines()
    assert rows[0].split("\t") == ["id", "score"]
    assert [row.split("\t")[0] for row in rows[1:]] == [
        "ENSG00000141510",
        "ENSG00000012048",
    ], "order is the result, so the surviving rows must still be in rank order"

    said = " ".join(run.notes)
    assert "renumbered" in said, f"the ranks closed up and nothing said so: {run.notes}"
    for lost in ("EGFR", "NKX2-1"):
        assert lost in said, (
            f"{lost} did not cross and must still be named: {run.notes}"
        )


def test_a_mapped_ranking_says_which_rank_became_the_first(
    tmp_path: object,
) -> None:
    """When the top-ranked identifier does not cross, the second becomes first.

    Which is the consequence D41 records and the one a reader is most
    likely to act on without noticing: take the top ten of a mapped
    ranking and you have the top ten of what crossed, not the mapping of
    the top ten. `ranked_top_unmapped.tsv` leads with EGFR, which has no
    row in the table, so the output's rank 1 held rank 2 in the input and
    the note has to carry that number rather than merely report a loss.
    """
    from pathlib import Path

    leading = lg.ranked_list(
        fixture("ranked_top_unmapped.tsv"), organism="Homo sapiens"
    )
    mapped = lg.map_ranked_identifiers(
        leading, mapping(), column="ensembl_gene_id", to="Ensembl gene"
    )
    report = lg.Pipeline(mapped).run(cache=Path(str(tmp_path)) / "cache")
    run = next(step for step in report.steps if step.step == "map_ranked_identifiers")

    rows = Path(run.output).read_text().splitlines()
    assert [row.split("\t")[0] for row in rows[1:]] == [
        "ENSG00000141510",
        "ENSG00000012048",
    ]

    said = " ".join(run.notes)
    assert "rank 2 of the list that was mapped" in said, (
        f"the new leader was the input's second, and the note has to say so: "
        f"{run.notes}"
    )


def test_a_ranking_that_loses_nothing_says_nothing_about_ranks(
    tmp_path: object,
) -> None:
    """No note where no rank moved.

    A note is for something true of the data that a reader cannot act on
    without being told. A ranking every one of whose identifiers crossed
    has the ranks it went in with, and saying so anyway would be the kind
    of noise that teaches people to skip the notes.
    """
    from pathlib import Path

    mapped = lg.map_ranked_identifiers(
        lg.ranked_list(
            fixture("ranked_named_column.tsv"),
            organism="Homo sapiens",
            id_column="gene",
        ),
        mapping(),
        column="ensembl_gene_id",
        to="Ensembl gene",
        id_column="gene",
    )
    report = lg.Pipeline(mapped).run(cache=Path(str(tmp_path)) / "cache")
    run = next(step for step in report.steps if step.step == "map_ranked_identifiers")

    assert not any("renumbered" in note for note in run.notes), run.notes
