# Ligase v0.2 — Feature Specification

*Written against v0.1.0 at commit `8c6eb3f` (21 Oct 2026). Read alongside
CLAUDE.md, `web/content/project/decisions.mdx` and
`web/content/project/milestones.mdx`. This document adds decisions and
milestones M7–M14; it does not replace anything.*

> **Numbering correction (22 Oct 2026).** The spec as written numbered its new
> decisions D31–D39. `decisions.mdx` already holds D1–D34, so D31–D34 are
> taken: D31 is "a codon no read spans keeps its per-base reading", D33 is
> "the frame a reference is numbered along is declared, not guessed", and both
> are cited by live pages. The new decisions are therefore **D35–D43**,
> keeping the spec's relative order:
>
> | As written | In `decisions.mdx` | Subject |
> |---|---|---|
> | D31 | **D35** | a kind must bring a dimension that catches a silent failure |
> | D32 | **D36** | `Release` is its own dimension; `Reference` keeps its meaning |
> | D33 | **D37** | `Organism` is set-valued |
> | D34 | **D38** | identifier namespaces never coerce |
> | D35 | **D39** | a remote is pinned or recorded, and both are replayable |
> | D36 | **D40** | a stochastic step declares itself, or it is an error |
> | D37 | **D41** | in a ranked list, rank order is the result |
> | D38 | **D42** | a library step ships with its dimensions and citation |
> | D39 | **D43** | the store is a blob tier with pluggable backends |
>
> Every reference below uses the corrected numbers. References to D5, D6, D10,
> D22 and D26 are to the existing decisions of those numbers and are correct
> as written.

---

## 0. What v0.2 is for

v0.1 proved the idea on one pipeline: biological data carries dimensions, and
the engine refuses to connect steps whose dimensions disagree. It has no
outside users. v0.2 exists to change that, and every item in it was chosen by
one test: **does this make Ligase useful to a scientist who has never heard of
NeoRx or SAWA?**

Three things make the cut by that test and nothing else does:

1. **The commonest silent bug in biology** — gene and protein identifiers from
   one namespace joined to another — becomes a refused connection. This serves
   every RNA-seq, GWAS, proteomics and single-cell analysis ever written.
2. **The first step of every pipeline** — a fetch from SRA, Ensembl, UniProt,
   ChEMBL — comes inside Ligase's guarantees. Today it sits outside them,
   including in Ligase's own demo.
3. **The time cost of adopting Ligase drops to near zero**: `ligase inspect`
   works on files you already have, and a library of typed steps means you
   compose a pipeline instead of writing shell and a methods paragraph.

NeoRx is the forcing case for several of these and will be the second
application domain in Ligase's paper. It is not the reason for any of them.

---

## 1. The scope rule (D35)

> **A kind earns its place in Ligase only if it brings a dimension whose
> mismatch is a documented silent failure in real research.**

A kind that is a file format with a schema is a parser, and Ligase does not
need more parsers. Every new kind in this document names the silent failure
it exists to catch. Add this rule to CLAUDE.md under "What Ligase is". When a
future request fails the test, the answer is no, however convenient the kind
would be for one project.

Corollary: Ligase accepts non-sequence kinds, because the dimensions below are
as sharp as `Coordinates` and more widely botched.

---

## 2. Decisions

Numbered to follow `decisions.mdx`. Each becomes a `## Dnn` section there.

### D35 — A kind must bring a dimension that catches a documented silent failure
As §1. The test for every future kind.

### D36 — `Release` is its own dimension; `Reference` keeps its meaning
Generalising `Reference` to "the pinned external version this data is relative
to" is rejected. `Reference` compares by *containment* (D10): contig names,
lengths and MD5s, with extra contigs in the reference allowed. A database
release has no contigs and compares by *equality*. One dimension with two
comparison rules is the kind of quiet complexity Ligase exists to refuse.
`Release` is added as a separate dimension meaning "which versioned external
resource this data came from" — a ChEMBL version, an Ensembl release, an HGNC
download date, a snapshot timestamp.

