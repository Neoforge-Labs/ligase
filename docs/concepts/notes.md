# What a result rests on

A run can succeed and still have something worth saying about what it
produced. That is a **note**: a statement about an output that its hash
cannot carry, printed beside the result and never instead of it. A note is
not an error. The step ran, its output is in the store, and the run
succeeded.

Two things put one there, and they look for different kinds of trouble.

- **Reading the output** finds a field that cannot be dropped and is still
  wrong — a sample identifier that is a file name rather than a sample.
- **The step that wrote it** says what its work rested on, which no reading
  of the file could recover: a resistance reading made without coverage
  takes every position the caller did not report to be reference, and the
  report it writes looks exactly like one that had full coverage.

Both end up in the same place, because to a reader they are the same
question: what should I know about this result that the number does not
say?

## The field that cannot be dropped

Ligase drops a great deal from a file before hashing it. The command line
`minimap2` records, the timestamp `bcftools` writes beside its own — none of
that is a result, so none of it is part of what a file
[means](semantic-hashing.md). The rule that makes this safe is simple: drop
what says *when and how* a file was made, keep what says *what is in it*.

Some fields fall on the wrong side of that line and are still wrong.

A VCF names its samples on the `#CHROM` line, in the columns the genotypes
line up with:

```text
#CHROM  POS  ID  REF  ALT  QUAL  FILTER  INFO  FORMAT  SRR8525904
```

That is not how the file was made. It is **what the file is about** — a
sample set is one of the four [dimensions](types-and-dimensions.md) this
engine exists to keep straight, and dropping it would mean two VCFs of two
different patients hashing the same. So it stays in the hash.

Now run `bcftools` on an alignment that carries no read group:

```text
#CHROM  POS  ID  ...  FORMAT  /Users/kelyn/ligase/.ligase/store/62/7db9fa0cbac.bam
```

With no `@RG SM` to read, `bcftools` names the sample column after the input
file's path — and the input file, in a Ligase run, lives inside the
content-addressed store. The calls now carry a cache path where a sample
identifier belongs.

## Why that is worse than untidy

The hash of that VCF now depends on **where the cache directory is**. Two
machines running the same pipeline on the same data produce calls that are
byte-identical in every variant record and disagree on the one line that is
hashed. A [reproducibility bundle](provenance.md) written on one of them
cannot verify on the other: every sample comes back `differs`, with nothing
scientific behind it.

That is exactly how this was found. M5's bundle verification reported
`differs` on every sample of the reference pipeline, and the whole of the
difference was the last column of one line.

It is also wrong in two quieter ways. A generated methods paragraph names a
cache path where it should name a run accession. And the sample-set
dimension — the thing that is supposed to catch shuffled sample IDs — is
holding a file name, so it cannot catch anything.

## So the engine says what it saw

Ligase does not drop the identifier, because a sample set is a dimension. It
does not rewrite it either, because silently repairing a wrong file is how a
pipeline comes to produce plausible numbers nobody can account for — and in
this case the repair would not even work.

!!! note "Why not just use the basename?"

    Reducing `/…/store/62/7db9fa0cbac.bam` to `7db9fa0cbac` looks like it
    would make the hash portable. It would not. A file in the store is named
    after the hash of its **bytes**, and an alignment's bytes contain the
    staging path `samtools sort -o` recorded — which is inside the cache. The
    basename moves with the cache root exactly as the whole path does. It
    would also collapse two genuinely different samples whose file names
    agree.

