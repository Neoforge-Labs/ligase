# M16 — Claim binding

*Written against v0.3 at commit `0c005b0` (27 Oct 2026), the M15 merge. The
second milestone of v0.3. Read alongside `CLAUDE.md`, §5.6 of
`docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md`,
`web/content/project/decisions.mdx` (D24, D25, D26, D35, D42) and
`web/content/project/milestones.mdx`.*

---

## 1. Intent

§5.6 of the v0.2 spec is three sentences:

> Bind a published number to the step and run that produced it, and report
> unbacked claims the way uncited tools are reported now. Extends `Methods`
> and `MethodsGap`. Needs real runs to bind to, so it follows M13.

Turning that into a design is half of this milestone. What it becomes:

A **claim** is a number or a phrase a paper states, declared on the pipeline
beside the outputs it is about, and bound by the engine to the step key and
the output hash that produced it. A claim that cannot be bound — because
nothing in the pipeline produces it, or because what produces it has no
record — is reported in the write-up the way an uncited tool is reported
today. A claim that *restates a value the engine holds* — a threshold, a tool
version — is compared against it, and a disagreement is reported as the
loudest thing in the write-up.

Provenance runs one way today: from a result back to how it was produced. It
does not run the other way, from a sentence in a manuscript to the
computation behind it, and that is the direction a reader of the paper cares
about.

## 2. The D35 reckoning, first — and it is not D35

D35 is about **kinds**:

> A kind earns its place only if it brings a dimension whose mismatch is a
> documented silent failure in real research.

Claim binding is not a kind and brings no dimension. Applying D35's letter to
it would be a category error, and a convenient one: by that test `ligase
audit`, the reproducibility bundle, early cutoff and the citation table all
fail too, and each of them is load-bearing. So the test to apply is the
*reasoning* D35 rests on, which generalises cleanly and which this milestone
is held to instead:

**Does the feature catch a documented silent failure that passes unremarked
today, and is the engine already holding what it needs to catch it?**

### The silent failure

A number in a manuscript stops corresponding to the analysis that produced
it, and nothing says so.

The shapes are ordinary. A draft says "variants above 1% allele frequency"
because that is what the pipeline ran in March; the threshold moved to 5% in
June and the sentence did not. A results table says 24 samples; one was
dropped and 23 remain. A methods section names `bwa 0.7.17` and the cluster
has been upgraded. A reported count came from a step that has since been
renamed, so it is now a number with no step behind it at all.

Each of these has the signature this project exists for: the join succeeds,
the paper is plausible, nothing fails, and the cause is bookkeeping. And
unlike a reference-build mixup, the artefact is not a file anyone can check
later — it is a sentence, and the only thing that could have caught it was a
link between that sentence and a run.

### Why the engine is already half-built for it

Three decisions already in the engine make the same promise, which is that
**the record names what it cannot account for**:

- **D24** names an uncited tool rather than omitting it, because "a tool
  silently missing from a reference list lets an incomplete methods section
  look complete".
- **D26** names a field that cannot be dropped rather than dropping it.
- The methods **`Gap`** names a step whose production is unknown rather than
  printing a paragraph with a silent hole in it.

So `ligase methods` will tell you that `fastp` has no reference. It will tell
you that a sample identifier in your VCF is really a file name. It will tell
you that one step's provenance is missing. It will not tell you that the
number in your abstract is not a number this pipeline produces — because
nothing has ever told it what that number is.

That asymmetry is the argument. This feature is not new capability bolted on;
it is the fourth instance of a promise the engine already makes three times,
in the one direction where the promise currently stops.

### What it must not become

The thing that would make this milestone a failure even if it shipped: a
`Claims` section that reprints the user's own sentences under a heading. That
is prose with a frame around it, and it would be worse than nothing — it
would make an unchecked paragraph look checked, which is precisely the
failure D24 refuses in the citation list.

M13 recorded the rule that forbids it, and it is the rule this design is
built to satisfy:

> D42 asks a step for four declarations, and **a declaration nothing can
> check is a comment.**

So every part of a claim's declaration is checked, and §5.3 lists the five
checks by name. A claim's *text* is not checked, cannot be, and is the one
part the design is explicit about not checking.

