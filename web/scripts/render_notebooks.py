#!/usr/bin/env python3
"""Render the marimo example notebooks to MDX, from real runs.

The notebooks are the examples, and they are executed on every docs build --
that part is unchanged. A marimo notebook is importable Python, so running it
is the whole check: if the API drifts from what an example claims, the build
fails here instead of shipping a page that does not work.

What is new is that the run's output is kept. The previous site executed the
notebooks in CI and then linked to them on GitHub, so a reader saw the source
and never the result. Here each cell's real output is rendered: a markdown
cell contributes the text it actually produced, with the values it actually
computed, and any other cell contributes its code and what it returned.

They are rendered, not made interactive. marimo can export a notebook that
runs in the browser through Pyodide, and Ligase cannot use it: it is a
compiled Rust extension with no emscripten wheel. The honest version of
"interactive docs" for a native extension is a notebook you can download and
run, not an iframe that cannot import the thing it documents.

No new dependency. `marimo export ipynb --include-outputs` would do much of
this, but it needs `nbformat`, and the project asks before adding one.
"""

from __future__ import annotations

import ast
import importlib.util
import io
import os
import re
import sys
import traceback
from contextlib import redirect_stdout
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
NOTEBOOKS = ROOT / "examples/notebooks"
OUT = Path(__file__).resolve().parents[1] / "content/notebooks"

# What each notebook is for, in the order a reader should meet them. Keeping
# this here rather than deriving it from filenames means the nav reads as
# prose, and a new notebook has to be placed deliberately.
ORDER: tuple[tuple[str, str, str], ...] = (
    (
        "01_what_a_dataset_knows",
        "What a dataset knows",
        "Dimensions, and the provenance of each one.",
    ),
    (
        "02_an_error_before_compute",
        "An error before compute",
        "A naming mismatch caught with nothing executed.",
    ),
    (
        "03_a_graph_that_has_not_run",
        "A graph that has not run",
        "Per-sample fan-out, and every error in one pass.",
    ),
    (
        "04_resistance_and_its_rules",
        "Resistance, and why the rules are hashed",
        "Vendored algorithms, and content hashing over version labels.",
    ),
)

REPO = "https://github.com/Neoforge-Labs/ligase/blob/main/examples/notebooks"


@dataclass
class Cell:
    """One `@app.cell` function: its body, and what running it produced."""

    source: str
    output: Any


def cell_sources(path: Path) -> list[str]:
    """Each cell's body, in file order, with marimo's bookkeeping removed."""
    tree = ast.parse(path.read_text(encoding="utf-8"))
    sources: list[str] = []
    for node in tree.body:
        if not isinstance(node, ast.FunctionDef):
            continue
        decorators = {
            ast.unparse(d.func if isinstance(d, ast.Call) else d)
            for d in node.decorator_list
        }
        if not any(d.endswith("app.cell") for d in decorators):
            continue
        # A cell ends with `return (x,)`, which is how marimo passes values
        # between cells. It is file format, not something a reader wrote.
        body = [s for s in node.body if not isinstance(s, ast.Return)]
        text = "\n".join(ast.unparse(s) for s in body)
        sources.append(text.strip())
    return sources


def run(path: Path) -> tuple[list[Any], str]:
    """Execute a notebook, returning its cell outputs and anything printed."""
    spec = importlib.util.spec_from_file_location(f"nb_{path.stem}", path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"{path.name}: cannot be imported")
    module = importlib.util.module_from_spec(spec)
    printed = io.StringIO()
    with redirect_stdout(printed):
        spec.loader.exec_module(module)
        outputs, _ = module.app.run()
    return list(outputs), printed.getvalue()


def scrub(text: str) -> str:
    """Make output reproducible: no build machine's paths in it.

    A `repr()` prints an absolute path, which differs between a laptop and
    CI. Relative to the repository root it is the same everywhere, and it is
    the form a reader can act on.
    """
    return text.replace(f"{ROOT}/", "").replace(str(ROOT), ".")


def demote(markdown: str) -> str:
    """Push a notebook's headings one level down, under the page's own H1."""
    out = []
    in_fence = False
    for line in markdown.split("\n"):
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
        elif not in_fence and line.startswith("#"):
            line = "#" + line
        out.append(line)
    return "\n".join(out)


