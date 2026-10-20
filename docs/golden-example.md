# Golden Example

This is the target user experience. Internals must serve this API, not the reverse. Names may evolve, but the shape is fixed.

One line has changed since this was written, and it is recorded here
rather than quietly amended. `resistance` was `lg.hivdb.interpret(v)`,
taking the calls alone. Implementing it in M6 showed that it cannot be
done: a VCF holds only the positions that *differ* from the reference, and
two of the three bases of a codon are usually not among them, so there is
nothing to translate without the reference sequence. The alignment is
there for coverage — `bcftools` writes only the sites it has something to
say about, so without it a sequencing dropout is indistinguishable from
agreement with the reference, and reads as a susceptible result. See
[D29](decisions.md#d29-an-interpretation-never-invents-a-susceptible-result).

```python
import ligase as lg
from ligase.types import Reads, Reference, Alignment, Variants, ResistanceReport

hxb2 = lg.reference("refs/HXB2.fasta", organism="HIV-1", build="HXB2")  # K03455
samples = lg.samples("data/*.fastq.gz", paired=True)

@lg.step(tool="minimap2")
def align(reads: Reads, ref: Reference) -> Alignment:
    return lg.sh("minimap2 -ax sr {ref} {reads.r1} {reads.r2} | samtools sort -o {out}")

@lg.step(tool="bcftools")
def call(aln: Alignment, ref: Reference, min_freq: float = 0.01) -> Variants:
    return lg.sh("bcftools mpileup -f {ref} {aln} | bcftools call -mv -Oz -o {out}")

@lg.step(tool="hivdb")
def resistance(v: Variants, ref: Reference, aln: Alignment) -> ResistanceReport:
    return lg.hivdb.interpret(v, ref, coverage=aln)

pipe = lg.Pipeline(
    samples.map(lambda s: resistance(call(aln := align(s, hxb2), hxb2), hxb2, aln))
)

pipe.check()      # type-checks dimensions; no compute
pipe.run()        # parallel, cached, per-sample fan-out
pipe.methods()    # generated methods paragraph + reproducibility bundle
```

## CLI equivalents

- `ligase check`
- `ligase run`
- `ligase methods`
- `ligase audit <step>`

## Expected failure the user should see

```
DimensionError in step `call`:
  aln.reference = HXB2 (from BAM @SQ header)
  ref.reference = hg38
  These must match. Did you pass the wrong reference?
```
