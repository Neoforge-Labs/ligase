# Reading what the engine noticed

A run can succeed and still have something to tell you about what it
produced. That is a **note**: a statement about an output that its hash
cannot carry.

There are two kinds, and this page covers both. One is something wrong with
the file — a sample identifier that is a file name rather than a sample,
which is the note that cost a reproducibility bundle its verification. The
other is an assumption the step's own work rested on, which nothing in the
file it wrote would show: a resistance reading made without coverage, or
one whose codons did not all reach the depth it asked for.

The first has a fix. The second often does not, and is worth reading
anyway, because it is the difference between a score and a floor.

For why the engine says either rather than refusing or repairing, see
[What a result rests on](../concepts/notes.md).

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

## When a reading hedges

The other kind needs no fixing in the same sense: the step did what was
asked, and is telling you what that rested on. The one to watch for is the
alignment:

```python
@lg.step(tool="hivdb")
def resistance(v: Variants, ref: Reference) -> ResistanceReport:
    return lg.hivdb.interpret(v, ref)          # no coverage
```

```console
$ ligase run
ran       `resistance` (sample SRR8525904)   1.2s

noticed   `resistance` (sample SRR8525904): no alignment was supplied, so coverage is unknown, no codon can be phased, and every position the caller did not report is taken to be reference
          Pass the alignment — `coverage=aln` — so uncalled positions are reported as unread rather than as reference.

3 steps: 3 ran.
```

That one *is* a fix, and it is the advice: pass the alignment.

```python
@lg.step(tool="hivdb")
def resistance(v: Variants, ref: Reference, aln: Alignment) -> ResistanceReport:
    return lg.hivdb.interpret(v, ref, coverage=aln)
```

What you get back instead is the truth about the coverage you actually
have, which on a real amplicon run is usually not silence:

```console
$ ligase run
ran       `resistance` (sample SRR8525904)   1.2s
ran       `resistance` (sample SRR8525908)   3.6s

noticed   `resistance` (sample SRR8525904): no codon of CA reached 100-fold coverage, so no drug scored against it could be assessed
noticed   `resistance` (sample SRR8525904): 220 of RT's 560 codons reached 100-fold coverage; the rest are reported as unread rather than as reference
noticed   `resistance` (sample SRR8525908): no codon of IN reached 100-fold coverage, so no drug scored against it could be assessed
          Sequence deeper, or lower the depth this reading asks for — `depth=` — so this gene's drugs can be assessed at all.
          Read a drug whose `unread` column is not empty as a floor, and sequence deeper to close it.

6 steps: 6 ran.
```

Read those as follows:

- **A gene no codon of which was read.** Its drugs are in the report with
  every position they name listed as `unread`. That is no data, not a
  negative result — these samples are PR/RT amplicons, so capsid was never
  sequenced and lenacapavir cannot be assessed from them.
- **A gene partly read.** Every drug whose rules mention an unread position
  has its score read as a **floor**: the mutations it looks for were not all
  looked at.
- **Codons no read spans.** Read base by base, so they can name a mutation
  no single read carries, which errs towards reporting resistance rather
  than away from it
  ([D31](../decisions.md#d31-a-codon-no-read-spans-keeps-its-per-base-reading)).
  Longer reads are the only real answer.

None of these changes a number in the report, and none of them is a
failure. What they change is how far you should trust a susceptible call.

!!! warning "A lowered `depth` is not more coverage"

    `depth=50` makes more codons count as read and makes these notes
    quieter. It does not make the reads exist. The honest use of it is
    deciding what depth a result should rest on, in the open, where the
    methods paragraph will say so.

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
resistance (SRR8525904): 220 of RT's 560 codons reached 100-fold coverage; the rest are reported as unread rather than as reference
```

To fail a CI job on one, which is a reasonable thing to want of a pipeline
that is supposed to be reproducible:

```python
noticed = [step for step in report.steps if step.notes]
if noticed:
    raise SystemExit(f"{len(noticed)} outputs have something to say")
```

Or to refuse only the one assumption you will not accept, which for a
clinical report is usually the coverage one:

```python
blind = [
    step
    for step in report.steps
    for note in step.notes
    if "coverage is unknown" in note
]
if blind:
    raise SystemExit("a resistance report was interpreted without coverage")
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

**A bundle** records every note in `notes.tsv` and gains a *What was
noticed* section in its `README.md`. The table is always written, even when
it is just a header line, so a reader never has to wonder whether an empty
file means "nothing to report" or "an older bundle".

**`ligase methods --verify`** reports the *first* kind beside the verdicts,
because a sample identifier that is a file is the second thing that
explains a `differs` with no science behind it. A reading's hedges are not
listed there: they are the same on every machine, so offering one as an
explanation for a hash that moved would send you after the wrong thing.

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
- **A shell step's assumptions.** A body can say what its work rested on; a
  command cannot be asked. A step that shells out and wants to hedge has to
  say it in the file it writes.

## Where to go next

| If you want to | Read |
|---|---|
| Know why the engine notices rather than refuses | [What a result rests on](../concepts/notes.md) |
| Know how a note is built, stored and said | [Notes](../architecture/notes.md) |
| Have a step of your own say what it rested on | [Notes](../architecture/notes.md#a-step-says-the-other) |
| Know what coverage buys a reading | [Interpreting resistance](resistance-interpretation.md) |
| Write up a run, with or without notes | [Writing up a pipeline](provenance.md) |
