# Showcase Notebooks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add four marimo notebooks (`05`–`08`) that run complete pipelines during the docs build, so the site demonstrates caching, early cutoff, identifier mapping, the `representation` dimension, stochastic audit and claim-bound provenance from real runs.

**Architecture:** Each notebook is a standalone PEP 723 Python file in `examples/notebooks/`. Its first cell writes its own inputs into a `tempfile` directory — copied from committed fixtures or generated deterministically — and every `run`/`audit`/`methods` call passes an explicit `cache=` inside that directory. `web/scripts/render_notebooks.py` executes each one on every docs build and renders the real output; a new `tests/test_notebooks.py` asserts each notebook's headline engine outcomes independently of the docs build.

**Tech Stack:** Python ≥3.11, marimo ≥0.25, `ligase` (the local editable install at `.venv/`), pytest, Nextra/MDX for the site.

**Spec:** `docs/superpowers/specs/2026-11-04-ligase-showcase-notebooks-design.md`

## Global Constraints

- No new dependency, and no widening of the `docs` extra. The docs build installs `.[docs]` (marimo) only — **no RDKit, no aligner, no samtools, no bcftools**.
- Every step body is `lg.write`-bodied pure Python. **Never** a `lg.sh` body that a run would execute, and never a Python body named after a tool it is not.
- A pure-Python step may only return `Intervals`, `EntitySet`, `RankedList`, `MoleculeSet`, `MoleculeRanking`, `KnowledgeGraph` or `ResistanceReport`. `Alignment` (`.bam`) and `Variants` (`.vcf.gz`) are BGZF and cannot be written.
- Fan-in is fixed-arity: one positional dataset parameter per input. `list[Intervals]` is not a dataset parameter.
- `StepRun.sample` is `None` throughout these notebooks. No page may claim a per-sample label.
- Every `run`, `audit` and `methods` call passes `cache=<tempdir>/cache`. Never the default `./.ligase/`.
- No temp path, duration, timestamp or wall-clock value reaches a rendered page. The one permitted machine-dependent string is the methods paragraph's own `Analyses were run with Ligase <version> on <platform>.`
- Each notebook is self-contained and generates its own inputs inline. **Do not factor a shared helper module out of them** — they are published and downloaded one file at a time, and a file that needs a sibling to run is a broken example. Duplication between notebooks is intended.
- No AI attribution anywhere: no `Co-Authored-By` naming a model, no "Generated with" footer, no 🤖. Commits are authored by Kelyn Njeri.
- Conventional commit messages, one commit per task.

## Review Focus

Five ways this ships broken that no task's happy path would catch. Each line's test is added to the task named.

1. **A second docs build in the same checkout reuses the first build's cache**, so the page says "from cache" where it said "ran". The classic form of this bug is a forgotten `cache=`. → Task 1, `test_a_cohort_is_cold_on_every_run`.
2. **A temp path reaches the page.** `scrub()` rewrites only the repo root, and every notebook's inputs live under `/var/folders` or `/tmp`. → Task 5, `test_no_temp_paths_on_any_page`.
3. **A notebook writes into the repository when the renderer runs it**, because the renderer `os.chdir(ROOT)` and a missed `cache=` lands in `./.ligase/`. → Task 5, `test_rendering_writes_nothing_into_the_repo`.
4. **The renderer pairs cells to outputs by position** and raises `SystemExit` on a mismatch, which fails the whole docs build for one miscounted cell. → Task 5, `test_every_notebook_renders`.
5. **A new page nothing links to** fails `npm run orphans`, and the generated notebook index is the only thing that will link these four unless a guide does. → Task 5, `test_index_lists_every_notebook` plus the guide links in that task's steps.

---

## File Structure

