# Ligase

Type-safe, reproducible computational biology pipelines. A Rust core with a
Python surface.

## The idea

Biological data types carry **dimensions** — organism, reference build,
coordinate system, sample set — and the engine refuses to connect steps whose
dimensions do not match, the way a physics engine refuses to add metres to
seconds.

The worst comp bio bugs are silent. hg19 against hg38. 0-based against
1-based. `chr1` against `1`. Sample IDs that shifted by one somewhere in a
join. None of these crash; they produce plausible numbers that are wrong.
Workflow managers like Snakemake and Nextflow move files between steps very
well, but they do not understand what is in them. Ligase does, and it checks
before it computes.

```python
pipe.check()      # type-checks dimensions; no compute
pipe.run()        # parallel, cached, per-sample fan-out
pipe.methods()    # generated methods paragraph + reproducibility bundle
```

When a pipeline is wrong, you should find out in milliseconds rather than
after an overnight run:

```text
DimensionError in step `call`:
  aln.reference = HXB2 (from BAM @SQ header)
  ref.reference = hg38
  These must match. Did you pass the wrong reference?
```

## Status

**Pre-alpha**, building towards v0.1. Milestones **M0 (scaffold)**,
**M1 (types and dimensions)**, **M2 (the graph)** and
**M3 (execution and cache)** are complete.

You can write a pipeline of annotated steps, have the whole graph
type-checked without running any of it, be told every mistake at once, and
then run it — in parallel, cached, one branch per sample:

```python
import ligase as lg
from ligase.types import Alignment, Reads, Reference, Variants


@lg.step(tool="minimap2")
def align(reads: Reads, ref: Reference) -> Alignment:
    return lg.sh("minimap2 -ax sr {ref} {reads.r1} {reads.r2} | samtools sort -o {out}")


@lg.step(tool="bcftools")
def call(aln: Alignment, ref: Reference, min_freq: float = 0.01) -> Variants:
    return lg.sh("bcftools mpileup -f {ref} {aln} | bcftools call -mv -Oz -o {out}")


hxb2 = lg.reference("data/refs/HXB2.fasta", organism="HIV-1", build="HXB2")
samples = lg.samples("data/runs/*.fastq.gz", paired=True)

pipe = lg.Pipeline(samples.map(lambda s: call(align(s, hxb2), hxb2)))
pipe.check()        # type-checks every step. Nothing runs.
```

```console
$ ligase check
checked 8 steps in pipeline.py

DimensionError in step `call` (sample SRR32217714):
  aln.reference = HXB2 (from step `align`)
  ref.reference = hg38 (from FASTA contigs: data/refs/hg38.fasta)
  Contig `K03455.1` is not in hg38.
  These must match. Did you pass the wrong reference?
  The same problem affects 3 other samples: SRR32217715, SRR32217716, SRR32217726.

4 errors in 4 steps.
```

The step bodies never ran, `aln.reference` came out of a step that does not
exist yet, and a problem that hit the whole cohort is stated once with every
affected sample named.

Point it at the right reference and it runs:

```console
$ ligase run
running 12 steps in pipeline.py

ran       `align` (sample SRR8525904)        4.5s
ran       `call` (sample SRR8525904)         513ms
ran       `resistance` (sample SRR8525904)   3ms
...

12 steps: 12 ran.
```

Run it again and nothing is recomputed, because a step's key is built from
its code, its parameters, its inputs' *contents* and its tools' versions —
and none of those moved:

```console
$ ligase run
cached    `align` for SRR8525904, SRR8525908, SRR8525920, SRR8525942
cached    `call` for SRR8525904, SRR8525908, SRR8525920, SRR8525942
cached    `resistance` for SRR8525904, SRR8525908, SRR8525920, SRR8525942

12 steps: 12 from cache.
```

Change `min_freq` and only `call` and `resistance` come back. Add a sample
and only that sample is processed. Edit a step so that it reruns but produces
the same output, and everything below it is skipped.

Semantic hashing arrives in M4 and provenance in M5. See
[milestones](milestones.md) for the order and the acceptance test for each
step.

## Where to go

| If you want to | Read |
|---|---|
| Install it and run the checks | [Installation](guide/installation.md) |
| Understand how the project is put together | [Project layout](concepts/project-layout.md) |
| Know why the workspace is shaped this way | [Workspace architecture](architecture/workspace.md) |
| See the target API | [Golden example](golden-example.md) |
| Understand why data carries dimensions | [Types and dimensions](concepts/types-and-dimensions.md) |
| Declare dimensions and read the errors | [Declaring dimensions](guide/declaring-dimensions.md) |
| Write a pipeline and check it | [Building a pipeline](guide/building-a-pipeline.md) |
| Run one, and understand what was cached | [Running a pipeline](guide/running-a-pipeline.md) |
| Understand why steps are never run to build a graph | [Pipelines and the graph](concepts/pipelines.md) |
| Know how the checker works inside | [Type system](architecture/type-system.md) |
| Know how the graph works inside | [Graph](architecture/graph.md) |
| Understand why a step did or did not rerun | [Running, and not running](concepts/caching.md) |
| Know how a run is scheduled | [Execution](architecture/execution.md) |
| Know how the cache and its keys work | [Cache](architecture/cache.md) |
| Know which dimensions exist and how they are inferred | [Dimensions](dimensions.md) |
| Follow the build order | [Milestones](milestones.md) |
| Understand a design choice | [Decisions](decisions.md) |

## Names

| Thing | Name |
|---|---|
| Rust core crate | `ligase` |
| PyPI distribution | `ligase-bio` |
| Python import | `ligase` |

## Licence

Dual-licensed under either MIT or Apache-2.0, at your option.
