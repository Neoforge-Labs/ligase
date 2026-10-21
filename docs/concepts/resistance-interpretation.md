# Resistance interpretation

Every other part of Ligase is about keeping data straight. This is the one
part that says what the data *means*: it takes variant calls against HXB2,
works out which amino acids each codon of protease, reverse transcriptase,
integrase and capsid holds, and scores them against the Stanford HIVdb
algorithm.

```python
@lg.step(tool="hivdb")
def resistance(
    v: Variants, ref: Reference, aln: Alignment
) -> ResistanceReport:
    return lg.hivdb.interpret(v, ref, coverage=aln)
```

The output is one row per drug: a score, a level from Susceptible to
High-Level Resistance, the mutations that got it there, and the algorithm
that decided.

## The rules are vendored, not fetched

Stanford publishes the HIVdb algorithm as ASI2 XML in its own repository,
[`hivdb/hivfacts`](https://github.com/hivdb/hivfacts), under the Unlicense
— a dedication to the public domain. Ligase ships two versions of it,
pinned by checksum: **9.8**, which is what the published results it is
validated against were produced with, and **10.2**, which is current.

Stanford also runs a web service that does this interpretation. Ligase
does not call it, and the reason is the same one behind every other
decision here: an answer that depends on the day it was asked for is not
reproducible. No cache key can see a remote service change its mind, and
no [reproducibility bundle](provenance.md) can carry it. `docs/decisions.md`
D8 puts public-data fetching out of scope for v0.1 in any case.

So the rules are part of the engine, and *which* rules were used is part
of the cache key — by the digest of their bytes rather than by their
version number. Upstream publishes corrections under near-identical names
(`HIVDB_10.0` and `HIVDB_10.0-1` both exist), and a key that moved only
with the name would happily serve a result computed under rules that have
since changed.

## Four judgements, all made the same way

An interpretation can be wrong in two directions, and they are not
equally bad. Reporting resistance that is not there costs a patient a drug
they could have taken. Reporting *susceptibility* that is not there costs
them a drug that will fail. Everywhere these two could be traded off,
Ligase takes the first.

### A mixture is every amino acid it could be

Where the reads support both the reference base and an alternate above the
mixture threshold, the codon holds **both** amino acids, and a rule is
satisfied if any of them satisfies it.

An ambiguous position is two viruses, not an average of one. Taking the
majority base would discard the minority virus — which is the entire
reason anyone sequences deeply, and which in practice is where resistance
shows up first.

### A codon is only the combinations the reads carry

A mixture at two bases of one codon is where the first judgement could go
wrong on its own. A VCF says what is at each position, not what travels
together, so enumerating the combinations of two ambiguous bases invents
codons no read carries.

On Sample 3 of PRJNA1218827 the caller credits G at the first base of RT
codon 219 in 43% of the reads there and at the second base in 2.4%. At a
2% threshold both pass, and the four combinations spell lysine, arginine,
glutamate and glycine — where the reads spanning the whole codon carry two
of them: `AAA` in 54% of themselves and `GAA` in 45%. The other two are
under two tenths of a per cent each.

So the alignment settles it. Reads spanning all three bases are counted as
the triplets they carry, and the codon holds the amino acids whose share
of those reads reaches the mixture threshold. Two rules keep this from
going wrong in the other direction:

- **Phasing narrows a reading and never widens one.** What is *present* at
  a position stays the caller's judgement, which weighs base quality; the
  reads only decide which combinations of called alleles are real.
- **A codon no read spans keeps its per-base reading**, noted rather than
  dropped or guessed at — see
  [D31](../decisions.md#d31-a-codon-no-read-spans-keeps-its-per-base-reading),
  which also says how far that note is visible today.

### A position that was not read is not a wild-type position

`bcftools` writes only the sites it has something to say about. A position
missing from a VCF is therefore ambiguous between "agrees with the
reference" and "nothing aligned here" — and those have opposite clinical
meanings.

So coverage is read from the alignment. A codon whose bases are not all
covered to the required depth is **unread**, not reference, and a drug
whose rules mention an unread position reports its score as a floor rather
than as an answer, naming the positions it could not see.

Without an alignment nothing can be known about coverage, and the reading
says so in a [note](notes.md) rather than assuming quietly. That assumption
has a direction — it manufactures susceptible results out of dropouts — and
the one thing it must never be is silent. The report itself cannot say it,
since a report with nothing *known* to be unread looks exactly like one
that was fully covered, so the step hands the note to the engine and the
engine says it: in the run report, in provenance, in a bundle and in the
methods section
([D32](../decisions.md#d32-a-step-says-what-its-own-work-rested-on)).

The hedges a reading makes *with* an alignment travel the same way. A gene
no codon of which reached the depth asked for, a gene only partly read, and
codons no read spans all three bases of are each the difference between a
score and a floor, and each is said rather than left for someone to infer
from an `unread` column.

The test that pins this is a no-template control. Sixty of its
thirty-two thousand reads map to HXB2. A pipeline that read "no variant
called" as "agrees with the reference" would report a tube of water as
fully susceptible to all twenty-seven drugs.

### A gene nobody sequenced is not assessed

HIVDB 9.8 scores lenacapavir against capsid, which is in *gag*. A pipeline
that sequenced only *pol* has nothing whatever to say about it — so the
report says "Not assessed" rather than "Susceptible".

This is [D24](../decisions.md#d24-a-citation-is-curated-or-declared-never-guessed)'s
reasoning about citations applied to a drug class: a gap that is named is
honest, and a gap that is filled with a cheerful default is not.

## What it is checked against

The scoring engine reproduces **3,348 published Stanford HIVdb results**
exactly — thirty clinical samples at four ambiguity thresholds, every drug,
every score, every level. Those expectations come from Sierra 3.5.3, a
different implementation in a different language by the algorithm's own
authors, so agreeing with them is evidence rather than circularity.

The pipeline end to end is checked in [validation](validation.md).

## Reading more

- [Using it](../guide/resistance-interpretation.md) — the guide, with a
  runnable example.
- [How it works](../architecture/hivdb.md) — the rule language, the
  scoring, and why the ruleset is derived rather than parsed as XML.
- [What it agrees and disagrees with](validation.md) — M6's acceptance.
