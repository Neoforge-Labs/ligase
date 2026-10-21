# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **A step says what its own work rested on** (**D32**). A resistance
  reading makes assumptions with a clinical direction — no coverage at all,
  codons below the depth asked for, codons no read spans — and all three
  were recorded on the reading, returned through the binding, and dropped
  by the Python wrapper. Nothing rendered any of them.

  Canonicalising the report cannot find them, because a run with full
  coverage and a run with none can write the same bytes, so the body now
  hands them to the engine: `Host::call` returns what it noticed, and
  `lg.write` lets a body return pairs of a note's code and its value. From
  there they take the path D26 built — a `noticed` line in the run report
  with its advice, `StepRun.notes` in Python, the provenance record, a
  bundle's `notes.tsv` and the methods section.

  They are stored against the step's cache key, not the output's hash,
  because they are not facts about those bytes; the key already carries
  what they depend on. A cache hit therefore still says them, which is
  D26's reasoning unchanged: a hedge that appeared on the first run and
  vanished on the second would be worse than none. `SCHEMA_VERSION` moves
  to 3 for the column, which clears the cache's tables.

  Two refusals rather than silences: a code this engine has no note for
  fails the step, and a body that returns something other than notes is
  told what to return — `lg.write(lambda out: out.write_text(...))` hands
  back a character count.
