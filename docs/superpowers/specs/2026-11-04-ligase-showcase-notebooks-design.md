# Showcase notebooks — design

Four new marimo notebooks (`05`–`08`) that run complete, advanced pipelines
inside the docs build, so the site shows the engine's full reach rather than
one idea per page.

Date: 2026-11-04. Author: Kelyn Njeri.

## Intent, and the why

The site has four notebooks. Each makes a single point — what a dataset
knows, an error before compute, a graph that has not run, why the rules are
hashed — and each is deliberately small. None of them *runs a pipeline*.
`01`–`03` build graphs and check them; `04` reads rule metadata and runs the
real interpretation only if the HIV-1 demo data happens to be present.

That leaves most of the engine undemonstrated. Nothing on the site shows a
cache hit, early cutoff, a `RunReport`, an audit verdict, a knowledge graph,
an identifier mapping, the `representation` dimension, a bound claim, a
generated methods paragraph, or a bundle replayed and verified. Those are the
strengths, and a reader currently has to take the guide pages' word for them.

These four notebooks are for the docs, to show what Ligase can do. A reader
should close one of these pages thinking *that is my pipeline, done better*,
and should be unable to doubt the numbers, because every number on the page
was computed during the build that produced the page.

## Goals

1. Four notebooks, each a complete pipeline that **executes** during the
   docs build, with real `RunReport`s, audit verdicts and methods output.
2. Between them, cover: per-sample fan-out; reference frame, contig naming
   and coordinate dimensions; semantic hashing and canonicalisation;
   content-addressed caching and early cutoff; identifier namespaces and
   many-to-one mapping; ranked-list-versus-set identity (D41); knowledge
   graphs and `source_set`; the `representation` dimension; stochastic steps
   with typed seeds and tolerance; audit mode; claim binding; the methods
   paragraph and its citations; the reproducibility bundle and its
   verification.
3. Run on a **fresh clone** with no downloads, no external tools, and no
   optional extras — i.e. on exactly what `pip install -e ".[docs]"` gives
   the docs workflow.
4. Deterministic across builds: the same input bytes, the same hashes, the
   same rendered page.
5. Hosted and shareable by the mechanism that already exists — rendered to
   `web/content/notebooks/` and live under `/notebooks/` — plus downloadable
   and runnable via each file's PEP 723 header.

## Non-goals

- **Pinned remotes and recorded-query replay.** Needs network state or a
  seeded bundle fixture that does not exist yet. Stays in the guide pages.
- **Cluster execution on a shared store** (`Pipeline.run(worker=...)`). Needs
  a store and more than one machine to mean anything.
- **Conversions and liftover.** Worth a notebook later; nothing to convert in
  the available data.
- **`lg.chem.standardize`.** Needs RDKit, which the docs build does not
  install. Named on the page as the fix for a mismatch, never called.
- **Interactive, in-browser notebooks.** Impossible for a native extension;
  the notebooks index already explains why and that explanation stands.
- **Any engine change.** If a notebook cannot show something with today's
  API, the notebook gives it up, not the other way round.

## Anti-goals

Things that would make this a failure even if every page built:

- **A fake tool.** A Python body named `align` that pretends to be minimap2
  would be a lie on a page whose whole claim is that it does not lie. Every
  step body does the work it says it does.
- **Invented data presented as real.** Generated inputs are described as
  generated, in the cell that generates them.
- **A typed-in number.** Anything a reader could mistake for engine output
  must *be* engine output.
- **A new dependency**, or widening the `docs` extra, to make a page work.
- **Non-determinism on the page** — a duration, a temp path, a timestamp —
  which would make the site's diff noisy and the hashes meaningless.

## Constraints discovered

1. **Step bodies can be pure Python.** `lg.write(produce)` takes a callable
   that writes the output file, so a pipeline runs with no external tool.
   `web/scripts/capture_ranked.py` already does this; it is the precedent and
   the pattern to follow.
