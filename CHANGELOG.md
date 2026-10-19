# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

**M5 — Provenance.** `ligase methods` says how the results were produced,
and writes a bundle that lets someone else check them.

- A step that runs now **records the command that ran**, with its
  placeholders filled in, beside every tool's full version, every parameter
  as it was hashed, every input by content hash and every reference by
  accession and checksum. A step served from the cache records nothing and
  needs to: the record from the run that did the work is still there, so a
  pipeline that recomputed nothing can still say how all of it was made.
- That is the debt **D22** took on. Dropping `@PG` lines before hashing was
  necessary — a step's output bytes contained its own key, so nothing below
  the first real tool could be cut off — and it cost the ability to prove
  which command produced an object. This is where that ability went.
- Provenance lives in its own `provenance.sqlite` with its own version,
  beside the cache rather than inside it (**D23**). A cache drops its tables
  whenever a key format moves, because an entry composed differently must
  never be read; a record of what a command did stays true however the
  engine later composes keys, and is the only data here that cannot be
  regenerated.
- **A reference needed nothing new to be pinned** by accession, version and
  checksum, as `CLAUDE.md` requires. The FASTA loader has read both since
  M1: HXB2's sequence name *is* `K03455.1`, the accession with its version,
  and the per-contig MD5 is the digest the SAM specification defines for
  `@SQ M5`. A name is only called an accession when it looks like one, so a
  methods section cannot cite `chr1` as one.
- **The paragraph is generated, in fixed sentence forms.** A generator free
  to phrase things fluently would also be free to leave something out, which
  is the one thing a methods section must not do. Nodes of one step are one
  sentence; nodes genuinely run differently get a sentence each.
- **Citations** come from a short curated table — `minimap2` (Li 2018),
  `samtools` and `bcftools` (Danecek et al. 2021) — or from `cite=` on a
  step. A tool neither covers is named as **uncited** rather than omitted,
  because a reference list that quietly dropped a tool would let an
  incomplete methods section look complete (**D24**). A citation never joins
  a cache key: it cannot change a result, so adding one reruns nothing.
- A step whose output predates provenance is reported as a **gap**, by name,
  with `ligase run --force` as the remedy. Nothing can reconstruct such a
  command — that is precisely what D22 gave up.
- **`--bundle DIR`** writes the methods paragraph, every result and input by
  content hash, every command verbatim, every reference by accession and
  checksum, the environment, the source of every step, the pipeline file,
  and a BLAKE3 manifest of all of it. The data is named rather than carried:
  sequencing runs are gigabytes and fetching them is out of scope (D8).
- **`--verify DIR`** checks that claim, and the second of its four verdicts
  is the one that matters (**D25**): `identical` is the same key and the same
  result, **`reproduced` is a different key and the same result anyway** — a
  tool version moved and the science did not — and `differs` is a finding.
  `reproduced` is what a reviewer on another machine actually gets. Inputs
  are checked by content as well, because a changed input explains a changed
  output and without that column a `differs` would be uninterpretable.
- The M5 criterion is proved on the real pipeline in `tests/test_demo.py`: a
  bundle written from one cache and verified against an **empty** one, which
  recomputes all nine steps with real minimap2, samtools and bcftools and
  lands on the same canonical hashes.

### Fixed

- **The demo's alignments now declare their read group.** `minimap2` writes
  no `@RG`, so `bcftools` fell back to naming the VCF's sample column after
  the input BAM's *path* — a file inside the content-addressed store. The
  calls therefore carried a cache path as a sample identifier, which is
  wrong in a methods paragraph, wrong for the sample-set dimension, and
  wrong for reproducibility: the canonical form of a VCF includes its sample
  names, so `call` hashed differently depending on where the cache was, and
  a bundle could not verify against a clean one. `align` now writes
  `@RG SM:<run accession>`, which is what the sample always was.
  M5's bundle verification is what found this.

### Changed

- `@lg.step` accepts `cite=`, one reference or several.
- `exec::produce` returns an `Execution` — the duration and the command it
  ran — and `exec::run` and `audit::audit` take a `Stores { cache, prov }`,
  so that a run recording provenance is visible in the type.

### Added

