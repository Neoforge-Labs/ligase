# The HIV minority-variant paper — design

**Date:** 2026-11-05
**Status:** awaiting approval

## Intent, and why

A standalone clinical-methods paper whose claim is about HIV drug-resistance
genotyping practice, not about Ligase:

> The variant-calling strategy in a resistance pipeline silently determines
> whether minority resistance is reported at all. In 8 of 29 clinical
> samples with published Stanford reports, a majority-consensus reading
> returns *susceptible* for drugs that a 5% reading returns as resistant.

Ligase is the instrument. It is the second paper in the sequence, after the
JOSS software paper (already written, merged, awaiting a Zenodo archive and
submission), and it is deliberately not a tool paper: the audience is people
choosing how to call variants in a genotyping workflow, which is a much
larger audience than the one choosing a pipeline framework.

The reason this is worth writing is that the evidence is already sitting in
the repository and nobody has stated the result. Ligase's HIVdb fixtures hold
Stanford's own reports for 30 clinical samples at four readings — consensus,
Amb15, Amb5, Amb2 — published as open-access supplementary data by Dotrang et
al. The consensus column and the threshold columns disagree, clinically, and
the disagreement has never been quantified as a finding in its own right.

## The result, measured

Computed from `crates/ligase-core/tests/fixtures/hivdb/expected.tsv`,
restricted to the 30 clinical samples (`Sample_N_T1`), comparing Stanford's
`.consensus` report against each threshold report, counting drug calls that
move from level 1 (susceptible) to level 3 or above (low-level resistance or
worse):

| Reading | Samples | Drug calls | Susceptible → low-level+ | Samples affected |
|---|---|---|---|---|
| consensus → Amb15 | 30 | 810 | 12 | 5 / 30 (17%) |
| consensus → Amb5  | 30 | 810 | 26 | 8 / 30 (27%) |
| consensus → Amb2  | 30 | 810 | 38 | 10 / 30 (33%) |

Level changes are monotone — resistance is only ever gained as the threshold
drops, never lost — and every transition originates at level 1, so no
level-2-to-3 moves are being quietly folded in.

### Sample 5 is in the denominator, and why that took a correction

An earlier draft of this document said one sample had no Amb5 or Amb2 report
and set n = 29. That was wrong, and it is the kind of wrong a reviewer finds
in an afternoon against an open-access supplement. Dotrang et al. published
both reports for Sample 5; they are filed under a different case name:

- `Sample_5_T1` → `consensus`, `Amb15`
- `QUALITY_FAIL_Sample_5_T1` → `Amb5`, `Amb2` (27 drugs each, complete)

So the upstream study flagged Sample 5's lower-threshold reads as a quality
failure rather than withholding them. **Decision:** n = 30 throughout, with
Sample 5's flag stated in the methods and its rows marked in the supplement.
Sample 5 contributes no susceptible-to-resistant move at any threshold, so
every numerator above is unchanged and the result is insensitive to whether
it is included — which is worth one sentence in the paper, because a result
that survives the reader's preferred denominator is stronger than one that
needs ours.

### The unit of analysis is the sample, not the drug call

The 26 moved drug calls at Amb5 arise from **ten distinct minority mutations
in eight samples**:

| Sample | Calls | Mutation(s) |
|---|---|---|
| Sample 3 | 6 | `L100LI` |
| Sample 9 | 2 | `D67DN` |
| Sample 10 | 5 | `V90VI`, `K103KN`, `Y188YF`, `P225PH` |
| Sample 12 | 1 | `T69TADN` |
| Sample 18 | 7 | `V82VA` (6 PIs), `N348IT` (NVP) |
| Sample 20 | 3 | `A98AG` |
| Sample 26 | 1 | `N348IT` |
| Sample 28 | 1 | `N348IT` |

"26 of 810 drug calls" is therefore eight biological events counted
twenty-six times, and a clinical-virology reviewer will say so first.
**Decision:** the headline is the **per-sample proportion with a binomial
confidence interval**; drug-call counts are reported as descriptive counts
and never as a rate; the per-mutation table above goes in the main text,
because it is the honest unit of the biology and it is also the most
informative thing in the analysis.