**Verdict: it passes**, on the reasoning behind D35 rather than on its letter,
and the argument is written onto the architecture page rather than left for a
reviewer to reconstruct.

## 3. What gets built

### 3.1 What a claim is

```python
pipe = lg.Pipeline(
    samples.map(lambda s: resistance(call(aln := align(s, hxb2), hxb2), hxb2, aln)),
    claims=[
        lg.Claim(
            "variants were called above 1% allele frequency",
            step="call",
            parameter="min_freq",
            value=0.01,
        ),
        lg.Claim(
            "resistance was interpreted for every sample",
            step="resistance",
        ),
        lg.Claim(
            "reads were aligned with minimap2 2.31",
            step="align",
            tool="minimap2",
            value="2.31",
        ),
    ],
)
```

Five fields, and each of the last four exists because it makes something
checkable:

| Field | What it is | What it is for |
|---|---|---|
| `text` | the sentence as the paper states it | what a reader searches for. **Never checked** |
| `step` | the step that produced it | the binding, and check 1 |
| `scope` | the sample it is about, where it is about one | check 2 |
| `parameter` | the parameter it restates, where it restates one | checks 3 and 5 |
| `tool` | the tool whose version it states, where it states one | checks 4 and 5 |
| `value` | what the paper says that parameter or version is | check 5 |

`text` and `value` are separate on purpose, and the separation is the design's
sharpest edge. A paper writes **1%**; the pipeline holds **0.01**. Comparing
the sentence against the parameter would be a guess about arithmetic in
English. So the sentence is what a reader greps for and the value is what the
engine compares, and the user writes the value the way the pipeline holds it.

`parameter` and `tool` are mutually exclusive: a claim restates one thing. A
claim with `value` and neither of them has nothing to compare against, and a
claim with either and no `value` has nothing to compare — both are refused
where they are written, by `__post_init__`, for the same reason `stochastic=`
and `seed=` are checked as one declaration in two parts.

### 3.2 Where a claim is declared, and why it is on the `Pipeline`

Three places were possible.

- **(a) On the step, as `@lg.step(claims=...)`.** Rejected. A claim is about
  one *run* of one pipeline — "24 samples" is not a property of the `resistance`
  step — and a step is reusable across pipelines. Worse, `@lg.step`'s
  declarations feed the cache key, and §3.5 says why a claim must never go
  near it.
- **(b) As an argument to `pipe.methods(claims=...)`.** Rejected, on one
  decisive fact: `ligase methods` loads a pipeline *file* and finds the
  `Pipeline` objects in it (`cli._pipelines`). A claim passed to `methods()`
  is invisible to the CLI, so the CLI would need a new flag, a new file
  format and a parser for it — and the engine has no JSON or TOML parser and
  does not want one (D6, D25).
- **(c) `lg.Pipeline(outputs, claims=[...])`.** *Chosen.* Keyword-only, so
  every existing call keeps working; declared in the pipeline file, which is
  the file the bundle already carries and the CLI already loads; so `ligase
  methods` reports claims with no new flag at all.

### 3.3 The five checks, and the three verdicts

A claim is checked against the methods section the run produced — the groups,
their records and their scopes — and comes out as one of three verdicts.

Checks, in order, each failing to **unbacked** with its own reason:

1. **`step` names a step this write-up accounts for.** Else: *nothing in this
   pipeline produces `X`*, naming the steps it does produce.
2. **`scope`, where given, is one of that step's scopes.** Else: *`align` ran
   on no sample called `SRR0000000`*, naming the ones it ran on.
3. **`parameter`, where given, is a parameter the record holds.** Else:
   *`call` takes no parameter `min_frequency`*, naming the ones it takes.
4. **`tool`, where given, is a tool the record holds.** Else: *`align` ran no
   tool called `bwa`*, naming the ones it ran.
5. **`value`, where given, equals what was recorded.** A disagreement is not
   unbacked — the claim *is* bound, to a computation that contradicts it — so
   it is its own verdict, **contradicted**, stating both numbers.

A claim surviving all five is **bound**, and carries the step key, the output's
canonical hash, and the date and platform the provenance record was written
on.

A fourth failure deserves naming because it is the one that connects this
milestone to the existing gap machinery: a claim whose step is a **`Gap`** —
in the cache from a run that recorded no provenance — is unbacked, with the
gap's own advice. A claim bound to a step nothing is known about is bound to
nothing, and reporting it as bound would be the plausible wrongness this whole
design is against.