**Create:**
- `examples/notebooks/05_a_cohort_and_what_changed.py` — cohort depth pipeline; cache, early cutoff, coordinates.
- `examples/notebooks/06_from_a_graph_to_a_shortlist.py` — knowledge graph → ranking → mapping → shortlist.
- `examples/notebooks/07_a_screen_and_its_seed.py` — molecule screen; `representation`, seeds, audit.
- `examples/notebooks/08_what_a_paper_needs.py` — claims, methods, bundle, verification.
- `tests/test_notebooks.py` — one test per notebook over its headline outcomes, plus the Review Focus guards.

**Modify:**
- `web/scripts/render_notebooks.py` — `ORDER` gains four entries (Tasks 1–4); `index()`'s hand-written prose is rewritten for eight notebooks (Task 5).
- `CHANGELOG.md` (Task 5).
- Guide and concept pages that gain a notebook link (Task 5).

Each notebook follows the existing four's shape: PEP 723 header, module docstring, `marimo.App(width="medium")`, `@app.cell` functions, `mo.md(...)` prose cells, `if __name__ == "__main__": app.run()`.

---

### Task 1: Notebook 05 — a cohort, and what changed

**Files:**
- Create: `examples/notebooks/05_a_cohort_and_what_changed.py`
- Create: `tests/test_notebooks.py`
- Modify: `web/scripts/render_notebooks.py` (`ORDER`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: the pattern Tasks 2–4 follow — a `_inputs()` cell returning `(dir, …)` from `tempfile.mkdtemp()`, a `cache` path under it, and prose cells that interpolate real values. `tests/test_notebooks.py` gains the module-loading helper `run_notebook(stem: str) -> tuple[list[object], str]` that Tasks 2–4 reuse.

The pipeline, verified by spike:

```python
@lg.step(coordinates="0-based half-open")
def depth(aln: lg.Alignment, regions: lg.Intervals) -> lg.Intervals: ...

@lg.step
def merge(a: lg.Intervals, b: lg.Intervals, c: lg.Intervals) -> lg.Intervals: ...
```

Inputs, generated into a temp directory: a 2-contig FASTA (`chr1` 400 bp, `chr2` 300 bp) from `random.Random(11)`; a GFF with two genes per contig; one SAM per sample for `sample_a`, `sample_b`, `sample_c`, each with 30 reads from `random.Random(100 + i)`. **The `@SQ` `M5` must be `hashlib.md5(sequence.encode()).hexdigest()` of that contig's sequence** or the reference check fails. Regions are GFF, not BED, because GFF is 1-based closed as SAM is.

- [ ] **Step 1: Write the failing test**

In `tests/test_notebooks.py`:

```python
def test_a_cohort_shows_cache_then_cutoff_then_propagation():
    outputs, printed = run_notebook("05_a_cohort_and_what_changed")
    page = printed + "".join(str(o) for o in outputs)
    assert "4 steps: 4 ran." in page
    assert "4 steps: 4 from cache." in page
    assert "4 steps: 1 ran, 2 from cache, 1 skipped." in page
    assert "4 steps: 2 ran, 2 from cache." in page


def test_a_cohort_refuses_the_bed():
    outputs, printed = run_notebook("05_a_cohort_and_what_changed")
    page = printed + "".join(str(o) for o in outputs)
    assert "DimensionError in step `depth`" in page
    assert "1-based closed" in page and "0-based half-open" in page


def test_a_cohort_refuses_prefixed_contigs():
    outputs, printed = run_notebook("05_a_cohort_and_what_changed")
    page = printed + "".join(str(o) for o in outputs)
    assert "contig naming" in page.lower()


def test_a_cohort_is_cold_on_every_run():
    """Review Focus 1: a second run of the notebook must still report `4 ran`."""
    for _ in range(2):
        outputs, printed = run_notebook("05_a_cohort_and_what_changed")
        assert "4 steps: 4 ran." in printed + "".join(str(o) for o in outputs)
```

`run_notebook(stem)` loads `examples/notebooks/<stem>.py` by `importlib.util.spec_from_file_location`, calls `module.app.run()` under `redirect_stdout`, and returns `(list(outputs), printed)` — the same mechanism as `render_notebooks.py:run`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_notebooks.py -v`
Expected: FAIL — the notebook file does not exist.

- [ ] **Step 3: Write the notebook**

Cells, in order:
1. imports and `tempfile.mkdtemp()`, returning the directory and the generator helpers.
2. `mo.md` — the title `# 5 — A cohort, and what changed`, and what the pipeline is.
3. generate the FASTA, GFF and three SAMs; show the cohort's sample names.
4. the two `@lg.step` definitions above.
5. build the pipeline (`lg.Pipeline(merge(*[depth(a, regions) for a in alns]))`); show `pipeline.steps` and `pipeline.errors()`.
6. `mo.md` — three `depth` nodes and one `merge` from a three-sample cohort, and why fan-in is written out by hand.
7. run 1, cold: print `report.summary` and each step's outcome.
8. reorder `sample_a`'s records; run 2; print the summary.
9. `mo.md` — semantic hashing: the bytes changed, the canonical form did not.
10. rewrite one read's bases in place; run 3; print the summary and outcomes.
11. `mo.md` — early cutoff: `depth` reran, wrote identical bytes, `merge` was `skipped`.
12. move one read out of every region; run 4; print the summary.
13. the two refusals, each a second pipeline over the same alignment, each printing `pipeline.errors()`: the regions as a **BED** (coordinates), and the regions as a GFF whose contigs are named `chr1`/`chr2` against the unprefixed reference (**contig naming**). Both are one-line edits to the generated files.
14. `mo.md` — closing: coordinates and contig names are semantics, which is why GFF joined where BED could not and why `chr1` is not `1`.

Print derived values (`report.summary`, `step.step`, `step.outcome`) — **never `repr(report)`**, which carries `duration_ms`. Never print a path from the temp directory.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `.venv/bin/python -m pytest tests/test_notebooks.py -v`
Expected: PASS, 4 tests.

- [ ] **Step 5: Verify it runs as a plain script and renders**

Run: `.venv/bin/python examples/notebooks/05_a_cohort_and_what_changed.py`
Expected: exit 0.

Add to `ORDER` in `web/scripts/render_notebooks.py`, after the `04` entry:

```python
(
    "05_a_cohort_and_what_changed",
    "A cohort, and what changed",
    "Caching, early cutoff, and coordinates that do not meet.",
),
```

Run: `cd web && ../.venv/bin/python scripts/render_notebooks.py`
Expected: prints `ran and rendered 05_a_cohort_and_what_changed`, and `rendered 5 notebooks from real runs`.

- [ ] **Step 6: Commit**

```bash
git add examples/notebooks/05_a_cohort_and_what_changed.py tests/test_notebooks.py web/scripts/render_notebooks.py
git commit -m "docs(notebooks): a cohort, and what changed"
```

---

### Task 2: Notebook 06 — from a graph to a shortlist

**Files:**
- Create: `examples/notebooks/06_from_a_graph_to_a_shortlist.py`
- Modify: `tests/test_notebooks.py`
- Modify: `web/scripts/render_notebooks.py` (`ORDER`)

**Interfaces:**
- Consumes: `run_notebook` from Task 1.
- Produces: nothing later tasks depend on.

The mapping call, whose signature is fixed:

```python
lg.map_ranked_identifiers(
    ranking, table, column="symbol", to="HGNC symbol",
    id_column="id", table_id_column="id",
)  # -> RankedList
```

The mapping table is loaded with `lg.entity_set(...)`, not a plain file. It is generated **Ensembl→HGNC** with two ENSGs landing on one symbol, so the engine reports a collision and drops both rows with a note. The committed `map_hgnc_to_ensembl.tsv` is the wrong direction and its ambiguity is one-to-many; do not use it.

- [ ] **Step 1: Write the failing test**

```python
def test_a_shortlist_maps_and_reports_the_collision():
    outputs, printed = run_notebook("06_from_a_graph_to_a_shortlist")
    page = printed + "".join(str(o) for o in outputs)
    assert "ran" in page
    assert "collide" in page.lower()


def test_a_shortlist_refuses_a_namespace_mismatch():
    outputs, printed = run_notebook("06_from_a_graph_to_a_shortlist")
    page = printed + "".join(str(o) for o in outputs)
    assert "identifier_namespace" in page


def test_a_ranking_reruns_when_reordered_and_a_set_does_not():
    """D41, the claim this notebook exists to show."""
    outputs, printed = run_notebook("06_from_a_graph_to_a_shortlist")
    page = printed + "".join(str(o) for o in outputs)
    assert "from cache" in page and "ran" in page
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_notebooks.py -k shortlist -v`
Expected: FAIL — file missing.

- [ ] **Step 3: Write the notebook**

Inputs, into a temp directory: a knowledge graph JSON in the shape of `crates/ligase-core/tests/fixtures/graph_targets.json` — nodes with `id` (ENSG), `type`, optional `ids`, `sources`; edges with `source`, `target`, `type`, `sources`, `score` — generated deterministically with enough nodes that the ranking is not readable at a glance; a DEG ranking TSV in HGNC symbols; the Ensembl→HGNC map described above.

Loaders: `lg.knowledge_graph(path, organism="Homo sapiens", identifier_namespace="Ensembl gene", subject=..., release=...)`; `lg.ranked_list(path, organism="Homo sapiens", identifier_namespace="HGNC symbol")`; `lg.entity_set(map_path, organism="Homo sapiens", identifier_namespace="Ensembl gene")`.

Steps: `score(graph: lg.KnowledgeGraph) -> lg.RankedList` (rank nodes by summed edge score, writing `id` and `score` columns, best first); then `lg.map_ranked_identifiers(...)`; then `shortlist(ranking: lg.RankedList, degs: lg.RankedList) -> lg.EntitySet` (the identifiers in both, as a set).

What the cells show: the graph's inferred `source_set`, and that listing the same sources in another order is the same graph; the namespace mismatch refused before compute (score the graph, then hand the Ensembl-space ranking straight to a step expecting HGNC); the mapping's collision note; and D41 — reorder the DEG ranking's rows and rerun (steps rerun), reorder the shortlist set's rows and rerun (cached).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `.venv/bin/python -m pytest tests/test_notebooks.py -k shortlist -v`
Expected: PASS.

Then tighten: replace the loose assertions with the exact `summary` strings the notebook really produced, read from the test's own output. A test that would pass against the wrong engine behaviour is not a test.

- [ ] **Step 5: Render it**

Add to `ORDER`:

```python
(
    "06_from_a_graph_to_a_shortlist",
    "From a graph to a shortlist",
    "Identifier spaces, a collision, and why a ranking is not a set.",
),
```

Run: `.venv/bin/python examples/notebooks/06_from_a_graph_to_a_shortlist.py` → exit 0.
Run: `cd web && ../.venv/bin/python scripts/render_notebooks.py` → `rendered 6 notebooks`.

- [ ] **Step 6: Commit**

```bash
git add examples/notebooks/06_from_a_graph_to_a_shortlist.py tests/test_notebooks.py web/scripts/render_notebooks.py
git commit -m "docs(notebooks): from a graph to a shortlist"
```

---

### Task 3: Notebook 07 — a screen, and its seed

**Files:**
- Create: `examples/notebooks/07_a_screen_and_its_seed.py`
- Modify: `tests/test_notebooks.py`
- Modify: `web/scripts/render_notebooks.py` (`ORDER`)

**Interfaces:**
- Consumes: `run_notebook` from Task 1.
- Produces: nothing later tasks depend on.

Inputs: the committed molecule fixtures, **copied unmodified** into the temp directory — `molecules_library.tsv`, `molecules_hits_salts.tsv`, `molecules_salt_stripped_anion.tsv`, `molecules_ranked.tsv` from `crates/ligase-core/tests/fixtures/`. Do not generate molecules: an invented InChIKey is an anti-goal.

Steps:

```python
@lg.step(stochastic=True, seed="seed")
def dock(library: lg.MoleculeSet, *, seed: int) -> lg.MoleculeRanking: ...

@lg.step
def top(ranking: lg.MoleculeRanking, *, k: int) -> lg.MoleculeSet: ...
```

The `dock` body must write the `#ligase representation: ...` header line itself, copied from its input's declaration, or the output has no representation and the next step refuses it.

The audit contrast is **`reproducible` for the declared, seeded step against `deterministic` for the same body with nothing declared** — D40's actual claim. Do **not** attempt a `not reproducible` verdict: it needs genuine randomness, it flakes, and a flaky cell fails the docs build.

- [ ] **Step 1: Write the failing test**

```python
def test_a_screen_refuses_a_representation_mismatch():
    outputs, printed = run_notebook("07_a_screen_and_its_seed")
    page = printed + "".join(str(o) for o in outputs)
    assert "representation" in page
    assert "salt-stripped" in page


def test_a_screen_is_reproducible_and_the_undeclared_body_is_deterministic():
    outputs, printed = run_notebook("07_a_screen_and_its_seed")
    page = printed + "".join(str(o) for o in outputs)
    assert "reproducible" in page
    assert "deterministic" in page


def test_a_screen_reruns_when_the_seed_changes():
    outputs, printed = run_notebook("07_a_screen_and_its_seed")
    page = printed + "".join(str(o) for o in outputs)
    assert "ran" in page and "from cache" in page
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_notebooks.py -k screen -v`
Expected: FAIL — file missing.

- [ ] **Step 3: Write the notebook**

Cells show: the library's declared `representation`, read off the file; the refusal — the salt-stripped library against the as-given hits set — with `lg.chem.standardize` **named** as the fix and the `chem` extra named as what it needs, and **not called**; the seeded run, then the same run again from cache, then a changed seed rerunning; `pipeline.audit("dock", cache=cache)` reporting `reproducible`; and a second pipeline whose body is the same with nothing declared, audited `deterministic`, with the prose explaining that a seeded step is never `deterministic` however identical its bytes.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `.venv/bin/python -m pytest tests/test_notebooks.py -k screen -v`
Expected: PASS. Then tighten the assertions to the exact verdict strings and summaries produced, as in Task 2 Step 4.

- [ ] **Step 5: Render it**

Add to `ORDER`:

```python
(
    "07_a_screen_and_its_seed",
    "A screen, and its seed",
    "What a name in a molecule table means, and what a seed is worth.",
),
```

Run the file directly → exit 0. Run the renderer → `rendered 7 notebooks`.

- [ ] **Step 6: Commit**

```bash
git add examples/notebooks/07_a_screen_and_its_seed.py tests/test_notebooks.py web/scripts/render_notebooks.py
git commit -m "docs(notebooks): a screen, and its seed"
```

---

### Task 4: Notebook 08 — what a paper needs

**Files:**
- Create: `examples/notebooks/08_what_a_paper_needs.py`
- Modify: `tests/test_notebooks.py`
- Modify: `web/scripts/render_notebooks.py` (`ORDER`)

**Interfaces:**
- Consumes: `run_notebook` from Task 1.
- Produces: nothing.

It rebuilds a compact version of `06`'s pipeline from scratch — notebooks are executed independently and inherit no state.

The API, with the values the spec pins:

```python
claim = lg.Claim(
    text="...",                      # the sentence a paper would carry
    step="shortlist",
    value=<the number>,              # or omitted for a non-numeric claim
    at=lg.cell(column="score", row=0),
    tolerance=<float>,               # where the value is numeric
)
pipeline = lg.Pipeline(output, claims=[claim])
m = pipeline.methods(cache=cache)                     # -> Methods
m = pipeline.methods(bundle=str(d / "bundle"), cache=cache)
m = pipeline.methods(verify=str(d / "bundle"), cache=cache)
```

`Methods.text` is rendered **whole**, platform line included — Kelyn's decision, recorded in the spec. Do not trim it and do not scrub it.

This task is also where the spec's **numeric tolerance** goal is met: the bound numeric claim carries `tolerance=`, and the notebook's prose says what it is for — a float that is equal for the purposes of the claim without being equal byte for byte. Pick a tolerance loose enough to hold and tight enough to mean something against the value being claimed.

- [ ] **Step 1: Write the failing test**

```python
def test_a_paper_binds_its_claims():
    outputs, printed = run_notebook("08_what_a_paper_needs")
    page = printed + "".join(str(o) for o in outputs)
    assert "Analyses were run with Ligase" in page


def test_a_paper_refuses_an_unbacked_claim():
    outputs, printed = run_notebook("08_what_a_paper_needs")
    page = printed + "".join(str(o) for o in outputs)
    assert "unbacked" in page.lower() or "not backed" in page.lower()


def test_a_paper_writes_a_bundle_and_verifies_it():
    outputs, printed = run_notebook("08_what_a_paper_needs")
    page = printed + "".join(str(o) for o in outputs)
    assert "identical" in page.lower()
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_notebooks.py -k paper -v`
Expected: FAIL — file missing.

- [ ] **Step 3: Write the notebook**

Cells show, in order: the pipeline and its result; a claim bound to a real cell of that result, with `accounted_for` true; the full methods paragraph, its `tools` and its `citations`; the same claim rewritten so the number no longer matches, and `methods()` reporting it in `unbacked` — with prose saying this is what makes a claim enforceable in CI; `methods(bundle=...)` and the relative file list it wrote; and `methods(verify=...)` reporting `reproduced` and its one-line summary.

The second, broken claim must be a **separate `Pipeline`** over the same output, so the good run and the refusal both appear on the page.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `.venv/bin/python -m pytest tests/test_notebooks.py -k paper -v`
Expected: PASS. Then tighten to the exact strings produced, as in Task 2 Step 4 — except the platform line, which must stay matched loosely (`"Analyses were run with Ligase"`) because its tail differs per machine.

- [ ] **Step 5: Render it**

Add to `ORDER`:

```python
(
    "08_what_a_paper_needs",
    "What a paper needs",
    "A claim bound to a number, and a bundle that reproduces it.",
),
```

Run the file directly → exit 0. Run the renderer → `rendered 8 notebooks`.

- [ ] **Step 6: Commit**

```bash
git add examples/notebooks/08_what_a_paper_needs.py tests/test_notebooks.py web/scripts/render_notebooks.py
git commit -m "docs(notebooks): what a paper needs"
```

---

### Task 5: The section, the links, and the gate

**Files:**
- Modify: `web/scripts/render_notebooks.py` (`index()`)
- Modify: `tests/test_notebooks.py`
- Modify: `CHANGELOG.md`
- Modify: the guide and concept pages that gain a notebook link

**Interfaces:**
- Consumes: all four notebooks and their `ORDER` entries.
- Produces: the finished section.

- [ ] **Step 1: Write the failing tests**

The four Review Focus guards:

`render_notebooks.py` lives in `web/scripts/`, which is not importable by
default. At the top of `tests/test_notebooks.py`, add
`sys.path.insert(0, str(ROOT / "web/scripts"))` and define the two module
constants the tests below use: `ROOT = Path(__file__).resolve().parents[1]`
and `WEB = ROOT / "web"`.

```python
def test_every_notebook_renders():
    """Review Focus 4: cell count must equal output count for all eight."""
    from render_notebooks import ORDER, NOTEBOOKS, cell_sources, run
    for stem, _, _ in ORDER:
        sources = cell_sources(NOTEBOOKS / f"{stem}.py")
        outputs, _ = run(NOTEBOOKS / f"{stem}.py")
        assert len(sources) == len(outputs), stem


def test_no_temp_paths_on_any_page():
    """Review Focus 2: scrub() only rewrites the repo root."""
    for page in (WEB / "content/notebooks").glob("*.mdx"):
        text = page.read_text()
        assert "/var/folders" not in text
        assert "/tmp/" not in text


def test_rendering_writes_nothing_into_the_repo():
    """Review Focus 3: a missed cache= lands in ./.ligase/."""
    import render_notebooks

    def entries() -> set[Path]:
        root = ROOT / ".ligase"
        return set(root.rglob("*")) if root.exists() else set()

    before = entries()
    render_notebooks.main()
    assert entries() == before


def test_index_lists_every_notebook():
    """Review Focus 5: npm run orphans fails on a page nothing links to."""
    from render_notebooks import ORDER, slug
    index = (WEB / "content/notebooks/index.mdx").read_text()
    for stem, _, _ in ORDER:
        assert f"/notebooks/{slug(stem)}/" in index
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_notebooks.py -v`
Expected: FAIL — `test_index_lists_every_notebook`, because `index()` is still written for four.

- [ ] **Step 3: Rewrite `index()` for eight notebooks**

In `web/scripts/render_notebooks.py`, the prose inside `index()` is hand-written and says "Four marimo notebooks", "the first three need **no data and no aligner**", and "The fourth runs the real interpretation". Rewrite it to describe eight in two groups: `01`–`04`, which make one point each, and `05`–`08`, which run complete pipelines. Keep the generated table (it already derives from `ORDER`), keep the "Why they are not interactive" section unchanged, and keep the `description` frontmatter honest about the count.

State in that prose that `05`–`08` need no data, no aligner and no optional extras — they generate their inputs or copy committed fixtures — which is the claim a reader most needs and is true of all four.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `.venv/bin/python -m pytest tests/test_notebooks.py -v`
Expected: PASS, all tests.

- [ ] **Step 5: Link the notebooks from the pages whose claims they demonstrate**

One link each, in prose, from: `web/content/concepts/semantic-hashing.mdx` and `web/content/guide/running-a-pipeline.mdx` → `05`; `web/content/guide/mapping-identifiers.mdx` and `web/content/guide/ranked-lists.mdx` → `06`; `web/content/guide/molecules.mdx` and `web/content/concepts/stochastic-steps.mdx` (or whichever page carries D40) → `07`; `web/content/guide/provenance.mdx` → `08`. Confirm each target page exists before linking; `npm run links` fails on a dangling one.

- [ ] **Step 6: Update `CHANGELOG.md`**

One entry under the unreleased heading, in the file's existing voice: four notebooks that run complete pipelines in the docs build, and what they demonstrate.

- [ ] **Step 7: Run the full gate**

Run: `cd web && npm run check`
Expected: PASS — it regenerates everything, builds, then fails on a broken link, a dangling anchor, an orphan page or an unresolved Markdown link. This is the real acceptance test for the whole plan.

Run: `.venv/bin/python -m pytest tests/ -q` and `.venv/bin/python -m ruff check . && .venv/bin/python -m mypy python/ligase`
Expected: PASS. Notebooks are excluded from mypy and exempt from ruff `ANN`/`B018`/`D103`, so only `tests/test_notebooks.py` must satisfy the strict settings.

- [ ] **Step 8: Commit**

```bash
git add web/scripts/render_notebooks.py tests/test_notebooks.py CHANGELOG.md web/content
git commit -m "docs(notebooks): eight notebooks, four of them whole pipelines"
```