## Goals

1. Quantify the effect across all 30 samples, by drug class and by threshold,
   against a published independent oracle.
2. Show from raw reads that the effect is a property of **pipeline
   construction**, not of reporting choice: a conventional caller lands on
   the consensus answer without anyone deciding that it should.
3. Every number in the manuscript bound with `lg.Claim`, so `ligase methods`
   exits non-zero when a number stops matching its output — scoped as below,
   because the binding mechanism runs the pipeline it checks.
4. Reproducible end to end from the repository by a reader.

## Non-goals

- Not a Ligase paper. The JOSS paper covers the software; this one cites it.
- No claim of clinical validity. The not-for-clinical-use position in the
  JOSS paper stands unchanged and is restated.
- No new resistance algorithm, and no argument with HIVdb. The algorithm is
  pinned at HIVDB_9.8 / Sierra 3.5.3 throughout.
- Not a reargument of the 2% sensitivity limit. It is a stated limitation.

## Anti-goals

Things that would make this a failure even if it published:

- **Reading as a tool advert.** If a reviewer's summary is "the authors
  describe their framework", the framing has failed. Leg 1 needs no Ligase
  at all, and the paper should be honest that the primary result is a
  reanalysis of published reports.
- **Overstating clinical consequence.** These are research specimens, and
  Dotrang et al. published the threshold reports themselves. The paper must
  not imply that any patient was mistreated, or that Stanford's service is at
  fault — the service offers all four readings. The finding is about what a
  pipeline hands it.
- **Quoting SAWA's unpublished cohort figures.** Those belong to SAWA.

## Constraints

- **Attribution.** Dotrang et al., *BMC Bioinformatics* 2025;26:168 is the
  source of both the reads (PRJNA1218827) and the oracle. CC BY 4.0, cited
  prominently, and the paper's contribution stated as reanalysis plus
  reproduction, not as new data.
- **All 30 samples are reachable.** ENA reports 32 runs for PRJNA1218827:
  Sample 1 through Sample 30 plus two no-template controls, mapped by
  `sample_alias` directly to the paper's sample numbers. Nothing rests on
  reading a figure.
- **Compute is modest.** 24k–163k reads per run; alignment is seconds per
  sample.
- **The 2% arm carries a known sensitivity limit.** Ligase aligns to HXB2
  where HIVGenoPipe realigns against a sample-specific reference, so some 2%
  disagreements are the alignment strategy's, not the caller's. Already
  documented as `KNOWN_AT_TWO_PERCENT`; reported as a limitation.

## Approaches

**A — the 30-sample comparison. Chosen.** Both legs below. It is the only
option that produces a rate rather than an anecdote, and the oracle for all
30 samples is already committed.

**B — a short report from the three samples already validated.** Fast, no
new compute, and n = 3 invites exactly the objection the paper exists to
survive. Rejected, but it is the fallback if leg 2 proves unexpectedly
expensive: leg 1 alone is already publishable as a research letter.

**C — a joint paper over SAWA's cohort.** Statistically the strongest and
the nearest to independent, but SAWA's agreement figures are unpublished and
theirs to report, so this is a co-authorship negotiation before it is a
paper. Rejected for now; it is the natural follow-up.

## The how, as decided

### Leg 1 — the reanalysis (no compute)

A committed script over the existing fixture producing the table above as a
TSV. This is the paper's Table 1 and the source of its headline numbers. It
needs no network, no tools and no reads, so a reader reproduces it in
seconds.

### Leg 2 — end to end from reads (compute)

Extend `tests/test_validation.py`'s `RUNS` from four runs to all 32, and run
each sample through the reference pipeline **twice from one alignment**:

- a *consensus arm*: `bcftools call -mv --ploidy 1`, the conventional
  construction, which D30 records as having been the demo's own original
  behaviour;
- a *minority arm*: pileup allele depths at each threshold, which is what the
  pipeline does now.

One alignment feeding both arms isolates the calling strategy as the only
variable. The prediction is that the consensus arm reproduces Stanford's
`.consensus` column and the minority arm reproduces the `Amb*` columns, from
raw reads — which is the step from "two columns in a supplement disagree" to
"the pipeline you built chose one for you".