2. **The docs build installs `.[docs]` only** (`.github/workflows/web.yml`),
   which is marimo. No RDKit, so no `lg.chem.standardize`; no aligner, no
   samtools, no bcftools.
3. **The renderer executes every notebook and fails the build on an
   exception** (`web/scripts/render_notebooks.py`). A notebook is therefore a
   test. It also pairs parsed cells to run outputs **by position**, so cell
   count must match output count.
4. **The default cache is `./.ligase/`, relative to the working directory**,
   and the renderer `os.chdir(ROOT)`. A notebook that ran with the default
   cache would write into the repository and — worse — would find its own
   previous build's cache, so the second build's page would say "cached"
   where the first said "ran". Every notebook therefore passes an explicit
   `cache=` inside its own temp directory.
5. **`scrub()` only rewrites the repo root** out of rendered output. A temp
   directory path would survive it and change on every build, so no temp
   path may reach the page.
6. **`RunReport` and `StepRun` carry `duration_ms`.** Whether `repr()`
   exposes it must be checked; if it does, the notebooks print derived
   summaries (`len(report.ran)`, step names, outcomes) rather than raw
   `repr()`.
7. **Coordinates are required and compared whenever `Intervals` meet
   anything** (`crates/ligase-core/src/check.rs:452`), and there is no
   registered coordinate conversion. `coordinates=` on `@lg.step` declares
   what the step **writes**, not how its inputs reconcile
   (`python/ligase/graph.py:1158`), and `lg.intervals` derives coordinates
   from the file format with no override. So a step cannot take a BED and a
   SAM together, by design.
8. **A pure-Python step can only produce BED, TSV, JSON or FASTA.**
   `output_suffix` (`crates/ligase-core/src/exec.rs:750`) gives `Alignment`
   a `.bam` and `Variants` a `.vcf.gz`, both BGZF, which a `lg.write` body
   cannot produce. Two consequences, both verified by spike:
   - Canonicalisation of an alignment is still demonstrable — on an *input*.
   - **No run-report sample labels.** `Pipeline._sample` reads the sample
     off the node's own dataset, and none of the writable kinds carries
     `sample_set`, so `StepRun.sample` is `None` throughout. Fan-out is
     shown structurally — N branches, N entries in `outputs()`, and only
     the changed sample's branch rerunning — never by a label.
9. **Fan-in is fixed-arity.** `_kind_of` recognises only a bare kind, so
   `list[Intervals]` is not a dataset parameter. A merge takes one
   positional parameter per sample, as `tests/test_pipeline.py:220` does.
10. **Audit has three verdicts**: `deterministic`, `reproducible`,
   `not reproducible`, and a step with a seed is never `deterministic`
   (`python/ligase/audit.py`). `not reproducible` requires a body that
   genuinely draws on randomness, which flakes —
   `web/scripts/capture_stochastic.py` retries four times for exactly that
   reason, and a flaky cell is a failed docs build.
11. **`Methods.text` opens with the engine version and the build machine's
   platform** — `Analyses were run with Ligase 0.4.0+dirty on
   macos-aarch64.` (`crates/ligase-core/src/prov/methods.rs:392`). `scrub()`
   does not touch it. See the open question below.
12. **The rendered pages are generated and gitignored**, so a changing page
   is not a noisy repository diff. The determinism goal stands on its own
   terms — a hash that moves between builds is a hash that means nothing —
   not on diff hygiene.
13. **The nav and index are generated** from the `ORDER` tuple in
   `render_notebooks.py`. Adding a notebook means adding to `ORDER`; the
   index's prose ("Four marimo notebooks", "the first three need no data",
   "the fourth runs the real interpretation") is hand-written inside that
   script's `index()` and must be rewritten for eight.

## Approaches

Three were considered, after it was established that pipelines can really run
in the build.

