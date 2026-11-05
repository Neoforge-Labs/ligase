"""The showcase notebooks run whole pipelines during the docs build.

A notebook that fails to run is a page that lies, and one that runs but no
longer shows what its prose says is worse. These tests load each notebook the
way `web/scripts/render_notebooks.py` does and read what it produced.
"""

from __future__ import annotations

import importlib.util
import io
from contextlib import redirect_stdout
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WEB = ROOT / "web"
NOTEBOOKS = ROOT / "examples/notebooks"


def run_notebook(stem: str) -> tuple[list[object], str]:
    """Execute a notebook, returning its cell outputs and anything printed."""
    path = NOTEBOOKS / f"{stem}.py"
    spec = importlib.util.spec_from_file_location(f"nb_{stem}", path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    printed = io.StringIO()
    with redirect_stdout(printed):
        spec.loader.exec_module(module)
        outputs, _ = module.app.run()
    return list(outputs), printed.getvalue()


def page_of(stem: str) -> str:
    """The text a notebook's page is made of, as one string.

    That is what it printed, plus the markdown of every `mo.md` cell, which
    is where the prose and its interpolated values live. Code cells'
    returned values are included as `str()` shows them. It is not the
    rendered page: source code is left out, as are the headings the
    renderer demotes.
    """
    outputs, printed = run_notebook(stem)
    parts = [printed]
    for output in outputs:
        render = getattr(output, "_repr_markdown_", None)
        parts.append(render() if render is not None else str(output))
    return "".join(parts)


def test_a_cohort_shows_cache_then_cutoff_then_propagation() -> None:
    page = page_of("05_a_cohort_and_what_changed")
    assert "4 steps: 4 ran." in page
    assert "4 steps: 4 from cache." in page
    assert "4 steps: 1 ran, 2 from cache, 1 skipped." in page
    assert "4 steps: 2 ran, 2 from cache." in page


def test_a_cohort_refuses_the_bed() -> None:
    page = page_of("05_a_cohort_and_what_changed")
    assert "DimensionError in step `depth`" in page
    assert "1-based closed" in page
    assert "0-based half-open" in page


def test_a_cohort_refuses_prefixed_contigs() -> None:
    page = page_of("05_a_cohort_and_what_changed")
    assert "ContigNamingError in step `depth`" in page
    assert "contig naming" in page.lower()


def test_a_cohort_is_cold_on_every_run() -> None:
    """A second run of the notebook must still report `4 ran`."""
    for _ in range(2):
        assert "4 steps: 4 ran." in page_of("05_a_cohort_and_what_changed")


SHORTLIST = "06_from_a_graph_to_a_shortlist"


def test_a_shortlist_maps_and_reports_the_collision() -> None:
    page = page_of(SHORTLIST)
    assert "run 1, a cold cache: 3 steps: 3 ran." in page
    assert "**0 problems**" in page
    # The two best-connected genes share a symbol, so both are dropped, named,
    # and the ranks below them are renumbered.
    assert (
        "the identifier `ENSG00000012055` maps to a target another "
        "identifier also maps to, so it is not in the output"
    ) in page
    assert (
        "the identifier `ENSG00000011096` maps to a target another "
        "identifier also maps to, so it is not in the output"
    ) in page
    assert (
        "2 identifiers did not cross into the target space, so the output "
        "is 2 entities smaller than the input"
    ) in page
    assert "rank 1 of this ranking was rank 3 of the list that was mapped" in page


def test_a_shortlist_reads_the_same_graph_in_either_source_order() -> None:
    page = page_of(SHORTLIST)
    assert page.count("**open-targets and disgenet**") == 2


def test_a_shortlist_refuses_a_namespace_mismatch() -> None:
    page = page_of(SHORTLIST)
    assert (
        "DimensionError in step `shortlist`:\n"
        "  ranking.identifier_namespace = Ensembl gene (from step `score`)\n"
        "  degs.identifier_namespace = HGNC symbol (declared)\n"
        "  These must match. Add a `map_identifiers` step; identifiers are "
        "never renamed implicitly."
    ) in page


def test_a_ranking_reruns_when_reordered_and_a_set_does_not() -> None:
    """D41, the claim this notebook exists to show."""
    page = page_of(SHORTLIST)
    assert (
        "run 2, the DEG ranking's rows reversed: 3 steps: 1 ran, 2 from cache."
    ) in page
    assert (
        "[('score', 'cached'), ('map_ranked_identifiers', 'cached'), "
        "('shortlist', 'ran')]"
    ) in page
    assert "run 3, the shortlist set: 1 step: 1 ran." in page
    assert "run 4, the shortlist set's rows reversed: 1 step: 1 from cache." in page


def test_a_shortlist_is_cold_on_every_run() -> None:
    """A second run of the notebook must still report a cold first run."""
    for _ in range(2):
        assert "run 1, a cold cache: 3 steps: 3 ran." in page_of(SHORTLIST)
