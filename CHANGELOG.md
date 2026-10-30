# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- **A claim is cited by its output digest rather than by its step key.** The
  key is the identity of a computation, so it moves when the step's code,
  parameters, inputs or tool versions move — and adding `cite=` to a step
  moves it while the artifact stays byte-identical and nothing reruns. A
  manuscript's identifier therefore stopped resolving for an edit that
  provably cannot change the number. `ligase methods` prints the digest of
  the output's canonical form instead, which is what `audit` compares two
  runs by and what a bundle verifies against; a claim over several samples
  lists `Outputs:` where it listed `Keys:`. The key is unchanged in
  provenance, in `claims.tsv` and on `bound.key`. D59.

- **`ligase methods` no longer reports a gap for ligase itself.** `ligase` is
  the tool of the built-in conversions, and those are steps the engine owns:
  there is no `@lg.step` for `cite=` to go on, so the advice the gap gave
  named a place that does not exist, and every write-up ended with a warning
  nobody could act on. The engine's own reference — naming the version, or
  versions, the run recorded — is now in the reference list of every
  write-up, and `ligase` is never in the uncited list. A user citing it by
  hand can stop. D58.

### Added

- **A claim may name where its number lives, and the engine reads it.**
  `lg.Claim` bound a sentence to a computation and could not say whether the
  number in the sentence was the number that computation produced — which is
  the gap that makes published numbers wrong in practice, because the
  pipeline is rerun after the table is pasted into the manuscript and nothing
  notices.

  `value=`, `at=lg.cell(column=..., row=...)` and `tolerance=` together ask
  the engine to read that cell and compare. The verdict is `verified` where
  they agree — `bound` and more — and `contradicted` where they do not, and a
  contradicted claim makes the write-up not accounted for, so `ligase
  methods` exits non-zero and CI fails when a manuscript's number stops
  matching its artifact.

  Four refusals come with it: only a declared location, never a search; one
  format, a TSV with a header; no defaulted tolerance, because a claim to
  three decimals and a claim to machine precision are different claims; and
  an unreadable location is `unbacked` rather than `contradicted`, since a
  missing column says nothing about whether a sentence is true. A claim with
  no `at` behaves exactly as before. D61.

  `Eq` is no longer derived on `Claim`, `Restated`, `ClaimVerdict` or
  `ClaimCheck`: a claim can hold an `f64`. `PartialEq` is unchanged.

- **`map_identifiers` takes `ambiguous="fan-out"`, and a ranking that fans
  out becomes a set.** An identifier the mapping table sends to several
  targets was always left out and named, on the grounds that choosing one
  would be a guess. That is right for an untidy table and wrong for the
  biology: *P. falciparum* `DHFR-TS` is one polypeptide with two catalytic
  domains answering to human `DHFR` and `TYMS`, a complex named as a unit
  maps to its subunits, and a paralogue family has no one-to-one orthologue.
  "Decide which target you mean" has no correct answer for those.

  `ambiguous="fan-out"` emits a row per target and says how many identifiers
  fanned and into how many rows, so the output being *larger* than the input
  is on the record. It is declared at the call site, never inferred, and it
  joins the cache key. A target two identifiers reach is still left out,
  because the output is a set.

  A ranking cannot fan out — rank is the row position (D41) — so
  `map_ranked_identifiers` has no such argument, and the new
  `lg.fan_out_ranked_identifiers` returns an `EntitySet` instead: the
  ranking did not survive the crossing, and saying so beats inventing tie
  ranks. D60.

### Changed

- **A bare gene-symbol list no longer states its own identifier space.**
  `identifier_namespace` is inferred from the identifiers' own pattern, and
  six of the seven shapes the engine recognises carry a marker — a prefix, an
  underscore, a colon, or digits. `HGNC symbol` carries none: `DHFR-TS` is
  ChEMBL's name for a *P. falciparum* bifunctional enzyme and `MT-CYB` is an
  approved HGNC symbol, and no pattern separates them. Through 0.3.0 a list of
  either inferred `HGNC symbol` with no hedge, and because both sides of a
  join inferred the same wrong answer, `ligase check` reported no problems and
  exited 0 for a pipeline joining two different identifier spaces.

  Inference now declines where the agreeing shape carries no marker, and the
  dimension is unknown with a reason naming why. The six marker-bearing shapes
  infer as before, and a declaration is still recorded without being checked.

  **This breaks pipelines that joined two symbol lists without declaring**,
  with a refusal at `check()` before anything runs. Add
  `identifier_namespace="HGNC symbol"` to each load. See
  [MIGRATING.md](MIGRATING.md) and D56.

  `KnownNamespace::infer` returns a three-way `Inference` rather than an
  `Option`, so a Rust caller gets a compile error rather than a changed
  answer.

- **The gene-coverage floor is counted over the codons the algorithm scores,
  not over the gene.** `gene_coverage` asked what share of a gene's codons a
  reading covered. Reverse transcriptase has 560 codons and HIVDB 9.8 scores
  40 of them, the highest at position 348 — so four fifths of the gene was
  448 codons, a floor no partial-pol record could clear whatever it covered
  of what the rules actually look at. A clinical genotype reading protease
  and RT 1–348, which is every position any drug is scored on, came back
  with the whole of RT *not assessed*.

  The share is now counted over the scored codons: those the chosen
  algorithm's rules name, restricted to the gene's own window. The same
  partial-pol reading passes with 40 of 40, its unread codons still reported
  per drug so the scores read as floors (D34).

  This is not only a loosening. A dropout across RT 40–120 leaves 86% of the
  gene read, which the old floor waved through, and takes 21 of the 40
  positions the rules score, leaving 19 against a floor of 32 — so the gene is
  now refused where it used to be scored against under half the evidence that
  could have refuted it. The
  failure D34 exists for is unchanged: 32% of integrase is five of the
  twenty-six scored positions against a floor of twenty-one, and is still
  reported as not assessed.

  The `gene-barely-read` note's second count is now the scored codons rather
  than the gene's, and is written `scored=`. A note stored by 0.3.0 or
  earlier under `codons=` is still read, with the number that run used.

- **A `hivdb` step's tool version names the engine as well as the rules.**
  It was the vendored rulesets' versions, dates and digests alone, so a
  release that changed how the engine *reads* those rules — which codons
  count as read, how much of a gene has to be — left every cached
  interpretation in place and served answers the new engine would not
  produce. The engine's own version is now part of it. A cache too cold
  costs time; one too warm is wrong (D20).

## [0.3.0] - 2026-10-28

### Added