**One flagship notebook.** A single end-to-end pipeline from knowledge graph
to methods paragraph. Rejected: it would run to several thousand words on one
page, any single break takes the whole showcase down, and one domain cannot
carry genomics and chemistry at once.

**Three use-case pipelines (`05`–`07`), publication folded into each.**
Rejected as the final shape: claim binding, the methods paragraph, the bundle
and verification are a distinct strength, and repeating them three times as a
tail on each page reads worse than giving them one page where they are the
subject.

**Three use-case pipelines plus a publication notebook (`05`–`08`).**
Chosen, by Kelyn. Each of the first three is a pipeline a reader recognises as
their own work, carrying a cluster of strengths; the fourth takes one result
through claims, methods, bundle and verification. Each page is readable on its
own, a break is isolated to one page, and between them the coverage in Goal 2
is complete.

## The four notebooks

Numbering continues the existing series. Each is `examples/notebooks/NN_*.py`
with the PEP 723 header the others have, authored in marimo, and added to
`ORDER`.

### 05 — A cohort, from alignments to calls

*Strengths: per-sample fan-out, reference frame, contig naming, coordinate
semantics, semantic hashing and canonicalisation, the content-addressed cache,
early cutoff, the `RunReport`.*

A cohort coverage-and-calls pipeline. It starts from **alignments**, not
reads, because alignment needs an aligner the docs build does not have —
and starting there is honest about where the Python work can really begin.

- Inputs: a reference FASTA, a **GFF** of regions, and one SAM per sample for
  a small cohort, written into a temp directory by the first cell. Content is
  derived from `crates/ligase-core/tests/fixtures/` (`tiny_bare.gff`,
  `tiny_bare.fasta`, `tiny_*.sam`), extended deterministically so the cohort
  has more than one sample and the records arrive in a realistic unsorted
  order.
- The regions are a GFF rather than a BED **because the engine would refuse
  the BED**, and that refusal is the notebook's second exhibit rather than an
  obstacle: GFF is 1-based closed, as SAM is, so the two can be joined; BED is
  0-based half-open, so they cannot. The depth step declares
  `coordinates="0-based half-open"` for the BED-style table it writes.
- Steps, both `lg.write`-bodied: compute depth over the regions per sample;
  merge the per-sample depths into one cohort table. There is **no sort step**
  — a pure-Python body cannot produce a BAM (Constraint 8), and the
  canonicalisation point does not need one.
- What it shows, by running — **four runs, every outcome verified by spike**:

  | Run | What changed in `sample_a.sam` | `RunReport` |
  |---|---|---|
  | 1 | nothing; cold cache | `4 steps: 4 ran.` |
  | 2 | records reordered — different bytes, same canonical form | `4 steps: 4 from cache.` |
  | 3 | one read's bases rewritten, same contig and position | `4 steps: 1 ran, 2 from cache, 1 skipped.` |
  | 4 | one read moved out of every region | `4 steps: 2 ran, 2 from cache.` |

  Run 2 is semantic hashing: the file changed and the engine knew it had not.
  Run 3 is **early cutoff** — a real change the depth counts cannot see, so
  `depth` reran, produced identical bytes, and `merge` was `skipped`. Run 4 is
  the same change made consequential, so it propagates. Runs 3 and 4 both show
  the other two samples `cached`, which is what fan-out independence looks
  like without a label to point at.
- The refusal: the same regions as a **BED**, rejected before compute on
  coordinates. This is a better exhibit than the "wrong build" the first draft
  named, which does not exist — `build=` is a display label, and an interval
  file's `reference` comes from its own contig names, so there is nothing to
  declare wrongly.

### 06 — From a knowledge graph to a shortlist

*Strengths: knowledge graphs, `source_set`, `subject`, `release`, identifier
namespaces, many-to-one mapping, ranked-versus-set identity (D41), fan-out
over identifiers.*