This runs as a separate opt-in job, not in the per-commit `demo` job: 32 runs
with downloads is too heavy for every push. The existing four-run demo job
stays exactly as it is.

### The manuscript

At `paper/hiv-minority-variants/`, alongside the JOSS paper. Every stated
number carries `lg.Claim(..., at=lg.cell(column=..., row=...), tolerance=...)`
against the leg-1 and leg-2 outputs, so a figure edited in the prose and not
in the analysis fails CI (D61). This is also the demonstration the third
paper — on claim binding — would argue from, which is why it is worth the
extra work here rather than later.

**Venue:** *Journal of Clinical Microbiology* first, *Journal of
Antimicrobial Chemotherapy* second. Both take methods-comparison work with a
clinical-virology readership.

**Docs:** a page under `project/` recording the analysis and its numbers, per
the documentation rule. It belongs there and not in a user-facing section,
because it is written about Ligase's own validation work rather than for
somebody using Ligase.

## Four calls this document makes

Each was raised by a builder reading this document cold, and each is settled
here rather than left open.

### Leg 2's success criterion, and the gate at which it stops

Goal 2 is stated as a prediction and has to be falsifiable. Today the
consensus arm's behaviour is demonstrated on **one** sample (Sample 3, D30),
and the 15% and 5% agreement on **three**. Extending to 30 is where a
prediction either holds or does not.

**Success:** the consensus arm reproduces Stanford's `.consensus` level for
every drug call at 15% and 5%, and the minority arm reproduces the matching
`Amb*` level, with each disagreement enumerated and its cause established —
the standard `KNOWN_AT_TWO_PERCENT` already sets. Not a tolerated rate; that
test's design refuses one, and so does this.

**The gate:** if disagreements at 5% appear in more than three samples with
causes that cannot be established, stop. Leg 1 is publishable on its own as
a research letter (Approach B), and a leg 2 that half works is worth less
than no leg 2 and an honest letter.

**One technical trap, recorded because it would invalidate the arm
silently:** the consensus arm must not receive `mixture`. Codons are read
from read-level haplotypes with called alleles gating which combinations
count (D31), so a consensus arm handed a mixture threshold will re-find the
minority alleles through the pileup and cannot reproduce the consensus
column at all. The arm has to differ in its evidence source, not only in its
threshold.

### Claim binding is scoped to what can be checked cheaply

`Claim` binds to a step and `lg.cell` reads a cell of that step's output, and
`ligase methods` runs the pipeline before checking. Leg-2 claims would
therefore need the 32-run download-and-align job on every commit, which this
document forbids. So:

- **Leg 1's numbers bind per commit.** It reads a committed fixture, needs no
  network and no tools, and is cheap enough to run in CI. Every number in the
  abstract and Table 1 is a leg-1 number, which is the main reason to lead
  with leg 1.
- **Leg 2's summary is committed as a TSV** and a cheap step re-reads it, so
  leg-2 claims bind per commit against that file. The opt-in 32-run job is
  what regenerates it, and it must be green immediately before submission.
- Table 1's physical layout and the claim set are designed **together**,
  since `lg.cell` addresses a cell by column name and row index.

### The gene-coverage arm is framed as the algorithm's property, not Ligase's

The arm demonstrates that HIVdb's score ranges read an unread gene as
*susceptible*, because a gene that refutes nothing scores zero and zero is
the susceptible range. That is a property of the scoring algorithm and of
every tool that applies it without a coverage floor — it is not a story about
a Ligase defect that Ligase fixed. The framing goes in before the arm is
built, not after, because retrofitted framing reads as exactly the tool
advert anti-goal 1 refuses. D34 is cited for the floor Ligase now applies;
the finding is about what the algorithm does without one.

### Ethics and data availability

Secondary analysis of a CC BY 4.0 supplement and public SRA reads
(PRJNA1218827), de-identified at source. Draft statement: no new ethics
approval sought; no new specimens, no new sequencing, no re-identification
attempted. **This needs the author's check** rather than a decision — if
there is an upstream approval reference worth citing, or any restriction on
the SRA deposit, that is the statement instead, and it is not something to
guess at in a clinical submission.

