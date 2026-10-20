# Example notebooks

Four [marimo](https://marimo.io) notebooks, easiest first. They are ordinary
Python files, so they run as notebooks or as scripts, and they are executed on
every docs build — if the API drifts from what they claim, the build fails
rather than the page misleading you.

| | Notebook | What it shows |
|---|---|---|
| 1 | [What a dataset knows](https://github.com/Neoforge-Labs/ligase/blob/main/examples/notebooks/01_what_a_dataset_knows.py) | Dimensions, and the provenance of each one |
| 2 | [An error before compute](https://github.com/Neoforge-Labs/ligase/blob/main/examples/notebooks/02_an_error_before_compute.py) | A naming mismatch caught with nothing executed |
| 3 | [A graph that has not run](https://github.com/Neoforge-Labs/ligase/blob/main/examples/notebooks/03_a_graph_that_has_not_run.py) | Per-sample fan-out, and every error in one pass |
| 4 | [Resistance, and why the rules are hashed](https://github.com/Neoforge-Labs/ligase/blob/main/examples/notebooks/04_resistance_and_its_rules.py) | Vendored algorithms, and content hashing over version labels |

## Running them

```bash
pip install ligase-bio marimo
marimo edit examples/notebooks/01_what_a_dataset_knows.py
```

Or without marimo at all, since a marimo notebook is importable Python:

```bash
python examples/notebooks/01_what_a_dataset_knows.py
```

The first three need **no data and no aligner** — they use the tiny fixtures
committed with the test suite, so they work on a fresh clone. The fourth runs
the real interpretation if the HIV-1 demo data is present, and explains itself
if it is not:

```bash
scripts/fetch_test_data.sh --project PRJNA1218827
```

## Why they are not interactive on this page

marimo can export a notebook that runs **in the browser**, through
Pyodide. Ligase cannot use that: it is a compiled Rust extension, and there is
no emscripten wheel for it — the published wheels are manylinux, macOS and
Windows. Building one would mean PyO3 and a bundled SQLite compiled for
emscripten, which is not a realistic target.

So these are rendered from real runs and meant to be downloaded and run
locally, where the engine actually works. The honest version of "interactive
docs" for a native extension is a notebook you can run, not an iframe that
cannot import the thing it documents.

## The full pipeline

The notebooks are deliberately small. For the reference pipeline end to end —
reads, alignment, variant calls, resistance — see
[`examples/hiv-resistance/pipeline.py`](https://github.com/Neoforge-Labs/ligase/blob/main/examples/hiv-resistance/pipeline.py)
and [running a pipeline](running-a-pipeline.md).