Target triage. A knowledge graph of gene–gene associations is scored by its
own structure, the ranking is mapped out of Ensembl identifiers into HGNC
symbols, and intersected with a differential-expression ranking to leave a
shortlist.

- Inputs: a knowledge graph generated deterministically in the shape of
  `fixtures/graph_targets.json` (nodes carrying `ids` and `sources`, edges
  carrying `sources` and `score`), large enough that the ranking is not
  trivially readable; a DEG ranked list; an HGNC→Ensembl map with a genuine
  **Ensembl→HGNC** map. The committed `fixtures/map_hgnc_to_ensembl.tsv` is
  the wrong direction for this pipeline and its `NKX2-1` row is one-to-many,
  not many-to-one, so the map is generated: two ENSGs landing on one symbol,
  which the engine reports as `IdentifierCollides` and drops both rows with a
  note. A collision is the right exhibit here — two identifiers turning out to
  be one gene is the thing a triage pipeline must not resolve by silently
  picking one.
- Steps: score the graph's nodes into a `RankedList` in the Ensembl
  namespace; `lg.map_ranked_identifiers` into HGNC symbols; intersect with the
  DEG ranking; take the top of it as an `EntitySet`. `map_ranked_identifiers`
  is already `@step`-decorated, so it is a real node in the graph rather than a
  script.
- What it shows by running: a namespace mismatch refused before compute; the
  mapping as a typed conversion rather than a script, with the many-to-one row
  handled visibly; `source_set` compared as a set, so listing the sources in
  another order is the same graph; and D41 — reorder the ranking's rows and
  the pipeline reruns, reorder the set's rows and it does not.

### 07 — A screen that knows what its names mean

*Strengths: the `representation` dimension, stochastic steps, typed and hashed
seeds, numeric tolerance, audit mode.*

A compound screen. The library is keyed by InChIKey with its standardisations
declared, a stochastic scoring step ranks it, and the top of the ranking is
taken as a set.

- Inputs: the committed molecule fixtures, used as they are. They hold real
  molecules with real InChIKeys in every standardisation state —
  `molecules_library.tsv`, `molecules_hits_salts.tsv`,
  `molecules_salt_stripped_anion.tsv`, `molecules_tautomer_normalised.tsv`,
  `molecules_ranked.tsv`. Generating more would mean inventing InChIKeys,
  which is an anti-goal; this notebook's point is not scale.
- Steps: a `@lg.step(stochastic=True, seed="seed")` scorer producing a
  `MoleculeRanking`; a top-k step producing a `MoleculeSet`.
- What it shows by running: a `representation` mismatch refused before compute
  — a salt-stripped set against an as-given one — and `lg.chem.standardize`
  named as the fix, with the `chem` extra named as what it needs and **not**
  called; the seed as a hashed parameter, so changing it reruns and reusing it
  does not; and `pipeline.audit(...)` returning **`reproducible`** for the
  declared, seeded step against **`deterministic`** for the same body with
  nothing declared. That is D40's actual claim — a seeded step is never
  `deterministic`, however identical its bytes — and it is the contrast
  `capture_stochastic.py` already pins. The first draft said "unreproducible",
  which the engine only returns for a body that genuinely draws on randomness;
  that flakes, `capture_stochastic.py` retries four times because of it, and a
  flaky cell is a failed docs build. This notebook does not go there.

### 08 — Publishing the result

*Strengths: claim binding to a cell of a real output, `methods` refusing an
unbacked claim, the methods paragraph and its citations, the reproducibility
bundle, verification by replay.*

The last mile. It rebuilds a compact version of `06`'s shortlist pipeline —
each notebook is executed independently, so it cannot inherit state — then
takes the result to the point where it could go in a paper.

- Claims declared with `lg.Claim`, bound to cells of the real output with
  `at=lg.cell(column=..., row=...)` and a `tolerance` where the value is
  numeric.
- `pipeline.methods()` showing: the generated paragraph, the tools and
  citations it collected, and `accounted_for`.
