"""The showcase notebooks run whole pipelines during the docs build.

A notebook that fails to run is a page that lies, and one that runs but no
longer shows what its prose says is worse. These tests load each notebook the
way `web/scripts/render_notebooks.py` does and read what it produced.
"""

from __future__ import annotations

import importlib
import importlib.util
import io
import sys
import tempfile
from contextlib import redirect_stdout
from pathlib import Path
from types import ModuleType

import pytest

ROOT = Path(__file__).resolve().parents[1]
WEB = ROOT / "web"
NOTEBOOKS = ROOT / "examples/notebooks"

sys.path.insert(0, str(ROOT / "web/scripts"))


def renderer() -> ModuleType:
    """The docs build's renderer, which lives outside any package."""
    return importlib.import_module("render_notebooks")


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
        "the identifier `ENSG00000010685` maps to a target another "
        "identifier also maps to, so it is not in the output"
    ) in page
    assert (
        "the identifier `ENSG00000010137` maps to a target another "
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


def test_a_shortlist_refuses_graphs_built_from_different_sources() -> None:
    page = page_of(SHORTLIST)
    assert (
        "DimensionError in step `overlap`:\n"
        "  a.source_set = open-targets and disgenet (from the sources named "
        "on its nodes and edges: work/targets.json)\n"
        "  b.source_set = open-targets (from the sources named on its nodes "
        "and edges: work/targets_one_source.json)\n"
        "  a also lists disgenet.\n"
        "  These must match. Was a source added or removed upstream?"
    ) in page


SCREEN = "07_a_screen_and_its_seed"


def flat(text: str) -> str:
    """Whitespace collapsed, so prose assertions survive re-wrapping."""
    return " ".join(text.split())


def test_a_screen_refuses_a_representation_mismatch() -> None:
    page = page_of(SCREEN)
    assert (
        "DimensionError in step `enrichment`:\n"
        "  library.representation = InChIKey, salt-stripped and "
        "charge-neutralised (from the representation line the standardiser "
        "wrote: work/library.tsv)\n"
        "  hits.representation = InChIKey, as given (from the representation "
        "line the standardiser wrote: work/hits_salts.tsv)\n"
        "  library also lists salt-stripped and charge-neutralised.\n"
        "  These must match. Were the two sets standardised differently? Add "
        "a `standardize` step; molecules are never re-keyed implicitly."
    ) in page
    # The fix is named on the page and is never called.
    assert "`lg.chem.standardize`" in flat(page)
    assert "`chem` extra" in flat(page)


def test_a_screen_is_reproducible_and_the_undeclared_body_is_deterministic() -> None:
    """D40: a declared, seeded step is never `deterministic`; its twin is."""
    page = page_of(SCREEN)
    declared, undeclared = page.split("audit, the same body with nothing declared:")
    assert "reproducible     `rescore`" in declared
    assert "deterministic    `rescore`" not in declared
    assert (
        "    because `seed` was held at 20260101, not because the step cannot wobble"
    ) in declared
    assert "1 step: 1 reproducible." in declared
    assert "deterministic    `rescore`" in undeclared
    assert "1 step: 1 deterministic." in undeclared
    assert "because `seed`" not in undeclared


def test_a_screen_reruns_when_the_seed_changes() -> None:
    page = page_of(SCREEN)
    assert "run 1, a cold cache: 2 steps: 2 ran." in page
    assert "run 2, the same seed: 2 steps: 2 from cache." in page
    assert "run 3, the seed changed: 2 steps: 2 ran." in page
    assert "[('rescore', 'ran'), ('top', 'ran')]" in page


def test_a_screen_checks_before_it_runs() -> None:
    page = flat(page_of(SCREEN))
    assert "The screen has **2 steps**" in page
    assert "`errors()` found **0 problems**" in page
    assert "The engine found **1 problem** and ran nothing." in page


def test_a_screen_is_cold_on_every_run() -> None:
    """A second run of the notebook must still report a cold first run."""
    for _ in range(2):
        assert "run 1, a cold cache: 2 steps: 2 ran." in page_of(SCREEN)


def test_a_screen_page_names_no_temp_path() -> None:
    page = page_of(SCREEN)
    assert "/var/folders" not in page
    assert "/tmp" not in page
    assert "duration" not in page


def test_a_screen_says_what_its_scoring_step_is_and_is_not() -> None:
    page = flat(page_of(SCREEN))
    assert (
        "The library names **aspirin, ibuprofen, warfarin, metformin**. The "
        "hits are **aspirin sodium, metformin hydrochloride**. By name, "
        "**2 of the 2** hits are compounds the library holds. By InChIKey, "
        "**0 of the 2** are found in the library."
    ) in page
    assert "the scoring step here is not docking" in page
    assert "/notebooks/a-cohort-and-what-changed/" in page
    assert "/notebooks/what-a-paper-needs/" in page


def test_a_screen_treats_no_declaration_as_unknown_and_refuses_it() -> None:
    """A table with no representation line is not assumed to be `as given`."""
    page = page_of(SCREEN)
    assert "dimensions of a table with no declaration: ['organism']" in page
    assert "its representation: None" in page
    assert (
        "UndeclaredDimensionError in step `enrichment`:\n"
        "  representation is unknown in work/no_representation.tsv (this "
        "table names its molecules by InChIKey but does not say what "
        "standardisation produced them, and the keys cannot say -- declare "
        "representation, or standardise the table)\n"
        "  `enrichment` needs representation to agree. Declare it where you "
        "load the data."
    ) in page
    assert "the engine found **1 problem** when that table met a declared one" in flat(
        page
    )


PAPER = "08_what_a_paper_needs"


def test_a_paper_binds_its_claims() -> None:
    page = page_of(PAPER)
    # Matched loosely: the tail of this line names the machine that built it.
    assert "Analyses were run with Ligase" in page
    assert "verdict: verified\naccounted for: True\n" in page
    assert "summary: 3 steps, all accounted for. 1 claim, all bound." in page
    prose = flat(page)
    assert (
        "rests on `shortlist`, whose output gives score = 3.6159 at row 0, "
        "within the 0.005 this claim allows"
    ) in prose
    assert "tools: [('ligase', 'ligase " in page
    assert "uncited: []" in page
    assert (
        "The verdict is **verified**, the write-up is accounted for: **True**"
    ) in prose


def test_a_paper_states_what_its_tolerance_is_for() -> None:
    prose = flat(page_of(PAPER))
    assert "The step wrote **3.6159**, and the paper prints **3.62**." in prose
    assert "the difference is 0.0041, inside 0.005" in prose
    assert (
        "the same claim with no tolerance: the claim \"The top candidate's "
        'summed edge score is 3.62." names a cell of the output and no '
        "`tolerance=`."
    ) in prose


def test_a_paper_refuses_an_unbacked_claim() -> None:
    page = page_of(PAPER)
    assert (
        "verdict: contradicted\n"
        "the run recorded: 3.6159\n"
        "accounted for: False\n"
        "summary: 3 steps, all accounted for. 1 claim: 0 bound, 1 not.\n"
        "an `assert methods.accounted_for` gate: failed\n"
    ) in page
    assert (
        "`methods()` put **1 claim** in `unbacked`, with the verdict "
        "**contradicted**, and the write-up is accounted for: **False**. "
        "The good pipeline above says **True**."
    ) in flat(page)


def test_a_paper_writes_a_bundle_and_verifies_it() -> None:
    page = page_of(PAPER)
    assert (
        "```text METHODS.md README.md claims.tsv commands.tsv environment.tsv "
        "inputs.tsv notes.tsv references.tsv remotes.tsv results.tsv "
        "steps/map_ranked_identifiers.py steps/score.py steps/shortlist.py ```"
    ) in flat(page)
    assert "verification: 3 results: 3 identical.\n" in page
    assert "reproduced: True\n" in page
    assert (
        "results: [('score', 'identical', True), "
        "('map_ranked_identifiers', 'identical', True), "
        "('shortlist', 'identical', True)]"
    ) in page
    prose = flat(page)
    assert "wrote **13 files** and accounts for **3 steps**" in prose
    assert "Of the 3 results, **3** have the canonical hash" in prose


def test_a_paper_runs_its_pipeline_and_links_its_neighbour() -> None:
    page = page_of(PAPER)
    assert "run: 3 steps: 3 ran." in page
    assert "GENE10\t3.6159" in page
    assert "/notebooks/from-a-graph-to-a-shortlist/" in page


def test_a_paper_page_names_no_temp_path_or_duration() -> None:
    page = page_of(PAPER)
    assert "/var/folders" not in page
    assert "/tmp" not in page
    assert tempfile.gettempdir() not in page
    assert "duration" not in page


def test_a_paper_shows_its_methods_inline_and_counts_its_tools() -> None:
    page = page_of(PAPER)
    prose = flat(page)
    assert "```text Analyses were run with Ligase" in prose
    assert " References Njeri K. Ligase: " in prose
    assert "The record names **1 tool**, **ligase**" in prose
    assert "the reference list holds **1 citation**" in prose
    assert "reports **0** here" in prose


def test_no_notebook_returns_a_bare_number_or_flag_from_a_cell() -> None:
    """A cell whose last line is a call renders its return value as a block.

    `Path.write_text` returns a character count, and a cell ending in it put a
    bare `81` on a page. The source gives no sign of it, so every notebook is
    held to the rule that no cell returns a bare int or bool.
    """
    order = renderer().ORDER

    for stem, _, _ in order:
        outputs, _ = run_notebook(stem)
        for output in outputs:
            assert not isinstance(output, int | bool), (stem, repr(output))


def test_the_pipeline_notebooks_show_prose_or_nothing_from_a_cell() -> None:
    """06-08 build their pages from `mo.md` alone; anything else is a stray.

    01-05 deliberately end some cells on a value, which the renderer shows as
    a `text` block under the code that made it. These three do not, so a
    block there is always a mistake.
    """
    order = renderer().ORDER

    for stem, _, _ in order[5:]:
        outputs, _ = run_notebook(stem)
        for output in outputs:
            assert output is None or hasattr(output, "_repr_markdown_"), (
                stem,
                repr(output),
            )


def test_every_notebook_renders() -> None:
    """Each cell of every notebook has exactly one output to render."""
    r = renderer()
    for stem, _, _ in r.ORDER:
        path = NOTEBOOKS / f"{stem}.py"
        outputs, _ = r.run(path)
        assert len(r.cell_sources(path)) == len(outputs), stem


def test_no_temp_paths_on_any_page() -> None:
    """`scrub()` only rewrites the repo root.

    Pages are rendered in memory: the output directory is untracked, so
    reading it would assert nothing on a fresh clone.
    """
    r = renderer()
    for stem, title, blurb in r.ORDER:
        page = r.render(stem, title, blurb)
        assert page.strip(), stem
        for leak in ("/var/folders", "/tmp/", tempfile.gettempdir()):
            assert leak not in page, (stem, leak)


def test_rendering_writes_nothing_into_the_repo(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A missed `cache=` lands in `./.ligase/`.

    `main()` also rewrites the untracked `web/content/notebooks`, and it
    changes directory; `monkeypatch.chdir` puts the latter back.
    """
    monkeypatch.chdir(ROOT)

    def entries() -> set[Path]:
        root = ROOT / ".ligase"
        return set(root.rglob("*")) if root.exists() else set()

    before = entries()
    renderer().main()
    assert entries() == before


def test_index_lists_every_notebook_and_counts_them_right() -> None:
    """The index page is the only way in; an orphan page fails the site check."""
    r = renderer()
    page = r.index()
    for stem, _, _ in r.ORDER:
        assert f"/notebooks/{r.slug(stem)}/" in page, stem
    text = flat(page)
    assert "Eight" in text
    for stale in ("Four marimo", "first three", "The fourth"):
        assert stale not in text, stale
    assert "no data, no aligner" in text
