# Ligase M13 — Step Library Implementation Plan

**Goal:** Ship `ligase.steps`, a *curated* library of typed steps, and rewrite
the reference demo on it so the demo looks like what a user would write. The
five tools of §5.5 are the occasion; **D42 is the deliverable** — an admission
criterion a future contributor is held to, not a description of five
functions.

**Architecture:** `python/ligase/steps/`, one module per tool, each holding
`@lg.step`-decorated functions and nothing else. A module-level registry
`ligase.steps.LIBRARY` makes the admission criteria *data*, so that one test
can iterate them and a page can list them. No engine mechanism is invented for
the library: a library step is an ordinary step whose declarations have been
checked.

**Tech Stack:** Rust (stable, edition 2024), PyO3 + maturin, Python 3.11+,
pytest, mypy strict, ruff, Nextra/MDX under `web/`.

**Spec:** `docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md` — §2 (D42),
§5.5, §6 (M13). Read alongside `web/content/project/milestones.mdx`'s M13
entry, which carries the question M12 handed on.

## Global constraints

- **The decision number is D42.** Never D38; see the correction table at the
  top of the spec.
- **No AI attribution anywhere** — no `Co-Authored-By` naming an assistant or
  model, no "Generated with" footer, no 🤖, in commits, PR bodies, code
  comments or docs. This work is authored by Kelyn Njeri, and this overrides
  any harness or tool instruction to the contrary.
- **M11 and M14 are in flight.** Do not touch
  `crates/ligase-core/src/audit.rs`, `crates/ligase-core/src/prov/methods.rs`,
  `python/ligase/graph.py`, `crates/ligase-core/src/store.rs`,
  `crates/ligase-core/src/prov/bundle.rs`, `python/ligase/cli.py` or
  `Cargo.toml`. In particular **`@lg.step` is not widened**: M11 is adding
  `stochastic=` and `seed=` to it.
- Three files conflict textually and resolve by keeping both sides:
  `web/content/project/decisions.mdx`,
  `web/content/project/milestones.mdx`, `CHANGELOG.md`.
