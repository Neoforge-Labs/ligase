---
title: 'Ligase: dimension checking for computational biology pipelines, so that mismatched data cannot be connected'
tags:
  - Rust
  - Python
  - computational biology
  - bioinformatics
  - reproducibility
  - workflow
  - type systems
  - provenance
authors:
  - name: Kelyn Paul Njeri
    orcid: 0009-0000-1068-4512
    affiliation: 1
affiliations:
  - name: NeoForge Labs, Independent Research
    index: 1
date: 30 October 2026
bibliography: paper.bib
---

# Summary

`Ligase` gives biological datasets the dimensions of what is inside them —
which organism, which reference build, which coordinate convention, which
samples in which order — and refuses to connect two steps whose dimensions
disagree. The refusal happens when the pipeline graph is built, before any
computation runs, so the failure mode is a diagnostic naming both sides and
where each was read from, rather than a number nobody checks.

Twelve dimensions are defined. Each is inferred from the file wherever the
format records it and declared once by the user where it does not: a SAM
`@SQ` header states contig names, lengths and optionally an MD5
[@li2009sam], a VCF header states its sample columns in order [@danecek2011],
and a FASTA states no organism at all. Reference identity is contig names
plus lengths plus checksums, never a filename, so two files both labelled
`GRCh38` are reported as different references when their sequence differs.

The engine is Rust; the user-facing API, decorators and CLI are Python.

# Statement of need

The costly errors in computational biology are the silent ones. Reads aligned
against GRCh37 and variants annotated against GRCh38; 0-based half-open
intervals handed to a tool expecting 1-based closed; `chr1` against `1`; a
sample column that shifted by one in a join. None of these crash. All of them
produce plausible numbers that are wrong, and the wrongness survives peer
review: converting variants between builds is a documented cautionary tale
[@ormond2021], gene identifiers silently corrupted by spreadsheet autoformat
appear in a fifth of papers with supplementary gene lists [@ziemann2016], and
the forensic reconstruction of published chemosensitivity results found
errors that reached clinical trial design [@baggerly2009].

Workflow engines do not catch this, and are not built to. Snakemake
[@molder2021], Nextflow [@ditommaso2017], the Common Workflow Language
[@crusoe2022] and Galaxy [@galaxy2022] manage *execution*: dependency order,
parallelism, containers, provenance of invocation. They are deliberately
agnostic about what flows between steps: a file path is an opaque token. A
rule that passes a GRCh37 alignment to a GRCh38 reference is a well-formed
rule. The mismatch is invisible because nothing in the execution layer looks
inside the file, and the recommendation to make pipelines reproducible
[@sandve2013; @nekrutenko2012] is satisfied by a run that reproduces the
wrong answer exactly.

`Ligase` types the data rather than the steps, and complements those tools
rather than replacing them: it has no scheduler and no cluster backend. What
it adds is a check unavailable at the execution layer by construction,
because it depends on reading headers and sequence rather than on moving
files. The documentation is organised around the failures themselves, one
page per symptom, each showing the diagnostic the engine really emits;
twenty-five are captured from live runs on every build, which fails if the
engine stops producing the error a page claims.

Dimensions that formats do not record are the ones that matter most, which is
why declaration is a first-class path and not a fallback. Neither FASTA nor
BAM carries a build identifier, so no amount of file inspection can recover
it: the user states it once, on load, and it propagates through the graph.
A declaration is still not taken on trust, and a declared build that
disagrees with contig identity is reported against the sequence.

# Reproducibility and provenance

A step's cache key is a hash over its code, its parameters, its inputs'
content digests and the versions of the external tools it calls, so a change
to any of them is a different computation and is not served from the cache.
Outputs are canonicalised before hashing — records sorted, volatile headers
such as timestamps stripped — so a tool that writes the same data in a
different order does not invalidate everything downstream. Random seeds are
explicit, typed, hashed parameters, and an audit mode runs a step twice to
report whether it is reproducible.

`ligase methods` writes a methods paragraph from the recorded run: tool
versions, parameters, references and citations. It names what it cannot
account for — a tool with no citation on file is reported as a gap
rather than omitted from the reference list, on the grounds that a tool
silently missing from a reference list lets an incomplete methods section
look complete.

A `Claim` binds a sentence a manuscript states to the computation behind it.
The sentence itself is never checked; everything else is — that the step
exists in this pipeline, that the parameter is one it takes, and that the run
held it at the stated value — and the result is cited by the step's output
digest. **A claim is bound to a computation, not verified against an
artifact**, unless it is also told where the number lives: given
`at=lg.cell(column="f1", row=0)` and a stated `tolerance`, `ligase methods`
reads that cell, compares it, and exits non-zero on a contradiction, so CI
fails when a manuscript's number stops matching its output. Without a
location the engine does not go looking, and a location it cannot read is
reported as unbacked — "I could not check", not "you are wrong".

# Validation and intended use

The reference pipeline is HIV-1 drug resistance: reads aligned to HXB2 with
BWA [@li2009bwa], pileup and variant calling with SAMtools and BCFtools
[@danecek2021], and interpretation by the Stanford HIVdb algorithm
[@liu2006]. The scoring engine is checked against 3,348 expectations from
Stanford's own Sierra implementation, all reproduced exactly; end to end on
public MiSeq data, 81 of 81 drug calls agree at the 15% and 5% ambiguity
thresholds, and the three that differ at 2% are enumerated with their cause
in a test that fails if one changes shape or a new one appears.

`Ligase` is research software. Its drug-resistance calls are **not for
clinical use** — not for diagnosis, not for treatment decisions, and not a
substitute for a validated clinical assay. The agreement above is evidence
that the implementation is faithful to the algorithm, not that it is fit for
patient care.

The package is on PyPI as `ligase-bio`, with a Bioconda recipe
[@gruning2018] for the dependency set the reference pipeline needs.

# References