def markdown_of(output: Any) -> str | None:
    """The markdown a `mo.md(...)` cell produced, if that is what this is."""
    render = getattr(output, "_repr_markdown_", None)
    if render is None:
        return None
    text = render()
    # marimo keeps the f-string's own indentation, and interpolated lines
    # arrive without it, so a blanket dedent would not work.
    flat = "\n".join(re.sub(r"^ {1,4}", "", line) for line in text.split("\n")).strip()
    return demote(scrub(flat))


def is_import_only(source: str) -> bool:
    """Setup cells are noise on a rendered page."""
    statements = [
        s for s in source.split("\n") if s.strip() and not s.strip().startswith("#")
    ]
    return bool(statements) and all(
        s.startswith(("import ", "from ")) or ("=" in s and "fixtures" in s)
        for s in statements
    )


def render(stem: str, title: str, blurb: str) -> str:
    """Render."""
    path = NOTEBOOKS / f"{stem}.py"
    if not path.is_file():
        raise SystemExit(f"notebook missing: {path}")

    sources = cell_sources(path)
    try:
        outputs, printed = run(path)
    except Exception:
        print(f"\nerror: {path.name} failed to run.\n", file=sys.stderr)
        traceback.print_exc()
        raise SystemExit(
            f"\n{path.name} is an example on the docs site, so a notebook that "
            f"does not run is a page that lies. Fix the notebook or the API it "
            f"uses."
        ) from None

    if len(sources) != len(outputs):
        raise SystemExit(
            f"{path.name}: parsed {len(sources)} cells but the run produced "
            f"{len(outputs)} outputs. The renderer pairs them by position, so "
            f"it cannot tell which output belongs to which cell."
        )

    body: list[str] = [
        "---",
        f'title: "{title}"',
        f'description: "{blurb} A marimo notebook, rendered from a real run."',
        "---",
        "",
        f"# {title}",
        "",
        "<Lede>",
        "",
        f"{blurb} This page is **rendered from a real run** on every docs build —",
        "every value below was computed by the engine, not typed out. The notebook",
        "itself is ordinary Python you can download and run.",
        "",
        "</Lede>",
        "",
        "```bash",
        f"marimo edit examples/notebooks/{stem}.py   # as a notebook",
        f"python examples/notebooks/{stem}.py        # or as a plain script",
        "```",
        "",
        f"[Read the source on GitHub]({REPO}/{stem}.py)",
        "",
        "---",
        "",
    ]

    for source, output in zip(sources, outputs, strict=True):
        markdown = markdown_of(output)
        if markdown is not None:
            # The cell's whole purpose is its prose; showing `mo.md(f"...")`
            # alongside the text it produced would be showing the same thing
            # twice, the second time worse.
            body += [markdown, ""]
            continue

        if is_import_only(source):
            continue

        body += ["```python", scrub(source), "```", ""]
        if output is not None:
            body += ["```text", scrub(repr(output)), "```", ""]

    if printed.strip():
        body += [
            "## What it printed",
            "",
            "```text",
            scrub(printed.strip()),
            "```",
            "",
        ]

    return "\n".join(body).rstrip() + "\n"


def main() -> int:
    """Run every notebook and write its page."""
    # As in capture_diagnostics.py: the notebooks resolve fixtures and print
    # paths, so where they run from has to be fixed, not inherited.
    os.chdir(ROOT)
    OUT.mkdir(parents=True, exist_ok=True)
    for stem, title, blurb in ORDER:
        (OUT / f"{stem[3:].replace('_', '-')}.mdx").write_text(
            render(stem, title, blurb), encoding="utf-8"
        )
        print(f"  ran and rendered {stem}")

    meta = [
        "import type { MetaRecord } from 'nextra'",
        "",
        "// Generated by web/scripts/render_notebooks.py. Do not edit.",
        "const meta: MetaRecord = {",
        "  index: 'About the notebooks',",
    ]
    meta += [
        f"  '{stem[3:].replace('_', '-')}': {title!r}," for stem, title, _ in ORDER
    ]
    meta += ["}", "", "export default meta", ""]
    (OUT / "_meta.ts").write_text("\n".join(meta).replace('"', "'"), encoding="utf-8")

    print(f"rendered {len(ORDER)} notebooks from real runs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
