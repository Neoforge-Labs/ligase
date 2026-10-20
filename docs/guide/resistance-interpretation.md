# Interpreting resistance

A `resistance` step turns variant calls into a drug-by-drug report.

```python
import ligase as lg
from ligase.types import Alignment, Reference, ResistanceReport, Variants


@lg.step(tool="hivdb")
def resistance(
    v: Variants,
    ref: Reference,
    aln: Alignment,
    mixture: float = 0.15,
) -> ResistanceReport:
    return lg.hivdb.interpret(v, ref, coverage=aln, mixture=mixture)
```

Run the reference demo and look at one:

```bash
ligase run examples/hiv-resistance/pipeline.py
```

```text
gene  drug_class  drug  drug_name     score  level  interpretation                   mutations  unread
RT    NRTI        3TC   lamivudine    60     5      High-Level Resistance            M184V
RT    NRTI        AZT   zidovudine    -10    1      Susceptible                      M184V
IN    INSTI       DTG   dolutegravir  90     5      High-Level Resistance            T97A,E138T,G140S,Q148H
CA    CAI         LEN   lenacapavir   160    5      High-Level Resistance            M66I,Q67H,N74D,A105T
```

Negative scores are not a bug. `M184V` scores −10 for zidovudine, because
it genuinely resensitises the virus to it, and Stanford's own reports carry
the same number.

## The three arguments

`interpret` needs more than the calls, and neither addition is optional.

**The reference** is needed because a VCF holds only the positions that
*differ*. Two of the three bases of a codon are usually not in it, so there
is nothing to translate without the sequence those positions sit in.
Passing it also puts it under `check()`, so interpreting one sample's
calls against another organism's reference is refused before anything runs.

**The alignment** is needed for coverage. Leave it out and every position
the caller did not report is taken to be reference — which turns a
sequencing dropout into a susceptible result. You can leave it out; the
report will say in a note that it made that assumption.

## Thresholds

Two separate numbers, because they answer different questions.

| Parameter | On | Question |
|---|---|---|
| `min_freq` | the `call` step | what fraction of reads must support an allele for it to be *reported* |
| `mixture` | the `resistance` step | what fraction must support it for it to count as *present* |

Keep `min_freq` low and set `mixture` to the threshold you are willing to
act on. The published results Ligase is validated against use 2%, 5% and
15%, and the default is 0.15.

The difference is not academic. On Sample 3 of PRJNA1218827:

| `mixture` | Result |
|---|---|
| 0.15 | K219KE — potential low-level resistance to two NRTIs |
| 0.05 | adds L100LI — **high-level resistance to four NNRTIs** |

Same reads, same alignment, same algorithm. A 5% minority virus is the
difference between "this regimen is fine" and "most of this class is
already lost".

## Choosing an algorithm

```python
lg.hivdb.ALGORITHMS      # ('HIVDB_10.2', 'HIVDB_9.8')
lg.hivdb.DEFAULT         # 'HIVDB_9.8'
lg.hivdb.fingerprint()   # what goes in the cache key
```

The default is the *older* of the two, deliberately: it is the version the
published results in `tests/test_validation.py` were produced with, and a
default that disagreed with the suite's own reference answers would make
every validation run an argument about algorithm drift. Name
`algorithm="HIVDB_10.2"` for the current science.

```python
return lg.hivdb.interpret(v, ref, coverage=aln, algorithm="HIVDB_10.2")
```

Changing it reruns `resistance` and nothing above it, and the methods
paragraph reports which one was used.

## What a report will not tell you

- **A drug whose gene you did not sequence** reads `Not assessed`, with no
  score. Lenacapavir is scored against capsid, which is in *gag*; a *pol*
  pipeline gets nothing for it, and saying "Susceptible" would be inventing
  a negative result.
- **A score resting on positions you did not cover** is a floor. The
  `unread` column names them. The score is still the best lower bound
  available — a rule naming one of those positions might have matched.
- **Comments.** Stanford's reports carry explanatory prose for each
  mutation; Ligase reports levels, scores and mutations. The comment text
  is in the vendored XML and is the obvious thing to add if a report ever
  wants it.

## Adding a newer algorithm

See `crates/ligase-core/rulesets/README.md`. In short: drop the XML in,
add a row to `PROVENANCE.tsv` with its SHA-256, run
`scripts/vendor_hivdb_algorithm.py --write`, and run the tests.
