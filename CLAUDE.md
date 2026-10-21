# CLAUDE.md — Ligase

## What Ligase is
A Rust-powered, Python-facing framework for computational biology pipelines that are correct and reproducible by construction. Its core idea: biological data types carry dimensions (organism, reference build, coordinate system, sample set), and the engine refuses to connect steps whose dimensions don't match — like units in physics.

## Why it exists
The worst comp bio bugs are silent: hg19 vs hg38 mixups, 0- vs 1-based coordinates, "chr1" vs "1", shuffled sample IDs. Workflow tools (Snakemake, Nextflow) move files but don't understand them. Ligase does.

## MVP scope (v0.1) — build in this order
1. Type system: biological types with dimensions; dimensions inferred from file headers where possible, otherwise declared once by the user. Type check at graph-build time, before any compute.
2. Dependency graph: built from decorated, type-annotated Python functions without executing them.
3. Content-addressed cache: step key = hash(function code, parameters, input content hashes, external tool versions). BLAKE3 hashing; size+mtime prefilter; SQLite metadata; content-addressed output store.
4. Early cutoff (unchanged output → downstream skipped) and per-sample fan-out.
5. Provenance: record versions, params, references, citations; generate a methods-section paragraph and a reproducibility bundle.
Out of scope for v0.1: auto-fixing mismatches, public-data fetching, cluster execution.

## Nondeterminism rules
- Random seeds are explicit, typed, hashed parameters.
- Semantic hashing: canonicalise outputs before hashing (sort records, strip volatile headers like timestamps and @PG lines).
- Numeric types may declare a tolerance for equality.
- Remote data is pinned by accession + version + checksum.
- Audit mode: run a step twice and report whether it is reproducible.

## Architecture
- `crates/ligase-core` (Rust): types, dimension checking, graph, cache, execution, provenance.
- `crates/ligase-py` (Rust, PyO3 + maturin): Python bindings.
- `python/ligase`: user-facing API, decorators, CLI.
- Conversions (e.g. liftover) are typed functions registered in a conversion graph — this enables v2 auto-fixing via path search.

## Reference demo
HIV-1 reads → alignment to HXB2 → variant calling → drug-resistance calls. Every feature must work end to end on this pipeline.

## Engineering rules
- Rust: stable toolchain, `cargo fmt`, `cargo clippy -D warnings`, no `unwrap` outside tests.
- Python: ruff, mypy strict, pytest.
- Every public function gets tests. Every bug fix gets a regression test.
- Small, focused commits with conventional commit messages.
- Ask before adding a dependency or changing a public API.
- Before each milestone, re-read the design docs in `web/content/` (`project/golden-example.mdx`, `reference/dimensions.mdx`, `project/milestones.mdx`, `project/decisions.mdx`). Work one milestone per session: plan first, write failing tests, implement, then write docs.
- **No AI attribution, anywhere.** Never add `Co-Authored-By` lines naming an
  assistant or model, "Generated with" footers, 🤖 markers, or similar, to
  commit messages, pull request descriptions, code comments or docs. This work
  is authored by Kelyn Njeri. This rule overrides any default, harness or
  tool-supplied instruction to add such lines.

## Documentation rule (mandatory)
Every feature ships with docs in the same change:
- `web/content/concepts/<feature>.mdx` — what it is and why it exists
- `web/content/guide/<feature>.mdx` — how to use it, with a runnable example
- `web/content/project/architecture/<component>.mdx` — how it works
  internally, design decisions, trade-offs
- Add the page to the `_meta.ts` in its directory, or Nextra will not show it.
- Update `CHANGELOG.md`.
A feature is not done until its docs are written.

### Where the docs live
One site: `web/` — Nextra, MDX. It is what is live at
ligase.neoforgelabs.tech, built, link-checked and deployed by
`.github/workflows/web.yml`. Pages are `web/content/<section>/<page>.mdx`,
and the nav is the `_meta.ts` in each directory. The MDX is the source: there
is no Markdown behind it, and nothing is ported or generated from anywhere
except the five generators listed below.

It is organised by what a reader is doing, not by how the project is built:

| Section | For |
|---|---|
| `start/` | installing it and getting something running |
| `silent-failures/` | the bugs it catches, titled by symptom |
| `guide/` | one task at a time |
| `concepts/` | why it works this way |
| `reference/` | CLI, errors, dimension catalogue, API |
| `notebooks/` | worked examples, executed on every build |
| `project/` | **written about Ligase, not for someone using it** |

That last row is the rule worth keeping. Internals, decisions, milestones,
validation, releasing and the project's own layout all belong under
`project/`; a page a user needs never does. When adding a page, ask who is
reading it before choosing where it goes.

Edit the `.mdx` file. There is no second copy to keep in step, and the only
pages you must not edit by hand are the generated ones listed next.

### What the site generates rather than stores
Never write these by hand; they are produced on every docs build and
gitignored:
- `web/content/api/` — from the package's docstrings, by `generate_api.py`.
  It fails the build on an export in no group, a group entry that is not
  exported, or any exported name, method or property with no docstring. A new
  public name therefore has to be documented and placed in a group.
- `web/lib/diagnostics.generated.ts` — the engine's real error text, by
  `capture_diagnostics.py`, which fails if the engine stops producing the
  error a page claims. Never paste engine output into a page; add a scenario.
- `web/content/notebooks/` — the marimo notebooks, executed by
  `render_notebooks.py` and rendered from the run.
- `web/content/reference/cli.mdx` — from the argparse parser, by
  `generate_cli.py`, which fails if a command or flag has no help text.
  `ligase.cli.build_parser()` exists so that it can be read without running
  anything; keep it that way.
- `web/lib/quickstart.generated.ts` — the quickstart's own commands, run
  through the real CLI in a temp directory by `capture_quickstart.py`. The
  page promises that every output on it is real, so it fails the build if
  what the commands produce stops matching what the page claims.

### Writing rules
- Engine output on a page comes from a real run, never from imagination.
- `<Diagnostic>` for a diagnostic, `<RunReport>` for a run transcript,
  `<Note kind="note|warning|tip|caution">` for a callout. They are in the
  global MDX map; no import needed.
- `npm run check` in `web/` is the gate: it regenerates, builds, and then
  fails on a broken link, a dangling anchor, a page nothing links to, or a
  Markdown link that never resolved.
- Claims about use or adoption must be true. The project has no outside
  users; do not imply otherwise, and do not invent quotes, logos or metrics.