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