**M4 — Semantic hashing and audit mode.** A step's inputs are now hashed by
what they *mean* rather than by which bytes they happen to hold, so a rerun
that changes nothing scientific stops where it started.

- Outputs are **canonicalised before hashing** (D4): records sorted,
  volatile headers stripped. Rerunning the demo's aligner with `threads=8`
  instead of `threads=1` reruns `align` — a thread count is a parameter, and
  nothing can know in advance that it will not change an answer — and then
  skips the eight steps below it, because the alignments are the same.
- This closes a hole rather than only widening a test. `minimap2` records
  its command line in `@PG CL`, that command line contains the output path,
  and the output path is the staging file named after the step's own key —
  so a step's output bytes contained its own key, and *any* change to a
  step, a comment included, cascaded to the bottom of the pipeline. Early
  cutoff could not get past the first real tool (**D22**).
- The canonical form is **a property of the type, not of the step**. A BAM,
  a VCF, a FASTA, a FASTQ, a BED and a resistance report each have their own
  rules, parsed by `noodles` where the encoding matters. Which formats sort
  their records and which keep their order follows D14, already decided for
  the dimension checker: a sequence dictionary's order is semantic, a BED's
  is not.
- A `samtools view -b` of a SAM hashes the same as the SAM, as D9 requires:
  re-encoding is not a biological change, so it must not cost a cache.
- **Numeric tolerance.** A field may declare one, and VCF `QUAL` declares
  0.1 Phred — a floating-point accumulation whose last digits move with the
  order the terms were added in. Everything else is exact. A tolerance is
  expressed by quantising, because "within ε" is not transitive and so
  cannot be an equality a hash expresses (**D20**).
- A file that does not parse as its type **falls back to its bytes**,
  domain-separated so the two can never collide (**D21**). Bytes are the
  stricter test, so the fallback can cost a rerun and can never serve a
  wrong answer; every parser is strict for the same reason.
- `ligase audit <step>` and `pipe.audit("step")` run a step twice and report
  one of three verdicts: `deterministic` (the same bytes), `reproducible`
  (different bytes, the same result) or `not reproducible`. Two would not
  do — a tool that stamps the time of day into its output is reproducible,
  and an audit that tested bytes alone would report the whole reference
  pipeline as broken. Under fan-out every node of the step is audited, with
  `--sample` to narrow. An audit records nothing: its two runs write into
  scratch files that are deleted.
- Every step in a run report now carries both hashes — `hash` for the bytes,
  which is the object's name in the store, and `canonical` for what they
  mean, which is what the steps below were keyed on.
  `report.canonicalised` counts the files that had to be parsed.
- What a file means is remembered against the hash of its bytes, so it is
  parsed once in its life — not again on the next run, after a copy, or when
  another step produces it identically. A second run still reads nothing,
  and now parses nothing either.
- The demo's `align` takes a `threads` parameter, which is what the
  milestone's acceptance test turns on.
