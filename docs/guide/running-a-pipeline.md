# Running a pipeline

Everything here runs today. [Building a pipeline](building-a-pipeline.md)
gets you as far as `pipe.check()`; this goes the rest of the way.

## Say how a step produces its output

A step's body is never asked to compute anything. It is called once, before
the run starts, and what it returns says *how* the output is produced:

```python
import ligase as lg
from ligase.types import Alignment, Reads, Reference, Variants


@lg.step(tool=("minimap2", "samtools"))
def align(reads: Reads, ref: Reference) -> Alignment:
    return lg.sh(
        "minimap2 -ax sr {ref} {reads.r1} {reads.r2} | samtools sort -o {out}"
    )
```

`lg.sh` builds a command; it does not run one, and it does not format the
string. The engine fills the placeholders in once it knows where everything
is:

| Placeholder | What it becomes |
|---|---|
| `{ref}` | the file bound to the parameter `ref` |
| `{reads.r1}`, `{reads.r2}` | the mates of a paired input |
| `{min_freq}` | a plain parameter's value |
| `{out}` | the file this step must write |

Two rules are worth knowing before you write a long command.

**`{out}` is the only output.** A step that writes anywhere else has
produced nothing, as far as the engine is concerned, and is reported as
having finished without writing its output.

**Every substituted value is one shell word.** A path with a space in it
stays one path. For a literal brace — an `awk` program, say — double it:

```python
return lg.sh("awk '{{print $1, $4}}' {regions} > {out}")
```

That is [Snakemake's convention](https://snakemake.readthedocs.io), and it
exists because `{print $1}` and `{refrence}` are otherwise
indistinguishable: one of them is a typo you want to be told about.

## Name the tools a step calls

`tool=` is how a tool's version reaches the cache key:

```python
@lg.step(tool="bcftools")                       # one tool
@lg.step(tool=("minimap2", "samtools"))         # a pipe of two
```

Each one's `--version` is hashed, so upgrading bcftools reruns the calling
steps and nothing else. A tool that is not installed fails the step before
it runs, naming the tool — a typo in `tool="minimap"` is a problem you want
at second zero, not at hour six.

## Steps whose work is Python

Not everything shells out. `lg.write` hands the engine a function to call
with the path to write to:

```python
@lg.step
def resistance(v: Variants) -> ResistanceReport:
    return lg.write(lambda out: interpret(v.file, out))
```

`v.file` works *inside* the function because by the time it is called, the
steps above it have run. Reading it earlier raises, and says so. It is typed
as a path rather than an optional one for the same reason: a body reached
through `lg.write` is a body whose inputs exist.

(`v.path`, which every dataset also has, is the honest answer to "where did
this come from, if anywhere?" and so can be `None`. Use `.file` when you are
about to open it.)

These steps take the GIL, so they do not run in parallel with each other —
the right trade for an interpretation step, the wrong one for anything
doing real work. Prefer `lg.sh`.

## Run it

```python
pipe = lg.Pipeline(samples.map(lambda s: resistance(call(align(s, hxb2), hxb2))))

report = pipe.run()
```

Or from the command line:

```console
$ ligase run pipeline.py
running 12 steps in pipeline.py

ran       `align` (sample SRR8525904)        4.5s
ran       `call` (sample SRR8525904)         513ms
ran       `resistance` (sample SRR8525904)   3ms
...

12 steps: 12 ran.
```

The pipeline is checked first, every time. A pipeline that does not
type-check does not run at all:

```console
$ ligase run pipeline.py
running 12 steps in pipeline.py

DimensionError in step `call` (sample SRR8525904):
  aln.reference = HXB2 (from step `align`)
  ref.reference = hg38 (from FASTA contigs: data/refs/hg38.fasta)
  These must match. Did you pass the wrong reference?

4 errors in 4 steps.
```

| Option | What it does |
|---|---|
| `-j N`, `--jobs N` | how many steps may run at once (default: one per core) |
| `--force` | run every step, cache or no cache |
| `--cache DIR` | where to keep the cache (default: `./.ligase`) |

`ligase run` exits 0 when everything worked, 1 when a step failed or the
pipeline has errors, and 2 when the pipeline could not be read at all.

## Read the report

```python
report = pipe.run()

report.summary            # '12 steps: 3 ran, 9 from cache.'
len(report.ran)           # 3
report.outputs("call")    # one path per sample, in graph order

for step in report.steps:
    print(step.step, step.sample, step.outcome, step.duration_ms)
```

An outcome is one of three things:

- **`ran`** — it ran.
- **`cached`** — its key was already known, and nothing above it ran.
- **`skipped`** — its key was already known *although* something above it
  reran, because that rerun produced the same output as before. This is
  [early cutoff](../concepts/caching.md#why-outputs-are-named-after-their-contents).

`report.hashed` and `report.prefiltered` say how many input files had to be
read against how many were recognised from their size and modification
time.

## When a step fails

`pipe.run()` raises, and the exception carries the whole run — including
everything that did work, which for a cohort is usually most of it:

```python
try:
    report = pipe.run()
except lg.RunError as error:
    print(error.report.text)
    for failure in error.report.failures:
        print(failure.step, failure.sample, failure.message)
```

One sample failing does not take the others down with it: independent
branches keep going, and the steps *below* a failure are simply not run. A
failure is reported once, against the node that caused it, in the tool's own
words:

```console
`align` (sample SRR8525942) failed:
  the command failed with exit status 1
  [E::sam_parse1] missing SAM header

11 steps: 9 ran, 1 from cache, 1 failed.
```

A failed step leaves nothing in the cache, so fixing the cause and running
again retries exactly that step.

## Where the outputs are

In the store, named after the hash of their contents:

```python
>>> report.outputs("resistance")
['/work/.ligase/store/3c/73431de6...eeaa85.tsv']
```

Reach them by asking the report, or through the handle the step returned:

```python
aligned = align(sample, hxb2)
pipe = lg.Pipeline(call(aligned, hxb2))
pipe.run()

lg.handle(aligned).file    # where that alignment landed
```

The store is derived data. Deleting `.ligase/` costs you a rerun and
nothing else.

## Try it on the real thing

The repository ships the reference demo — HIV-1 reads, HXB2, real tools:

```console
$ scripts/fetch_test_data.sh --project PRJNA517147
$ ligase run examples/hiv-resistance/pipeline.py
```

Then run it a second time, change `min_freq` in the file and run it again,
and watch which steps come back.
