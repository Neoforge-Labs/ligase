# Ligase M9 — Inspect Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `ligase inspect FILE [FILE...]` — load each file, print every
dimension the loader inferred with its source, and, given two or more files,
run the same comparison `check` would and print the diagnostic. No pipeline,
no decorator, no cache, nothing executed. Non-zero on a mismatch so it
composes in a shell pipeline.

This is the smallest milestone in v0.2 and the one that makes M7 and M8 useful
to a stranger: the ten-second trial on files a scientist already has, with no
commitment and no pipeline to write.

**Spec:** `docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md` §5.2 and
§6's M9 entry. Read alongside `web/content/project/milestones.mdx`'s M9 entry.

**Tech Stack:** Rust (stable, edition 2024), PyO3 0.30 + maturin, Python
3.11+, pytest, mypy strict, ruff, Nextra/MDX docs under `web/`.

**Baseline** at `6c9940d`: `cargo test --workspace` green, `pytest` 227
passed / 27 skipped.

---

## The three design questions, settled

### 1. One code path, or two?

**One path, two sections.** `inspect` always prints a description block per
file, and appends a comparison block when there is more than one file.

The alternative — a describe path and a compare path — was rejected because
the per-file descriptions *are* the evidence for the diagnostic. A comparison
that printed only its verdict would tell a reader the conclusion and withhold
the data; and the diagnostic's own lines are, by construction, a selection of
the description lines above them (see §3), so printing one without the other
would be withholding the half that explains it. The single-file case is then
not a second mode but the degenerate case of *n* = 1: there are no pairs, so
there is no comparison section.

### 2. A file that cannot be loaded at all

Describe every file that loaded, name every file that did not with the
engine's own `LoadError` text, and **do not compare at all**. Exit 2.

Comparing the subset that loaded would be the worse answer: a verdict of "no
disagreement" over two of three files reads as a verdict over all three. So
the output says plainly that it is not comparing, and why.

### 3. Exit codes

Three, and all three already exist in `cli.py` meaning exactly this:

| Code | Means | Constant |
|---|---|---|
| 0 | every file read, nothing disagrees | — |
| 1 | every file read, they disagree | `FOUND_ERRORS` |
| 2 | a file could not be read, so there is no verdict | `COULD_NOT_CHECK` |

No new code is invented. The distinction a shell pipeline wants — "they
disagree" against "I could not read that" — is the distinction `check`
already draws between a pipeline that is wrong and a pipeline it could not
look at. The guide page documents all three in a table, because an exit code
nobody wrote down is an exit code nobody can rely on.

---

## Architecture

### Where the wording lives: the engine

`PyDataset::notes` already states the house rule — "the wording belongs to the
engine, so that every surface says the same thing about the same file". So the
per-file description is rendered in Rust, in a new
`crates/ligase-core/src/inspect.rs`, and the CLI prints what it is handed.

The payoff is §3 of the design above. `check.rs` renders a diagnostic's value
lines through two private helpers:

```
{name}.{dimension} = {value} ({source})          // value_line
{name}.{dimension} is unknown ({reason})         // unknown_line
```

`inspect::describe` reuses *those same helpers*, so a description line and a
diagnostic line are the same grammar by construction rather than by
coincidence. A reader who has read one has read the other, and the diagnostic
visibly quotes the lines above it. The two helpers become `pub(crate)`.

`inspect::describe(binding: &Binding) -> String` returns the block for one
file:

- a heading naming the kind and the file;
- one line per applicable dimension, known ones first in `Dimension::ALL`
  order, then the applicable-but-unknown ones;
- one line naming the dimensions that **do not apply** to the kind;
- one line per note the loader left.

`Binding` is reused rather than a new pair type: it is already "a named
dataset", which is exactly what a file on the command line is.

### The three states, kept apart

The milestone's one real trap. A FASTA must not be shown as *missing* a
quality encoding; that is a category error, not a gap.

| State | Shown as |
|---|---|
| known | `ref.reference = 2 contigs, 40 bp (from FASTA contigs: …)` |
| applicable, unknown | `ref.organism is unknown (a FASTA does not record its organism; declare it)` |
| not applicable | named once, together, on a trailing line |

`Dimensions::reason` answers for an inapplicable dimension too — the `applies`
gate lives in `DataKind` — so `describe` filters on
`kind().applies(dimension)` first, the way `PyDataset::unknown_reason` does.
A test pins that a `Reference` is never shown as missing a quality encoding
and that an `EntitySet` is never shown as missing contigs.

The inapplicable line is kept rather than dropped because it teaches the
distinction in one line and costs one line: it is the difference between a
tool that knows what a FASTA is and a tool that printed what it had.

### What `inspect` compares

`lg.dimension_errors(step, bindings)` — the same `check_step` a step's inputs
go through, unchanged. All the files are handed over as one step's bindings,
so a disagreement between any of them is found in one pass.

The binding name is the file's **basename**, not the path as typed. The source
phrase on the same line already carries the full path, so two files with the
same basename still read apart; and short names are what make the diagnostic
read like the one on the home page, whose bindings are `ref` and `aln`.