- **Example notebooks**, four [marimo](https://marimo.io) notebooks from
  dimensions and their provenance, through an error caught before compute and
  a fanned-out graph that has not run, to resistance interpretation and why
  the rules are content-hashed rather than named by version. They are plain
  Python, so they run as notebooks or scripts, and the docs build executes
  every one of them: an example that no longer works fails the build instead
  of misleading a reader. The first three need no data and no aligner.
- **An API reference** generated from the package's docstrings with
  mkdocstrings, grouped by what you are trying to do, and a **licence page**
  recording the vendored algorithm's provenance.
- **A docs site that actually publishes.** `mkdocs build --strict` ran in CI
  but nothing ever deployed, so the configured Pages URL had never served
  anything. A `Docs` workflow now builds and deploys to GitHub Pages, with
  `docs/CNAME` for `ligase.neoforgelabs.tech`.

### Changed

- **Codons are read from the reads, not from the combinations of their
  bases** (**D31**). A codon used to be assembled from the VCF — the
  alleles credited at each of its three bases, then every combination of
  them — and a VCF says what is at each position rather than what travels
  together, so the combinations included codons no read carried. On one
  validation sample at a 2% threshold that read `K219KEGR` where the reads
  carry only `AAA` in 54% of themselves and `GAA` in 45%: arginine and
  glycine were arithmetic, not virus. Reads spanning all three bases of a
  codon are now counted as the triplets they carry, and the codon holds
  the amino acids whose share of those reads reaches `mixture`.

  Three things about it are deliberate. Phasing only ever *removes* a
  combination, because which alleles are present at a position stays the
  caller's quality-aware judgement. The threshold applies to the amino
  acid, summed over the triplets that spell it, so a lysine split between
  two synonymous codons is not lost. And a codon the reads cannot settle —
  no read spanning it to `depth`, as paired-end mates can do to a codon
  that falls between them — keeps its per-base reading, with a note, which
  is wider than the truth rather than narrower.

  The reading costs a pass over the BAM's sequences where coverage alone
  needed only its spans: the `resistance` step goes from 0.6s to 1.1s on
  the deepest run of the validation.
- The docs now match the other NeoForge Labs sites: author and copyright,
  `navigation.instant` and `navigation.tracking`, the `search` plugin, and a
  nav ending in the API reference and the licence. mkdocstrings reads
  google-style docstrings here, which is the convention ruff enforces for this
  package, rather than the numpy style the sibling repositories use.

### Fixed

- **Six of the nine validation discrepancies at the 2% threshold.** They
  were all one cause, the phasing a VCF cannot carry: five NRTI rows on
  Sample 3 reading `K219KEGR` for `K219KE`, and Sample 4's lenacapavir row
  reading `A105AT` for `A105T`. None of them changed a score or a level,
  and all of them were wrong. Three remain, and they are one mutation the
  caller credits in 1.7% of the reads it counted — a sensitivity limit
  rather than a disagreement about what the reads mean. The 81-of-81
  agreement at the 15% and 5% thresholds is unchanged.
- D26's link to the canonicalisation notes rendered as the literal text
  ``[`Note`]``. A reference-style label containing a code span does not match
  its definition in Python-Markdown, and `mkdocs --strict` does not catch an
  unresolved reference definition. It is an inline link now.

## [0.1.0] - 2026-10-20

### Added

**M6 — Validation.** The reference pipeline interprets as well as
computes, and its answers are checked against someone else's.

- **`lg.hivdb.interpret`** scores a sample's variants against the Stanford
  HIVdb algorithm and reports a level for each of twenty-seven drugs, with
  the mutations that got it there. The last unimplemented call in
  `docs/golden-example.md` now exists.
- **The rules are vendored, not fetched** (**D28**). HIVDB 9.8 and 10.2
  are committed byte-for-byte from Stanford's own repository, which
  carries the Unlicense, and pinned by SHA-256. An interpretation that
  reached the network would depend on the day it was asked for, which no
  cache key can see and no bundle can reproduce.
- Each algorithm is committed **twice**: the ASI2 XML as published, and a
  `.ruleset` derived from it, which is what the engine reads — so there is
  no XML parser in the core and no new dependency. The test suite
  re-derives one from the other on every run.
- **The rules enter the cache key by content**, through `tool="hivdb"`,
  which reports every vendored algorithm by version, date and the BLAKE3
  of its bytes. Pinning by version *number* would not have been enough:
  upstream publishes corrections under near-identical names, and a key
  that moved only with the name would serve a result computed under rules
  that have since changed. It picks up provenance, the methods sentence
  and D24's citation table at the same time.
- **An interpretation never invents a susceptible result** (**D29**). A
  mixture is every amino acid it could be; a position that was not read is
  reported as unread rather than as wild-type, so a score reads as a
  floor; and a drug scored against a gene the pipeline never supplied is
  `Not assessed` rather than `Susceptible`. The test that pins all three
  is a no-template control, which a less careful engine would report as a
  fully susceptible patient.
- **The demo's variant caller changed** (**D30**). `bcftools call -mv` is
  a consensus caller, and on one validation sample it emitted no record at
  all for a 45% zidovudine-resistance mutation. The pileup's own allele
  depths are the evidence now, `min_freq` means what its name says, and
  what counts as *present* is a separate `mixture` parameter.

### Validated

- The scoring engine reproduces **3,348 published Stanford HIVdb
  expectations** exactly — thirty samples at four thresholds, every drug,
  every score, every level — from reports produced by Sierra 3.5.3, an
  independent implementation by the algorithm's own authors.
- The pipeline end to end agrees with the published calls for
  PRJNA1218827 on **81 of 81 drug calls at both the 15% and 5%
  thresholds**. The nine that differ at 2% each carry a recorded cause:
  six are a phasing artefact of reading codons from a VCF and change no
  score, and three are one mutation present in 0.77% of reads here,
  below any threshold that could report it.
- PRJNA517147 could not supply expected answers — its per-sample calls are
  published only as colour in a heatmap, reverse transcriptase only. It
  remains the demo's default dataset. See `docs/concepts/validation.md`.

### Fixed

- **Reverse transcriptase ran to 440 codons**, the p51 subunit, where
  HIVdb numbers RT 1–560 and its published mutation lists reach 558.
- **Integrase ran to HXB2 5096**, which is where *pol* ends and includes
  its stop codon; the protein is 288 amino acids, ending at 5093.

  Both were silent: a rule about a position outside the window would
  simply never have fired. The gene windows are now checked against the
  rules.

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

- **A run no longer deadlocks against its own hashing.** The executor spawned
  `--jobs` worker loops onto a rayon pool and parked a worker with nothing
  ready on a condition variable of its own. Rayon counts such a thread as
  busy, so it can neither be seen as idle nor steal — and because a step
  hashes its own inputs, and BLAKE3 hashes a large file across the pool it is
  called from (**D3**), a step queues work on that same pool as a matter of
  course. Under enough load the only workers left able to take it were the
  ones already inside it, and a `ligase methods` run on the demo stopped for
  forty minutes with no child process alive. The scheduler no longer waits
  for work anywhere: a step is one rayon job that starts the steps it
  releases before it ends, and a node waiting for a key another node is
  producing is parked on that key rather than blocked on it (**D27**). Two
  tests pin it — each has a step queue work on the pool it is running on and
  wait with a timeout, covering both places a worker used to block.
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
- **The engine now notices this rather than only the demo avoiding it**
  (**D26**). A sample identifier that is a file — one holding a path
  separator, or ending in an extension naming a format the engine reads — is
  reported as a `Note` carried beside the canonical hash, into the run
  report as a `noticed` line, into the provenance record, into a bundle's
  `notes.tsv` and its *What will not travel* section, into
  `ligase methods --verify`, and into the generated methods section after
  the paragraph a reader would paste into a manuscript. Every pipeline whose
  alignments lack `@RG SM` will now be told, instead of walking into it.
- A note is **not** an error: the step ran and its result stands. It is also
  neither a refusal nor a repair. Refusing would turn a format problem into
  an engine failure in the layer **D21** forbids it, since a VCF whose
  sample column holds a path is valid VCF; refusing at `ligase check` time
  cannot work at all, because the checker sees inputs and the wrong
  identifier is on an *output*. Rewriting the identifier to its basename
  would be a silent repair that does not even work — a file in the store is
  named after the hash of its bytes, which move with the cache root exactly
  as the path does.
- Notes are **stored** with the canonical hash rather than recomputed,
  because a cache hit does not reopen the file and a warning that vanished
  on the second run would be worse than none. `CANON_VERSION` does not move:
  nothing about what is hashed changed.

### Changed

- The cache's `SCHEMA_VERSION` is 2, for the `notes` column on its
  `canonical` table, and `PROV_VERSION` is 2, for the `produced_notes`
  table. The cache's tables are cleared, as they are for any format move;
  provenance keeps every row, as it always does (**D23**).
- `Cache::semantic` returns a whole `Canonical` — the hash, the form and the
  notes — for callers that have somewhere to put a note. `semantic_hash`
  keeps its signature.
- `exec::StepResult` and `prov::Record` gain `notes` and `output_notes`, and
  a bundle holds eight tables rather than seven.
- `StepRun.notes` in Python is a tuple of one sentence each, so a script can
  fail a CI job on an output that will not travel rather than grep the
  report.
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

[Unreleased]: https://github.com/Neoforge-Labs/ligase/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Neoforge-Labs/ligase/releases/tag/v0.1.0