- A claim rewritten so it no longer matches the output, and `methods()`
  refusing it — `unbacked` non-empty — which is the mechanism that makes a
  claim enforceable in CI, as `capture_claims.py` already relies on.
- `methods(bundle=...)` writing the bundle, its files listed.
- `methods(verify=<bundle>)` replaying it and reporting `reproduced`.

## Decided, and why

These are my calls unless noted as Kelyn's.

- **Inputs are written into a temp directory by the notebook's first cell**,
  either copied from a committed fixture or generated deterministically, and
  the cell says which. Nothing is downloaded; nothing outside the temp
  directory is written. This is what keeps "works on a fresh clone" and
  "stable hashes" true at once.
- **`07` uses committed fixtures unmodified**, for the InChIKey reason above.
- **Every `run`, `audit` and `methods` call passes an explicit `cache=`**
  inside the temp directory (Constraint 4).
- **No temp path, duration or timestamp is rendered** (Constraints 5, 6). The
  notebooks print derived summaries and relative names.
- **Pages are kept to a readable length** by making each notebook one
  pipeline with one arc, not a tour of the API.
- **The shape — three pipelines plus publication — is Kelyn's choice**, from
  the approaches above.
- **Documentation.** This is docs work, not a feature, so the mandatory
  per-feature trio in `CLAUDE.md` does not apply. What does: `ORDER` and the
  hand-written prose in `render_notebooks.py`'s `index()` are rewritten for
  eight notebooks; `CHANGELOG.md` gets an entry; and the guide and concept
  pages whose claims these notebooks now demonstrate get a link to the
  relevant notebook. `npm run check` in `web/` is the gate, and it fails on a
  page nothing links to, so the links are not optional.
- **One commit per notebook plus a final docs commit**, conventional
  messages, so a page that breaks can be bisected to its own change.

## The methods paragraph, verbatim

**Decided by Kelyn: `08` renders `Methods.text` whole**, including
`Analyses were run with Ligase <version> on <platform>.` — engine version,
`+dirty` marker and all. Recording the environment is what a methods section
is for, and a page that trimmed the environment line out of a provenance
exhibit would be undersellling the feature it exists to show.

Two consequences the builder should accept rather than work around:

- The line names whichever machine built the page — `linux-x86_64` from CI,
  a contributor's own platform locally — and may carry `+dirty`. That is
  correct output, not noise.
- It is therefore the one place where the page is not byte-stable across
  machines. The determinism requirement still binds everywhere else: no temp
  paths, no durations, no timestamps, and hashes that hold build to build.

- `Methods.text` also carries a **recorded date**, alongside the version and
  platform, so `what-a-paper-needs.mdx` changes every calendar day. Any future
  byte-stability check over the generated pages must exempt that fence.

Everything else in `08` was verified to render clean, relative and path-free:
`Bundle.files` is a relative list, and `Verification` reports hashes and
`reproduced`.

## Reviewed

A builder read this document cold, built throwaway pipelines against the
engine to test its assumptions, and found four claims in the first draft that
the engine does not support: the BED/SAM join, the "wrong build" refusal, the
pure-Python alignment sort, and the unreproducible audit verdict. All four
were confirmed against the source and are corrected above. The mapping
direction in `06` was wrong too, and is corrected. The one question only Kelyn could answer — whether
the methods paragraph is rendered whole — is answered above.

## Left to the builder

- The exact cohort size in `05`, the graph size in `06`, and the top-k in
  `06`/`07` — whatever makes the rendered page legible.
- The precise generated content, subject to determinism and to the generating
  cell being honest about what it is.
- Whether `repr(RunReport)` leaks `duration_ms`, and therefore whether the
  notebooks print reprs or derived summaries (check first; prefer summaries
  if in doubt).
- Cell boundaries and the prose in each `mo.md` cell, in the voice of the
  existing four.
- Which guide and concept pages get which notebook link.
