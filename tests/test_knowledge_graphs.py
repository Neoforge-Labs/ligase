"""M15: a knowledge graph on the Python surface, and the sources that built it.

A knowledge graph is the first kind in Ligase that is *assembled* rather than
measured. What it holds follows from which evidence sources were folded in,
no node or edge records that list, and two graphs differing only in it join
and score against each other in silence. So the sources are a dimension, and
that is what admits the kind under D35.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

import ligase as lg
from ligase.types import KINDS, KnowledgeGraph
from tests.conftest import fixture

#: The five axes a graph claims, which is `EntitySet`'s four plus the one the
#: kind was admitted for.
AXES = frozenset(
    {"organism", "identifier_namespace", "subject", "release", "source_set"}
)


def graph(name: str, **declared: object) -> KnowledgeGraph:
    """Load a committed graph fixture."""
    return lg.knowledge_graph(fixture(name), **declared)  # type: ignore[arg-type]


# ---------------------------------------------------------- the dimensions --


def test_a_knowledge_graph_is_a_kind_a_step_may_annotate() -> None:
    assert KINDS["KnowledgeGraph"] is KnowledgeGraph
    assert issubclass(KnowledgeGraph, lg.Dataset)


def test_a_graph_claims_five_axes_and_no_coordinate_system() -> None:
    # Asking a graph for its coordinates is a category error, not a gap --
    # the same distinction a gene list draws, over a kind that holds no
    # sequence at all.
    loaded = graph("graph_targets.json", organism="Homo sapiens")

    assert set(loaded.applicable) == set(AXES)
    for absent in ("coordinates", "sample_set", "reference", "contig_naming"):
        assert absent not in loaded.applicable
        assert loaded.dimension(absent) is None
        assert loaded.unknown_reason(absent) is None


# -------------------------------------------- the sources are read, not asked --


def test_a_graphs_sources_are_read_off_its_own_nodes_and_edges() -> None:
    loaded = graph("graph_targets_disgenet.json", organism="Homo sapiens")

    found = loaded.dimension("source_set")
    assert found is not None
    assert found.value == "open-targets and disgenet"
    # And it says where it got that, so the reading is checkable.
    assert "nodes and edges" in found.source


def test_a_graph_may_record_its_assembly_once_at_the_top() -> None:
    loaded = graph("graph_assembly_at_the_top.json", organism="Homo sapiens")

    found = loaded.dimension("source_set")
    assert found is not None
    assert found.value == "open-targets and disgenet"


def test_a_graph_naming_no_sources_leaves_the_axis_unknown() -> None:
    # Not a known, empty set: two graphs that each named none would agree on
    # the axis, which is the silence the dimension exists to break.
    loaded = graph("graph_no_sources.json", organism="Homo sapiens")

    assert loaded.dimension("source_set") is None
    reason = loaded.unknown_reason("source_set")
    assert reason is not None
    assert "source_set" in reason


def test_a_declared_source_set_wins_over_what_the_graph_says() -> None:
    loaded = graph(
        "graph_targets.json",
        organism="Homo sapiens",
        source_set=["Open Targets 24.09"],
    )

    found = loaded.dimension("source_set")
    assert found is not None
    assert found.value == "Open Targets 24.09"
    assert found.source == "declared"


def test_a_graphs_namespace_is_read_off_its_node_ids() -> None:
    loaded = graph("graph_targets.json", organism="Homo sapiens")

    found = loaded.dimension("identifier_namespace")
    assert found is not None
    assert found.value == "Ensembl gene"
    assert "pattern" in found.source


def test_a_graph_of_genes_and_diseases_declines_to_infer_a_namespace() -> None:
    # Two spaces in one graph is ordinary for a graph rather than a fault,
    # so the reason says so instead of implying something went wrong.
    loaded = graph("graph_mixed_ids.json", organism="Homo sapiens")

    assert loaded.dimension("identifier_namespace") is None
    reason = loaded.unknown_reason("identifier_namespace")
    assert reason is not None
    assert "ordinary for a graph" in reason


# ------------------------------------------------------------- the refusal --


def test_two_graphs_assembled_from_different_sources_refuse_to_join() -> None:
    """The milestone's headline, on the surface a user meets it on."""
    with pytest.raises(lg.DimensionError) as caught:
        lg.check_step(
            "compare",
            {
                "ours": graph("graph_targets.json", organism="Homo sapiens"),
                "theirs": graph("graph_targets_disgenet.json", organism="Homo sapiens"),
            },
        )

    message = str(caught.value)
    # Both sets in full, the member that makes them differ named by name,
    # and the question a user can act on.
    assert "source_set" in message
    assert "open-targets" in message
    assert "theirs also lists disgenet" in message
    assert "Was a source added or removed upstream?" in message