### D37 — `Organism` is set-valued
A host–pathogen dataset, a metagenome, a xenograft and a disease knowledge
graph all legitimately hold more than one organism. `Organism` becomes an
ordered set; a single organism is a set of one. Comparison is set equality,
with the error naming the symmetric difference ("`aln` has HIV-1; `ref` has
HIV-1 and Homo sapiens"). This touches `check.rs`'s comparison semantics and
is the one change in v0.2 that must land before any new loader is written.

### D38 — Identifier namespaces never coerce
`HGNC`, `Ensembl gene`, `Entrez`, `UniProt accession`, `RefSeq`, `ChEMBL
target` and a curated list's own spelling are different spaces. Mapping
between them is a lookup with failures, not a rename, so the check refuses a
join across namespaces and the only way across is a `map_identifiers` step
that declares its output namespace and records its mapping source and
`Release`. Unmapped identifiers are *noticed* (D26), never dropped silently.

### D39 — A remote is pinned or recorded, and both are replayable
Two kinds of remote input, and conflating them would be the usual mistake:

| | `PinnedRemote` | `RecordedQuery` |
|---|---|---|
| Examples | an SRA run, a PDB entry, a numbered ChEMBL release, HXB2 by accession | a live Open Targets query, a REST call to Monarch |
| Promise | the same bytes forever, from the source; pinned by digest | the same bytes forever, from the record; the source promises nothing |
| On change at the source | loud failure: "the remote moved" | reported as *drift* by `--refresh`; never a silent re-fetch |
| Reproducible from | the source — anyone, anywhere, any time | the bundle — anyone holding the record |
| Audit verdict | **pinned** | **replayed** (or **live**, on the run that made the record) |

There are two different reproducibility claims here and the design keeps them
apart. *Source reproducibility* — that a third party will serve the same
answer next Tuesday — cannot be promised for a live query. *Record
reproducibility* — that anyone with the bundle can run the pipeline again and
get the same result — can, and it needs only one thing: a `RecordedQuery`
stores the canonicalised response **body** in the content-addressed store,
not just its digest. The body is then an ordinary input with a content hash,
and everything downstream is deterministic by construction. This is the
record/replay pattern of vcrpy and Nix fixed-output derivations; Ligase's
contribution is that the record is typed, content-addressed, part of the
cache key, and enforced by the engine rather than by discipline.

**Three modes, set per run, not per step:**

| Mode | What happens |
|---|---|
| record | no record exists for this query: fetch live, canonicalise, store the body, log locator + timestamp + digest in provenance |
| replay | a record exists: serve it; no network; the run report says "replayed from record of 2026-10-22" |
| refresh (`--refresh`) | fetch live anyway, compare against the record, and report *drift*: which queries changed and how |

Default is replay when a record exists and record when it does not, and the
run report always says which it did. **In replay mode a query with no record
is an error, never a silent live fetch** — a half-replayed run is the
plausible-but-wrong result Ligase exists to refuse.

A record's identity is the *canonical request*: method, URL with sorted
parameters, request body, nothing volatile. A step making twenty paginated
calls records twenty entries keyed by request, not by sequence, so call order
is irrelevant. Auth headers are stripped before storage; a bundle never
carries a secret. The response is canonicalised like any other kind — JSON
keys sorted; request IDs, server timestamps and rate-limit headers dropped —
so two responses that mean the same thing hash the same (the `@PG`-line
precedent, D22).

This retires the limitation in `python/ligase/provenance.py` that "a bundle
cannot fetch its own data": with recorded bodies in the bundle, it does not
need to. A reviewer replays the run with no network and no API keys. The
methods paragraph reads: "retrieved from Open Targets on 22 October 2026;
responses are included in the reproducibility bundle."

A surveillance pipeline that refreshes weekly is not reproducing one run; it
is a series of runs, each with its own record, each replayable forever.
`--refresh` is the weekly job and the records are the archive. **A refresh
never replaces a record: it writes a new dated record beside the old one**,
so every earlier run stays replayable, and the run report names both.

### D43 — The store is a blob tier with pluggable backends; the index is local and rebuildable
The content-addressed store (outputs, canonical forms, recorded bodies)
is defined by a small trait — `get(hash)`, `put(hash, bytes)`, `has(hash)` —
with the local filesystem as the only default backend. An optional backend
over Apache's `object_store` crate serves S3 and every S3-compatible store
(MinIO, Cloudflare R2, Backblaze), GCS and Azure, behind a cargo feature and
the `ligase-bio[remote-store]` extra, so the default install stays
zero-config (D6). This is the Nix binary-cache model: a local store plus a
remote substituter.

Two rules that are not negotiable. The SQLite metadata index **stays local**
— SQLite over object storage is a known failure — and is rebuildable from
the store, which it can be because everything in it derives from content
hashes. And **nothing read from a remote is trusted**: every object is hashed
on arrival and rejected if it does not match its key, because a shared cache
others can write to is otherwise a way to serve a wrong answer quickly.
Objects are written once and never overwritten. Credentials come from each
provider's standard chain (environment, profile, instance role), never from
Ligase's own configuration.

What this buys: a lab points every machine at one bucket (`LIGASE_STORE=s3://…`
or `ligase.toml`), one person aligns the cohort, and everyone else's
`ligase run` hits the cache. Bundles become *thin* (manifest, hashes, store
URL) for collaborators and *fat* (bodies included) for reviewers. It is also
the precondition for cluster execution in v0.3: workers need a shared store
before they can share work.

### D40 — A stochastic step declares itself, or it is an error
A step whose tool is stochastic (IQ-TREE, RAxML, BEAST, AutoDock Vina, any
model training) declares `stochastic=True` and names its seed parameter. An
unseeded stochastic step is a `StepDefinitionError` at decoration time, like
an untyped return. The audit vocabulary already distinguishes *deterministic*
from *reproducible*; this is what gives it teeth. "This pipeline is
reproducible" is a far stronger claim when the engine refuses to let you write
an irreproducible step by accident.

### D41 — In a ranked list, rank order is the result
`canon/report.rs` sorts rows because a report is a set of findings. A
`RankedList` is the opposite: order *is* the output. Its canonical form
preserves order. The module says so in a comment, or someone will "fix" it.

### D42 — A library step ships with its dimensions, its citation and its canonical form
`ligase.steps` is a curated library, not a convenience wrapper. A step is
admitted only when it declares its tool's version capture, its citation, the
dimensions of its output and how that output canonicalises. A step that
cannot say what it produces is not a library step.

---

## 3. New dimensions

Each adds a variant to `Dimension` in `crates/ligase-core/src/dim/mod.rs`
(bump `ALL`, `name()`, `leading_question()`), a comparison arm in
`crates/ligase-core/src/check.rs`, `applies()` rows in
`crates/ligase-core/src/types.rs`, and a row in
`web/content/reference/dimensions.mdx`.

| Dimension | Name | Meaning | Comparison | Leading question |
|---|---|---|---|---|
| `IdentifierNamespace` | `identifier_namespace` | which identifier space the entities live in | equality | "These must match. Add a `map_identifiers` step; identifiers are never renamed implicitly." |
| `Release` | `release` | which versioned external resource the data came from | equality | "These must match. Did one input come from a different release?" |
| `Subject` | `subject` | what the data is about: a disease, a cell line, a cohort | equality | "These must match. Did you join data about different subjects?" |
| `SourceSet` | `source_set` | ordered set of sources that contributed | set equality | "These must match. Was a source added or removed upstream?" |

`Organism` changes from scalar to set (D37). `ContigNaming`, `Coordinates`,
`Pairing` and `QualityEncoding` return `false` from `applies()` for every
non-sequence kind — a gene list has no coordinate system, and demanding one is
the category error `types.rs` already warns against.

---

## 4. New kinds

Per kind: a `DataKind` variant and `applies()` row in `types.rs`; a loader in
`load/<kind>.rs` that infers what it can and leaves the rest unknown; a
canonical form in `canon/<kind>.rs` via `Digest`; a dispatch arm and
description in `canon/mod.rs`; a `Dataset` subclass and `KINDS` entry in
`python/ligase/types.py`; a loader function on the Python surface.

### 4.1 `EntitySet` — the gene list
**Silent failure caught:** a gene list in one namespace joined to a table in
another; a curated truth set scored against results that spell genes
differently; a list whose symbols Excel has turned into dates.

An unordered set of entities, each with an identifier and optional
attributes. Format: TSV with a header; the identifier column named by the
loader argument (default `id`). Dimensions: `Organism`, `IdentifierNamespace`
(declared, or inferred when every identifier matches one namespace's pattern
unambiguously — e.g. `ENSG\d{11}`, `[OPQ][0-9][A-Z0-9]{3}[0-9]`), `Subject`,
`Release`. Canonical form: rows sorted by identifier; attribute columns in
header order; header kept as a row.

Covers: a DEG list, GWAS hits as a set, pathway membership, a curated
ground-truth set, a panel of resistance genes.

### 4.2 `RankedList` — the ranked table
**Silent failure caught:** the same as `EntitySet`, plus a rank-ordered result
compared to one in a different namespace or from a different release.

As `EntitySet` but ordered, and canonicalised in order (D41). Rank is the row
position; a `score` column is optional and, per `canon/mod.rs`, exact by
default — no tolerance until audit mode has measured drift.

Covers: ranked DEGs, ranked targets, a leaderboard of candidates.

### 4.3 `KnowledgeGraph` — v0.3
Typed nodes and edges with per-edge provenance; JSON. Dimensions: `Organism`
(set), `IdentifierNamespace`, `SourceSet`, `Subject`, `Release`. Canonical
form: nodes sorted by id, edges by `(source, target, type)`, source sets and
id sets sorted within a node; fetch timestamps, cache paths and response order
excluded. The hardest loader, so it goes last and after `SourceSet` has been
exercised by `EntitySet` and `RankedList`.

### 4.4 `MoleculeSet`, `BindingScores` and `Representation` — v0.3, optional extra
Ship behind `pip install ligase-bio[chem]` so RDKit never burdens the
genomics install. `Representation` records canonicalisation source and
standardisation state (salt-stripped, tautomer-normalised, charge-neutralised,
stereo kept); canonical form of a `MoleculeSet` is the sorted set of
InChIKeys, not SMILES strings. Measure InChIKey stability across RDKit
versions before committing to it as a canonical form.

---

## 5. New engine features

### 5.1 `map_identifiers` — the first typed conversion
A library step `EntitySet[ns=A] → EntitySet[ns=B]` (and the same for
`RankedList`) backed by a mapping table the user supplies or a bundled HGNC
or Ensembl mapping pinned as a `PinnedRemote` with its `Release`. Output
declares namespace B. Unmapped identifiers are noticed, counted, and listed
in the methods paragraph. This is the first edge in the **conversion graph**
(D5): design the registry so that `liftover` (reference build → reference
build) and `rename_contigs` (contig naming → contig naming) are the next two
entries without changing its shape. v0.2 ships only `map_identifiers`; the
registry is the point.

### 5.2 `ligase inspect`
`ligase inspect FILE [FILE...]`: load each file, print every dimension the
loader inferred with its source ("from SAM @SQ header"), and — given two or
more files — run the same comparison `check` would and print the diagnostic.
No pipeline, no decorator, no cache. This is the ten-second trial on files a
scientist already has. Exit non-zero on a mismatch so it works in a shell
pipeline.

### 5.3 Pinned remotes
`lg.remote(locator, digest=None, kind=...)` returns a `PinnedRemote` dataset
handle; on first run it fetches, records the digest in the provenance store,
and later runs verify before use and fail loudly on mismatch.
`lg.query(locator, kind=...)` returns a `RecordedQuery`; the engine records
the canonical request, stores the canonicalised response body in the
content-addressed store, and logs locator, timestamp and digest in provenance.
`ligase run` takes `--refresh` (re-fetch and report drift) and `--replay`
(strict: a missing record is an error); the default replays what exists and
records what does not, and says so. Both kinds appear in the methods paragraph
with what was fetched and when, and recorded bodies travel in the bundle.
Fetching is the engine's business (Rust, `reqwest`), with a closed set of
locator schemes.

M10 is also where the store trait of D43 is introduced, because records are
the first thing written to the store that a lab will want to share. The
filesystem backend is the only implementation until M14.

### 5.7 Remote store backend
The `object_store`-backed implementation of the D43 trait, behind the
`remote-store` cargo feature and the `ligase-bio[remote-store]` extra.
Configured by `LIGASE_STORE` or a `store` key in `ligase.toml`; a local
store is always kept as the first tier and the remote is consulted on a
local miss and written through on a local put. `ligase store` gains
`status` (what is configured, reachable, and how many objects are local
only) and `push`/`pull` for moving a run's objects explicitly. `ligase
methods --bundle` gains `--thin`.

Retrofit the HIV demo: `scripts/fetch_test_data.sh` becomes
`lg.remote("sra:SRR32217726", ...)` and `lg.remote("ncbi:K03455.1", ...)` in
`examples/hiv-resistance/pipeline.py`. The shell script is kept only for CI
caching until the cache can key on remote digests, then deleted.

### 5.4 Stochastic step declaration
`@lg.step(tool="iqtree", stochastic=True, seed="seed")`. The named parameter
must exist and be typed `int`; its absence is a `StepDefinitionError` at
decoration. Audit reports the step as *reproducible* when two runs with the
same seed agree and *not reproducible* when they differ, and never calls a
stochastic step *deterministic*. The methods paragraph states the seed.

### 5.5 `ligase.steps` — the typed step library
A new module, `python/ligase/steps/`, one file per tool. First five, chosen
for how many pipelines begin with them:

| Step | In → Out | Why first |
|---|---|---|
| `steps.fastp.trim` | `Reads → Reads` | nearly every pipeline's first real step |
| `steps.minimap2.align` | `Reads, Reference → Alignment` | the demo already uses it |
| `steps.bwa.mem` | `Reads, Reference → Alignment` | the other aligner everyone uses |
| `steps.samtools.sort`, `steps.samtools.index` | `Alignment → Alignment` | always |
| `steps.bcftools.mpileup_call` | `Alignment, Reference → Variants` | the demo already uses it |

Each ships per D42: tool version capture, a `cite=` reference that resolves
through `prov/cite.rs`, the output's dimensions, and a test that runs the
step on the tiny fixtures. The demo is rewritten on the library so its own
`align` and `call` definitions disappear — the demo should look like what a
user would write.

### 5.6 Claim binding — v0.3
Bind a published number to the step and run that produced it, and report
unbacked claims the way uncited tools are reported now. Extends `Methods` and
`MethodsGap`. Needs real runs to bind to, so it follows M13.

---

## 6. Milestones

One per session: re-read the design docs, plan, write failing tests,
implement, write docs (concepts, guide, architecture, `_meta.ts`,
`CHANGELOG.md`), then `npm run check` in `web/`.

### M7 — Foundations
Scope rule in CLAUDE.md. `Release`, `Subject`, `SourceSet` dimensions added
with `applies()` returning `false` for all existing kinds except `Release` on
`Reference` (a genome build has a release). `Organism` set-valued, with D37's
symmetric-difference error. D35–D37 written into `decisions.mdx`.

**Done when:** all existing tests pass unchanged; a `Reference` declared
`organism=["HIV-1", "Homo sapiens"]` checked against an `Alignment` declared
`organism="HIV-1"` fails with an error naming `Homo sapiens` as the
difference; `ligase check` on the HIV demo produces byte-identical output to
v0.1.0.

### M8 — Identifiers
`IdentifierNamespace` dimension; `EntitySet` kind with loader, canonical form
and Python surface; `map_identifiers` with a user-supplied mapping table; the
conversion registry it sits in. D38 written.

**Done when:** an `EntitySet` of HGNC symbols and one of Ensembl IDs refuse to
join with the D38 leading question; `map_identifiers` across them succeeds,
declares Ensembl, and notices every unmapped symbol by name; the fixture set
includes `SEPT7`-style symbols corrupted to dates and the loader notices them;
namespace inference is exercised by fixtures for HGNC, Ensembl, Entrez and
UniProt and declines to infer when a file mixes two.

### M9 — Inspect
`ligase inspect` as §5.2, generated into `reference/cli.mdx` by the existing
`generate_cli.py`.

**Done when:** `ligase inspect tiny_bare.fasta tiny_prefixed.sam` prints the
same `ContigNamingError` the home page shows and exits non-zero; a single
file prints every dimension with its source; the quickstart gains an
"inspect first" section captured by `capture_quickstart.py`.

### M10 — Remotes and replay
`PinnedRemote`, `RecordedQuery` with record/replay/refresh, the *pinned*,
*replayed* and *live* audit verdicts, drift reporting, recorded bodies in the
bundle, methods-paragraph wording. HIV demo retrofitted. D39 written.

**Done when:** the demo runs end to end from `lg.remote` calls with no shell
script; a deliberately wrong digest fails loudly before any step runs; a
pipeline with a `RecordedQuery` step runs once against a local mock server,
then runs again with the server **down** and completes from the record with
an identical result; `--replay` on a query with no record fails before any
step runs; `--refresh` against a mock whose response changed reports the
drift and does not overwrite the record without being told to; a bundle
exported from the recorded run replays on a clean machine with no network;
two paginated calls recorded in one order replay correctly when issued in the
other; a request made with an auth header stores no trace of it; the CI
`demo` job caches on remote digests.

### M11 — Stochastic steps
§5.4. D40 written.

**Done when:** decorating an unseeded `stochastic=True` step raises
`StepDefinitionError` naming the missing seed; a seeded stochastic step
audits as *reproducible* on two runs with the same seed; the methods paragraph
names the seed; the test uses a real stochastic tool on the tiny fixtures
(IQ-TREE on a small alignment, or a seeded Python step if the tool is absent).

### M12 — Ranked lists
`RankedList` kind, canonicalised in order. D41 written.

**Done when:** two files with identical rows in different orders produce
*different* canonical hashes (the inverse of every other kind's test, and the
test says so); a `RankedList` and an `EntitySet` in different namespaces
refuse to join; a `RankedList` with a `Release` of `HGNC 2026-09` refuses to
score against an `EntitySet` released `HGNC 2026-03`.

### M13 — Step library
`ligase.steps` with the five tools in §5.5; demo rewritten on it. D42 written.

**Done when:** `examples/hiv-resistance/pipeline.py` contains no `lg.sh` call
of its own; every library step has a fixture test, a citation that resolves,
and a docs page under `guide/steps/`; the validation suite
(`tests/test_validation.py`) still passes 81/81 at 15% and 5% with the same
nine explained discrepancies at 2%.

### M14 — Remote store
§5.7, on the trait M10 introduced. D43 written.

**Done when:** the full test suite passes against a MinIO container in CI
with `LIGASE_STORE` set, with no test changed; a second checkout on a clean
machine pointed at the same bucket completes the HIV demo entirely from
cache with zero steps run; an object in the bucket deliberately corrupted
in place is rejected on read with an error naming the hash mismatch, and
the step reruns; deleting the local SQLite index and running again rebuilds
it from the store; a thin bundle replays on a machine with store access and
fails with a clear message on one without; the default install has no
`object_store` dependency and `ligase store status` says so.

**v0.2 ships after M14.** v0.3 is `KnowledgeGraph`, claim binding, the
chemistry extra and cluster execution on the shared store, in that order,
with the NeoRx 7-disease experiment ported across as its validation case.

---

## 7. Out of scope for v0.2
`liftover` and `rename_contigs` (the registry is built; the conversions are
v0.3). `KnowledgeGraph`. Claim binding. Chemistry. Cluster execution (the
shared store is its precondition, not its delivery). Store garbage
collection. Any bucket management beyond reading and writing objects. A GUI.
Any lint over a project's own source identifiers (that is project policy, not
pipeline typing).

---

## 8. Open questions, settled in M7's plan

These were listed as open. They are answered here; the answers are the
premises the later milestones build on.

1. **Can `check.rs` express set-valued comparison without a wider refactor?**
   **Yes.** `Dim<T>` is already generic over the value type, so set-valuedness
   lives entirely in the value: `Organism` becomes a set type and nothing
   about `Dim`, `Dimensions`' shape, `check_step`'s loop or `graph::inherit`
   changes. The only engine edits are the value type itself and the
   `Dimension::Organism` arm of `compare`, which gains a symmetric-difference
   detail line. Verified against `dim/mod.rs` and `check.rs` at commit
   `17316ce`.

2. **Does namespace inference belong in the Rust loader or the Python
   surface?** **Rust.** `ligase inspect` (M9) then gets it without a Python
   round trip, and the two surfaces cannot disagree about what a file's
   identifiers are.

3. **The locator scheme for remotes.** A closed set of four: `sra:`, `ncbi:`,
   `ensembl:` and `https:`. Anything else is an error naming the four. M10.

4. **The size above which a recorded body is refused.** 32 MiB, with the
   refusal saying "pin this as a release instead". A response that large is a
   dataset, and a dataset has a version to pin. M10.

5. **Should the local tier evict?** **No — grow only in v0.2.** An LRU whose
   source of truth is a remote turns a cache miss into a network failure mode,
   and v0.2 has no measurement to size it with. `ligase store gc` is a v0.3
   item. M14.

Two notes on the spec's own references:

- `docs/design/neorx-expansion-notes.md` is not in this repository. Nothing in
  the milestones depends on it; where the spec cites it, the decision it
  informs is stated here in full.
- The spec cites `python/ligase/provenance.py`'s "a bundle cannot fetch its
  own data" limitation (D39) and `canon/report.rs`'s sorting (D41). Both are
  present and the citations are accurate.