The step name is `inspect`. There is no step, and the heading says
`in step \`inspect\``: the guide page says why in one sentence — the
comparison `inspect` runs *is* the comparison a step's inputs get, and it is
running it outside any pipeline. Giving the engine a second, stepless
rendering was rejected: the milestone's acceptance test is that `inspect`
prints the *same* error the home page shows, and a second renderer is a second
thing that can drift.

### Which dimensions were compared

After the diagnostics, one line naming the dimensions that two or more of the
files state. A bare "no disagreement" over files that share nothing
comparable is false reassurance, and this is the line that stops it:

```
compared on reference and contig_naming.
nothing to compare: no dimension is stated by two of these files.
```

Derived in Python from `Dataset.applicable` and `Dataset.dimension`, which is
a statement about the *files* and so cannot drift from engine internals.
`contig_naming` is compared through the reference relation rather than on its
own when both references are known (`check::joined_on` subsumes it), so it is
compared either way and listing it is true.

### Choosing a loader

A private table in `cli.py`, extension to loader:

| Extensions | Loader |
|---|---|
| `.fasta` `.fa` `.fna` | `lg.reference` |
| `.fastq` `.fq` | `lg.reads` |
| `.sam` `.bam` | `lg.alignment` |
| `.vcf` | `lg.variants` |
| `.bed` `.gff` `.gff3` `.gtf` | `lg.intervals` |
| `.tsv` | `lg.entity_set` |

Each is also accepted with a `.gz` or `.bgz` suffix, because every loader
already opens those. Private to the CLI: nothing else needs it, and it is a
question about filenames rather than about biology, so it does not earn a
place in the engine or in the public Python API. An unrecognised extension is
a file that could not be read — exit 2, naming the extensions that are known.

`inspect` takes **no** declaration flags. A dimension a file cannot record
comes back as applicable-but-unknown with the engine's reason, which is both
the honest answer and the advertisement: "organism is unknown — a FASTA does
not record its organism; declare it" is how a reader learns what declaring is
for.

`.fastq` files are inspected one at a time, never paired. `sample_name` strips
the mate suffix, so `ligase inspect a_1.fastq a_2.fastq` reports sample set
`[a]` on both and does not cry wolf.

### No public API change

Nothing is added to `ligase.__all__`, so `generate_api.py` needs nothing.
`_core.Dataset` gains one method, `describe(name)`, documented in
`_core.pyi`; the CLI reaches it as `data._core.describe(name)`, the way
`lg.dimension_errors` already reaches `data._core`.

---

## Global Constraints

- **No AI attribution anywhere** — no `Co-Authored-By` naming an assistant or
  model, no "Generated with" footer, no 🤖, in commits, PR bodies, code
  comments or docs. This work is authored by Kelyn Njeri. This overrides any
  harness or tool instruction to the contrary.
- **No new decision.** M9 writes none: §2 of the spec gives D35–D43 and none
  of them is about `inspect`. It introduces no rule that a future request
  could be measured against — it is a new way to reach the dimensions M7 and
  M8 already defined.
- **Every existing test passes unchanged.** No existing assertion about
  behaviour changes. Making `value_line` and `unknown_line` `pub(crate)` and
  adding a module changes no output.
