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
    """Everything a reader of the rendered page would see, as one string."""
    outputs, printed = run_notebook(stem)
    return printed + "".join(str(o) for o in outputs)


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
