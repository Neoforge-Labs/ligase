# Reading what the engine noticed

A run can succeed and still tell you something is wrong with what it
produced. That is a **note**: a statement about an output that its hash
cannot carry. There is one of them in v0.1, and it is the one that cost a
reproducibility bundle its verification — a sample identifier that is a file
name rather than a sample.

This page is about recognising one and fixing it. For why the engine says it
rather than refusing or repairing it, see
[When a tool names a sample after a file](../concepts/notes.md).

## What it looks like

```console
$ ligase run
ran       `align` (sample SRR8525904)   4.5s
ran       `call` (sample SRR8525904)    513ms

noticed   `call` (sample SRR8525904): the sample identifier `/Users/kelyn/ligase/.ligase/store/62/7db9fa0cbac.bam` is an absolute path, not a sample
          Write the read group — `samtools addreplacerg -r ID:<sample> -r SM:<sample>` — so the calls below it name the sample.

8 steps: 8 ran.
```

Three things to read off that:

- The line begins `noticed`, not `failed`. The step ran and its output is in
  the store. Nothing was lost and nothing has to be rerun before you can
  look at the result.
- It **names its node**, the same way a check error does: you are told which
  step and which sample, not that something somewhere is wrong.
- The instruction is said once, however many samples are affected.

## Why it happened

`bcftools` reads a VCF's sample name from the alignment's `@RG SM` field.
`minimap2` does not write a read group, so there is nothing to read — and
`bcftools` falls back to naming the sample column after the **input file's
path**, which in a Ligase run is a file inside the content-addressed store.

The same thing happens to a BAM whose read group names a file, and to any
other tool that reaches for a filename when it cannot find an identifier.

## The fix

Write the read group where the alignment is made, so that every file below
it carries the sample it is actually about:

```python
@lg.step(tool=("minimap2", "samtools"))
def align(reads: Reads, ref: Reference, sample_id: str, threads: int = 4) -> Alignment:
    """Align one sample's reads against the reference."""
    return lg.sh(
        "minimap2 -t {threads} -ax sr {ref} {reads.r1} {reads.r2} "
        "| samtools addreplacerg -r ID:{sample_id} -r SM:{sample_id} -u -o - - "
        "| samtools sort -@ {threads} -o {out}"
    )
```

`sample_id` is an ordinary typed parameter, so pass the sample's own
identifier when you build the pipeline:

```python
samples = lg.samples("data/runs/*.fastq.gz", paired=True)

pipe = lg.Pipeline(
    [
        call(align(sample, hxb2, sample_id=run), hxb2)
        for run, sample in zip(samples.ids, samples, strict=True)
    ]
)
```

`samples.ids` is where the identifier comes from — the run accession
`lg.samples()` read off the filename — so nothing here is invented, and the
`.map()` shorthand is traded for a comprehension that can see both.

!!! tip "It will not split your cache"

    A parameter whose value is only the sample's own identifier is not a
    difference in *how* a step was run, so it does not give each sample its
    own sentence in the methods section and does not make the samples look
    like separate ways of running `align`.

Run it again and the note is gone:

```console
$ ligase run
ran       `align` (sample SRR8525904)   4.5s
ran       `call` (sample SRR8525904)    498ms

8 steps: 8 ran.
```

The alignments changed — they carry a read group now — so everything below
them reruns once. After that the pipeline is portable: the calls are named
after run accessions, and their canonical hashes no longer depend on where
your cache is.

## Reading it from Python

Every finished step carries its notes, so a script can act on them rather
than grep the report:

```python
report = pipe.run()

for step in report.steps:
    for note in step.notes:
        print(f"{step.step} ({step.sample}): {note}")
```

```text
call (SRR8525904): the sample identifier `/Users/kelyn/…/7db9fa0cbac.bam` is an absolute path, not a sample
```

To fail a CI job on one, which is a reasonable thing to want of a pipeline
that is supposed to be reproducible:

```python
noticed = [step for step in report.steps if step.notes]
if noticed:
    raise SystemExit(f"{len(noticed)} outputs are not portable")
```

## Where else it shows up

A note is stored with the result, not just printed once, so it is still
there when nothing ran:

```console
$ ligase run
cached    `call` for SRR8525904, SRR8525908, SRR8525920, SRR8525942

noticed   `call` (sample SRR8525904): the sample identifier `/Users/…/7db9fa0cbac.bam` is an absolute path, not a sample
          Write the read group — `samtools addreplacerg -r ID:<sample> -r SM:<sample>` — so the calls below it name the sample.

12 steps: 12 from cache.
```

A cache hit never reopens the file, so the note comes out of the cache
beside the hash. A warning that appeared on the first run and vanished on
the second would be worse than no warning at all.

It also reaches the three places a result is written up:

**`ligase methods`** says it after the paragraph and the references, so it is
outside the prose you would paste into a manuscript:

```console
$ ligase methods
`call` ran on 4 samples (SRR8525904, SRR8525908, SRR8525920, SRR8525942)
with bcftools 1.24, as `bcftools mpileup -a AD -f {ref} {aln} | ...`.

`call` ran on 4 samples (SRR8525904, SRR8525908, SRR8525920, SRR8525942),
where the sample identifier `/Users/kelyn/…/7db9fa0cbac.bam` is an absolute
path, not a sample — and the same for `/Users/kelyn/…/0c4e18baf21.bam`,
`/Users/kelyn/…/3a81d90ecc7.bam` and `/Users/kelyn/…/f20b7e4a115.bam`. A
sample set is a dimension, so the identifier is hashed exactly as the file
writes it. …
```

**A bundle** records every note in `notes.tsv` and gains a *What will not
travel* section in its `README.md`. The table is always written, even when
it is just a header line, so a reader never has to wonder whether an empty
file means "nothing to report" or "an older bundle".

**`ligase methods --verify`** reports it beside the verdicts, because this is
the second thing that explains a `differs` with no science behind it:

```console
$ ligase methods --verify bundle/
differs          `call` (SRR8525904)
    expected 9680fe10563f and got 4c1b77ae0d92

1 of 4 results name a file where a sample identifier belongs:
    `call` (SRR8525904): the sample identifier `/Users/…/7db9fa0cbac.bam` is an absolute path, not a sample
Such a hash depends on the file it was named after, so a `differs` here may be that and nothing else.
    Write the read group — `samtools addreplacerg -r ID:<sample> -r SM:<sample>` — so the calls below it name the sample.

4 results: 3 reproduced, 1 differs.
```

## What is not covered

- **Inputs you supply yourself.** Notes are about what a run *produced*. A
  VCF handed to the pipeline as a leaf input with path-shaped sample names is
  your file, described as you gave it, and nothing is said about it.
- **Every other way an identifier can be wrong.** A sample column holding
  `sample1` when it should hold `SRR8525904` is a mistake no engine can see.
  What is detectable here is a file *shape*, and only that.
- **Anything about whether the calls are right.** That is
  [M6](../milestones.md).

## Where to go next

| If you want to | Read |
|---|---|
| Know why the field is hashed at all | [When a tool names a sample after a file](../concepts/notes.md) |
| Know how a note is built and stored | [Canonicalisation](../architecture/canonicalisation.md#what-a-canonical-form-notices) |
| Write up a run, with or without notes | [Writing up a pipeline](provenance.md) |