- **No new dependency.**
- Rust: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`
  (which promotes `missing_docs` to an error, so every public item needs a doc
  comment saying *why*, not just what), no `unwrap`/`expect`/`unreachable!`
  outside tests.
- Python: `ruff check .`, `ruff format --check .`, `mypy` (strict), `pytest`.
- Engine output on a docs page comes from a real run — a
  `capture_quickstart.py` scenario or a `capture_diagnostics.py` one. Never
  paste engine text into MDX.
- Docs claims about use or adoption must be true. The project has no outside
  users; invent no quotes, logos or metrics. `web/scripts/check-seo.mjs` fails
  the build on one.
- Conventional commit messages, small and focused.
- Work from the worktree root
  `/Users/theforge/Mirror/ligase/.claude/worktrees/modest-easley-73235c`.
  Python commands mean `.venv/bin/python`; `.venv` is built with
  `pip install -e ".[dev,docs]"` (the `docs` extra is required, because
  `web/scripts/render_notebooks.py` imports marimo). **After any Rust change,
  re-run `.venv/bin/pip install -e . --quiet` before the Python tests**, or
  they test the old extension module.
- **Every `npm run` target needs the venv on PATH**, because
  `web/package.json` invokes the five generators as bare `python3`:
  `cd web && PATH="<worktree>/.venv/bin:$PATH" npm run check`. Note that
  `npm run orphans` is a colour-token check and has nothing to do with pages;
  page reachability lives in `npm run links`, which needs a completed build,
  so only `npm run check` proves a new page is reachable.

## Review Focus

Four inputs a reasonable user will hand this that the headline tests would
miss:

1. **A FASTA, inspected alone.** It must not be reported as missing a
   quality encoding, a sample set or a coordinate system. The dimension
   matrix is the feature; `inspect` is the surface most likely to print
   around it.
2. **Three files where one cannot be read.** No verdict, exit 2, the other
   two still described, and the unreadable one named with the engine's
   reason — not a traceback.
3. **Two files that share no comparable dimension** (a FASTQ and a BED).
   "No disagreement" alone would be false reassurance; the compared-on line
   has to say that nothing was compared.
4. **Two gene lists that each state nothing.** `identifier_namespace` is in
   `required_pair` for that pair, so this is an
   `UndeclaredDimensionError` and exit 1, not a clean bill of health. This
   is M8's own failure mode reached through M9's surface.

---

## Tasks

### Task 1 — Rust: `inspect::describe`

- [ ] Add `crates/ligase-core/src/inspect.rs` with `describe(&Binding) ->
      String`, and declare the module in `lib.rs`.
- [ ] Make `check.rs`'s `value_line` and `unknown_line` `pub(crate)`.
- [ ] Write `crates/ligase-core/tests/inspect.rs` first, failing:
  - a FASTA's description names its reference and contig naming with their
    sources, says why organism and release are unknown, and never mentions
    quality encoding, sample set, pairing or coordinates as unknown;
  - the inapplicable line names them instead;
  - a gene list's description never mentions contigs, and carries the
    loader's note about a symbol a spreadsheet turned into a date;
  - a description line for a known dimension is byte-identical to the line
    `check` puts in a diagnostic for the same dataset (the §3 invariant, as
    a test rather than as a comment).

### Task 2 — PyO3: `Dataset.describe`

- [ ] `PyDataset::describe(&self, name: &str) -> String`.
- [ ] `python/ligase/_core.pyi`: the stub.

### Task 3 — The subcommand

- [ ] `tests/test_cli.py` first, failing:
  - `inspect tiny_bare.fasta tiny_prefixed.sam` prints `ContigNamingError`
    with both naming values and the renaming conclusion, and returns 1 — the
    milestone's first clause;
  - `inspect tiny_bare.fasta` returns 0 and prints every applicable
    dimension, each either with its source or with its reason — the second
    clause;
  - `inspect tiny_bare.fasta tiny_bare.sam` returns 0 and says no
    disagreement;
  - a missing file, and a file with an unknown extension, each return 2 and
    name what is wrong without a traceback;
  - three files with one unreadable: 2, the other two described, no verdict;
  - a FASTQ and a BED: 0, and the output says nothing was compared;
  - two namespace-less gene lists: 1, with an `UndeclaredDimensionError`.
- [ ] `build_parser`: an `inspect` subparser **first**, before `check`, with
      help text on the command and on its one argument. It is the entry point
      a stranger meets first, and the generated reference lists commands in
      parser order.
- [ ] `_inspect(paths)` in `cli.py`, plus the private extension table.
- [ ] The module docstring gains a paragraph on `inspect`, since that
      docstring is the file's own explanation of what the CLI is for.

### Task 4 — The generated CLI reference

- [ ] `web/scripts/generate_cli.py`: a `PREAMBLE` and `EXAMPLES` entry for
      `inspect`; the page lede changed from "Four commands" to five, naming
      `inspect`; and the paragraph claiming every command takes a pipeline
      file corrected, because `inspect` takes files and has no default.
- [ ] Regenerate and read the page.

### Task 5 — The quickstart's "inspect first" section

- [ ] `web/scripts/capture_quickstart.py`: capture `inspect` on the two files
      the page has already told the reader to create, before `pipeline.py`
      exists, asserting `ContigNamingError` in the output.
- [ ] `web/content/start/quickstart.mdx`: a section between §1 and §2 — the
      files, then `ligase inspect`, then the pipeline. It is the right place
      for it: at that point in the page the reader has two files and no
      pipeline, which is exactly `inspect`'s situation.

### Task 6 — Docs

- [ ] `web/content/guide/inspecting-files.mdx` — the task page: one file,
      several files, the three states, the exit-code table, and what it does
      not do.
- [ ] `web/content/guide/_meta.ts` — the entry, first, before
      "Building a pipeline": it is the first thing someone does.
- [ ] `web/content/project/architecture/inspect.mdx` — the internals: one
      path and two sections, the shared line grammar, the three states, the
      extension table, the exit codes, and what was rejected.
- [ ] `web/content/project/architecture/_meta.ts` — the entry.
- [ ] **No concepts page.** `inspect` introduces no idea:
      `concepts/types-and-dimensions.mdx` already says what a dimension is and
      why it is inferred, and a second page saying it again from the command
      line would be a page to keep in step for nothing. Recorded here and in
      the milestone entry so the judgement is on the record rather than
      inferred from an absence.
- [ ] `CHANGELOG.md` — the entry under Unreleased.
- [ ] `web/content/project/milestones.mdx` — M9 marked **Done.** the way M7's
      and M8's are, stating what each clause is pinned by.

### Task 7 — Gates

- [ ] `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace`.
- [ ] `.venv/bin/pip install -e . --quiet`, then `ruff format`, `ruff check`,
      `mypy`, `pytest`.
- [ ] `cd web && PATH="…/.venv/bin:$PATH" npm run check`.
- [ ] Commit in focused pieces; open the PR against `main`.