Three verdicts rather than two because they say different things to a reader,
and the vocabulary is deliberately the bundle's shape (D25): a short word per
outcome, the interesting one given its own name.

### 3.4 How a claim binds to a *run* when the cache served the step

This is the question §5.6's "the step and run that produced it" leaves open,
and it has a clean answer.

**A claim binds to the computation, not to the occasion.** The binding is the
step **key** and the **canonical hash** of the output. The key already *is*
the identity of a computation — the same code, parameters, inputs and tool
versions — which is the property D22 spent a milestone restoring to
provenance, and the reason `prov::methods`'s own doc comment says a pipeline
run entirely from cache "is as good a subject as one that recomputed
everything".

So a step served from the cache backs its claim completely. What the binding
*additionally* names is when that computation happened: the record's own
`recorded_at` and `platform`, which may be months before this run. A bound
claim therefore reads as *produced by `call` at key `a1b2…`, output `c3d4…`,
recorded 22 October 2026 on darwin-aarch64* — the computation and when it
happened, not "this run did it".

That is both the honest statement and the stronger one. The alternative —
refusing to bind a cached step, and binding only what this run recomputed —
would make `ligase methods --force` the only honest way to write up a paper.
A content-addressed cache exists so that a result does not have to be
recomputed to be trusted; a claim mechanism that did not believe the cache
would be arguing with the engine it lives in.

What this does *not* promise, and the architecture page says so: the engine
cannot tell that a claim's **text** has gone stale. If `min_freq` moves from
0.01 to 0.05, the key moves, the claim stays bound to the new computation, and
the sentence "above 1%" is now wrong. For a claim that states its value, check
5 catches exactly that and reports **contradicted** — which is the whole
reason `parameter=` and `tool=` are in the surface. For a claim that states
only a sentence, the key in the write-up and in `claims.tsv` is what a
reviewer compares, and nothing pretends otherwise.

### 3.5 A claim never enters a cache key

Emphatically, and it is half of D45. A claim is read at write-up time and at
no other time. It is not in `Task`, not in the key, not in the provenance
record of the step it names.

The reason is a reductio: a claim in the key would mean that editing a
sentence in your manuscript reruns your aligner. Nothing about the
computation changed, so nothing downstream may move — and a mechanism whose
cost was a re-alignment per draft would simply not be used, which is the worst
outcome available for a feature whose whole job is to be kept up to date.

This is also why a claim can never fail `check()`. `check()` is about
dimensions, before any compute; a claim is a statement about a paper. A
pipeline whose write-up names a renamed step is not a pipeline that must
refuse to run. It is reported where it is read, and the enforcement is the
exit code (§3.7).

### 3.6 How `Methods` and `MethodsGap` extend — and where the spec is reshaped

`Methods` extends straightforwardly: a `claims: Vec<ClaimCheck>` field, a
`Claims` section in `text()`, and a claims clause on `summary()`.

`MethodsGap` does **not**, and this design says so rather than forcing it.
§5.6 says "Extends `Methods` and `MethodsGap`", and taken literally that means
an unbacked claim arriving as a `MethodsGap` — which today is
`(step, scopes, nodes)` in Rust, in the bindings and in Python. A claim has no
`nodes`: it is one sentence, not a count of graph nodes. It has one `scope`,
not a list. And its gap has **five** distinguishable reasons where a step's
gap has exactly one. Pushing it through that shape would mean a `nodes` column
that is always 1 and means nothing, a `scopes` list that is always length 0 or
1, and a reason that has nowhere to live — a record whose columns are not
true, which is the one thing the provenance store is not allowed to be.

So the reading taken is that **the gap-reporting `MethodsGap` stands for**
extends to claims: same section of the report, same consequence, same promise
that the record names what it cannot account for — in a type that is honest
about what a claim is. `MethodsGap` keeps its meaning and its shape, and every
existing use of it is untouched.

One existing meaning *does* change, and it is the right change: Python's
`Methods.accounted_for` is `not self.gaps` today and becomes `not self.gaps
and not self.unbacked`. A write-up with a number nothing in the pipeline
produces is not fully accounted for, and a property called `accounted_for`
that returned `True` for one would be making the engine's own promise
falsely.

