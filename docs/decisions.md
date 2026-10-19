# Decision Records

## D1 — Rust core, Python surface
Scientists live in Python; correctness-critical logic (types, hashing, graph, cache) needs speed and safety. PyO3 + maturin give a single `pip install`.

## D2 — Content hashing, not timestamps
Timestamps lie (copies, clones, touched files). Cache keys hash code, parameters, input content and tool versions.

## D3 — BLAKE3 with size+mtime prefilter
BLAKE3 is fast enough for multi-GB BAMs; the prefilter avoids rehashing unchanged files.

## D4 — Semantic hashing
Outputs are canonicalised before hashing (sorted records, volatile headers stripped) so scientifically identical results share a hash.

## D5 — Dimensions as types (units analogy)
Errors are caught before compute. Conversions are typed functions in a conversion graph, enabling v2 auto-fixing via path search.

## D6 — SQLite metadata, content-addressed store
Zero-config, embedded, portable; no server required.

## D7 — HIV-1 reference pipeline
One real, validated pipeline drives every feature and ties Ligase to SAWA.

## D8 — Out of scope for v0.1
Auto-fixing, public-data fetching, cluster execution, GUI.

## D9 — Coordinates are semantics, not encoding

The `coordinates` dimension records what a step sees, not the on-disk
convention. SAM and BAM both present 1-based closed alignment records, so
re-encoding between them changes nothing. BED (0-based half-open) and GFF
(1-based closed) differ *semantically*, and that is the difference worth
catching.

The rejected alternative — BAM 0-based because its binary encoding is — would
make `samtools view -b` appear to change a dimension. False alarms are not a
safe failure mode: a checker that cries wolf gets bypassed, and then the real
hg19/hg38 mixup goes through unchallenged.

## D10 — References relate by containment

Two references compared with each other must be identical. Data checked
*against* a reference need only be contained in it: every contig the data
mentions must appear in the reference with a matching length and MD5, while
extra contigs in the reference are fine.

Strict equality everywhere would reject a VCF whose header lists only the
contigs it has variants on — correct input, rejected. Contig order still gets
its own error, because htslib matches by name but GATK requires matching
sequence dictionary order.

## D11 — `noodles` for format I/O, and the MSRV follows it