What the engine does instead is **notice**, and tell you
([D26](../decisions.md#d26-a-field-that-cannot-be-dropped-is-noticed-instead)):

```console
$ ligase run
ran       `call` (sample SRR8525904)   513ms

noticed   `call` (sample SRR8525904): the sample identifier `/Users/kelyn/ligase/.ligase/store/62/7db9fa0cbac.bam` is an absolute path, not a sample
          Write the read group — `samtools addreplacerg -r ID:<sample> -r SM:<sample>` — so the calls below it name the sample.

4 steps: 4 ran.
```

A **note** is not an error. The step ran, its output is there, and the run
succeeded. It is a statement about something the hash cannot express, and it
travels with the result: into the run report, into the provenance record,
into the bundle and into the generated methods section. The fix is one line
in the pipeline, and it is a line only you can write.

## Why nothing louder

Refusing the file would be the obvious loud answer, and it is the wrong
layer for it. A VCF whose sample column holds a path is perfectly valid VCF;
canonicalisation has no business failing a run over a file that parses
([D21](../decisions.md#d21-canonicalisation-falls-back-to-the-bytes)).

Refusing it at `ligase check` time is loud *and* ineffective, which is worse.
The checker runs before any compute, so the only files it can see are
inputs — and the file with the wrong identifier is an **output** of the step
below. A check-time rule would have missed the failure that prompted all of
this, and fired only on hand-supplied files that are not wrong.

## What counts as a file

Narrowly, because a rule that guessed would report good identifiers as
mistakes and a warning nobody trusts gets ignored. An identifier is a file
when it:

- holds a path separator — `/` or `\` — or
- ends in an extension naming a format this engine reads: `.bam`, `.cram`,
  `.sam`, `.vcf`, `.bcf`, `.fastq`, `.fasta`, `.bed` and their usual
  spellings, with or without a `.gz`.

So `K03455.1` has a dot and is not a file. `SRR8525904`, `sample_a` and
`patient-04` have neither, and nothing is said about them.

## The assumption only the step can see

The other kind of note is not in the file at all, and no amount of reading
one would find it.

`bcftools call -mv` writes only the sites it called a variant at. A position
missing from a VCF is therefore ambiguous between two things with opposite
clinical meanings: the sample agrees with the reference, which is a
susceptible result, or nothing aligned there, which is no result at all.
Resolving that towards the first manufactures susceptible calls out of
dropouts, which is the thing
[D29](../decisions.md#d29-an-interpretation-never-invents-a-susceptible-result)
exists to refuse.

So [resistance interpretation](resistance-interpretation.md) reads the
alignment for coverage where it is given. Where it is not, it has to assume
something, and the assumption it makes has a direction — and here is the
problem the note solves. **The report cannot say so.** Its `unread` column
is empty when nothing is known to be unread, which is exactly what a
perfectly covered sample's report looks like. Two runs, one with full
coverage and one with no coverage at all, can come out byte for byte the
same.

Nothing downstream can recover the difference, so the step that knows says
it, and the engine carries it
([D32](../decisions.md#d32-a-step-says-what-its-own-work-rested-on)):

```console
$ ligase run
ran       `resistance` (sample SRR8525904)   1.2s

noticed   `resistance` (sample SRR8525904): no alignment was supplied, so coverage is unknown, no codon can be phased, and every position the caller did not report is taken to be reference
          Pass the alignment — `coverage=aln` — so uncalled positions are reported as unread rather than as reference.

3 steps: 3 ran.
```

The same channel carries the hedges a reading makes *with* an alignment,
which are the ones a real sample usually produces: a gene no codon of which
reached the depth asked for, a gene only partly read, and codons no read
spans all three bases of, which are read base by base and can name a
mutation no single read carries
([D31](../decisions.md#d31-a-codon-no-read-spans-keeps-its-per-base-reading)).

Each of those is the difference between a score and a floor, which is a
clinical distinction. None of them changes a result: a note never does.

## Why a note and not a column

A column in the report would travel with the file, which is a real
advantage, and it was not chosen for two reasons.

The report's columns are part of what a report *is* — the canonical hash
keeps the header as a row — so adding one changes every report's hash and
makes a comparison against Stanford's own reports harder to read, which is
what [validation](validation.md) rests on. And the sentence would be
repeated on all twenty-seven rows, because the hedge is about the reading
rather than about any one drug.

The deciding argument is that a note is not only for the person reading one
report. It reaches the run report, the provenance record, the bundle and
the methods section, in the same words each time, which is where a reviewer
asking "what did this rest on?" will be looking.

## Where to go next

| If you want to | Read |
|---|---|
| Read one, and fix it in a pipeline | [Reading what the engine noticed](../guide/notes.md) |
| Know why a sample set is hashed at all | [What a file means](semantic-hashing.md) |
| Know how a note is built, stored and said | [Notes](../architecture/notes.md) |
| Read the decisions and what was rejected | [D26](../decisions.md#d26-a-field-that-cannot-be-dropped-is-noticed-instead), [D32](../decisions.md#d32-a-step-says-what-its-own-work-rested-on) |