- New docs: `concepts/semantic-hashing.md`, `guide/semantic-hashing.md`,
  `architecture/canonicalisation.md`, and decision records **D20** (a
  tolerance is a quantisation), **D21** (canonicalisation falls back to the
  bytes) and **D22** (a tool's signature is not part of its result).
- `INPUT_POLICY` is `canonical` rather than `raw` and `KEY_VERSION` is 2, so
  step entries written by an older Ligase retire themselves. The file hashes
  survive: a file's byte hash is a fact about the file however keys are
  made. A third version, `CANON_VERSION`, clears the canonical hashes and
  the keys built from them when the rules change (D19).

**CI runs the HIV-1 reference demo.** The pipeline CLAUDE.md calls the
project's validation spine is exercised on every push and pull request now,
and not only on the author's machine.

- A `demo` job installs minimap2 2.31, samtools 1.24 and bcftools 1.24 from
  bioconda — pinned in `.github/demo-tools.yml`, since a step's cache key
  includes its tools' versions and an aligner that moved on its own would
  present as a Ligase regression — fetches the four PRJNA517147 runs the
  demo names, builds one real alignment and runs the whole Python suite
  against them. Fetched inputs are cached between runs under the hash of
  `scripts/fetch_test_data.sh`, which is where the accessions and their
  checksums live: edit the manifest and the data is fetched again, change
  anything else and it is not.
- `LIGASE_DEMO_REQUIRED` turns a skip for want of a tool or a fetched file
  into a failure that names what was missing. The nine tests driving the
  real pipeline — the five M3 acceptance tests, plus four wanting real HXB2
  or a BAM a real aligner wrote — skipped on every CI run until now, and a
  skip read as success. The one pipeline every milestone is signed off on
  was the one thing CI never ran.

**M3 — Execution and the content-addressed cache.** A checked pipeline now
runs: in parallel, cached, one branch per sample.

- `pipe.run()` and `ligase run` compute whatever is not already known. A
  step's key is `hash(function code, parameters, input content hashes,
  external tool versions)`, so a step runs when something that could change
  its result has changed and not otherwise — and nothing runs until the
  whole pipeline type-checks.
- A step body now says *how* its output is produced rather than producing
  it. `lg.sh("minimap2 -ax sr {ref} {reads.r1} {reads.r2} ... -o {out}")`
  builds a command whose placeholders the engine fills in; `lg.write(f)` is
  for a step whose work is Python, and reads its inputs through
  `v.file` — typed as a path rather than an optional one, because a body
  reached through `lg.write` is a body whose inputs exist. Bodies are asked once, before anything
  runs, which is what lets the engine own the whole run without the GIL in
  the hot path. A placeholder naming nothing is refused, and `{{` escapes a
  literal brace so an `awk '{{print $1}}'` still works.
- `@lg.step(tool=...)` accepts several tools — `tool=("minimap2",
  "samtools")` for a body that pipes one into the other — and each one's
  `--version` joins the key, so upgrading bcftools reruns the steps it could
  have changed. A tool that is not installed fails the step before it runs,
  naming the tool, rather than being hashed as no version at all.
- Outputs go into a content-addressed store under `.ligase/`, named after
  the BLAKE3 hash of their bytes, with SQLite metadata beside them. Two
  steps that produced identical results share one object. Stored objects are
  read-only: the store is the cache's evidence.
- **Early cutoff** falls out of that rather than being bolted on. A
  downstream key is built from its inputs' contents, so a step that reruns
  and produces the same bytes leaves every key below it unchanged. The
  report tells the two cases apart: `cached` means nothing above it ran,
  `skipped` means something did and came out the same.
- Input files are hashed with a size and mtime prefilter, so a second run
  over a cohort of BAMs reads none of them again. A timestamp can only ever
  save work: it never contributes to a hash or a key, and the one case it
  can miss — a file edited in place, same length, same timestamp — is what
  `--force` is for.
- A failure stops its own branch and nothing else. One bad FASTQ no longer
  costs three good samples their results; steps below a failure are not run
  and are not reported, one cause giving one failure. `pipe.run()` raises
  `RunError` carrying the whole report, including everything that did work.
- The run report is worded by the engine, as the check report already was,
  so `ligase run` and `report.text` cannot drift. A step that ran gets its
  own line with its own cost, costs line up in a column, and steps that did
  not run are grouped with their samples *named* rather than counted.
- `examples/hiv-resistance/pipeline.py` is the reference demo as a real,
  runnable pipeline: PRJNA517147 reads, minimap2 to HXB2, bcftools calls,
  and a report placing each variant in the pol gene and codon it falls in.
  The four M3 acceptance criteria are tested on it directly, with real
  tools.

New dependencies, all in the core: `blake3` (with `mmap` and `rayon`, which
are what make hashing a multi-gigabyte BAM affordable on every run),
`rusqlite` with `bundled`, and `rayon` for the executor's worker pool.
Bundling SQLite means a source build now needs a C compiler; in exchange
every install has the identical SQLite, which an on-disk format that
outlives a run needs more.

**M2 — The dependency graph.** A pipeline of annotated steps is now built as
a graph from the signatures alone and type-checked before any of it runs.

- `@lg.step` reads a function's signature once, at import, and defers the
  body: calling a step records the call and returns a handle. The return
  annotation is the only statement of what a step produces, so a missing one
  — or an unannotated parameter, which would be a connection the graph never
  checks — is refused as the module is imported rather than at check time. A
  `Step` is typed so that a type checker follows the pipeline too and catches
  a mis-wired one while you are still editing.
- `lg.Pipeline(outputs)` walks the handles into a graph, sharing a node where
  one file was passed to two steps. `pipe.check()` raises the first problem
  with every other one attached; `pipe.errors()` returns them without
  raising; `pipe.report()` adds the summary line.
- A step's output carries the dimensions of its inputs, re-sourced as coming
  from that step, so a value renders as `from step `align`` and a mistake
  found three steps downstream can still be traced to the file that caused
  it. A derived alignment therefore knows its pairing from the reads it was
  built from, which M1 could only report as unknown. Coordinates are
  introduced rather than inherited, since nothing upstream of an alignment
  has them.
- Requirements now come from the graph, which is what the catalogue's rule
  always meant: an unknown dimension is allowed unless a downstream step
  requires it. A sample set nothing joins on is nobody's problem. When a step
  does require one, the error names the input that has to declare it, the
  step that needs it and the route between them, once, however many steps the
  gap travels through.
- `StepTypeError` for a step wired to the wrong kind of data, reported
  instead of that step's dimension errors rather than alongside them — those
  bindings are not what the step declared. It is a `CheckError` but not a
  `DimensionError`, because the fix is a different argument rather than a
  declaration.
- `lg.samples(pattern, paired=True)` globs, pairs mates by filename and loads
  each sample; `.map()` applies a chain of steps to every one of them, so one
  written pipeline expands into one graph per sample. Mates are matched
  anywhere in the name, so Illumina's `SAMPLE_R1_001.fastq.gz` gives the
  sample `SAMPLE`. A file whose mate cannot be identified is an error rather
  than a guess, and a lone mate is an error rather than a single-end sample:
  pairing two samples' files produces results for a mixture of two people and
  nothing crashes.
- `lg.intervals()` and the `Intervals` type load BED, GFF3 and GTF, which is
  where the `coordinates` dimension starts doing real work — BED is 0-based
  half-open, GFF is 1-based closed, and feeding one to a tool that expected
  the other is off by a base and looks plausible. Contigs come from the
  records, carrying no length and no checksum, and are compared against a
  reference by name. Their order is never compared: records have no sequence
  dictionary, and reporting their incidental order would be a false alarm.
- `ligase check [pipeline.py]` type-checks a pipeline file from the command
  line, reporting every error in one pass. The module is imported, the way a
  Snakefile is; the step bodies are not run. Exit codes separate a wrong
  pipeline (1) from one that could not be checked at all (2), so it works as
  a CI gate. With no argument it reads `pipeline.py`.
- New decision records: **D12** (steps defer; the graph is built from
  signatures), **D13** (the graph decides requirements and blames the origin),
  **D14** (contig order is compared only between sequence dictionaries) and
  **D15** (an error identifies its node, not just its step).
- 126 Rust tests and 79 Python tests, including both M2 acceptance criteria:
  the golden example's shape builds a graph with every step body raising if
  it runs, and `ligase check` reports every error in one pass. The demo graph
  is also built against the real HXB2 reference and a real minimap2
  alignment, which skip when `data/` has not been fetched.

**M1 — Types and dimensions.** Biological data now carries the dimensions of
what is inside it, and `check_step` refuses to connect steps whose dimensions
disagree — before any compute runs.

- Seven dimensions as real types in `ligase-core`: `organism`, `reference`,
  `contig_naming`, `coordinates`, `sample_set`, `pairing` and
  `quality_encoding`. Each value carries its own provenance (inferred from a
  named file, declared by the user, or derived from an upstream step), so an
  error can say *where* a value came from rather than only that two values
  differ. That provenance is what makes the message actionable.
- Reference identity from contig names, lengths and MD5 checksums — never
  from a filename or a label. Each contig's MD5 is computed from the FASTA
  and compared against the `@SQ M5` an aligner wrote, so a BAM that claims
  the wrong reference is caught even when the contig names and lengths match.
  Two references that describe themselves identically fall back to
  fingerprints in the message.
- A `chr1` against `1` mismatch raises `ContigNamingError`, distinct from a
  reference mismatch, because the fix is distinct. Comparison runs twice —
  once on names as written, once normalised — and a failure that becomes a
  success under normalisation *is* a naming difference. `chrM` and `MT` are
  reconciled, but only in the prefixed form, since hg19's `chrM` and GRCh37's
  `MT` are genuinely different sequences.
- Contig order gets its own `ContigOrderError`: htslib matches by name so
  `bcftools` does not mind, but GATK requires matching sequence dictionary
  order and would reject the input.
- Header inference for FASTA (deflines, computed MD5), SAM/BAM (`@SQ`
  SN/LN/M5/AS, `@RG` SM), VCF (`##contig`, sample columns) and FASTQ (pairing,
  quality encoding from the observed score range). Gzip and BGZF inputs are
  read transparently.
- "Checked" and "required" are separate: values that are both stated must
  always agree, while only a narrow table of kind pairs demands that a value
  be known at all. Aligning reads to a reference therefore does not demand a
  declared organism, keeping the golden example's API intact.
- Unknown is a first-class answer with a reason attached, distinct from "does
  not apply". A FASTQ whose scores fall in the range Phred+33 and Phred+64
  share reports its encoding as unknown rather than guessing, and a reference
  is never asked to declare a quality encoding.
- Python surface: `lg.reference`, `lg.alignment`, `lg.variants`, `lg.reads`,
  `lg.check_step` and the non-raising `lg.dimension_errors`, plus
  `ligase.types` and an exception hierarchy (`ContigNamingError`,
  `ContigOrderError` and `UndeclaredDimensionError` all subclass
  `DimensionError`). Every disagreement is found in one pass and attached to
  the raised error's `errors` attribute.
- `scripts/make_demo_alignment.sh` aligns a fetched HIV-1 run against HXB2
  with minimap2 and rebuilds the header with `samtools dict` so the `@SQ`
  lines carry M5 checksums — minimap2 omits them, and a reference check
  without checksums is the weak check Ligase exists to replace.
- 64 Rust tests and 28 Python tests, including the three M1 acceptance
  criteria and a real-data test whose checksum we compute from the FASTA and
  compare against one `samtools` computed independently. Format fixtures are
  committed as reviewable text and regenerated by a committed generator; BAM
  is written at test time from a committed SAM header, so no binaries enter
  git.
- Documentation: [types and dimensions](docs/concepts/types-and-dimensions.md),
  [declaring dimensions](docs/guide/declaring-dimensions.md) and the
  [type system architecture](docs/architecture/type-system.md).

**M0 — Scaffold.** The project builds, installs and checks itself. No pipeline
functionality yet; the engine's only behaviour is reporting its own version.

- Cargo workspace with `crates/ligase-core` (published as the `ligase` crate)
  and `crates/ligase-py` (PyO3 bindings compiled to the private
  `ligase._core`), sharing one version and one lint configuration.
- Python package at `python/ligase`, distributed as `ligase-bio` and imported
  as `ligase`. Built with maturin against `abi3-py311`, so one compiled
  artifact serves Python 3.11 and later. Ships `py.typed` and a stub for the
  extension module, so `mypy --strict` passes for users too.
- `__version__` is read from the compiled Rust rather than hardcoded, with
  tests asserting it matches the workspace manifest — a wheel cannot report a
  version it was not built from.
- GitHub Actions CI: `cargo fmt`, `cargo clippy -D warnings` and `cargo test`;
  the Python gate (`pip install -e .`, `ruff check`, `ruff format --check`,
  `mypy --strict`, `pytest`, `import ligase`) across Python 3.11 and 3.14 on
  Linux and macOS; `mkdocs build --strict`; and `shellcheck` over `scripts/`.
- Clippy denies `unwrap_used` and `expect_used` workspace-wide, turning the
  "no `unwrap` outside tests" rule into a build error. `missing_docs` is a
  warning locally and an error in CI.
- `scripts/fetch_test_data.sh` fetches the HIV-1 reference pipeline inputs
  into a gitignored `data/`: HXB2 (NCBI `K03455.1`) and the four smallest
  paired runs from each of PRJNA517147 and PRJNA1218827. Reads are verified
  against ENA's per-file MD5 and come over HTTPS, so no SRA toolkit is
  needed. The reference is pinned by the SHA-256 of its canonical sequence
  rather than of the raw file, so an upstream reflow or defline change does
  not break the build while a real sequence change still fails.
- MkDocs Material documentation site, with the design docs published
  alongside the concepts, guide and architecture pages.
- Dual MIT / Apache-2.0 licensing.

### Changed

- `docs/guide/installation.md` lists minimap2, samtools and bcftools among
  the requirements, with the versions the demo is signed off on and how to
  insist the tests actually run them. The demo has always needed all three
  and nothing said so.

- The error classes moved to `ligase.errors` under a new `CheckError` base,
  so `StepTypeError` can sit beside `DimensionError` without being one.
  `DimensionError` and its subclasses keep every name and attribute they had,
  and `except DimensionError` still catches every disagreement about data;
  `except CheckError` additionally catches a step wired to the wrong kind.
- `CheckError::dimension()` in the core is now an `Option<Dimension>`: a
  step-type error blames no dimension, and storing one anyway would mean
  choosing a lie for the Python layer to report.
- CLAUDE.md gained an engineering rule: re-read the design docs before each
  milestone, and work one milestone per session as plan, failing tests,
  implementation, docs.
- **MSRV is now Rust 1.91**, set by `noodles` 0.118. CI already builds on
  stable.
- `docs/dimensions.md`: the `coordinates` dimension now describes record
  semantics rather than on-disk encoding, so SAM and BAM are both 1-based
  closed and a lossless `samtools view -b` no longer appears to change a
  dimension (decision D9). The reference rule gains containment: data checked
  against a reference need only be contained in it, so a VCF listing a subset
  of contigs is accepted (decision D10).
- New decision records: **D9** (coordinates are semantics, not encoding),
  **D10** (references relate by containment) and **D11** (`noodles` for format
  I/O, and the MSRV follows it).

### Fixed

- The check summary counts the steps that failed rather than the step *names*
  that failed. Under per-sample fan-out one written step is many steps, so
  four samples failing the same way reported "4 errors in 1 step" — a
  cohort-wide problem described as a localised one, which calls for a
  completely different response.
- An error at a fan-out step now names its sample in the heading, as
  `DimensionError in step `call` (sample SRR32217714):`, and carries it as
  `error.scope`. Four byte-identical messages left no way to tell one problem
  from four, or to see which samples were affected. The name is added only
  where the step's own name is ambiguous, and only with something that tells
  those steps apart: a step nobody could confuse keeps exactly the shape
  `docs/golden-example.md` specifies, and same-named steps over one sample
  fall back to the file that distinguishes them rather than repeating a
  sample that does not.
- A problem that affects several samples is stated once in the report, with
  every affected sample named on a line of its own rather than counted. A
  sample whose problem differs keeps its own paragraph, and an error that
  cannot name itself is never collapsed, since there would be nothing to list
  it under. `pipe.errors()` is not collapsed either — one error per failing
  step, each with its own scope; `pipe.report().text` is the collapsed form
  the command line prints.
- Two datasets neither of which is the reference no longer agree *vacuously*
  when their contig names share nothing. Containment ignores contigs the
  other side lacks, so a `chr1`-named VCF against a bare-named VCF had
  nothing left in common to disagree about and reported no error at all. A
  comparison pass that matched nothing can no longer claim agreement;
  normalising the names finds the overlap and reports the naming mismatch it
  always should have. Genuinely disjoint data — a chr20-only VCF against a
  chr21-only one — still agrees, because both are correct.

### Dependencies

- None added. The CLI uses `argparse` from the standard library, globbing
  uses `glob`, and BED and GFF are parsed by hand: only the first column is
  needed, and `noodles`' BED reader is generic over the field count.
- `noodles` 0.118 for SAM, BAM, VCF and FASTQ. Pure Rust, so wheels build
  without a C htslib — unlike `rust-htslib`. FASTA is scanned by hand instead,
  because `noodles` materialises each sequence and a three-gigabase reference
  must be checksummed in constant memory.
- `md-5` for the `@SQ M5` contig checksum. Not a security choice: MD5 is what
  the SAM specification prescribes, so we must compute exactly that digest to
  compare against it. Cache keys still use BLAKE3 (D3).
- `flate2` for plain gzip, with a multi-member decoder so BGZF files are read
  whole rather than to the end of their first block.
- `tempfile` as a dev-dependency, for writing BAM fixtures during tests.

[Unreleased]: https://github.com/Neoforge-Labs/ligase/commits/main