### 3.7 Unbacked and contradicted claims are findings

`ligase methods` exits non-zero on a gap today (`found += len(methods.gaps)`)
and **not** on an uncited tool. The distinction is sound and it tells us where
claims belong: an uncited tool is a stated absence with nothing wrong behind
it — you cite it by hand and move on. A gap is a hole in the record.

An unbacked claim is worse than a gap: it is a number in a paper that the
analysis does not support. A contradicted one is worse again — the analysis
positively disagrees. Both are findings, both raise the exit code, and a bound
claim does not. That is what makes a claim enforceable in CI, which is the
only place a drifting manuscript is actually caught.

### 3.8 Claims travel in the bundle

One more table, `claims.tsv`, with the claim, its verdict, its step, its scope,
its key and its output hash. Nine flat TSV tables where there were eight,
no dependency added, consistent with D25's reasoning about the format.

The reason it belongs there is D25's own title: *a bundle states a claim and
can be checked*. The bundle's existing claim is about reproducibility; a
`Claim` here is the paper's claim, and the bundle is the artefact that travels
with the paper. A bundle that carried every tool version and no record of
which sentences rested on them would be missing the half a reviewer reads.

`verify` is **not** extended with a claim verdict, deliberately. A claim bound
to a step whose result comes out `differs` on another machine is already
visible in `results.tsv`'s verdict for that step, under the key the claim
names. Adding a parallel verdict path would duplicate the comparison that
`verify` already does, in a second place that could disagree with the first.
Named as a boundary on the architecture page, not papered over.

### 3.9 Surfaces

**Rust** (`crates/ligase-core/src/prov/claim.rs`, new; `methods.rs` extended):

- `prov::Claim { text, step, scope, restates: Option<Restated> }`, where
  `Restated` is `Parameter { name, value } | Tool { name, version }`. A sum
  type rather than three optional fields, so the core **cannot hold** a claim
  that restates both a parameter and a tool. The Python surface is flat
  (`parameter=`, `tool=`, `value=`) because that is how a user writes one, and
  its `__post_init__` is the one place the incoherent combinations are
  refused.
- `prov::ClaimCheck { claim, verdict, bound: Vec<Bound> }`, with
  `Bound { scope, key, canonical, recorded_at, platform }` — one row per
  computation, because an unscoped claim over a four-sample step is bound to
  four of them.
- `prov::ClaimVerdict::{ Bound, Unbacked(Unbacked), Contradicted { stated, recorded } }`.
- `Methods::produced: Vec<Produced>` — every result the write-up accounts for,
  one row per node with its key, output hash, date and platform. Not a
  duplicate of `groups`: a `Group` keeps one representative `Record` and
  discards the other nodes' keys, and a claim binds to a computation.
- `Methods::bind(&mut self, claims: &[Claim])`, which fills `Methods::claims`.
  **Not** a new argument to `prov::methods()`: every check a claim needs is
  against the `Methods` value itself, so binding is an operation over the
  built write-up rather than part of building it — and `methods()`' eleven
  existing call sites keep working unchanged, which clause 10 is about.
- `Methods::claims`, `Methods::unbacked()`, and their sections in `text()` and
  `summary()`.

**Bindings** (`crates/ligase-py/src/lib.rs`): `PyClaim` in, `PyClaimCheck` out,
carried in `PyMethods` as a tenth element.

**Python**: `ligase.provenance.Claim`, `ClaimCheck`, the verdict constants
`BOUND`, `UNBACKED`, `CONTRADICTED`; `Methods.claims`, `Methods.unbacked`;
`Pipeline(outputs, claims=...)`; all exported from `ligase`, all documented,
all placed in `generate_api.py`'s provenance group.

**CLI**: no new flag. `ligase methods` finds claims on the pipeline it already
loads, and counts the unbacked and contradicted ones toward its exit code.

## 4. Goals, non-goals, anti-goals

**Goals.** The claim type, the five checks, the three verdicts, the write-up
section, the bundle table, the Python surface, the exit code, D45, and the docs
in the same change.

**Non-goals.**

- **Reading a number out of an output file.** The engine does not open your
  VCF and count its rows to check that you reported three mutations. It cannot
  do it generically, and a mechanism that did it for the formats it knows and
  silently skipped the rest would be the lenient-canonicaliser failure in a new
  costume. A claim about a count binds to the computation; it does not verify
  the count.