def test_a_graph_and_a_gene_list_in_different_spaces_refuse_to_join() -> None:
    # Scoring a graph's neighbourhood against a curated gene list is the
    # commonest reason to join those two kinds at all, so the namespace is
    # demanded of the mixed pair and not only of two graphs. M12 learned
    # that the hard way on the other mixed pair.
    with pytest.raises(lg.DimensionError):
        lg.check_step(
            "score",
            {
                "kg": graph("graph_targets.json", organism="Homo sapiens"),
                "truth": lg.entity_set(
                    fixture("entities_hgnc.tsv"), organism="Homo sapiens"
                ),
            },
        )


def test_a_source_set_is_compared_exactly_where_an_organism_folds() -> None:
    # D36's reserved right, carried out. `s3://evidence/A.json` and
    # `s3://evidence/a.json` are two objects by specification, and folding
    # the case would drop one of them without a word -- turning two graphs
    # that differ into two that agree. An organism still folds, because
    # `HIV-1` and `hiv-1` are one organism by convention.
    with pytest.raises(lg.DimensionError, match="source_set"):
        lg.check_step(
            "compare",
            {
                "upper": graph("graph_source_upper.json"),
                "lower": graph("graph_source_lower.json"),
            },
        )

    lg.check_step(
        "compare",
        {
            "one": graph("graph_targets.json", organism="HIV-1"),
            "two": graph("graph_targets.json", organism="hiv-1"),
        },
    )


def test_two_graphs_from_different_releases_refuse_to_join() -> None:
    with pytest.raises(lg.DimensionError, match="release"):
        lg.check_step(
            "compare",
            {
                "old": graph("graph_targets.json", release="Open Targets 24.06"),
                "new": graph("graph_targets.json", release="Open Targets 24.09"),
            },
        )


# ------------------------------------------------------- what is refused --


@pytest.mark.parametrize("blank", ["", "   "])
@pytest.mark.parametrize("field", ["identifier_namespace", "subject", "release"])
def test_a_blank_declaration_is_refused_where_it_is_written(
    blank: str, field: str
) -> None:
    with pytest.raises(ValueError, match=field):
        graph("graph_targets.json", organism="Homo sapiens", **{field: blank})


@pytest.mark.parametrize("empty", [[], ["", "  "]])
def test_a_source_set_naming_nothing_is_refused(empty: list[str]) -> None:
    # Rather than falling back to reading the graph, which is not what
    # somebody who passed a list meant.
    with pytest.raises(ValueError, match="source_set"):
        graph("graph_targets.json", organism="Homo sapiens", source_set=empty)


def test_a_node_without_an_id_is_not_a_graph() -> None:
    with pytest.raises(lg.LoadError, match="id"):
        graph("graph_node_without_an_id.json")


def test_a_file_that_is_not_json_is_not_a_graph(tmp_path: Path) -> None:
    path = tmp_path / "not-a-graph.json"
    path.write_text("id\tscore\nTP53\t0.9\n", encoding="utf-8")

    with pytest.raises(lg.LoadError):
        lg.knowledge_graph(str(path))


# ------------------------------------------------------- in a pipeline --


def test_a_step_over_a_graph_carries_every_axis_it_claims(tmp_path: Path) -> None:
    """A graph's dimensions survive a step, which is what a pipeline rests on."""

    @lg.step
    def prune(kg: lg.KnowledgeGraph) -> lg.KnowledgeGraph:
        """Drop the edges below a score, which this test does not care about."""
        return lg.sh("cp {kg} {out}")

    source = tmp_path / "graph.json"
    source.write_text(
        Path(fixture("graph_targets.json")).read_text(encoding="utf-8"),
        encoding="utf-8",
    )
    loaded = lg.knowledge_graph(
        str(source),
        organism="Homo sapiens",
        subject="breast carcinoma",
        release="Open Targets 24.09",
    )

    held = prune(loaded)
    pipeline = lg.Pipeline(held)
    pipeline.check()

    # Every axis the graph claimed survives the step, each attributed to it.
    propagated = lg.handle(held).dataset
    assert propagated is not None
    for axis, expected in (
        ("organism", "Homo sapiens"),
        ("identifier_namespace", "Ensembl gene"),
        ("subject", "breast carcinoma"),
        ("release", "Open Targets 24.09"),
        ("source_set", "open-targets"),
    ):
        found = propagated.dimension(axis)
        assert found is not None, axis
        assert found.value == expected, axis
        assert found.source == "from step `prune`", axis

    report = pipeline.run(cache=tmp_path / "store")
    assert not report.failures
    [out] = report.outputs("prune")
    # The output is a graph, and reading it back as one works: the suffix a
    # step writes is `.json`, so a tool reading a format off the name can.
    assert out.endswith(".json")
    assert json.loads(Path(out).read_text(encoding="utf-8"))["nodes"]