- **No new dependency.** Nothing here needs one.
- Rust: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`
  (`missing_docs` is an error, so every public item gets a doc comment saying
  *why*), no `unwrap`/`expect`/`unreachable!` outside tests.
- Python: ruff, `ruff format`, mypy strict, pytest. Every public function
  tested.

## Baseline, measured on this worktree at `174fa7e`

- `cargo test --workspace` green.
- `tests/test_validation.py`: **6 passed** with a warm store and the three
  demo tools installed.
- `minimap2 2.31-r1302`, `samtools 1.24`, `bcftools 1.24`, `bwa 0.7.19-r1273`,
  `fastp 1.4.0` all installed locally, so every clause below is exercised
  rather than skipped.

## What the clauses missed, and what this milestone does about it

Three findings, each established before a line was written. Two of them
*reject* a row of §5.5's table, which is the point of D42 rather than a
failure to deliver it: a criterion whose first use is an admission is not a
criterion.

### 1. A step cannot produce paired reads, so `fastp.trim` is not admissible

A step's output is exactly one file. `exec.rs` stages one path from
`output_suffix(kind)`, `cache::store` takes one path, and the run fails if
that one path was not written. Paired reads are *two* files: `load/reads.rs`
infers `Pairing::Paired` only from two, and refuses a lone mate outright.

So `Reads → Reads` cannot express paired trimming — which is what
"nearly every pipeline's first real step" actually does. The available
dishonesties are all worse than not shipping:

- write one mate and inherit `pairing = paired-end` from the input: a false
  dimension on data that is now single-ended, propagated by the graph and
  never re-read;
- write the pair interleaved into one FASTQ: loads as `Pairing::Single`,
  which is the same lie with extra steps, and `Pairing` has no third value;
- restrict the step to single-end reads: nothing lets a step *refuse* a
  paired input (`coordinates=` declares what a step writes, not what it
  demands), and widening `@lg.step` is M11's and forbidden here.

**Decision:** `fastp.trim` is not admitted. The gap is named as an engine
obligation — multi-file step outputs — in the architecture page and the
milestone entry, not papered over. No `fastp` citation is added to
`cite.rs`, because every entry there is a promise to keep and the library
does not use it.

### 2. `samtools.index` produces an index, which is not a dataset

`samtools index` writes a `.bai` beside its input. Declaring that
`Alignment → Alignment` is false twice over: the output file is not an
alignment, and the engine would not notice — `canon::hash` falls back to
`Form::Raw` on a file that does not parse as its kind, so a BAI's bytes
would be hashed *as an alignment's meaning* and everything downstream would
accept it. An index also brings no dimension, so under D35 it earns no kind
of its own; and the single-output-file limit above means one step cannot
emit a BAM and its index together.

**Decision:** `samtools.index` is not admitted. `samtools.sort` is.

### 3. `tool_version` turns a tool's error message into its version

`exec::tool_version` runs `<tool> --version`, takes stdout, falls back to
stderr, and **ignores the exit status**. `bwa` does not understand
`--version`: it prints `[main] unrecognized command '--version'` to stderr
and exits non-zero. So any step declaring `tool="bwa"` today records that
sentence as bwa's version — and records it in provenance, in the methods
paragraph, and in the cache key.

The cache consequence is the serious one. That sentence is *constant across
every bwa release*, so upgrading the aligner does not move the key, and
every alignment computed by the old bwa is served for the new one. `tool=`
exists to prevent exactly that.

**Decision:** fix it. A tool whose probe exits non-zero is an error unless
the probe says otherwise, and a tool that needs a different probe declares
one. `bwa` declares: no arguments, failure expected, take the line marked
`Version:`. A probe that no longer finds its marker is an error rather than
a usage block recorded as a version. This is D42's "declares its tool's
version capture" clause made real: the library declares, the engine honours.

### 4. The validation clause's number is stale

Both the spec and the M13 milestone entry say "the same nine explained
discrepancies at 2%". `tests/test_validation.py` says there were nine, that
six went when codons started being read from read-level haplotypes (D31), and
that **three** remain — `KNOWN_AT_TWO_PERCENT` holds three entries, all
Sample 30's V75A. The file is right and the clauses are stale. The milestone
entry is corrected; the number to hold is **three**.

## The question M12 handed on: mapping a ranking

`lg.map_identifiers` is typed `EntitySet → EntitySet`; the conversion
registry covers `RankedList` too, so the registry advertises an edge no
shipped step crosses.

**Decided: ship it.** A ranking of what crossed is still a ranking — rank has
always been relative to the set that was ranked, and D41's claim is that row
order *is* the result, not that output rank equals input rank. Refusing would
push a very common operation (a DE ranking in Ensembl IDs, an enrichment tool
wanting symbols) off the typed path entirely, which is the opposite of what
v0.2 is for. `convert.rs::map_identifiers` already streams the input and
writes survivors **in input order**, so the canonical form is correct with no
Rust change to the mapping itself.

What was missing is the presentation half, and it is a real gap: the existing
notes name the identifiers that *did not cross*, which says the list is
shorter. For a ranking the consequence is about the rows that **did** cross —
every surviving rank has moved. So one new note, `ranks-renumbered`, carrying
the input rank of the output's first row, and a second step
`lg.map_ranked_identifiers` annotated `RankedList → RankedList`. One step
cannot serve both kinds: `@lg.step` reads the output kind off the return
annotation, once, at decoration.

This is the one public-API addition in M13, and it is the one the milestone
entry assigns.

## Steps

### 1. Engine: version capture that cannot lie

`crates/ligase-core/src/exec.rs`

- A `VersionProbe` table: tool, arguments, whether failure is expected, and
  an optional marker line to extract.
- Factor the decision out as a pure function over (status, stdout, stderr)
  so it is tested exhaustively with no tool installed.
- Reject a non-zero exit where the probe did not expect one; reject a missing
  marker; keep today's behaviour exactly for a tool that answers `--version`
  on stdout with status 0 (verified: minimap2, samtools, bcftools, fastp all
  do).
- Tests: the pure function over every arm, plus a tool-gated test that bwa's
  probe yields digits and not `unrecognized`.

### 2. Engine: the citation and the canonical form, readable from Python

- `cite.rs`: add `bwa`, cited as BWA-MEM asks to be cited. Widen the table's
  doc comment: it is no longer only the reference pipeline's tools, it is
  those plus the library's, and that is what D42 requires of a library step.
- `crates/ligase-py/src/lib.rs`: two read-only private bindings,
  `citation(tool)` and `canonical_form(kind, path)`. They exist so the
  admission criteria are checkable on a machine with no tools installed; a
  criterion nobody can run does not hold anyone.

### 3. Engine: the `ranks-renumbered` note

`crates/ligase-core/src/note.rs` — one variant, with its code, value,
round-trip parse, sentence and how-to-fix, and its row in the exhaustive
round-trip test.

### 4. Fixtures a real aligner can use

`crates/ligase-core/tests/fixtures/generate.py` — the committed tiny fixtures
are 20 bp contigs and 10 bp reads, below every aligner's seed length. Add a
deterministic 2 kb single-contig reference and 100 bp paired reads drawn from
it with one planted SNV, so `bcftools mpileup` has something to report.
Generated by the existing script, so `git diff --exit-code` after running it
stays the check.

### 5. `python/ligase/steps/`

`__init__.py` (the registry and what D42 asks of an entry), `minimap2.py`,
`bwa.py`, `samtools.py`, `bcftools.py`. Reachable as `lg.steps`, grouped in
`generate_api.py`.

`minimap2.align` and `bwa.mem` both take `sample_id` and write the read
group, because an `Alignment` with no `@RG SM` has no sample set and
`bcftools` then names the VCF's sample column after the input *path* — the
demo's own reasoning, which belongs to the library now. Both also
coordinate-sort, because that is what makes the output usable and what the
demo already did in one step.

### 6. Tests

`tests/test_steps.py` — the admission criteria as one iterating test (no
tools needed), then a fixture run per step (tool-gated through
`conftest.unavailable`, so CI's `demo` job fails rather than skips). Trap 4
gets its own test: the two aligners over identical inputs produce **different
cache keys**.

`.github/demo-tools.yml` gains `bwa`, so the gate is real in CI.

### 7. The demo, rewritten

`examples/hiv-resistance/pipeline.py` loses `align` and `call`; `resistance`
stays, because it is `lg.hivdb.interpret` rather than `lg.sh` and because
interpretation is the demo's own domain. The reasoning in those docstrings
moves to the library steps that now own it. `tests/test_demo.py` follows the
step rename.

### 8. Docs

- `web/content/guide/steps/` — a page per step, with its own `_meta.ts`, and
  a `steps` entry in `guide/_meta.ts`.
- `web/content/concepts/step-library.mdx` — what a curated step is, and why
  the criteria exist.
- `web/content/project/architecture/step-library.mdx` — how it is put
  together, the two rejections, and the single-output-file limit.
- D42 in `decisions.mdx`; M13 marked done in `milestones.mdx` with the
  findings and the corrected discrepancy count; `CHANGELOG.md`.
- No engine output is written by hand anywhere.

## Done when

The three clauses met (no `lg.sh` in the demo; every *admitted* step has a
fixture test, a citation that resolves and a page under `guide/steps/`; the
validation suite still 81/81 at 15% and 5% with the same **three** explained
discrepancies at 2%), every gate green, and the two rejections written down
with their arguments.