- **Claim binding: a number in a paper, bound to the computation behind it.**
  `lg.Claim` names a sentence a manuscript states and the step that produced
  it, declared on the `Pipeline` beside the outputs it is about, and
  `ligase methods` reports what the run has to say about each one — `bound`,
  `unbacked` or `contradicted`.

  The asymmetry it closes: the engine already names an uncited tool rather
  than omitting it (D24), names a sample identifier that is really a file
  name (D26), and names a step whose production it cannot account for. Three
  instances of one promise, that the record names what it cannot account for.
  The number in the paper was the one thing it could not account for and
  never mentioned, because nothing had told it what that number was.

  ```python
  pipe = lg.Pipeline(
      call(hxb2, min_freq=0.05),
      claims=[
          lg.Claim(
              "variants were called above 1% allele frequency",
              step="call",
              parameter="min_freq",
              value=0.01,
          ),
      ],
  )
  ```

  The threshold moved and the sentence did not, so the write-up states both
  numbers and `ligase methods` exits non-zero. An unbacked or contradicted
  claim raises the exit code where an uncited tool does not — an uncited tool
  is a stated absence you cite by hand, and a number the analysis does not
  support is a finding. CI is the only place a drifting manuscript is caught.

  Five checks keep the declaration from being prose: the step is one the
  write-up accounts for, the sample is one it ran on, the parameter or tool
  it restates is one the record holds, the value agrees, and a step that is
  only a methods gap stops the chain — a claim resting on a step nothing is
  known about rests on nothing. Each failure names what there *was*, so a
  renamed step, a dropped sample, a renamed parameter and a swapped tool are
  four one-line fixes rather than one hunt. The claim's *text* is never
  checked and cannot be, which is why the sentence and the value are separate
  fields: a paper writes 1% and a pipeline holds 0.01, and comparing them
  would be a guess about arithmetic in English.

  A claim binds to the **computation**, not the occasion (D45). The key is
  already the identity of a computation, so a step served from the cache
  backs its claim completely and the date reported is when that computation
  happened — which may be long before the run that wrote the paper up.
  Binding only what the current run recomputed would make
  `ligase methods --force` the only honest way to write up a paper. And a
  claim is in no cache key, so editing a manuscript reruns nothing: a
  mechanism costing a re-alignment per draft would not be kept current.

  Claims travel in the bundle as `claims.tsv`, one row per computation with
  the whole key, including the unbacked ones — the same promise the methods
  paragraph makes about a step it cannot account for. `pyproject.toml`,
  `Cargo.toml` and the dependency tree are untouched.

  What the engine does **not** do: read a number out of your output file. It
  could do that for the formats it knows and would have to skip the rest,
  which is some numbers verified and the others silently waved through. A
  claim with a value is compared; a claim without one is bound to a key, a
  hash and a date, and the documentation says which is which.

  New public names: `Claim`, `ClaimCheck`, `BoundTo`, `Methods.claims`,
  `Methods.unbacked`, the verdict constants, and `claims=` on `Pipeline`.
  See [a number in a paper](https://ligase.neoforgelabs.tech/concepts/claims/),
  [binding a number you published](https://ligase.neoforgelabs.tech/guide/claims/)
  and [the internals](https://ligase.neoforgelabs.tech/project/architecture/provenance/#claims).

- **`MoleculeSet` and `MoleculeRanking`, and the axis that admits them.** An
  InChIKey is a hash of a molecular structure, and standardisation changes
  the structure on purpose — so the same substance has several keys, and the
  file records which one it used nowhere. Measured across six RDKit releases
  spanning four years, **twelve of thirty ordinary drugs get a different key
  under a different standardisation state**; aspirin, arriving as its sodium
  salt, has three, two of which differ in the final character.

  So two compound sets prepared differently overlap in nothing for the
  substances affected while the rest still match — an overlap in the tens of
  per cent, which is exactly the range people attribute to assay noise or a
  real finding. And unlike every other axis, no inspection catches it: two
  columns of twenty-seven-character InChIKeys look identical whatever
  produced them.

  `representation` records an identity (`InChIKey` or `SMILES`) and the
  standardisations applied, as a closed set. It is read off a
  `#ligase representation:` line the standardiser writes, declared where a
  file has none, and **demanded** wherever two molecule tables are joined, so
  two tables that each say nothing are refused rather than allowed to agree
  by silence. See
  [my compound overlap is almost empty](https://ligase.neoforgelabs.tech/silent-failures/mixed-standardisation/).

- **A chemistry extra, `pip install 'ligase-bio[chem]'`.** The engine
  acquires no chemistry at all: comparing two representations and refusing a
  join is string work, so loading a compound table, hashing it, early cutoff
  and every refusal work on a default install. RDKit is needed only to
  *produce* a standardised set, and lives in one module that imports it
  inside the one function that uses it.

  That division is asserted rather than described — a subprocess checks that
  `import ligase` imports no RDKit, a featureless Rust build does the whole
  of the checking, and the error for a missing toolkit names the extra and
  says what works without it. The docs site builds with no toolkit either.

- **A tool may be a library (D46).** `tool=` meant two things that coincide
  only for a program: the name a citation resolves from, and something the
  engine *execs* to ask its version. So a library could not be a tool at
  all — RDKit is software an analysis rested on and a reviewer wants cited,
  with nothing on `PATH` to interrogate.

  There is now a third kind of tool, after a program the engine runs and a
  built-in it carries: a library, whose version the host supplies because
  the library lives in the interpreter rather than on `PATH`. The part of
  D42 that was load-bearing is untouched — a library nothing registered is
  an **error** naming it and what to install, never an empty version, since
  a non-answer is the same text for every release and would move no cache
  key.

  `lg.chem.standardize` therefore declares `tool="rdkit"` like any other
  step declares its tool, the version joins the cache key by the ordinary
  mechanism rather than as a hand-passed parameter, and `ligase methods`
  credits RDKit. The engine refuses to *run* a library even where a program
  shares its name: what the step calls is the library, so a program of that
  name would be a version of the wrong thing.

- **`lg.chem.standardize`, the conversion registry's second entry.** It
  crosses `representation`, and molecules that do not standardise are named
  and counted rather than dropped. It is also the first entry with a
  **Python** body, and the registry needed a row and nothing else — which is
  better evidence than another Rust-bodied row that `liftover` and
  `rename_contigs` will be rows rather than redesigns.

- **Cluster execution: several machines run one pipeline between them, with
  no coordinator.** `ligase run --worker=node03` on as many machines as you
  like, all pointed at one `LIGASE_STORE`. They divide the pipeline, they
  talk to nothing but the store, and an ordinary `ligase run` afterwards
  reports the whole graph and writes it up. Ligase submits no jobs and knows
  nothing about Slurm: the `sbatch --array` recipe is in the guide, because
  a group with a cluster already knows how to start twenty of something and
  what it lacked was twenty processes that could divide one pipeline without
  a coordinator.

  The claim worth stating is that **correctness needs no lock**. Two workers
  that compute the same step key agreed on the code, the parameters, the
  inputs' canonical hashes and the tool versions, so a race between them
  wastes CPU and cannot produce a wrong answer. What carries the argument is
  canonicalisation: two workers almost never write the same bytes — a tool
  stamps its own command line into its output — and nothing downstream is
  keyed on bytes. The property added in M4 to make early cutoff earnable is
  the property that makes lock-free distribution safe.

  So the lease that divides the work is **advisory**, and expiry does not
  have to be right: a worker that dies holding leases needs no heartbeat, no
  fencing and no coordinator to notice it, because its deadline passes and
  somebody else computes the step. Both ways of mis-setting `--lease` are
  costs rather than wrong answers. Resumability falls out for nothing.

  Where the argument does not reach is said out loud and reported rather
  than hidden: a kind whose canonical form does not fix its result, and a
  seeded stochastic step whose tool is stochastic in its thread count as
  well — two cluster nodes with different core counts being the normal case.
  The run report names the step and both canonical hashes, the store's
  answer is the one in force everywhere so nothing below the step disagrees,
  and `ligase audit` is what says which of the two reasons it is.

  A worker takes its own cache directory by default, because the index is
  SQLite and twenty array tasks defaulting to one `$HOME/.ligase` over NFS
  is a documented way to lose a database. A worker with no shared store is
  refused outright: it would compute the whole pipeline alone and look like
  a success. No new dependency and no cargo feature — a `file://` store on a
  shared filesystem runs a cluster in a default build, which is the right
  answer at most HPC sites.

### Fixed

- **A published row of one format could make a step key unpublishable for
  ever.** `published::key_of` hashed the domain, the table and the key, and
  not the row format's version — so every format competed for one object
  address, and objects are write-once. A bucket holding a version 1 row
  would have refused every version 2 row written to that key for as long as
  the bucket existed: the write is dropped, the read finds a version the
  build will not parse, and that step key can never be published by anyone
  again. Not the single recomputation a format bump is supposed to cost, but
  all of them. Latent since the store tier shipped and unreachable until the
  first bump; the version is in the address now, so each format addresses
  its own rows and the next bump costs what it promises.

- **Two concurrent writers of one published row could both believe they had
  written it.** `FsStore::install` checked for the file and then renamed,
  which is two steps with a window, so the second writer's rename replaced
  the first's object. For an ordinary object that is invisible — a name is
  the hash of its contents, so the loser's bytes are the winner's bytes —
  and for a row, whose name is the hash of the *question* it answers, it is
  not. It now installs with `hard_link`, which fails rather than replaces
  and does it in one syscall. The object-store backend had the same hole
  from the other direction and now writes with `PutMode::Create`, which is
  what makes "objects are written once and never overwritten" true of a
  bucket rather than merely intended.

- **A methods paragraph named one platform for a run that spanned several.**
  Every provenance record has always carried the engine version and the
  platform it was produced on, and the paragraph took the first one it found
  — correct for as long as one machine ran everything. A mixed cluster, an
  ARM workstation beside x86 nodes, or one worker on an older wheel made it
  a half-truth in a methods section. It names every distinct one now, and
  the reproducibility bundle gives each its own row.

### Changed

- **`Methods.accounted_for` covers claims as well as gaps.** It was
  `not self.gaps` and is now `not self.gaps and not self.unbacked`. A
  write-up that states a number the analysis does not support has a hole in
  it, whatever else is complete, and a property with that name returning
  `True` for one would be making the engine's own promise falsely. Nothing
  changes for a pipeline that declares no claims — including the methods
  text and summary, which are byte-identical, since a heading over an empty
  section is a promise nobody made.

- **Measured: InChIKey is stable across RDKit versions, and canonical SMILES
  is not.** 1,080 keys over six releases, zero disagreements between
  versions. In the same span RDKit's canonical SMILES for cisplatin changed
  while its InChIKey did not move at all:

  ```text
  2022.09.5-2024.09.6  Cl[Pt]Cl.N.N
  2025.09.6-2026.09.1  N.N.[Cl][Pt][Cl]
  ```

  So a canonical form over SMILES would have moved the content hash of every
  file holding that compound on an upgrade that changed no chemistry. The
  probe is `scripts/measure_inchikey_stability.py` and the values are
  asserted by `tests/test_inchikey_stability.py`.

- **RDKit's tautomer canonicaliser discards stereochemistry**, which the
  design brief it was built from said it did not. Ibuprofen's two
  enantiomers both become `HEFNNWSXXWATRW-UHFFFAOYSA-N` — one active
  compound and one inactive one, indistinguishable. So `representation`
  records `stereo-discarded` as an operation that *was applied* rather than
  one somebody chose, and the standardiser checks its own work
  asymmetrically: stereo lost and not declared is an error naming the count,
  declared and not lost is only a note.

- **The scope rule's second limb is now written down.** D35 admits a kind
  only for a dimension, which does not admit `MoleculeRanking` — nor, read
  the same way, the `RankedList` the project shipped in M12. The limb both
  rely on is that a kind may also earn its place by a canonical form whose
  absence makes the cache serve a wrong answer, which is D41's argument
  relocated from the data to the engine. It is deliberately narrow, and D48
  says what it does not admit.

- **The README explains the project rather than announcing it.** It had a
  paragraph of premise, a status line and an install command, and not one
  line of example — a reader could not see what a pipeline looks like, what
  a diagnostic says, or why any of it is worth their afternoon. It now opens
  on the failure mode (a plausible number, not a crash), shows three real
  diagnostics and a real `ligase inspect` run, gives a pipeline whose
  signatures type-check against the shipped package, and explains the
  mechanism — inferred dimensions, graph before compute, content-addressed
  keys, declared randomness, pinned remotes, provenance as a product.

  Every block of output in it is byte-verbatim from the captures the
  documentation site generates on each build, and the example type-checks
  under `mypy` against the installed package, because a README is where a
  stale example costs the most. Worth knowing for the next one:
  `ruff format --check .` formats Python **inside Markdown**, so a code
  block in this repository is formatted code and not prose that resembles
  it — the gate catches a README example the same way it catches a module.

  Links are absolute throughout: this file is also the PyPI long
  description and the `ligase` crate's README, where relative links resolve
  nowhere.

- **`Methods.engine` and `Methods.platform` are now `engines` and
  `platforms`, and are lists.** A run on one machine is the one-entry case
  rather than a different shape. `RunReport` gains `worker` and
  `divergences`, and `Pipeline.run` gains `worker=` and `lease=`.

## [0.2.0] - 2026-10-27

### Added

- **A `KnowledgeGraph` kind, and the one dimension that follows from being
  assembled.** Every other kind in Ligase is *measured*: a FASTQ is what a
  sequencer wrote, a BAM is what an aligner made of it. A knowledge graph is
  whatever somebody decided to merge, and what it holds is a function of that
  list — fold in one more evidence source and edges appear, neighbourhoods
  widen, and every score that propagates along edges moves further than the
  new edges go.

  Nothing in the file says which sources went in. A node says where *it* came
  from; the list that decides the graph is the union over all of them, and
  nothing computes it. So two assemblies diff and score against each other
  without complaint, and the difference reads as biology:

  ```
  DimensionError in step `compare`:
    ours.source_set = open-targets (from the sources named on its nodes and edges: graphs/2026-03.json)
    theirs.source_set = open-targets and disgenet (from the sources named on its nodes and edges: graphs/2026-09.json)
    theirs also lists disgenet.
    These must match. Was a source added or removed upstream?
  ```

  `source_set` is read **off the graph**, from the `sources` named on its
  nodes, its edges and its top level, and a graph naming none leaves the axis
  unknown rather than known and empty — two graphs that each named nothing
  would otherwise agree on the one axis the kind exists for. That is
  [D44](https://ligase.neoforgelabs.tech/project/decisions/).

  The kind is admitted on that axis and on nothing else. There is no standard
  knowledge-graph file format, so Ligase defines the node/edge JSON it reads —
  and defining a shape is exactly what the scope rule (D35) says does not earn
  a kind its place. **Take `source_set` away and this kind should not exist.**

- **`source_set` has its own equality, which three milestones deferred.** The
  shared set type folded members differing only in ASCII case, which is right
  for Latin binomials and wrong for object keys. `s3://evidence/A.json` and
  `s3://evidence/a.json` are two objects by specification, and folding them
  would have dropped one without a word — turning two graphs that differ into
  two that agree. D36 reserved the right to split the type for whichever
  milestone first applied the axis; M8, M10 and M12 each declined it.

  The set type is now generic over how two members are compared, so the
  algebra is written once and each rule once: `Organism` folds ASCII case,
  `SourceSet` does not. The two axes are two *types*, so an arm of
  `check::compare` that read `organism` where it meant `source_set` no longer
  compiles — which is stronger than the test M7's standing requirement asks
  for. `GSE12345` and `gse12345` are therefore two sources, and that costs one
  consistent spelling.

- **A JSON body now goes through a recorded query end to end.** `response.rs`
  canonicalises a JSON answer — keys sorted at every depth, the service's own
  `generatedAt` and `requestId` dropped from the top level — and with no kind
  reading JSON, that branch was exercised by unit tests and never through a
  pipeline. A graph is that kind: a pipeline over a recorded graph replays with
  the server **down** to an identical result, a source folded in upstream is
  found by `--refresh` as drift, and the same graph served a minute later with
  a new `generatedAt` is still the same graph.

- **`ligase.steps` — four steps with an entrance exam, and the reference demo
  rewritten on them.** `examples/hiv-resistance/pipeline.py` no longer
  contains a single shell command of its own. Alignment and variant calling
  come from the library and the pipeline reads

  ```python
  aligned = minimap2.align(sample, hxb2, sample_id=run, threads=threads)
  called = bcftools.mpileup_call(aligned, hxb2, min_freq=min_freq)
  ```

  which is what it means. The twelve-line incantation that gets a read group
  onto a BAM has not gone anywhere; it has moved into the step that owns it,
  with the reasoning that made it those three programs.

  `ligase.steps` is curated and not convenient, which is
  [D42](https://ligase.neoforgelabs.tech/project/decisions/): a step is
  admitted only once it declares its tool's version capture, its citation, the
  dimensions of its output and how that output canonicalises. **A step that
  cannot say what it produces is not a library step.** The declarations are
  held as data in `ligase.steps.LIBRARY` so that one test iterates them, and
  the half needing no tool installed runs everywhere — a criterion that only
  fails in CI is a criterion nobody is held to while writing the step.

  Four steps: `minimap2.align`, `bwa.mem`, `samtools.sort` and
  `bcftools.mpileup_call`. The two aligners are deliberately interchangeable
  in a pipeline and never in provenance: swapping one for the other is a
  different cache key and a different reference, which falls out of `tool=`
  rather than being arranged, and is tested.

  Two candidates from the spec's own table were **refused**, which is what an
  admission criterion is for. `samtools index` produces an index, which is not
  a dataset: it brings no dimension, so it earns no kind, and declared
  `Alignment → Alignment` its output would have a BAI's bytes standing in for
  an alignment's meaning, silently, because a file that does not parse as its
  kind falls back to `Form::Raw`. And no step can trim paired reads at all: a
  step's output is exactly one file and paired reads are two. Both are one
  missing engine feature, named as a gap on the
  [architecture page](https://ligase.neoforgelabs.tech/project/architecture/step-library/)
  rather than papered over.

- **`lg.map_ranked_identifiers`, and the note a mapped ranking needs.** M12
  registered the identifier conversion for `RankedList` as well as
  `EntitySet`, leaving the registry advertising an edge no shipped step
  crossed. It crosses now.

  A second step rather than an argument, because `@lg.step` reads the output
  kind off the return annotation once and one function cannot produce both
  kinds. The mapping itself already wrote survivors in input order, so rank
  order survives for free. What needed deciding was the half that is about
  the rows which *did* cross: rank is the row position, so dropping a row
  renumbers every row beneath it, and the output is a well-formed ranking
  with ranks 1..n and nothing in it to show that n is smaller than it was.
  So there is a new `ranks-renumbered` note carrying what rank the output's
  first row held in the input — the only note in the engine about what a
  conversion *kept*. Its practical reading: take a cut-off before mapping,
  not after, because the top ten of a mapped ranking is the top ten of what
  crossed.

- **Stochastic steps declare themselves, or they are refused.** A great deal
  of computational biology is sampling — IQ-TREE and RAxML search tree space,
  BEAST runs chains, AutoDock Vina docks from random starts, `seqtk sample`
  takes a subset — and none of it gives the same answer twice unless it is
  told to. Ligase can run a step twice and compare what came out; what it
  cannot do is look at a tool and know whether randomness is in it. So the
  step says:

  ```python
  @lg.step(tool="seqtk", stochastic=True, seed="seed")
  def subsample(reads: Reads, fraction: float = 0.1, seed: int = 20260101) -> Reads:
      return lg.sh("seqtk sample -s{seed} {reads.r1} {fraction} > {out}")
  ```

  Two new keyword arguments to `@lg.step`, and they are one declaration:
  `stochastic=True` says there is randomness in the tool and `seed=` says
  which parameter holds it still. Neither is accepted alone. A missing seed —
  or one naming a parameter the step does not take, or one not typed `int` —
  is a `StepDefinitionError` as the pipeline file is imported, in the same
  breath as a missing return annotation, because an unseeded stochastic step
  is not a step that fails. It is a step that works, caches a plausible
  number, and gives a different one next Tuesday for reasons nothing records.

  What it buys is a word that means what it says. `ligase audit` reports
  `deterministic`, `reproducible` or `not reproducible`, and the first of
  those now requires byte equality **and** a step that declared nothing. A
  correctly seeded stochastic step is byte-identical across two runs — that is
  the success case — and calling it `deterministic` would tell a reader there
  is no randomness in the step. Move the seed and the bytes move. So such a
  step saturates at `reproducible`, and the report says what the equality
  rests on:

  ```text
  reproducible     `permute`
      the same bytes both times   6c636bdb27d9
      because `seed` was held at 20260101, not because the step cannot wobble
  ```

  The same body with nothing declared reports `deterministic` over the same
  hash. A declaration adds no verdict; it takes one out of reach.

  A step that names a seed and still disagrees with itself has a declaration
  that is false — a renamed flag, a library reading the global generator, a
  parallel reduction that adds in whatever order it finishes — and the audit
  now names the parameter you believed was controlling the tool and says it is
  not, rather than filing it under ordinary irreproducibility.

  `ligase methods` states the seed as a seed: `seeded with seed = 20260101`,
  not `with seed = 20260101`, because a reader of the latter cannot tell
  whether the number is a sampling seed or a cutoff.

  The seed itself needed nothing new. It is a plain typed parameter, so it is
  already in the cache key, and two seeds are two keys and two results —
  verified and now pinned, since two seeds sharing a key would serve the
  second run the first one's answer in silence.
  [D40](https://ligase.neoforgelabs.tech/project/decisions/) has the whole
  argument, including why the verdict stays on the audit's own scale rather
  than becoming a second enum beside it.
- **A shared cache: a local store with a bucket behind it.** Everything
  Ligase computes already goes into a content-addressed store, where an
  object's name is the hash of its contents. That is what makes a cache hit
  trustworthy, and it is also what makes the store shareable: if a name is
  a statement about bytes, it does not matter which machine wrote them.

  ```bash
  export LIGASE_STORE=s3://our-lab-cache
  ligase run
  ```

  One person aligns the cohort; everybody else's run finds it done. The
  model is Nix's binary cache — a local store, always first, and a remote
  *substituter* behind it, consulted on a miss and written through on a
  put. `s3://` and every S3-compatible store (MinIO, Cloudflare R2,
  Backblaze), `gs://`, `az://`, and `file://` for a directory on a shared
  mount, which needs no special build.

  Configured by `LIGASE_STORE` or a `[store]` table in `ligase.toml`, the
  environment winning. That table takes a URL and nothing else: a key that
  looks like a credential is refused by name, because `ligase.toml` is a
  file people commit and the one place a user would try to put a secret is
  the one place that has to say no. Everything else comes from the
  provider's own chain.

  **What a second tier changes is failure, not storage,** and three rules
  settle it. An unreachable remote is a *miss*, not a failure: it holds
  nothing the engine cannot recompute, so falling back to computing is
  always correct — but the run report says so, in as many words, because a
  run that quietly recomputed for an hour because a bucket was away is a
  misleading answer even though it is a right one. An object that is not
  what its name says is *refused*, named in full along with the store that
  served it, and its step reruns. And nothing enters the trusted local tier
  unverified, because a corrupt object copied down would be caught once and
  then never again.

  The index stays local — SQLite over object storage is a known way to
  corrupt a database — and it is rebuildable, which took more than the
  design assumed. A step key is a hash of ingredients no object carries, so
  a clean machine would find every object it needed and still rerun
  everything. The rows that make a hit possible are therefore published
  into the store too, including the provenance record: without that, the
  whole point of the feature — one person computes, everyone else hits the
  cache — would hand every colleague a run that completed perfectly and a
  methods paragraph saying "is in the cache from a run that recorded no
  provenance" for every step.

  Be exact about what this is as trustworthy as. Hashing an object proves
  its bytes are what its name claims; it proves nothing about the claim
  *"step K produced object O"*, and nothing can short of running the step.
  So **a shared store is as trustworthy as write access to it**: point
  Ligase at a bucket your lab writes. Nix answers the same question with
  signatures; v0.2 does not have them, and says so rather than implying a
  guarantee it has not got.

  New: `ligase store status`, which says whether this build can open a
  bucket at all, which store is configured and where that setting was read
  from, whether it answers, and how much is here that the remote has not
  got — exact up to a thousand objects and sampled above, saying which,
  because a cheap number read as an exact one is its own kind of wrong
  answer. Plus `ligase store push` and `ligase store pull`.

  The tier is behind a build-time feature and **the default install
  acquires no dependency on it**, which is asserted rather than documented:
  a test reads `cargo tree` and fails if `object_store`, `aws-lc-sys`,
  `tokio`, `reqwest` or `hyper` reaches a default build. The reason it
  matters is that `object_store`'s AWS stack needs `cmake` at build time
  and there is no configuration of it that does not. Getting a build that
  can reach a bucket is
  `MATURIN_PEP517_ARGS="--features remote-store" pip install --no-binary
  ligase-bio "ligase-bio[remote-store]"`; `ligase store status` tells you
  which one you have.

- **`ligase methods --bundle --thin`.** A bundle that names the store
  instead of carrying the object bodies: small, and not self-contained.
  What it gives up is the one thing a recorded query's body buys — a
  reviewer replaying with no network and no API key — so it says so in
  three places: the bundle's README, a `store.tsv` naming the store, and a
  warning on the way out. Seeding one reads the bodies from that store, and
  what it cannot get is reported as a single failure naming the store and
  every missing hash, before any step runs. A half-replayed run is the
  plausible-but-wrong result Ligase exists to refuse.


- **Recorded queries — the second half of remote inputs, for a source that
  promises nothing.** `lg.remote` rests on a promise: NCBI undertakes that
  `K03455.1` will mean the same sequence next Tuesday, so a digest is how you
  hold it to that. Most of what a modern pipeline wants comes with no such
  promise — Open Targets will answer differently next quarter, because it is
  supposed to — and asking such a source for a pin is asking for a promise it
  would be wrong to make.

  ```python
  targets = lg.query(
      "https://api.example.org/v4/targets?gene=EGFR&format=tsv",
      kind=lg.types.EntitySet,
      organism="Homo sapiens",
      auth_env="OPENTARGETS_TOKEN",
  )
  ```

  So a `RecordedQuery` makes a different claim, and the two are kept apart
  because conflating them is the mistake
  [D39](https://ligase.neoforgelabs.tech/project/decisions/) exists to
  prevent. A pinned remote promises *source* reproducibility: the same bytes
  forever from the source, obtainable by anyone, anywhere. A recorded query
  cannot promise that and does not try. It promises *record*
  reproducibility — the same bytes forever from the record, for anyone
  holding the bundle — and that turns out to be enough, because the question
  a reviewer asks is "did this analysis produce these numbers?" rather than
  "does the world still look like this?".

  The whole mechanism is one sentence: **the canonicalised response body goes
  into the content-addressed store, not merely its digest.** The body is then
  an input like any other, with a content hash in every cache key below it,
  so everything downstream is deterministic by construction rather than by
  anybody's discipline.

- **Three modes, set per run rather than per step.** The default replays a
  record that exists and records one that does not; `ligase run --replay`
  serves every query from its record and reaches no network; `--refresh` asks
  every source again and reports the drift. The run report always says which
  of the three each query did, because "it worked" and "it worked without
  asking anybody" are different statements and only one of them is about
  reproducibility.

  Per run and not per step is deliberate. A run that replayed some of its
  queries and quietly fetched the rest would be a plausible-looking result
  that nobody could repeat, so **in replay mode a query with no record is an
  error** — before any step runs, because a query resolves while the pipeline
  file is still being read. The two flags are mutually exclusive: one says
  never ask the source and the other says always ask it.

- **A refresh writes a new dated record beside the old one.** It never
  replaces what it refreshes, so every earlier run stays replayable. A
  surveillance pipeline refreshing weekly is not reproducing one run — it is
  a series of runs, each with its own record, and this quarter's refresh must
  not quietly destroy last quarter's evidence. The `records` table is keyed on
  `(request, revision)` rather than on a timestamp, because two refreshes
  inside the same second must still be two records and a clock is the wrong
  thing to rest "never overwrite" on. A refresh that finds the same bytes
  writes nothing new.

- **A record is identified by the canonical request**, not by the order it was
  asked in: method, URL with its query parameters sorted, request body. A step
  making twenty paginated calls writes twenty records and finds them all again
  next time whatever order it issues them in, and two URLs differing only in
  parameter order are one record. The URL that goes on the wire is the one you
  wrote — sorting decides which record this is and never decides what to ask,
  because a server may care about the order.

- **Recorded bodies travel in the reproducibility bundle**, in `records/` and
  listed in `records.tsv`, and `ligase methods --verify` seeds them into the
  reviewer's store before the pipeline is read. A reviewer on a clean machine
  with no network and no API key replays the whole run. This retires the
  limitation `python/ligase/provenance.py` used to record, that "a bundle
  cannot fetch its own data": for a recorded query the bundle *is* the source.
  A pinned remote is still named rather than carried, because anyone can
  obtain it and check it against its digest.

- **A bundle never carries a secret, structurally.** `auth_env=` names an
  environment variable rather than taking a token, because a bundle copies the
  pipeline file and a literal token would travel in every bundle you shared.
  Nothing the engine stores has a field for a request header — not the record,
  not the bundle — so an `Authorization` header is not *stripped* before
  storage; it is never representable in anything that is stored, which is the
  only version of the guarantee a field added later cannot quietly undo. The
  test asserts over every file in the store and the bundle rather than over a
  list of header names.

- **The `replayed` and `live` audit verdicts**, beside `pinned` and in the
  same enum, reported in their own block rather than folded into the
  step verdicts. They are three different claims rather than a scale: a
  replayed query is not a worse pinned remote, and a pipeline's verdict should
  not degrade for using a live source. The run that writes a record is *live*,
  which is the honest word for it, and every later run is *replayed*.

### Changed

- **`exec::Task::citations` no longer claims that editing a citation reruns
  nothing.** The field is outside the cache key, which is true and was being
  read as a promise that it is free to edit. It is not: a step's source is
  hashed *as written, decorator line included*, so editing `cite=` moves the
  step's code and reruns it, with early cutoff then stopping the rerun
  travelling downstream. The doc comment now says so, beside the new `seed`
  field, which would otherwise have copied the error.

- **A recorded response is canonicalised before it is hashed**, like every
  other kind: JSON keys sorted at every depth, arrays left alone because there
  order is the answer, and a body that is not JSON standing in for itself
  (D21). A server's own bookkeeping — a `requestId`, a `generatedAt`, a
  `took` — is dropped on D22's `@PG`-line precedent, from **the top level
  only**: inside a data record the same key is what the record says, and
  dropping it recursively would silently delete a time series (D26). Each drop
  is a note, in case the key was yours after all.

- **A response over 32 MiB is refused**, with the refusal saying to pin it as
  a release instead. An answer that large is a dataset rather than a query
  result, and a dataset has a version. A pinned remote's own limit stays
  generous, because an SRA run is legitimately gigabytes.

- `Transport::get(url, into)` is now `Transport::fetch(&Request, into)`, so a
  query can be authenticated. One method taking one type rather than two,
  because a second method would have a default implementation that quietly
  dropped the authentication.

- One new runtime dependency, `serde_json` with `arbitrary_precision`, which
  costs exactly two crates in this workspace's tree and no build tool at all.
  Its default map is a `BTreeMap`, so key sorting is the library's own
  behaviour; `arbitrary_precision` keeps every number as the text the server
  sent, so a twenty-digit identifier is stored exact rather than rounded
  through a double on its way in.

- `web/content/guide/remotes.mdx` is listed in its `_meta.ts`, which M10a
  omitted: the page built and was linked, so the gate passed, but Nextra
  appended it unlisted with a default title instead of showing it in its
  intended place.

- **A `RankedList` type — the ranked table, where the order is the result.**
  A ranking of differentially expressed genes, of predicted drug targets, of
  candidate compounds, answers the question *which of these matters most*,
  and rank is simply the row position. So a `RankedList` is an `EntitySet`
  and one more thing: its canonical form **preserves row order**, where every
  other type in Ligase sorts its records because the order they arrived in is
  not a fact about the biology
  ([D41](https://ligase.neoforgelabs.tech/project/decisions/)).

  ```python
  ranked = lg.ranked_list("results/degs_ranked.tsv", organism="Homo sapiens")
  truth = lg.entity_set("truth/known_drivers.tsv", organism="Homo sapiens")
  ```

  It is the exact inverse of its own unranked sibling over the very same
  bytes. Reorder a gene list and nothing downstream reruns; reorder a ranking
  and everything below it does, because the two orderings are two different
  answers and a cache that called them the same would serve the wrong one.
  Both are right, and the choice of type is the choice of claim.

  Everything else about the two types is identical, deliberately. A ranking
  carries the same four dimensions — `organism`, `identifier_namespace`,
  `subject`, `release` — reads its identifier space off its own identifiers
  the same way, and notices the same `SEPT7`-eaten-by-a-spreadsheet
  corruption. They share one loader; they do not share a canonical form,
  because that is the one place they differ. A `score` column is an ordinary
  attribute column and is compared **exactly**: no tolerance is declared for
  one, because none has been measured.

  This brings two refusals to a ranked result. A ranking of symbols scored
  against a truth set of Ensembl IDs is refused rather than returning a
  precision of nearly zero that reads as a verdict on your method. And a
  ranking named by `HGNC 2026-09` scored against a set named by
  `HGNC 2026-03` is refused too — the subtler one, since both files are
  symbols and both look right, over six months in which a symbol can be
  retired, renamed or moved to another gene. See
  [comparing ranked lists](https://ligase.neoforgelabs.tech/guide/ranked-lists/).

- **`ligase inspect FILE [FILE...]` — the whole idea, on files you already
  have.** Until now the cheapest way to find out whether Ligase was worth
  anything to you was to write a pipeline. `inspect` needs no pipeline, no
  decorator and no commitment: it loads each file you name, says what kind it
  read it as, and prints every dimension the loader worked out along with
  **where it read each one from** — and, given two or more files, compares
  them the way one step's inputs are compared and prints the diagnostic.
  Nothing is executed, no tool is invoked, nothing is written and no cache
  appears. It reads headers.

  ```
  Reference  refs/genome.fasta
    genome.fasta.reference = 2 contigs, 40 bp (from FASTA contigs: refs/genome.fasta)
    genome.fasta.contig_naming = bare (Ensembl/NCBI style) (from FASTA deflines: refs/genome.fasta)
    genome.fasta.organism is unknown (a FASTA does not record its organism; declare it)
    genome.fasta.release is unknown (a FASTA does not record which release it came from; declare it)
    coordinates, sample_set, pairing, quality_encoding, subject, source_set and identifier_namespace do not apply to a Reference.
  ```

  Three states, and keeping them apart is most of what the command is for. A
  dimension with a value is shown with its value *and its provenance*, which
  is the half that lets you go and look. A dimension that applies without a
  value is shown with the engine's reason, which is usually an instruction —
  `declare it`, or, for a gene list a spreadsheet has eaten, *re-export the
  file*. A dimension that does **not apply** is named once as not applying: a
  FASTA is not missing a quality encoding, it has no such property to have,
  and reporting it as unknown would send a reader after a declaration that
  does not exist.

  Given two or more files, the diagnostic is **the one a pipeline would have
  printed**, because it is the same `check_step` over the same comparison. It
  is rendered through the same helpers too, so the error quotes the
  description's own lines rather than restating them: a reader who has read
  the blocks has already read the error. The summary line then says which
  dimensions the verdict covers, because "no disagreement" is a claim nobody
  can size — a FASTQ and a BED share no dimension either of them records, and
  they agree the way two sentences in different languages agree.

  It exits 0 when the files agree, 1 when they disagree, and 2 when a file
  could not be read at all — so `ligase inspect refs/*.fasta aln/*.bam &&
  snakemake` refuses to start a run over files that disagree, and a file
  nobody could read does not pass for a clean bill of health. A 2 withholds
  the comparison rather than comparing whatever loaded, since a verdict over
  two of three files reads as a verdict over all three; the files that did
  load are still described, and each one that did not is named with the
  engine's reason.

  The loader is chosen by extension, with `.gz` and `.bgz` ignored: `.fasta`,
  `.fa` and `.fna` as a `Reference`, `.fastq` and `.fq` as `Reads`, `.sam`
  and `.bam` as an `Alignment`, `.vcf` as `Variants`, `.bed`, `.gff`,
  `.gff3` and `.gtf` as `Intervals`, and `.tsv` as an `EntitySet`. There are
  no declaration flags, on purpose: a declaration belongs where the data is
  loaded, so that it reaches provenance and the cache key instead of a
  prompt. The guide page is "Inspecting files you have", and the quickstart
  now opens with it.

- **A pipeline says where its data comes from, and the engine fetches it**
  (**D39**). The first step of nearly every pipeline is a fetch, and until now
  it sat outside everything Ligase promises — including in its own demo, where
  a three-hundred-line shell script downloaded the reads and the engine took
  its word for what had arrived.

  ```python
  hxb2 = lg.remote(
      "ncbi:K03455.1",
      kind=Reference,
      digest="1f460b25ab23b95345d579c234c43ca6d0abf7c1adf6aeffb43b974b2b1941ad",
      organism="HIV-1",
      build="HXB2",
  )
  ```

  `ligase run` fetches it, verifies it, and hands back the same object
  `lg.reference` would have — contigs, checksums, dimensions and all — with
  its content hash in every cache key below it and its locator in the methods
  paragraph. The second run fetches nothing, so a pipeline built on remotes
  runs offline once it has run online.

  **A pinned remote promises the same bytes forever, from the source.** That
  is a promise an SRA run accession or a numbered release actually makes, and
  `digest=` is how you hold a source to it. The other kind of remote — a live
  query, whose source promises nothing — needs a record rather than a pin, and
  is the next milestone; D39 is written whole because the two halves are only
  coherent together.

  **The digest is over what the file means, not over its bytes.** The shell
  script this replaces pinned HXB2 by the checksum of its *sequence* for a
  reason it wrote down: NCBI may reflow lines or reword a defline without the
  biology changing. An alarm that fires on a cosmetic reformat teaches you to
  bump pins without reading, and then the one that matters gets bumped too. So
  a changed *meaning* is a loud failure naming both digests, and bytes that
  moved with the meaning intact are noticed instead.

  **A wrong digest fails before any step runs**, and that needed no arranging:
  a remote resolves as the pipeline file is read, because its dimensions
  cannot be known until its bytes are in hand and the engine checks dimensions
  before any compute. There is no graph yet when it fails.

  **Four locator schemes and no more** — `sra:`, `ncbi:`, `ensembl:`,
  `https:` — because a scheme is a promise that the engine knows how to turn
  an accession into bytes *and* how to recognise the answer. A fifth is an
  error naming the four, and a bare `SRR8525904` with no scheme is refused
  rather than guessed at: a guess would be right often enough to be trusted
  and then one day fetch from the wrong archive.

  **A 200 is not a promise about the body.** NCBI answers a bad accession with
  an HTTP 200 and an HTML page, so what arrives is read as the kind it was
  declared to be and a body that will not parse as one is refused. With no
  digest declared, that is the only thing between an error page and a pipeline
  treating it as a reference.

  Reads fetched from a remote must name their sample. A local FASTQ's sample
  comes from its filename and a fetched one is stored under the hash of its
  bytes, which is no sample identifier — and the run accession is not assumed,
  because one sample can have several runs.

  `ligase.remotes.configure(offline=True)` makes a run resolve only what the
  store already holds, which is what a reviewer with no connection wants.
  `ligase check` gained `--cache DIR` for the same reason remotes resolve
  early: the store has to be known before the pipeline file is read.

- **`ligase audit` reports each remote as *pinned*,** beside the step verdicts
  rather than folded into them. The two are not on one scale: a remote's
  verdict says who else could obtain this input, and a step's says whether the
  same input twice gives the same answer. A pipeline whose every step is
  deterministic can still rest on an input nobody else can get, and folding
  the verdicts would let that average away.

- **The methods paragraph says what was fetched, from where and when,** and a
  reproducibility bundle names each remote in `remotes.tsv` by locator and
  digest. Named rather than carried, because that is what *pinned* means:
  anyone can obtain it from the source and check it against the digest
  recorded there, so copying gigabytes of reads into every bundle would buy
  nothing.

- **The content-addressed store is a trait** (`store::BlobStore`), with the
  local filesystem as its only backend. Records are the first thing written
  there that a lab will want to share, so the shape is settled now and M14
  implements it over object storage. Two additions to D39's three operations
  are not conveniences: a key carries a **suffix** beside the hash, because a
  step's output is handed to `bcftools` as a path and those tools dispatch on
  the extension; and verification is a **provided method on the trait**, so
  "nothing read from a store is trusted" is not something each backend author
  remembers separately.

- `Dataset` gained `files`, every file a dataset is in order, and `origin`,
  where it was fetched from. `lg.remote` is typed on its `kind`, so it returns
  a `Reference` or a `Reads` rather than a bare `Dataset`.


- **A gene list knows which identifier space it is in, and two spaces refuse
  to join** (**D38**). `TP53`, `ENSG00000141510`, `7157` and `P04637` are four
  correct names for one gene and no two of them are equal as strings, and a
  gene list records nowhere which space its names belong to. So joining a list
  of symbols to a table of Ensembl IDs is a string comparison between two
  naming conventions: it does not fail, it returns the rows the two columns
  share, and an empty or nearly-empty overlap reads as poor concordance rather
  than as a mismatch. The failure is now written up as "My gene-set overlap is
  empty".

  `identifier_namespace` is a new dimension, compared by equality. An
  `EntitySet` — the gene or protein list, and the first kind here that holds
  no sequence — carries it, along with `organism`, `subject` and `release`,
  and carries none of the others: a list of gene names has no coordinate
  system, no contigs, no read pairing and no quality scores, and demanding one
  is the category error the dimension matrix exists to prevent. Load one with
  `lg.entity_set("degs.tsv", organism="Homo sapiens")`: a TSV with a header,
  the identifier column named by `id_column=` and defaulting to `id`, and
  every other column carried as an attribute.

  The space is the one dimension the engine usually reads **without being
  told**. Seven are recognised — `Ensembl gene`, `UniProt accession`,
  `RefSeq`, `HGNC ID`, `ChEMBL target`, `Entrez gene` and `HGNC symbol` —
  tried most specific first, because a symbol has no distinguishing marker and
  would otherwise claim a UniProt accession, which would make a protein list
  and a gene list *agree*. Each shape admits what people actually have:
  ten-character TrEMBL accessions as well as six-character reviewed ones,
  versioned Ensembl IDs such as `ENSG00000141510.18`, mixed-case symbols such
  as `C9orf72`, readthrough symbols such as `TNFSF12-TNFSF13`, and the whole
  RefSeq prefix set rather than six of it. Inference succeeds only when
  every identifier in the file resolves to the same space, and declines in
  three cases rather than guessing: a file that mixes two spaces, a file with
  a header and no rows, and a file holding one identifier nothing claims. It
  happens in the Rust loader, so `ligase inspect` will get it without a Python
  round trip and the two surfaces cannot disagree about what a file's
  identifiers are. A declaration, where you give one, is recorded rather than
  checked — the engine knows seven spaces and a curated list's own spelling is
  a space too.

  Unlike `release`, `subject` and `source_set`, this axis is **demanded**
  where two gene lists are joined: a user can always satisfy it, and two lists
  that each say nothing would otherwise agree by silence, which is the failure
  in its purest form.

- **`lg.map_identifiers`, and the conversion registry it is the first entry
  in.** The only way across an identifier space, because mapping between
  spaces is a lookup with failures rather than a rename:

  ```python
  in_ensembl = lg.map_identifiers(
      genes, hgnc_to_ensembl, column="ensembl_gene_id", to="Ensembl gene"
  )
  ```

  The output declares the space named by `to=` and carries every other
  dimension of its input unchanged, because a conversion crosses one axis and
  is not a new list. The mapping table is itself an `EntitySet` rather than a
  path, which buys two things: its contents join the cache key, so editing the
  table reruns the mapping, and its own space is checked against the list
  being mapped, so an Entrez table handed a list of symbols is refused before
  anything runs.

  Every identifier that does not cross is **named, and counted**. Three kinds
  do not: one the table has no row for; one it maps to several targets, which
  is left out rather than resolved because emitting a row for each would make
  the output larger than the input without saying so; and one whose target
  another of your identifiers also reaches, which would otherwise make the
  output name a single gene twice with two sets of attributes — not a set, and
  a thing anything downstream double-counts. The named ones stop at twenty,
  which is enough to recognise a pattern, and a count note carries the total,
  because a conversion leaving four thousand identifiers behind needs a number
  before it needs four thousand sentences. The output is therefore smaller
  than the input by exactly the identifiers the notes name, and those notes
  travel into the run report, provenance, a bundle and the methods paragraph.

  The registry is the deliberate half. A conversion declares the one axis it
  crosses, the kinds it crosses it for, and what the user must supply, and
  reads its destination from a parameter spelled `to` for every conversion
  there will be — so `liftover` and `rename_contigs` are later rows rather
  than a later redesign, and D5's conversion graph has edges it can search
  rather than functions somebody calls. `@lg.step(conversion=...)` refuses, at
  decoration, a step claiming a conversion the engine does not have or taking
  no `to` parameter.

- **A loaded dataset can say what loading it noticed.** `dataset.notes` is a
  tuple of sentences, the same shape `StepRun.notes` already has. Until now a
  note could only come out of a step, which left a file nobody computes with
  nothing to say — and a gene list is exactly such a file. Open one in Excel
  and some symbols come back as dates: `SEPT7` as `2-Sep`, `MARCH1` as
  `1-Mar`, measured across published supplementary files at rates that made
  HGNC rename the affected gene families. Such a file loads, because it is
  valid TSV, and the loader says what it saw rather than refusing it or
  repairing it — `2-Sep` could have been `SEPT7` or `SEPT2`. It also declines
  to infer a space for a file it cannot fully read, and the reason it gives
  names the dates rather than advising a declaration, since declaring the
  space is what would make the corruption permanent. A file written with
  carriage returns alone, as Excel for Mac 2011 wrote them, is read as the
  rows it holds rather than as a single line — which would have loaded a
  four-hundred-gene list as an empty one without a word. The other corruption, a
  symbol turned into a date serial such as `42615`, is deliberately not
  detected: it is indistinguishable from an Entrez gene ID.

- **`ligase` is a built-in tool**, beside `hivdb`. A step whose work is engine
  code — a conversion — has nothing to probe with `--version`, and nothing
  about the engine otherwise reaches a cache key, so such a step would hash
  the same however the conversion changed underneath it. It now reports the
  crate version.

- **A dataset may be about more than one organism, and a reference can name
  its release** (**D35**, **D36**, **D37**). `organism` held a single string,
  so a host–pathogen sample, a metagenome or a xenograft had to be written
  down as whichever one organism the analysis was about. That declaration is
  true of the analysis and false of the data; two of them agree with each
  other, so the check passed and a later step treated everything in the file
  as one organism's. The failure is now written up as "My pathogen results
  include host reads".

  `organism` is an ordered set compared by set equality: members are trimmed,
  compared without regard to case, repeats collapse, and the order they were
  given in is not part of the identity, so `["Homo sapiens", "HIV-1"]` and
  `["HIV-1", "Homo sapiens"]` are the same set. Where two sets differ the
  error names the difference — `ref also lists Homo sapiens` — because the
  two value lines state both sets in full but cannot point at the member that
  makes them unequal. A list naming nothing is refused where it is passed,
  since two datasets that each declared no organism would otherwise *agree*.
  In the Python API `organism=` still accepts a plain string, which is a set
  of one and renders exactly as it did, so **no v0.1 pipeline written against
  the Python API needs editing.** The Rust crate is another matter, and is
  under *Changed* and *Removed* below.

  Three dimensions arrive with it. `release` says which versioned external
  resource the data came from, and it is its own dimension rather than a
  generalised `reference` because a reference compares by containment and a
  release by equality, and one dimension carrying two comparison rules is the
  quiet complexity this engine exists to refuse. It applies to a reference
  today — `lg.reference(..., release="GRCh38.p14")`, where two releases of
  one build can differ in sequence while sharing a name — and `subject` and
  `source_set` apply to no kind the engine reads yet. All three were
  *appended* to the dimension list rather than slotted in beside the axes
  they resemble, because a report of several disagreements is ordered by that
  list and inserting one would have reordered every existing report. None of
  the three is ever demanded: each is compared wherever both sides state it.

  And the scope rule that governs what may be added next: a kind earns its
  place in Ligase only if it brings a dimension whose mismatch is a documented
  silent failure in real research. A kind that is a file format with a schema
  is a parser, and Ligase does not need more parsers.
- **The docs can be found, and read by something that is not a browser.**
  The site published no sitemap, no `robots.txt`, no canonical address on
  any page and no structured data, so a crawler learned about it only by
  following links. There is now a sitemap generated from the page map
  rather than listed by hand, a `robots.txt` that names the AI crawlers
  and allows them, a canonical on every page equal to the page's own
  address, and one JSON-LD block describing Ligase as a software
  application. That block says "pre-alpha" in its description and carries
  no rating, review, download count or award, because there are none to
  carry.

  The same pages are published as prose: `llms.txt` indexes them,
  `llms-full.txt` holds all of them, and every page has an `index.md` beside
  its HTML with the components resolved to text. Each page states the
  address of the HTML it came from. A page that still contains a JSX tag
  after conversion fails the build, since that means a component was left
  to render itself in a place with no renderer.

  The honesty rule in `CLAUDE.md` is now a check rather than a promise: the
  prose is scanned for the phrases an adoption claim is made of ("trusted
  by", "widely used", "production-ready" and the rest of the list in
  `check-seo.mjs`), and one sentence is allowed to contain one — the one on
  the in-use page that says there is no "trusted by" list. It is pinned
  exactly, once, in one file, and the check also fails if that sentence
  stops appearing, so the exemption cannot outlive the page it was written
  for.
- **A social card and a home-screen icon.** Neither existed, so a link to
  the docs unfurled as a bare URL. The card (1200 by 630) and the Apple
  touch icon (180 by 180) are generated at build time from the same
  geometry as the mark and written to `/opengraph-image.png` and
  `/apple-icon.png`. They are route handlers in folders named `*.png`
  rather than Next's file conventions, because under a static export the
  conventions write an extensionless file and GitHub Pages chooses a
  `Content-Type` from the extension. `check-assets.mjs` reads the PNG
  signature and the dimensions from each file's header and fails if any
  page references one that was not emitted.
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
- **A quickstart**: a two-step pipeline with a naming mismatch in it, caught
  before anything runs, then fixed, run, and run again for the cache hit. No
  aligner, no reference genome and no downloads — the step works in Python
  through `lg.write` — so it works on a bare machine, and every output on the
  page came from running exactly those commands.
- **A command-line reference**, generated from the argparse parser by
  `web/scripts/generate_cli.py`. `check`, `run`, `audit` and `methods` were
  named throughout the docs and documented nowhere: their flags existed only
  in `--help`. A command or option with no help text now fails the build.
  `ligase.cli.build_parser()` was split out of `main` so the parser can be
  read without running anything; the interface is unchanged.
- **An errors reference** organised by what the reader saw rather than by
  class name, with the hierarchy as a tree — the useful cut being that a
  `CheckError` cost milliseconds and anything else cost a run — and a closing
  section on what `check` cannot tell you.
- **A new documentation site, in Nextra, under `web/`.** It is what serves
  ligase.neoforgelabs.tech, built, link-checked and deployed by the `Docs`
  workflow. A pilot for a house style across the NeoForge Labs projects, so it
  is a custom design rather than a themed install: serif body text at a 72-character measure, a four-colour signal
  palette that means what the engine means, and components that render the
  engine's own voice — `<Diagnostic>` sets a diagnostic the way rustc does,
  with the mismatched values in a shared column so the mismatch lines up, and
  `<RunReport>` gives `ran`/`cached`/`skipped`/`noticed` a column and a colour
  each.
- **Pages for the failures Ligase exists to catch**, titled after the symptom
  somebody would search for rather than the feature that catches it: a BED
  file that intersects nothing, variant positions that are wrong, intervals
  off by one, sample labels that look swapped, a resistance report that names
  the wrong gene, and a sequence dictionary GATK rejects. Each states the bug,
  says why nothing crashed, and then shows the engine's real diagnostic. The
  resistance one is the only hazard on the list that was found in this engine
  rather than collected from the literature.
- **`web/scripts/capture_diagnostics.py`**, which captures those diagnostics
  from a live run on every docs build rather than letting them be pasted in.
  It fails the build if the engine stops producing the error a page claims it
  produces, so it is a test of the engine as much as a generator.
- **`web/scripts/generate_api.py`**, the replacement for mkdocstrings, which
  has no equivalent in the JavaScript ecosystem. It imports the package and
  writes MDX, grouped by what a reader is trying to do. It treats the grouping
  as a contract with `__all__` in both directions, which mkdocstrings could
  not: a name exported and ungrouped fails the build, and so does a grouped
  name that is not exported, or any exported name, method or property without
  a docstring.
- **`web/scripts/render_notebooks.py`**, which executes the four marimo
  notebooks and renders each page from the run. The previous site executed
  them in CI and then linked to GitHub, so a reader saw the source and never
  the result; now every value on those pages was computed during the build.
- **`web/scripts/check-links.mjs`**, the replacement for `mkdocs --strict`,
  run over the exported HTML. It fails on a broken internal link, an anchor
  that does not exist on its target, and a page nothing links to. It also
  catches the class `--strict` missed: a reference-style link whose definition
  is absent is not a broken link to Markdown but literal text, which is how a
  broken link survived in the design decisions.
- **Case pages for what has actually been run**: the HIV-1 validation as a
  worked case, and a note that SAWA is evaluating Ligase for drug-resistance
  surveillance. Both say plainly that the project has no outside users.
- **A reference carries the coordinate frame it is numbered along.**
  `lg.reference(..., frame=("K03455", 2085))` declares that base 1 of a file
  is base 2085 of K03455, which is what a pol-relative reference is. It is
  declared because no FASTA records it, and a whole genome needs no
  declaration because its length identifies it.
- **A gene read too thinly is reported as not assessed**, rather than scored
  against the little that was seen. A `gene_coverage` parameter on
  `lg.hivdb.interpret` says how much of a gene must be read, four fifths by
  default.
- **Example notebooks**, four [marimo](https://marimo.io) notebooks from
  dimensions and their provenance, through an error caught before compute and
  a fanned-out graph that has not run, to resistance interpretation and why
  the rules are content-hashed rather than named by version. They are plain
  Python, so they run as notebooks or scripts, and the docs build executes
  every one of them: an example that no longer works fails the build instead
  of misleading a reader. The first three need no data and no aligner.
- **An API reference** generated from the package's docstrings, grouped by
  what you are trying to do, and a **licence page** recording the vendored
  algorithm's provenance.
- **A docs site that actually publishes.** The docs build ran in CI but
  nothing ever deployed, so the configured Pages URL had never served
  anything. The `Docs` workflow now builds and deploys to GitHub Pages at
  `ligase.neoforgelabs.tech`.

### Changed

- **The HIV-1 demo fetches its own inputs, and `scripts/fetch_test_data.sh` is
  gone.** `examples/hiv-resistance/pipeline.py` declares seventeen remotes —
  HXB2 and eight paired runs — and is itself the manifest of their pins, so
  there is nothing to run before `ligase run`. ENA's directory layout rule,
  which lived only in that script, moved into the engine and is now tested at
  all four of its branches.

  The test suite is a different matter and is **offline by default**: a suite
  that downloads a hundred megabytes because you ran it is a suite people stop
  running, so a test needing the demo's inputs skips unless the store is warm.
  `python scripts/fetch_demo_data.py` warms it. CI sets
  `LIGASE_DEMO_REQUIRED`, which both allows the fetch and turns a skip into a
  failure.

  The CI `demo` job now caches the engine's own store, keyed on the hash of
  the demo pipeline — which is to say on the digests. A restored cache cannot
  be stale, only incomplete, and an incomplete one is a fetch rather than a
  wrong answer.

  `scripts/make_demo_alignment.sh` and the Rust tests that wanted real HXB2
  now ask the engine where its own input is rather than looking for a path.

- **The docs moved onto the Neoforge Labs identity.** They were warm paper
  and violet, designed before the project had to belong to anything; they are
  now near-black (`#0A0A0B`) with cream (`#F5F5F0`) ink and the flame orange
  (`#E36414`) the other NeoForge sites use, and a visitor arrives in the dark
  theme. Light is kept and retuned rather than dropped, because this is
  long-form reading and the cream is what it reads best on.

  Orange owns hue 23, so the four signal colours were moved to stay clear of
  it: `refuse` is a cool crimson, `agree` a green, `notice` a yellow, and the
  contrast gate holds the brand 25 degrees of hue from the two it could be
  confused with. Every text colour is held to 4.5:1 on every surface it can
  sit on, in both themes, and four of the design's first-draft values failed
  that and were moved to pass it; the gate also fails if the two themes stop
  declaring the same tokens, which is what a half-converted theme looks like.

  **Light-mode orange is `#a8460a`, not the brand's.** The brand orange is
  3.15:1 on cream, which fails AA as link text. Only light-mode text
  darkens; dark mode and the two SVGs use the brand hex exactly. The cost is
  that the light site is a darker, browner orange than the brand's own, which
  is a divergence from the house style chosen over an inaccessible link.
- **A mark, in two optical sizes.** The logo is a ligature binding the `l` and
  `i` of "Ligase": a tall serif stem, a shorter one with its tittle, and one
  slanted orange tie across both, which is what the project does to two
  steps. It is drawn twice, because a mark that works at 120px does not work
  at 16. `icon.svg` has the fine slabs; `favicon.svg` has thicker slabs and
  tie, and it carries its own colour scheme handling so it does not vanish
  on a light tab strip. Only the small variant is declared as the favicon.
- **The docs are set as a research journal.** Playfair Display for the page
  title and display titles only, Source Serif for prose, Geist for headings
  and the interface and Geist Mono for labels and code; an orange letterspaced eyebrow; each
  `h2` numbered `§ 01`, each figure `FIG. 01`, both counters restarting on
  every page and incremented only by an `h2` inside the content, so a
  generated page with no `h2` never shows a stray count; and a film grain and
  an aurora behind the landing hero. The grain and the aurora switch off
  under `prefers-reduced-motion`. The engine's own panels, `<Diagnostic>` and
  `<RunReport>`, are numbered as figures.

  **Playfair Display has no weight 300.** The brand's CSS asks for it; the
  family is 400 to 900, and the font service answers a request for 300 with
  an error. Declaring the weight anyway would make the browser fall back to a
  different face and say nothing, so the display face is 400, the lightest
  weight that exists, and `check-furniture.mjs` fails if any rule asks the
  display face for a weight `layout.tsx` does not load.

  **`VOL. 02 — THE ENGINE` is a knowing placeholder.** The landing page
  opens with the parent site's device, a three-part letterspaced row above
  the hero (`VOL. 02 — THE ENGINE`, `◉ WESTLANDS, NAIROBI`, `MMXXVI`) with a
  hairline beneath, and the footer repeats the place and the year on every
  page. The parent site is `VOL. 01`, so `02` is a guess at the sequence, and
  which volume this is belongs to the author, who has not said. It ships as
  written so the cover is judged with a realistic line in it, and is recorded
  here so it is not mistaken for a settled number. The masthead was named in
  the design and went unbuilt until the last review caught it; the checker
  now reads the built HTML and fails unless it is on the landing page exactly
  once, on no other page, and every footer carries the locality line.
- **The 404 is the mark, refusing.** It is the same `l` and `i`, but the tie
  is cut in two, the halves offset, and drawn in the refusal colour rather
  than the brand orange. Ligase's job is to refuse a connection that does not
  hold, and this is that refusal drawn. It is noindex, and declares its own
  address as its canonical rather than Next's internal `/_not-found`, which
  would have leaked into public HTML.
- **The docs print, now that they are dark.** A dark default makes a printed
  page a block of ink unless the stylesheet intervenes, and about 52,000
  words of prose are the kind of thing people print. A print stylesheet puts
  every colour token on white, rules and tokens included, pins the signal
  colours to ink (a printer drops backgrounds, and a pale-on-dark
  `Diagnostic` would print as pale on white; the words that carry the
  meaning are in the text, not the colour), hides the sidebar, contents list,
  grain and aurora, and takes syntax tokens from the light theme whichever
  theme the reader was in. It hides the grain's pseudo-element and not the
  grain itself: the grain is mounted on `body`, and hiding it would print
  every page blank, which an earlier draft did and the checker now forbids.
- **Code blocks use the high-contrast GitHub themes.** The pair chosen for
  the old violet palette, `github-light` and `github-dark-default`, was
  measured against the surfaces a block is actually drawn on (the stylesheet
  overrides the theme's own background): on white its keywords were 3.8:1 and
  its variables 2.9:1. `github-light-high-contrast` and
  `github-dark-high-contrast` clear 4.5:1 on every token colour they emit,
  the closest being light-mode comments at 5.04:1 on white (the worst dark
  token is 8.42:1), and their hues (deep red, blue, brown, green) stay out of
  the way of the orange. `npm run shiki` re-measures this against the built
  HTML, because `check-contrast.mjs` reads `tokens.css` and a Shiki theme is
  not in it. Still one theme per colour
  scheme rather than one forced through a filter, and still Shiki's bundled
  themes, so nothing was added to `package.json`.
- **`npm run check` is the whole gate.** The branch wrote five checkers and
  the gate ran two of them. The favicon-variant guard, the display-weight
  guard, the print-blanking guard and the one that enforces the project's
  no-adoption-claims rule were run by hand or not at all, and `--orphans`
  was referenced by no script. All of them now run from `check`, the
  source-only ones first so a palette failure surfaces in a second rather
  than after a build, and CI runs `npm run check` itself instead of a copy of
  its steps, which is how a new checker would otherwise be written and never
  wired in. The dependency guard compares `package.json` with the merge-base,
  so CI now checks out full history and names `origin/main` as the base.
  Two checks were added after that: `npm run shiki` measures every syntax
  colour in the built HTML against the surface code is drawn on, and
  `--orphans` now also requires theme-dependent non-colour tokens
  (`--lg-shadow-raised`) in both themes.
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
- The docs carry the same author and copyright as the other NeoForge Labs
  sites, and the API reference is generated from google-style docstrings,
  which is the convention ruff enforces for this package, rather than the
  numpy style the sibling repositories use.
- `Step.output` and `Step.parameters` are documented. Both are public and
  neither appeared in `Step`'s `Attributes:` section; the new API generator
  refused to build until they did, which is the point of it.
- **The guide and concept pages open with the reader rather than the
  project.** Fourteen of them began by reporting project status — "Everything
  here runs today" — where a reader wants to know what the page lets them do.
  Each guide now states the task, the outcome and the prerequisites; each
  concept opens with the question it answers. The bodies are almost untouched.
  "Declaring and checking dimensions" also began by telling the reader to run
  a repository script they would not have if they installed from PyPI, so its
  walkthrough was unrunnable from a wheel; "Semantic hashing and `ligase
  audit`" and "Reading what the engine noticed" were named after mechanisms
  and are now "Checking that a step repeats" and "Fixing a noticed line".
- **The new site is organised by what a reader is doing, not by how the
  project is built.** Milestones, design decisions, internals, the project
  layout, the validation record and the release process all sat in a reader's
  path and between them outnumbered the pages a user needs; they move under
  `project/`, and the sections are now Start here, Silent failures, Guides,
  Concepts, Reference, Notebooks and Project. The dimension catalogue moves to
  `reference/` on its merits — it is a lookup table, not a design document —
  and the API reference joins the CLI and errors pages there.
- Installation was a contributor setup page that told readers they needed a
  Rust toolchain, which has not been true since the release workflow began
  publishing five abi3 wheels. It is now `pip install ligase-bio`; the source
  build, the nine-check gate, the demo data and the docs build moved to
  `project/building.mdx`. **The MkDocs site still carries the stale claim.**
- The documentation rule in `CLAUDE.md` points at `web/content/` and says
  what the site generates rather than stores, and the `docs` extra in
  `pyproject.toml` is down to `marimo`. `mypy --strict` now covers
  `web/scripts`, the code most likely to break on an API change and least
  likely to be covered by a test.

- **The live site is the Nextra one.** `ligase.neoforgelabs.tech` is built
  from `web/` and deployed by a single `Docs` workflow, which never publishes
  from a pull request, queues deploys in the `pages` concurrency group, and
  puts `.nojekyll` in the artifact — a static Next.js export keeps every asset
  under `_next/`, which Jekyll would drop. The custom domain is untouched: it
  is configured on the repository's Pages REST endpoint, where a `CNAME` file
  in the artifact would have no effect.
- The ninth check of the gate is `npm run check` in `web/` rather than
  `mkdocs build --strict`. It regenerates, builds, and then fails on a broken
  link, a dangling anchor, a page nothing links to, or a Markdown link that
  never resolved.
- **The Rust API of the `ligase` crate changed shape with the set-valued
  organism.** `crates/ligase-core` is published, and `organism` held a single
  string in its public types, so making it a set could not be additive.
  `load::Declared.organism` is `Option<Vec<String>>` where it was
  `Option<String>`, which breaks every site that sets `organism`: wrap the one
  name in a list, `Some(vec![name])`. A site that leaves it to
  `..Declared::default()` is untouched. `dim::Organism` is now an alias
  of `dim::NameSet`, a set of names, where it was its own struct. The Python
  API is not affected.
- **A `Reference` now has a `release`.** Its applicable dimensions gained a
  fourth entry, `release`, and `unknown_reason("release")` returns a reason
  (`a FASTA does not record which release it came from; declare it`) where it
  used to return `None`. This is visible on both surfaces: `lg.reference(...)
  .applicable` is `('organism', 'reference', 'contig_naming', 'release')`
  rather than three names, so a script that iterates it and prompts for every
  unknown dimension will now ask for a release. Nothing is demanded: a
  reference with no release declared still loads and still checks.

### Removed

- **`dim::Organism::as_str`.** `Organism` is a set now, so there is no single
  string to borrow. `to_string()` (the `Display` impl) is the replacement, and
  returns `HIV-1` for a set of one and `HIV-1 and Homo sapiens` for two; a
  caller that wants the members takes `as_slice()`.
- **The MkDocs site.** `docs/`, `mkdocs.yml`, the `docs.yml` workflow and the
  `mkdocs`, `mkdocs-material` and `mkdocstrings` entries in the `docs` extra
  are gone; `marimo` stays, because the notebooks still need it. The MDX under
  `web/content/` is the source for every page now, so the design docs are
  `project/decisions.mdx`, `project/milestones.mdx`,
  `project/golden-example.mdx` and `reference/dimensions.mdx`, and the
  citations of them in the Rust and Python sources point there.
- `web/scripts/port_docs.py` and `web/scripts/gen_meta.py`, the two migration
  aids that carried pages and nav order over from `docs/`. They ran for the
  last time before `docs/` was deleted, and there is nothing left to port.

### Fixed

- **A tool that does not understand `--version` had its complaint recorded as
  its version — and the cache key therefore never moved when the tool did.**
  `exec::tool_version` ran `<tool> --version`, read stdout, fell back to
  stderr and **ignored the exit status**. `bwa` prints
  `[main] unrecognized command '--version'` and exits non-zero, so that
  sentence was bwa's recorded version, in provenance, in the methods
  paragraph and in the cache key. It is the same sentence for every bwa ever
  released, so upgrading the aligner moved no key at all and every alignment
  computed by the old one would have been served for the new. Preventing
  exactly that is the only thing `tool=` promises.

  There is now a version-probe table — arguments, whether a non-zero exit is
  expected, which line holds the version — and a probe that fails
  unexpectedly, or stops finding its line, is an error naming what to do. The
  suite's own stand-in tool changed from `wc` to `awk` for the same reason:
  `wc --version` is a BSD error message.

- **`bcftools mpileup -f` left a `.fai` inside the content-addressed store.**
  It writes an index beside the path it is handed, and in a run that path is
  an object whose name is the hash of its own bytes. An unaddressed file in an
  addressed store is a lie about what the store is, it would not travel with
  the object when the store gains a blob tier, and on a read-only or remote
  store it is not untidy but a failure. The library step indexes a symlink in
  a directory of its own, and `bwa index -p` likewise.

- **Every step the package exports was published in the API reference as a
  `repr()` line with its documentation dropped.** `@lg.step` replaces a
  function with a `Step` *instance*, which is neither a class nor a routine,
  so `generate_api.py` fell through to its plain-value branch — and the branch
  that fails the build on a missing docstring was never reached.
  `lg.map_identifiers` had been published as
  `map_identifiers: Step = <Step map_identifiers -> EntitySet>` since it was
  added. The generator now documents the function the decorator read, and its
  coverage contract walks nested submodules so that `steps.minimap2.align` —
  the name a reader actually calls — has to be grouped and documented rather
  than hidden behind `steps.minimap2`.

- **Documentation: fifteen false claims about what the engine is.** Nothing
  checks a hand-written sentence that counts things, and five milestones
  landing in succession left a trail of them. The loader table in
  [declaring dimensions](https://ligase.neoforgelabs.tech/guide/declaring-dimensions/)
  said "there are four loaders" above four rows when the Python surface
  exposes nine, and described `release` as applying to a reference and
  nothing else when a gene list and a ranking both carry it. The "in brief"
  dimension table in
  [types and dimensions](https://ligase.neoforgelabs.tech/concepts/types-and-dimensions/)
  omitted `identifier_namespace` altogether, counted the axes as ten rather
  than eleven, and said `subject` applied to no kind the engine reads.
  Three pages still announced the absence of things that M5 and M8 built: a
  methods paragraph and a reproducibility bundle, dimensions as "not
  converted", and conversions as "still v2". The
  [silent-failure index](https://ligase.neoforgelabs.tech/silent-failures/)
  listed seven hazards and the section has eight pages — the missing one
  being the gene-set overlap that M8 added. `LoadError` named five of the
  nine loaders that raise it; `CheckError` was said to catch five of its six
  subclasses; D35 counted six kinds of data where there are eight; the docs
  gate's generators were listed as five of six; the type system called eleven
  dimensions ten; a gene list was said to claim three axes where it claims
  four; and the `source_set` deduplication question was still described as
  open with M10 and M12, both of which have since answered it.

  Each fix was checked against the code rather than against another page —
  `Dimension::ALL`, `DataKind::ALL`, `types::applies`, `Conversion::ALL`, the
  `loading` group in `generate_api.py`, and `web/package.json`'s `generate`
  script — and where a claim was false the sentence was made true rather than
  made vague: the four-row loader table became a nine-row one, not "several
  loaders".

- **A `map_identifiers` step annotated to return a ranked list now declares
  the space it arrived in.** The conversion registry listed `EntitySet`
  alone, and the graph guards a conversion's axis override on whether the
  conversion covers the step's output kind — so such a step carried the
  *input's* identifier namespace onto an output that had crossed, which is a
  false statement about data produced by the engine itself. Found while
  adding `RankedList`, and not reachable before it existed.

- **A ranking and a gene list joined in one step now demand an identifier
  namespace of each other.** It was demanded between two gene lists and, by
  a catch-all arm, of nothing else — so the mixed pair, which is what
  scoring a ranking against a curated truth set is, could join by silence.
  Two lists that each say nothing overlapping in nothing is the failure the
  dimension exists to break.

- The fetch releases the GIL. Holding it through a minutes-long download would
  freeze every other Python thread, and it deadlocked outright against a local
  server a test was fetching *from* — the server's thread could not run while
  the fetch it was answering held the lock.


- **The errors reference showed a `ReferenceFrameError` nobody had run.** It
  was pasted in and hand-wrapped, and it named a fixture file
  (`hxb2_pol.fasta`) that does not exist, so a reader could not go and look at
  the input that produced it. It is captured from a live run now, by
  `web/scripts/capture_diagnostics.py`, against a committed *pol*-relative
  fixture — which means the docs build fails if the engine stops producing
  that error. Capturing it needed the script to be able to check a whole
  one-step pipeline, because a frame mismatch is a disagreement between an
  input and what the *step* reads, not between two inputs.
- **A pol-relative reference is refused rather than read as protease.** The
  HIV-1 gene windows are positions in HXB2 K03455 counted from the first
  base of the genome, and nothing checked that the reference handed to
  `lg.hivdb.interpret` was in that frame. Handed a pol-relative reference —
  K03455 2085..5096, so every position 2084 lower — the engine read what is
  really integrase as protease and reported intermediate
  protease-inhibitor resistance, with no error and no note. It is now a
  `ReferenceFrameError`, a `DimensionError` like every other disagreement
  about data, raised by `check()` before any compute and by the reading
  itself for a caller who arrives without a pipeline. Found by SAWA, and it
  is exactly the class of bug Ligase exists to prevent.
- **A gene barely covered no longer reports as susceptible.** A gene read in
  part refutes nothing and shows nothing, so every drug scored against it
  landed at zero, which the algorithm's ranges read as susceptible — a clean
  clinical negative from a gene the run never covered. SAWA saw a sequence
  with 32% of integrase report all five integrase inhibitors susceptible.

- **Three pages still said nine discrepancies at 2%.** The count dropped to
  three when codons started being read from the reads spanning them (D31),
  and the validation page was corrected with it. The home page's figure and
  the HIV-1 resistance page under `project/in-use/` were written for this
  site rather than ported to it, so the change that corrected the Markdown
  never reached them, and the site said both numbers at once. The home page
  also cited the validation page at the path it had before the docs switch.
- `demo'''s` on the milestones page, which the MkDocs page it was ported
  from had too.
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

### Fixed

- **Two writers of one object no longer trip over each other.** `FsStore`
  staged every write of a key at the same path, so two concurrent writers
  of the same object raced: the loser found its own partial file already
  renamed into place by the winner and failed with `NotFound` — an error on
  a put that had in fact succeeded. Content addressing is supposed to make
  that race benign, and it is, once the writers do not share a staging
  path. Reachable before the shared store by two fan-out nodes recording
  the same paginated response.

- **The aarch64 Linux wheel builds again, in a newer container.** v0.2.0's
  first tag died on `ring`:
  `#error "ARM assembler must define __ARM_ARCH"`. The cross toolchain behind
  `manylinux: auto` is CentOS 7's **GCC 4.8.5**, which defines `__aarch64__`
  and not `__ARM_ARCH` — and that pair is precisely the condition `ring`'s
  `asm_base.h` refuses. The `manylinux_2_28` image's GCC 7.5.0 defines
  `__ARM_ARCH 8`, in both the C and the `assembler-with-cpp` paths, so the
  aarch64 wheel is built there and carries a glibc 2.28 floor. x86_64 keeps
  manylinux2014 and glibc 2.17, which is the floor that matters for the
  cluster login nodes a lot of this audience is handed; no distro older than
  2.28 ran aarch64 in anger.

  Two things made it expensive out of proportion to the cause. `ring` is not
  a direct dependency — it arrived in v0.2 underneath `ureq`'s rustls, which
  is why the same workflow built v0.1.0's aarch64 wheel without complaint.
  And `verify-wheel`, PyPI and crates.io all `needs:` the wheels, so one
  broken target skipped every remaining job and the release published
  nothing.

- **A release can no longer publish nothing and call it success.** That same
  tag was pushed on a tree still calling itself `0.1.0`, with every entry in
  this file still under `[Unreleased]`, and nothing checked either one. Each
  publishing step is built to be re-runnable, which is the same thing as
  being quiet — PyPI skips a file it already has, the crates.io probe skips a
  version the index already lists — so with the wheel fixed, that release
  would have gone green having uploaded nothing at all.

  A `version` job now refuses a tag that disagrees with
  `[workspace.package]`, and refuses a version with no dated section in this
  file; `tests/test_import.py` asserts the second without CI's help. It is
  [D42](https://ligase.neoforgelabs.tech/project/decisions/) one layer out: a
  declaration nothing can check is a comment, and that includes the version
  number.

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

[Unreleased]: https://github.com/Neoforge-Labs/ligase/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/Neoforge-Labs/ligase/releases/tag/v0.3.0
[0.2.0]: https://github.com/Neoforge-Labs/ligase/releases/tag/v0.2.0
[0.1.0]: https://github.com/Neoforge-Labs/ligase/releases/tag/v0.1.0