Format parsing uses [`noodles`](https://github.com/zaeleus/noodles) rather than
`rust-htslib`. It is pure Rust, so wheels build without a C htslib on the
machine — which is the difference between `pip install ligase-bio` working and
not. The cost is that `noodles` is a pre-1.0 crate that bumps minor versions
often, and its MSRV sets ours (currently Rust 1.91).

MD5 arrives via `md-5`. That is not a security choice: MD5 is what the SAM
specification prescribes for the `@SQ M5` contig checksum, so we must compute
exactly that digest to compare against it. Cache keys use BLAKE3 (D3); the two
are unrelated.

## D12 — Steps defer; the graph is built from signatures

`@lg.step` reads a function's signature once and replaces the function with a
`Step`. Calling it records the call and returns a handle; the body never runs
while a graph is being built. The return annotation is therefore the only
statement of what a step produces, and an unannotated parameter is refused at
decoration time — it would be an edge the graph does not contain, which means
an input nothing is ever compared with.

The handle is `cast` to the data type the step declares, which is a
deliberate lie to the type checker and buys static checking of the pipeline's
wiring: a mis-wired `call(align(...), calls)` is an error in the editor, not
only at `check()`. `lg.handle()` is the typed way back to the real object.

The rejected alternative was a handle that genuinely subclasses the data
types. It would have needed graph identity stored on each dataset, and a
dataset that knows which graph it belongs to cannot be used in two.

## D13 — The graph decides requirements and blames the origin

`docs/dimensions.md` allows an unknown dimension unless a downstream step
requires it. Only a graph can answer that, so `required_pair` — which reads
requirements off the pair of kinds a step joins — becomes the default, and
the graph adds what the table cannot know: where the gap started.

An undeclared-dimension error therefore names the *input* that has to declare
it, the step that requires it, and the route between them, and is reported
once however many steps the gap travels through. Pointing only at the step
would send a user to a file they cannot change; one error per step the value
passes through would make the report unreadable, and a report nobody finishes
is a report that does not work.

A step handed the wrong *kind* of data is reported instead of that step's
dimension errors rather than alongside them. Those bindings are not what the
step declared, so comparing their dimensions would describe a pipeline the
user never wrote.

## D14 — Contig order is compared only between sequence dictionaries

`OrderDiffers` exists because GATK requires two sequence dictionaries to
agree on order ([D10](#d10-references-relate-by-containment)). A BED's
contigs come from its records, which have an order too — whatever order the
intervals were sorted in. Comparing it would report a contig order mismatch
for an ordinary BED that happens to list contig 2 before contig 1.

So `ReferenceId` records whether its contig list is a `Dictionary` or merely
`Observed`, and order is compared only between two dictionaries. The rejected
alternative, sorting record-derived contigs before storing them, would have
made the stored list disagree with the file.

The same reasoning keeps `reference` out of what a step requires of an
interval file: its contigs cannot be declared, so demanding them would be an
unsatisfiable check.

## D15 — An error identifies its node, not just its step

Per-sample fan-out makes one written step into one step per sample, so a
report that knows only step names understates every cohort-wide problem. Two
consequences follow, and both are about what a user does next rather than
about catching more.

The summary counts the **places** that failed rather than the names they
share. "4 errors in 1 step" describes a localised problem you fix by looking
at one sample; "4 errors in 4 steps" describes a cohort-wide one you fix by
rethinking the pipeline. Reporting the second as the first is not a cosmetic
understatement.

An error **names its node** in the heading rather than on a line of its own,
since the name says which step this is and so belongs with the step. It is
added only where the step's own name is ambiguous — more than one node in the
pipeline being checked shares it — and then only with something that tells
*those* nodes apart.

Both halves of that condition matter. `docs/golden-example.md` fixes the
shape as `in step `call`:` and the project treats it as fixed, so a suffix on
a step nobody could confuse is noise on the commonest path. And the sample is
not always the discriminator: two same-named steps fed different alignments
of one sample would both read `(sample sample_a)`, which looks exactly like
an identifier while being none. Discriminators are therefore tried in order —
the sample, then the first file the step was built from — and the first that
gives every node in the group a distinct value wins. If none does, nothing is
added, because no suffix beats a misleading one. A step spanning a whole
cohort likewise gets no sample: labelling it after one of them would send a
user to the wrong data.

Identical problems are then stated once, with every affected sample **named**
rather than counted, because a user who must expand something to find out
which samples failed has a shorter report rather than a better one. Two rules
keep that honest: an error with no name is never collapsed, since there would
be nothing to list it under; and grouping is on the problem's full text, so a
sample that failed differently keeps its own paragraph.

Collapsing applies to the rendered report, not to the error list.
`pipe.errors()` stays one error per failing node, because counting affected
samples — or retrying one of them — wants that granularity.

## D16 — Execution lives in the core; recipes are extracted first

A step's body is Python; a run is dozens of external processes over tens of
gigabytes. If the scheduler had to call into Python to learn what each step
does, the GIL would sit in the hot path of every parallel run.

So the body is called once, before the run starts, and returns a *recipe* —
a command template, or a function to call later. The core then owns the
whole run (ready queue, worker pool, rendering, hashing, store, metadata)
with the GIL released, and takes it again only for the `lg.write` steps that
genuinely are Python.

This is only available because bodies were already deferred
([D12](#d12-steps-defer-the-graph-is-built-from-signatures)). A body
that computed something could not be called before its inputs existed; a
body that describes work can.

The rejected alternative was a Python thread pool driving the core
step by step. It is simpler, and external processes release the GIL anyway —
but every task would have to re-enter the interpreter to fetch its recipe,
and the scheduler's correctness would live in the layer with no tests
against it. Putting the ready queue in Rust also means `cargo test` can
assert that a fan-out branch is independent, which is the property the whole
milestone rests on.

## D17 — A placeholder that names nothing is an error, and `{{` escapes

`lg.sh` fills `{ref}`, `{reads.r1}` and `{out}` from the step's own
parameters. A mistyped `{refrence}` could either be passed through to the
shell as literal text or refused. It is refused.

Passing it through would mean a command that fails obscurely much later —
or, with an `echo`, succeeds. The cost of refusing is that a brace meant
literally has to be doubled, so an `awk '{{print $1}}'` is written the way
Snakemake writes it, which is already familiar to the people this is for.
Anything between braces that is not an identifier is reported as a brace
needing an escape rather than as a mistyped placeholder, because that is the
thing the user has to change.

Every substituted value is also quoted into exactly one shell word. A
reference under `/my refs/` is one path, not two arguments, and a value can
never end the quoting and continue as code.

## D18 — A failure stops its own branch and nothing else

A step that fails is recorded against its node. Independent branches keep
running, and steps below the failure are not run and are not reported.

Losing three good samples to one bad FASTQ is the overnight-run problem this
project exists to remove, so a cohort-wide abort was never an option. The
other half — reporting the steps below a failure — was rejected for the same
reason the checker reports one error per missing declaration rather than one
per step it reaches
([D13](#d13-the-graph-decides-requirements-and-blames-the-origin)): one
cause, one failure. A cascade of "its input is missing" lines makes a report
longer without making it more useful.

Nothing is written to the cache for a failed step, so a half-written output
can never be served as a result later.

## D19 — A cache from another version is discarded, not migrated

Two version numbers sit in the cache's metadata. A change to the directory
layout drops the tables; a change to how keys are composed deletes the step
entries and keeps the file hashes.

That asymmetry is the point. A file's hash is a fact about the file,
whatever keys are made of. A step entry is a claim that *these ingredients,
combined this way* produced that output — so an entry from another key
format is not stale data to be migrated, it is a claim about something else,
and reading it is the one failure a cache must never have.

Nothing is migrated because nothing needs to be: every byte in the cache is
derived, recomputing is merely slow, and being wrong is not.

## D20 — A tolerance is a quantisation

`CLAUDE.md` allows a numeric type to declare a tolerance for equality. The
only way a hash can express one is to **quantise**: put the value in a
bucket of that width and hash the bucket.

"Within ε" is not transitive. `a` near `b` and `b` near `c` does not make
`a` near `c`, so it is not an equivalence relation — and a hash can only
ever express an equivalence relation. The rejected alternative, true
ε-equality, cannot be hashed at all: it would need every value compared with
every other, which is the opposite of what a content-addressed cache is for.

The honest consequence, which belongs in the open rather than in a footnote:
two values closer together than the tolerance can still fall either side of
a bucket edge and hash differently. A tolerance therefore means "the same
bucket", not "within ε". It removes the common case — a last digit that
moved — and does not promise more than that.

Exactness stays the default. `QUAL` in a VCF declares a tolerance of a tenth
of a Phred unit, because it is the field with the motive: a floating-point
accumulation whose last digits move with the order the terms were added in.
A tenth of a Phred unit is two orders of magnitude below the smallest
threshold anyone filters on, so it cannot merge two qualities a user would
treat differently. Nothing else declares one, because a tolerance nobody has
measured would be a guess baked into every cache key, and the project's bias
runs the other way: a cache that is too cold costs time, one that is too
warm is wrong.

## D21 — Canonicalisation falls back to the bytes

A file that does not parse as its type hashes as its bytes, tagged as such,
with the tag inside the digest so the two can never collide.

Canonicalisation means parsing, and parsing fails on real inputs: a
truncated BAM, a `.vcf.gz` that is not gzipped, a file still being written, a
`.bam` holding something else entirely. Refusing to hash such a file would
mean a format problem surfacing as an engine failure, in a layer that has no
business judging whether a file is valid — that is M6's job, and it has a
whole milestone because it is not a line of code.

The direction is what makes this safe. Bytes are the **stricter** test, so
the fallback can cost a rerun and can never serve a wrong answer. Falling
back the other way — guessing at a canonical form from a partial parse —
would mean two files that differ in the part that could not be read hashing
the same.

For the same reason every parser here is strict. A canonicaliser that read
what it could and ignored the rest would be the dangerous kind of lenient:
it would reduce a file to the fields it managed to understand, and two files
differing only outside that set would agree. So a FASTA with no `>` line, a
BED row with two columns, a VCF record with seven, and a report row that
does not fit its header are all failures that fall back, rather than
partial successes.

## D22 — A tool's signature is not part of its result

Every tool in the reference pipeline records how it was run, inside the file
it produces. `minimap2` writes `@PG CL:minimap2 -t 8 -ax sr ...`;
`bcftools` writes `##bcftools_callCommand=call -mv ...; Date=Sat Oct 17
21:14:12 2026`. These are dropped before hashing.

The reason is stronger than tidiness. The command line a tool records
contains its output path; a step's output path is the staging file, named
after the step's own key. So before M4 **a step's output bytes contained
that step's own key**, and any change to the step at all — a parameter, a
tool version, a comment — moved its bytes, which moved the key of the step
below, whose bytes moved in turn. Early cutoff, which M3 documented as
falling out of content addressing, could not get past the first real tool.

The rejected alternative was to make the staging path stable so that the
`@PG` line would repeat. It fails for two reasons: the timestamps remain,
and two nodes sharing a staging path could not run at once — which would
cost the per-sample fan-out the whole project is built around.

Dropping these fields has a cost worth naming: a cache hit can no longer
prove which command produced an object, because the field that said so is
not in the hash. That is the right place for the loss. A hash answers "is
this the same result?"; "how was this produced?" is provenance, it is M5,
and it wants its own storage and its own lifetime
([cache](architecture/cache.md#what-is-not-here)).

## D23 — Provenance has its own store and its own lifetime

The cache answers "has this been computed?" and is designed to be discarded:
everything in it can be recomputed, so a cache written under another key
format is dropped rather than migrated (D19). Provenance answers "how was
this produced?", and that answer cannot be recomputed. The machine, the
installed tool versions and the exact command that produced an object last
year do not come back.

So it lives in `provenance.sqlite` beside `cache.sqlite`, with its own
`PROV_VERSION` and nothing that ever drops a row. `Cache::open` clears
*tables*, not the directory, so a separate file survives every key-format
and canonicalisation bump for free.

The rejected alternative was more columns on the cache's `steps` table,
which already held the tool versions and the duration because it had them.
It fails on lifetime: the question that table exists to answer is retired
whenever the ingredients are recombined, and a methods section is not.
Discarding a year of provenance because `KEY_VERSION` moved from 2 to 3
would be an unforced loss of the only data here that cannot be regenerated.

A record is keyed on the step key, which is already the identity of the
computation. Two things are kept out of it deliberately. The sample label
belongs to the pipeline a step was reached from rather than to the step, and
`Graph::scopes` can derive it whenever it is wanted. Citations are not
stored at all: a citation cannot change a result, so editing one does not
rerun the step, so a stored one would go stale while looking current.

## D24 — A citation is curated or declared, never guessed

`ligase methods` cites `minimap2`, `samtools` and `bcftools` from a table in
the engine, anything else from `cite=` on the step, and reports a tool
covered by neither as **uncited** rather than omitting it.

Two failure modes to avoid, and they pull in opposite directions. A
reference synthesised from a tool's name would be a fabrication in a
manuscript — the one kind of error this project exists to prevent, and worse
than no reference at all. But a tool silently missing from a reference list
lets an incomplete methods section look complete, which is the same failure
wearing a quieter coat.

Naming the gap satisfies both. The table stays short for the same reason:
every entry is a standing promise to keep a reference correct as versions
move, and a long list of half-checked citations is worth less than a short
list of checked ones beside an honest admission.

The rejected alternative was no table at all, with every citation declared.
It is defensible — there is nothing to maintain and nothing to get wrong —
but it leaves the reference pipeline's own methods paragraph with no
references until three are typed by hand, and those three are the ones the
project is built on.

## D25 — A bundle states a claim and can be checked

"A bundle that reproduces the results on a clean machine" cannot be built as
written. The reference pipeline's inputs are sequencing runs from the SRA; a
bundle carrying them would be gigabytes, and fetching them is out of scope
for v0.1 (D8).

So the bundle carries everything *except* the data and names the data firmly
enough that nothing else can be substituted: every input by content hash,
every reference by accession and checksum. A clean machine is then, to this
engine, an empty cache — nothing computed, nothing remembered, every tool
probed again — and the claim is that a run there produces outputs whose
canonical hashes are the recorded ones. That is testable, and
`tests/test_demo.py` tests it on the real pipeline by writing a bundle from
one cache and verifying it against another.

`verify` matches rows by step and sample rather than by key, because the key
is the thing that may legitimately have moved, and that is what makes the
verdicts worth having:

- `identical` — the same key and the same result.
- `reproduced` — a different key and the same result anyway.
- `differs` — a different result.
- `missing` — this pipeline no longer produces that step.

`reproduced` is the one that justifies the mechanism. Keys are
content-addressed, so a check on a machine with the same data and the same
tools is `identical` and tells you little; a check where the aligner is a
different build is `reproduced`, which is a stronger statement than a cache
can make and is what a reviewer on their own machine actually gets.

Inputs are checked by content too. That is not a bonus feature: without it a
`differs` is uninterpretable, because a changed input explains a changed
output, and a bundle that could not distinguish "irreproducible" from
"different experiment" would be misleading in the one situation it exists
for.

The format is eight flat TSV tables rather than JSON. No dependency to add,
greppable, openable in a spreadsheet, and consistent with the engine's one
structured store being SQLite (D6). Tabs, newlines and backslashes in a
field are escaped, so a recorded command containing a tab cannot shift a
column.

## D26 — A field that cannot be dropped is noticed instead

`bcftools` names a VCF's sample column after its input BAM's **path** when
the BAM carries no `@RG SM`, and a Ligase run hands it a file inside the
content-addressed store. The calls then carry something like
`/Users/kelyn/ligase/.ligase/store/62/7db9….bam` where a sample identifier
belongs, and the consequence is not cosmetic: the canonical form of a VCF
includes its sample names, so such a file's canonical hash depends on where
the cache directory lives. A reproducibility bundle written from one cache
cannot verify against a clean one. M5's bundle verification is what found
it — `call` came back `differs` on every sample with byte-identical variant
records, the only difference being the `#CHROM` line's last column.

This is [D22](#d22-a-tools-signature-is-not-part-of-its-result)'s pattern in
a field that cannot be dropped. A command line says how a file was made; a
sample identifier is part of what the file is **about**, and a sample set is
one of the dimensions this engine exists to keep straight. So the hash
stands and the engine **says what it saw**: a [`Note`], carried beside the
hash into the run report, provenance, the bundle and the methods section.
A note is not an error, never changes a hash, and the fix is one line in the
pipeline the user is the only one who can make.

Both louder alternatives were rejected, and for different reasons.

**Refusing it** at canonicalisation time is what
[D21](#d21-canonicalisation-falls-back-to-the-bytes) forbids: a VCF whose
sample column holds a path is perfectly valid VCF, and turning a format
problem into an engine failure is the thing that layer must not do.
Refusing it at `ligase check` time instead cannot work, because the file
does not exist yet: the checker runs before any compute, on inputs, and the
file with the wrong identifier is an **output**. A check-time rule would
miss the failure that prompted this one and fire only on hand-supplied files
that are not wrong.

**Normalising it** — reducing a path-shaped identifier to its basename
before hashing — is the silent repair of a wrong file that this project
refuses on principle, and it risks collapsing two genuinely different
samples whose basenames agree. It also would not have worked. The basename
of a file in the store is the hash of its **bytes**, and an alignment's
bytes hold the staging path that `samtools sort -o` recorded, so the
basename moves with the cache root exactly as the whole path does: the
bundle would still have reported `differs`.

The rule for what counts is deliberately narrow, because a rule that
guessed would report good identifiers as mistakes. An identifier is a file
when it holds a path separator or ends in an extension naming a format this
engine reads. `K03455.1` has a dot and is not a file; `SRR8525904` and
`sample_a` have neither.

Three consequences worth stating. The notes are **stored** with the
canonical hash rather than recomputed, because a cache hit does not reopen
the file and a warning that stopped appearing on the second run would be
worse than none. `CANON_VERSION` does **not** move: the rule changes nothing
about what is hashed. The cache's `SCHEMA_VERSION` does, for the new column,
which clears the cache's tables — the same practical cost, for a reason that
can be stated honestly.

[`Note`]: architecture/canonicalisation.md#what-a-canonical-form-notices