- **A claim verdict in `verify`.** §3.8.
- **Claims entering the cache key.** §3.5.
- **A claim refusing a run.** §3.5.

**Anti-goals** — things that would make this a failure even if it worked:

- A `Claims` section that reprints the user's sentences with no check behind
  them (§2).
- A claim reported as bound when its step is a gap.
- An existing pipeline's `ligase methods` output changing. A pipeline that
  declares no claims must produce byte-identical text, and that is a test.
- A claim comparison that guesses: 1% against 0.01, or `samtools 1.24`
  against `1.24` by arithmetic rather than by the rule §5 states.

## 5. Open details left to the builder

- The exact wording of each unbacked reason, following the pattern the
  existing `Gap` and uncited-tool sentences set.
- How a tool version is compared, given that `samtools --version` prints
  `samtools 1.24` and `minimap2 --version` prints `2.31-r1287`. The intended
  rule is the one `describe_tool` already encodes: compare against the first
  line with the tool's own name stripped, and accept a stated value that the
  recorded version starts with, so `2.31` matches `2.31-r1287` and `2.4` does
  not match `2.41`. Textual, and never numeric.
- Whether `value` accepts `int` and `float` as well as `str` in Python. It
  should: `value=0.01` is what a user will write, and `str()` renders it the
  way the parameter was hashed.

## 6. Done when

1. A claim naming a step the pipeline runs is **bound**, carrying that step's
   key, the output's canonical hash, and the date and platform its record was
   written on.
2. A claim naming a step nothing in the pipeline produces is **unbacked**,
   naming the steps it does produce.
3. A claim whose step is a methods **gap** is unbacked rather than bound.
4. A claim naming a sample the step did not run on, a parameter it does not
   take, or a tool it did not run, is unbacked, each with its own reason
   naming what there was.
5. A claim restating a parameter value or a tool version that the run
   disagrees with is **contradicted**, stating both the claimed and the
   recorded value.
6. A claim bound to a step **served from the cache** is bound, to the
   computation that produced it and the date that computation happened — D45,
   pinned by a test that runs a pipeline twice and binds against the second
   run.
7. A claim **never enters a step key**: adding, editing or removing a claim
   reruns nothing.
8. An unbacked or contradicted claim is named in the write-up the way an
   uncited tool is, and raises `ligase methods`' exit code; a bound one does
   not.
9. A claim's declaration is **checked where it is written**: both `parameter`
   and `tool`, a `value` with neither, or either with no `value`, is refused
   at declaration time.
10. A pipeline that declares **no claims** produces byte-identical `methods`
    text and summary to what it produced before this milestone.
11. Claims travel in the bundle as `claims.tsv`.
12. Every clause above is pinned by a **named** test, in Rust and in Python
    where both surfaces are touched.
13. Docs ship in the same change: concepts, guide, architecture, a
    silent-failures page titled by symptom, `_meta.ts` entries, D45, the M16
    milestone entry, `CHANGELOG.md` under `[Unreleased]`, and any engine
    output on a page captured from a real run by a generator rather than
    pasted.
14. All gates green: `cargo fmt --all --check`, `cargo clippy --workspace
    --all-targets -- -D warnings`, `cargo test --workspace`, `ruff check .`,
    `ruff format --check .`, `mypy`, `pytest`, and `npm run check` in `web/`.

## 7. Gaps named, not papered over

Each of these goes on `architecture/provenance.mdx`:

- **The text of a claim is never checked.** It cannot be. The engine checks
  the binding and the values a claim restates; the sentence is the user's, and
  a claim that states no value is bound to a computation rather than verified
  against one.
- **No number is read out of an output.** §4.
- **A claim is not bound per sample unless it says so.** A claim with no
  `scope` is about the step across every sample it ran on, which is the right
  default for "every sample was interpreted" and the wrong one for a count
  from one sample. The `scope` field is how a user says which they mean; there
  is no inference.
- **An output with a note (D26) behind a bound claim is not surfaced on the
  claim.** The note is already reported in its own section of the same
  write-up. Linking the two would be a fourth verdict for a connection a
  reader of one page makes anyway.
