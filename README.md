# Ligase

**Computational biology pipelines that refuse to run when the data does not
line up.**

Biological data carries *dimensions* — which organism, which reference build,
which coordinate system, which samples in which order, which identifier
namespace. Ligase reads those dimensions out of your files, propagates them
through the pipeline, and type-checks every connection when it builds the
graph, before a single step runs. The way a physics engine refuses to add
metres to seconds.

```bash
pip install ligase-bio
```

A Rust engine with a Python surface. Documentation:
**[ligase.neoforgelabs.tech](https://ligase.neoforgelabs.tech)**

## Why it exists

The worst bugs in this field do not crash. They produce a plausible number.

Align reads to hg19 and call variants against hg38, and you get a VCF — a
well-formed one, full of positions quietly shifted. Join a `chr1`-named
alignment to a `1`-named reference and the caller finds no contig in common,
reports no variants and exits zero, which reads downstream as a sample with
nothing wrong with it. Merge two VCFs whose sample columns happen to be in
different orders, and every genotype lands under the wrong subject.

None of these is exotic, and none is really a programming error. Each is a
*unit* error — a length added to a time — and the data was carrying enough
information to catch it the whole time. Workflow managers move files between
tools without looking inside them, so they cannot catch any of it. Ligase
opens the files and knows what they are about.

## What that looks like

A step that takes an alignment and a reference, given two files from
different builds:

```text
DimensionError in step `call`:
  ref.reference = GRCh37 (from FASTA contigs: crates/ligase-core/tests/fixtures/tiny_bare.fasta)
  aln.reference = GRCh38 (from SAM @SQ header: crates/ligase-core/tests/fixtures/tiny_other.sam)
  Contig `1`: same name and length but different sequence (MD5 44e7acaf against a965a71a).
  These must match. Did you pass the wrong reference?
```

Every diagnostic names the two values, **where each was read from**, and what
to do about it. Nothing ran, and nothing was written.

Those paths are the engine's own test fixtures, because every block of output
on this page was captured from a real run rather than written by hand — the
documentation site regenerates them on every build and fails if the engine
stops producing them. Two more:

```text
DimensionError in step `merge`:
  left.sample_set = [sample_a, sample_b] (from VCF header sample columns: crates/ligase-core/tests/fixtures/tiny_two_samples.vcf)
  right.sample_set = [sample_b, sample_a] (from VCF header sample columns: crates/ligase-core/tests/fixtures/tiny_swapped_samples.vcf)
  The same samples appear in both, in a different order.
  These must match, in order. Did the sample order change upstream?
```

```text
DimensionError in step `score`:
  truth.identifier_namespace = HGNC symbol (from the identifiers' own pattern: crates/ligase-core/tests/fixtures/entities_hgnc.tsv)
  found.identifier_namespace = Ensembl gene (from the identifiers' own pattern: crates/ligase-core/tests/fixtures/entities_ensembl.tsv)
  These must match. Add a `map_identifiers` step; identifiers are never renamed implicitly.
```

That last line is the house style: Ligase will not quietly fix a mismatch for
you. It tells you which conversion to add, and the conversion is a step in
your pipeline with its own cache key and its own line in the write-up.

## Try it without writing a pipeline

The quickest way to see the idea is on files you already have. No pipeline,
no decorators, nothing executed:

```bash
ligase inspect refs/genome.fasta aln/sample01.sam
```

```text
Reference  refs/genome.fasta
  genome.fasta.reference = 2 contigs, 40 bp (from FASTA contigs: refs/genome.fasta)
  genome.fasta.contig_naming = bare (Ensembl/NCBI style) (from FASTA deflines: refs/genome.fasta)
  genome.fasta.organism is unknown (a FASTA does not record its organism; declare it)
  genome.fasta.release is unknown (a FASTA does not record which release it came from; declare it)
  coordinates, sample_set, pairing, quality_encoding, subject, source_set and identifier_namespace do not apply to a Reference.

Alignment  aln/sample01.sam
  sample01.sam.reference = 2 contigs, 40 bp (from SAM @SQ header: aln/sample01.sam)
  sample01.sam.contig_naming = chr-prefixed (UCSC style) (from SAM @SQ header: aln/sample01.sam)
  sample01.sam.coordinates = 1-based closed (from SAM/BAM record convention: aln/sample01.sam)
  sample01.sam.sample_set = [sample01] (from SAM @RG header: aln/sample01.sam)
  sample01.sam.organism is unknown (not recorded in SAM/BAM headers; declare it)
  sample01.sam.pairing is unknown (recorded in the SAM flags, which reading the header alone does not reach)
  quality_encoding, release, subject, source_set and identifier_namespace do not apply to an Alignment.

ContigNamingError in step `inspect`:
  genome.fasta.contig_naming = bare (Ensembl/NCBI style) (from FASTA deflines: refs/genome.fasta)
  sample01.sam.contig_naming = chr-prefixed (UCSC style) (from SAM @SQ header: aln/sample01.sam)
  The contigs are otherwise identical. Add a renaming step, or use one naming style throughout.

2 files, compared on reference and contig_naming: 1 disagreement.
```

Three things are worth noticing. Each value says where it came from, so you
can go and look rather than guess. The dimensions with no value say *why*
they have none, which is how you find out what declaring is for. And the last
line of each block names the dimensions that **do not apply** — a FASTA is
not missing a quality encoding, it has no such property to have.

It exits non-zero, so it composes with whatever you already use:

```bash
ligase inspect refs/*.fasta aln/*.bam && snakemake
```

The [quickstart](https://ligase.neoforgelabs.tech/start/quickstart/) takes
this to a running pipeline in five minutes, with no aligner and no downloads.

## A pipeline

A step is an ordinary Python function. The type annotations are not
decoration — they are how the engine knows what each argument is, which
dimensions apply to it, and what comes out:

```python
import ligase as lg
from ligase.steps import bcftools, minimap2

HXB2 = "1f460b25ab23b95345d579c234c43ca6d0abf7c1adf6aeffb43b974b2b1941ad"
SRR8525904 = "a7800f3dace6464faa08c34ce765f0398ab883c76c5475102318dae6c3d21923"


@lg.step(tool="hivdb")
def resistance(
    v: lg.Variants, ref: lg.Reference, aln: lg.Alignment
) -> lg.ResistanceReport:
    """Interpret the calls as drug resistance."""
    return lg.hivdb.interpret(v, ref, coverage=aln)


hxb2 = lg.remote(
    "ncbi:K03455.1", kind=lg.Reference, digest=HXB2, organism="HIV-1", build="HXB2"
)
reads = lg.remote(
    "sra:SRR8525904",
    kind=lg.Reads,
    digest=SRR8525904,
    organism="HIV-1",
    samples=["SRR8525904"],
)

aligned = minimap2.align(reads, hxb2, sample_id="SRR8525904")
called = bcftools.mpileup_call(aligned, hxb2, min_freq=0.01)

pipe = lg.Pipeline(resistance(called, hxb2, aligned))
```

`lg.Pipeline(...)` **runs nothing** — calling a step records a call rather
than performing one, which is what leaves something to type-check. Then:

```bash
ligase check     # the whole graph, before any compute
ligase run       # compute what is out of date
ligase audit     # run a step twice and say whether it is reproducible
ligase methods   # the paragraph you paste into a paper
```

Declared dimensions propagate. `organism="HIV-1"` is stated once on the
reference and the reads, and every output below inherits it, so a second
pipeline that mixes in another organism's reference is refused at the join
rather than three steps later.

## How it works

**Dimensions are inferred wherever the file records them, and declared
exactly once where it does not.** A reference's identity comes from its
contig names, lengths and MD5 checksums — never from its filename. A SAM's
coordinate system comes from the specification. An organism is nowhere in a
FASTA, so you say it, and it travels. There are eleven axes today; the
[catalogue](https://ligase.neoforgelabs.tech/reference/dimensions/) lists
each one with the file fields it is read from.

**The graph is built from your function signatures, without executing
anything,** and checked in one pass, so `ligase check` reports every
mismatch in the pipeline at once rather than the first one.

**The cache is content-addressed.** A step's key is a hash of its code, its
parameters, its inputs' *contents* and its external tools' versions. Change
the thread count and the alignment reruns, produces the same canonical
output, and everything downstream is skipped — outputs are canonicalised
before hashing, so sort order and timestamps do not count as changes.
Per-sample fan-out means adding one sample computes one sample.

**Randomness has to declare itself.** A step that samples or searches states
`stochastic=True` and names the parameter holding its seed, or it is an error
at import time. `ligase audit` then distinguishes *deterministic* from
*reproducible* — the second meaning "the same bytes both times, because the
seed was held, not because the step cannot wobble".

**Remote inputs are inputs.** `lg.remote("sra:SRR8525904", digest=...)` puts
the fetch inside the engine's guarantees: pinned by accession and digest,
checked before any step runs, stored content-addressed, and named in the
methods paragraph. The digest is over what the file *means* — its canonical
form — so a reflow at NCBI does not break your pin.

**Provenance is a product, not a log.** `ligase methods` writes the
methods-section paragraph, naming every tool at the version it actually ran
at, every parameter as it was hashed, and every reference by accession and
checksum; `--bundle` packages it so somebody else can check the result.

## What it catches today

| Silent failure | The dimension that catches it |
|---|---|
| hg19 reads against hg38 annotations | `reference` (contig names, lengths, MD5) |
| `chr1` against `1` | `contig_naming` |
| BED and GFF coordinates added together | `coordinates` |
| Sample columns reordered upstream | `sample_set` (by identity *and* order) |
| HGNC symbols joined to Ensembl IDs | `identifier_namespace` |
| Host and pathogen data crossed | `organism` (set-valued) |
| A reference numbered along another frame | reference frame, declared |
| Annotations from two resource releases | `release` |

Each has a page of its own under
[silent failures](https://ligase.neoforgelabs.tech/silent-failures/), titled
by the symptom you would actually notice — "my gene-set overlap is empty" —
with the error the engine really produces.

Eleven kinds carry dimensions today: `Reads`, `Reference`, `Alignment`,
`Variants`, `Intervals`, `EntitySet`, `RankedList`, `KnowledgeGraph`,
`MoleculeSet`, `MoleculeRanking` and `ResistanceReport`. A kind earns its
place only by bringing a dimension whose mismatch is a documented silent
failure, or a canonical form whose absence would make the cache serve a wrong
answer; a kind that is merely a file format with a schema is a parser, and
Ligase is not trying to be a parser collection.

## Install

```bash
pip install ligase-bio
```

Wheels cover Linux (x86_64 and aarch64), macOS (Intel and Apple silicon) and
Windows, for Python 3.11 and newer, so this needs no compiler. Building from
source instead needs a Rust toolchain *and* a C compiler, because SQLite is
compiled from bundled source — see the
[installation guide](https://ligase.neoforgelabs.tech/start/installation/).

The Rust engine on its own:

```toml
[dependencies]
ligase = "0.4"
```

For development:

```bash
git clone git@github.com:Neoforge-Labs/ligase.git && cd ligase
pip install -e ".[dev]"
```

## Status

**v0.4.0**, and pre-alpha: the API will change, and the project has no
outside users yet.

v0.1 built the foundations — the type system, the graph, the
content-addressed cache, semantic hashing with early cutoff, provenance, and
validation against published resistance results. v0.2 is about being useful
to someone who has never heard of the project: gene and protein identifiers
as a dimension, remote inputs pinned and replayable, stochastic steps that
declare themselves, `ligase inspect` on files you already have, a curated
[step library](https://ligase.neoforgelabs.tech/project/architecture/step-library/)
so a pipeline composes rather than shells out, and a store with a bucket
behind it so a second machine can finish a pipeline from cache.

[The milestones](https://ligase.neoforgelabs.tech/project/milestones/) record
what each one had to prove, including the clauses that were met differently
and why. v0.3 added claim binding, so a number in a manuscript is checked
against the computation behind it; a chemistry extra, with two molecule kinds
and the standardisation that makes them comparable; and cluster execution, so
several machines run one pipeline between them through the shared store, with
no scheduler and no job submission.

v0.4 is the first release the project did not plan. Two integrations pinned
it, used it on their own data, and reported what they found: the identifier
dimension was guessing a space it could not read and two guesses were
agreeing, so a join across two identifier spaces passed; the methods write-up
was reporting a citation gap nobody could close; and a claim's printed
identifier moved under edits that could not change the number. A claim can
now name the cell its number lives in, so a manuscript's number is checked
against the artifact rather than merely bound to the computation. **The
identifier change refuses pipelines that used to pass** — see
[MIGRATING.md](https://github.com/Neoforge-Labs/ligase/blob/main/MIGRATING.md),
which exists because of this release and is checked by the one after it.

## Intended use

Ligase is **research software**. The drug-resistance calls in its reference
pipeline come from applying the Stanford HIVdb algorithm to variants Ligase
called, and they are **not for clinical use** — not for diagnosis, not for
treatment decisions, and not a substitute for a validated clinical assay or a
clinician's judgement.

The [validation](https://ligase.neoforgelabs.tech/project/validation/) checks
those calls against published results on two public datasets: 81 of 81 drug
calls agree exactly at the 15% and 5% thresholds, and the three that differ
at 2% each carry a stated cause. That is evidence the implementation is
faithful to the algorithm. It is not evidence that it is fit for patient
care, and it was never meant to be.

## Documentation

| | |
|---|---|
| [Quickstart](https://ligase.neoforgelabs.tech/start/quickstart/) | five minutes, no aligner, no downloads |
| [Silent failures](https://ligase.neoforgelabs.tech/silent-failures/) | the bugs it catches, titled by symptom |
| [Guides](https://ligase.neoforgelabs.tech/guide/building-a-pipeline/) | one task at a time |
| [Concepts](https://ligase.neoforgelabs.tech/concepts/types-and-dimensions/) | why it works this way |
| [Reference](https://ligase.neoforgelabs.tech/reference/cli/) | CLI, errors, dimensions, API |
| [Decisions](https://ligase.neoforgelabs.tech/project/decisions/) | every design choice, with its argument |

## Names

| Thing | Name |
|---|---|
| Rust core crate | [`ligase`](https://crates.io/crates/ligase) |
| PyPI distribution | [`ligase-bio`](https://pypi.org/project/ligase-bio/) |
| Python import | `ligase` |

## Licence

Dual-licensed under either
[MIT](https://github.com/Neoforge-Labs/ligase/blob/main/LICENSE-MIT) or
[Apache-2.0](https://github.com/Neoforge-Labs/ligase/blob/main/LICENSE-APACHE),
at your option.
