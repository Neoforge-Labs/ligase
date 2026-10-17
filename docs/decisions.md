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

An error **names its node**, as the sample where the graph can work one out,
in the heading rather than on a line of its own — the sample says which step
this is, so it belongs with the step. A step spanning a whole cohort gets no
name: labelling it after one of its samples would send a user to the wrong
data, which is worse than saying nothing.

Identical problems are then stated once, with every affected sample **named**
rather than counted, because a user who must expand something to find out
which samples failed has a shorter report rather than a better one. Two rules
keep that honest: an error with no name is never collapsed, since there would
be nothing to list it under; and grouping is on the problem's full text, so a
sample that failed differently keeps its own paragraph.

Collapsing applies to the rendered report, not to the error list.
`pipe.errors()` stays one error per failing node, because counting affected
samples — or retrying one of them — wants that granularity.