## Corrected: where the JOSS paper lives

The builder check reported no `paper/` directory on `main`. That is true of
the stale local `main` and false of the repository: `paper/paper.md`,
`paper/paper.bib` and `paper/check_citations.py` are all on `origin/main`,
and the follow-up commit that brought the paper in line with M20 is an
ancestor of it. PR #49 merged on 3 November.

So this work branches from `origin/main`, `paper/` already exists, and there
is no conflict to sequence around. What remains on the JOSS paper is the
Zenodo archive and the submission itself, neither of which blocks this paper
— though whether this paper can cite it **by DOI** does depend on the archive
existing before submission, which is one more reason to do that first.

## Open — needs the author's decision

### 1. The headline number: 27%, or 20%?

`N348I` — a reverse-transcriptase connection-domain mutation at codon 348 —
drives the whole effect in Samples 26 and 28 and the NVP call in Sample 18.
Most clinical Sanger genotyping covers RT to about codon 240, some to 335;
codon 348 is outside what many assays report at all. Restricting the analysis
to mutations inside the window a standard genotype actually sequences:

| Scope | Drug calls | Samples affected |
|---|---|---|
| All positions | 26 | 8 / 30 (27%) |
| RT ≤ 240 | 23 | 6 / 30 (20%) |
| RT ≤ 335 | 23 | 6 / 30 (20%) |

Sample 18 survives the restriction on `V82VA`, a protease mutation. The
window figure is the same at 240 and at 335, which means the finding does not
depend on exactly where the line is drawn between them — a robustness point
worth stating either way.

**Recommendation:** lead with **20% within the standard window** and report
27% as the all-positions figure. The paper's audience chooses a calling
strategy for a genotyping workflow, so the number that speaks to them is the
one inside the region their assay covers, and the smaller honest number is
harder to argue with than the larger one. But this is the title number and
the abstract's first sentence, so it is the author's call, not this
document's.

A note for whoever builds it: the window filter must be **gene-aware**.
Protease codon 82 and RT codon 82 are different positions, and a filter on
the codon number alone happens to give the right answer here only because
`N348I` is the sole mutation above 240 in the set.

### 2. Authorship, and whether Dotrang et al. are told

A reanalysis of another group's supplementary data, submitted to a clinical
journal by the sole author of the instrument used, is a different submission
from one with a clinical-virology co-author. The project already applies this
scepticism to itself: the SAWA page states plainly that a shared author means
an evaluation is not independent validation. The same reasoning lands on this
paper, and a reviewer will reach it unaided.

Three things are undecided and only the author can decide them: whether to
recruit a clinical-virology co-author; whether to notify Dotrang et al. as a
courtesy before submission, which needs lead time if so; and how the paper
states its own position on independence. Nothing in the plan proceeds
differently until the first of those is settled, but the third has to be
written by the person whose independence is in question.

### 3. The gene-coverage arm

**The gene-coverage arm.** The SAWA evaluation found that a sequence covering
32% of integrase scored zero on every drug, which the algorithm's own ranges
read as *susceptible*, where Stanford's service declines to report the gene
(D34). That is a second silent route to a false-susceptible report and it
would strengthen the paper considerably. But it was found by SAWA, it is
unpublished, and whether it is Ligase's to publish or SAWA's is not a call
this document can make.

Three ways it could go: include it as a second arm with SAWA credited;
include only the mechanism, demonstrated on synthetic truncations of the 30
samples rather than on SAWA's data; or leave it out and let SAWA publish it.
The middle option is the one that needs no permission and still makes the
point, and is what this document assumes unless told otherwise.

## Left to the builder

- Table layout and which cuts go in the paper versus a supplement.
- How the consensus arm is expressed as a Ligase step — a parameter on the
  existing `call` step or a separate step in the library.
- Whether leg 2 reports per-drug or per-class agreement in the main text.
- Where the leg-1 script lives (`scripts/` or `paper/hiv-minority-variants/`)
  and whether its output is committed or regenerated.
