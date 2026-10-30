# Migrating

Every change that can alter a pipeline's **results** or refuse something it
used to accept is written here, newest first, with what to do about it.

This file exists because a changelog body is not a release signal. A
consumer pinned `ligase-bio>=0.1.0` picks a minor bump up on their next
lock, and two integrations lost a debugging cycle to migrations that were
documented only inside a long entry nobody reads before upgrading
([D57](https://ligase.neoforgelabs.tech/project/decisions/#d57--a-change-that-alters-results-is-named-where-a-pin-is-chosen)).

**What is in here.** A refusal that did not used to happen, a default whose
value changes what a report contains, a change to what a number means, and
a renamed or re-typed public API. Each entry says what breaks, what the
symptom looks like, and the exact edit.

**What is not.** New capability you can ignore, performance, internals,
docs. `CHANGELOG.md` has all of it.

**How to read the headings.** A `## <version>` section is everything that
needed migrating in that release. Versions with no section needed nothing.

## Unreleased

### A list of bare gene symbols no longer states its own identifier space

**What breaks.** A pipeline that joined two gene lists without declaring
`identifier_namespace`, where the identifiers are bare symbols (`TP53`,
`DHFR-TS`, `MT-CYB`). It now refuses at `pipeline.check()`, before anything
runs.

**Why.** The symbol shape is the only one in the catalogue with no marker,
and a ChEMBL component synonym, an in-house panel and an approved HGNC
symbol are the same string shape. Through 0.3.0 all of them inferred
`HGNC symbol`, and because both sides of a join inferred the same wrong
answer, the join passed: `ligase check` reported no problems and exited 0
for a pipeline scoring a ranking against a truth set across two identifier
spaces. One consumer measured 0 of 7 entities recovered undeclared against
2 of 7 after an explicit mapping step, with the undeclared pipeline
reporting neither.

**The symptom.**

```
UndeclaredDimensionError in step `join`:
  ranked.identifier_namespace is unknown (not declared, and the identifiers
  are bare gene-symbol shapes -- symbols and several synonym spaces are
  indistinguishable by pattern, so declare identifier_namespace)
  ...
  `join` needs identifier_namespace to agree, and nothing states it.
  Declare it where you load the data.
```

**The edit.** One argument per load:

```diff
-ranked = lg.ranked_list("ranked.tsv", organism="Homo sapiens")
+ranked = lg.ranked_list(
+    "ranked.tsv",
+    organism="Homo sapiens",
+    identifier_namespace="HGNC symbol",
+)
```

A declaration is recorded and not checked against the identifiers, so an
in-house spelling (`"NeoRx panel v3"`) is accepted on your word. The six
marker-bearing spaces — `Ensembl gene`, `UniProt accession`, `RefSeq`,
`HGNC ID`, `ChEMBL target`, `Entrez gene` — infer exactly as before.

**Rust callers:** `KnownNamespace::infer` returns `Inference` rather than
`Option<&KnownNamespace>`. `.settled()` gives the old value, so the change
is a compile error rather than a different answer.

### `ligase methods` prints a claim's output digest, not its step key

**What breaks.** Nothing runs differently. What changes is the identifier in
the write-up: a bound claim's sentence ends `output 4a366b6cfeb0` where it
used to end `at key b4884fc9df9aa706`, and a claim over several samples
lists `Outputs:` where it listed `Keys:`.

**Why.** The key is the identity of a computation, so it moves when the
step's code, parameters, inputs or tool versions move — including under
`cite=`, which changes no number and reruns nothing. A manuscript's
identifier has to survive those edits; the output digest does, and it is
what `audit` and a bundle verify against.

**The edit.** If you quoted a claim's key in a manuscript, requote the
digest. Scripts reading `bound.key` are unaffected: the key is unchanged in
provenance, in `claims.tsv` and on that attribute.

### The gene-coverage floor is counted over scored codons

**What breaks.** Nothing refuses that used to pass, but `lg.hivdb.interpret`
answers differently for partial records, in both directions, and every
`hivdb` step reruns once.

**Why.** `gene_coverage` asked what share of a gene's own codons a reading
covered. Reverse transcriptase is 560 codons and HIVDB 9.8 scores 40 of
them, the highest at 348, so the 0.8 default asked for 448 — a floor no
partial-pol record could clear whatever it covered of what the rules
consult. A record covering K03455 2550..3869 — the full p51 reverse
transcriptase, and every position any rule scores — returned no RT drugs at
all, eight codons short of 448, with the entire shortfall falling in RNase
H.

It is not only a loosening. A dropout across RT 40–120 leaves 86% of the
gene read, which the old floor waved through, and takes 21 of the 40 scored
positions, leaving 19 against a floor of 32 — refused now where it used to
be scored.

**The edit.** None, unless you had set `gene_coverage` to work around the
old denominator. `gene_coverage=0.0` restores the per-drug `unread`
behaviour exactly, which was the workaround in use; with the floor counted
over scored codons it should no longer be needed.

**Caches.** A `hivdb` step's tool fingerprint now ends `read by ligase
<version>`, because this changes what the engine answers for identical
calls, reference, parameters and rules. Without that, every existing cache
would have gone on serving the old refusals.

## 0.3.0 and 0.2.0

### `lg.reference()` requires `frame=` for a reference that is not a whole genome

**What breaks.** Any `lg.reference()` call on a sub-genomic or padded
reference. Both releases shipped it; a consumer's padded pol reference
(2084 leading `N`s, 5096 bases, named `K03455`) is refused by each.

**Why.** A reference whose coordinates are not the build's own produces a
silent coordinate shift, which in one consumer's evaluation invented
Intermediate-level PI resistance. The frame is declared, never guessed
([D33](https://ligase.neoforgelabs.tech/project/decisions/#d33--the-frame-a-reference-is-numbered-along-is-declared-not-guessed)).

**The symptom.** `ReferenceFrameError`, at load, naming its own fix.

**The edit.** Declare the frame the file is numbered along:

```diff
-hxb2 = lg.reference("refs/hxb2_pol.fasta", organism="HIV-1", build="HXB2")
+hxb2 = lg.reference(
+    "refs/hxb2_pol.fasta",
+    organism="HIV-1",
+    build="HXB2",
+    frame=("K03455", 1),
+)
```

The refusal happens at `pipeline.check()`, before anything runs, which is
the treatment every result-altering change should get and the reason this
one cost a consumer four broken tests rather than a wrong report.
