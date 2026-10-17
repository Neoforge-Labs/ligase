# Graph architecture

How a pipeline is built from signatures, how dimensions travel through it,
and why requirements come from the graph rather than from a table. The
user-facing view is [Pipelines and the graph](../concepts/pipelines.md).

## Where the code lives

```text
crates/ligase-core/src/
  graph.rs          Graph, NodeId, Argument, StepSpec, propagation, check
  load/intervals.rs BED and GFF records
  check.rs          disagreements(), requires(), the errors the graph builds

crates/ligase-py/src/lib.rs   the Graph pyclass; translation only
python/ligase/graph.py        @lg.step, Step, StepOutput, Pipeline
python/ligase/samples.py      Samples and .map()
python/ligase/errors.py       the exception hierarchy
python/ligase/cli.py          `ligase check`
```

The graph is in `ligase-core` and has no `pyo3` dependency, like everything
else that must not be wrong. The Python layer reads signatures and calls
`add_step`; it holds no checking logic, and all error wording is generated in
Rust so the Rust tests own it.

## Deferral: how a body never runs

```python
class Step(Generic[P, R]):
    def __call__(self, *args: P.args, **kwargs: P.kwargs) -> R:
        bound = self._signature.bind(*args, **kwargs)
        bound.apply_defaults()
        return cast("R", StepOutput(self, dict(bound.arguments)))
```

`inspect.signature` and `get_type_hints` run once, at decoration. Calling the
step binds the arguments — so a missing or surplus argument still fails at
the call site, exactly as it would for the undecorated function — and returns
a `StepOutput`.

That `cast` is a deliberate lie to the type checker, and it buys something
real. A type checker sees `align` as `(Reads, Reference) -> Alignment`, so it
follows the pipeline and catches a mis-wired one *while you are editing*:

```python
pipe = lg.Pipeline(call(align(reads, hxb2), calls))  # error: Variants, not Reference
```

At runtime the value is a `StepOutput`, and `lg.handle()` is the typed way to
reach it. The alternative — a handle class that really is a `Dataset` — would
have meant putting graph identity on the data types, and a dataset that knows
which graph it belongs to is a dataset that cannot be used in two.

`check()` catches the same mistake for code that is not type-checked, which
is most pipelines in practice.

### Why annotations are mandatory

An unannotated parameter is refused at decoration time, not at check time. A
missing annotation on a dataset parameter would not be a missing label — it
would be an **edge the graph does not contain**, so that input would never be
compared with anything. Silently dropping a connection is the exact failure
mode this project exists to remove, so the sooner it is said the better.

## Construction: cycles are unrepresentable

```rust
pub fn add_input(&mut self, label: &str, dataset: Dataset) -> NodeId
pub fn add_step(&mut self, spec: StepSpec) -> Result<NodeId, GraphError>
```

`add_step` takes `NodeId`s that already exist, so a node can only ever depend
on an earlier one. There is no cycle detection in this module because there
is nothing to detect: **index order is dependency order**, checking is a
single forward pass, and no topological sort is needed anywhere.

`Pipeline.__init__` walks back from the outputs, keyed by object identity, so
a file passed to two steps becomes one node. Identity rather than equality is
the right key: two datasets loaded from the same file are two nodes, because
the user wrote them as two, and a cache that notices they are identical is
M3's job rather than the graph's.

## Propagation

A step's output carries the dimensions of its inputs, re-sourced as
`Source::Derived { step }` — the variant M1 added and deliberately left
unused.

```text
for each dimension the output kind applies to:
    coordinates  -> introduced by the step (see below)
    otherwise    -> the first argument, in signature order, that states it
                    and, if none states it, that argument's own gap verbatim
```

Three details carry weight.

**Signature order decides.** Where two inputs both state a dimension, the
first wins. They are also checked against each other, so a disagreement is
already reported at this step; picking one keeps a single root cause from
echoing down the rest of the pipeline as a second error.

**An unknown keeps its reason.** When nothing states a dimension, the output
inherits the first applicable input's `Unknown { reason }` rather than a
fresh one. `"no @RG lines with an SM tag in the header"` is still the
instruction three steps later; `"this step did not have one"` would not be.

**Coordinates are introduced, not inherited.** Nothing upstream of an
alignment has a coordinate system — FASTQ and FASTA have none — so inheriting
would leave it unknown forever. Alignments and variant calls are 1-based
closed by specification ([D9](../decisions.md)). Intervals are the exception:
a tool that writes BED from a VCF *changes* the convention, so a step
producing `Intervals` must declare what it writes, and reports its
coordinates as unknown until it does. Inheriting 1-based closed there would
be a lie that passes `check()`.

A pleasant side effect: a derived alignment knows its `pairing` from the
reads it was built from, where M1 could only report an alignment's pairing as
unknown because the information is in the SAM flags.

## Checking

```rust
pub fn check(&self, roots: &[NodeId]) -> Vec<CheckError>
```

One forward pass over the nodes the roots reach, in index order. Per step:

1. **Kinds.** Each argument's supplied kind against the kind the step
   declared. A mismatch is reported *instead of* that step's dimension
   errors, not alongside them — the bindings are not what the step declared,
   so comparing their dimensions would describe a pipeline the user never
   wrote. One error per root cause.
2. **Disagreements.** `check::disagreements`, which is M1's comparison logic
   with the requirements half removed.
3. **Requirements.** Below.

`roots` restricts the walk, so a module that builds two pipelines checks each
against its own outputs and an unreachable branch is nobody's problem.

### Requirements come from the graph

`docs/dimensions.md`:

> Unknown dimensions are allowed only if no downstream step requires them.

M1 could not implement that sentence. Checking one step in isolation has no
downstream, so `check_step` reads "required" off the pair of kinds in front
of it (`required_pair`). That table is now the *default*, and the graph adds
what the table cannot know: **where the gap started**.

```text
for each dimension some step requires:
    for each argument that applies to it and does not know it:
        origin = walk the inheritance chain back to where it entered
        report once per (origin, dimension), naming the requiring step
                                            and the route between them
```

The walk follows exactly the rule propagation followed — the first argument
that can speak to the dimension — so the origin is the node the value was
supposed to come from. A step that *introduces* a dimension is its own
origin, which is how a step that forgot to declare its coordinate system gets
blamed rather than its input.

Two things follow, and both are about readability rather than about catching
more.

**The error names a file, not just a step.** Without the graph the message
points at `merge`, where the user can change nothing; with it the message
points at the BAM, and says that it reaches `merge` through `call`.

**One missing declaration is one error.** Deduplicated on
`(origin, dimension)`, so a value that flows through five steps that all
require it produces one error rather than five. A report nobody reads to the
end is a report that does not work.

## Reporting under fan-out

Per-sample fan-out makes one written step into many steps, and two things
that were adequate for a single chain stop being adequate.

### An error carries its node, and its node's name

`CheckError` gains `node: Option<usize>` and `scope: Option<Scope>`, both
stamped by `Graph::check` and both `None` for a single step checked with
`check_step`, which has nothing to distinguish it from itself.

```rust
pub struct Scope { pub kind: &'static str, pub value: String }
```

`Graph::scope(node)` derives it from the step's **output** sample set, when
that set holds exactly one sample. A step whose output spans a whole cohort
gets nothing: labelling it after one of its samples would send a user to look
at the wrong data, which is worse than saying nothing. Where the output kind
cannot carry a sample set at all — an interval-producing step — the inputs
are asked instead, and only answer if every one of them that can speak to it
names the *same* single sample.

The scope goes in the heading, `DimensionError in step `call` (sample
SRR32217714):`, rather than on a line of its own. The error shape M1
specified is step, both values, where each came from, a leading question; the
sample says *which* step, so it belongs with the step.

### The summary counts places, not names

`summarise` used to deduplicate the step *names* it saw, so four failing
`call` nodes reported "4 errors in 1 step". That is not a cosmetic
understatement: "1 step" describes a localised failure you fix by looking at
one sample, and "4 steps" describes a cohort-wide one you fix by rethinking
the pipeline. It now deduplicates on the node, falling back to the name when
there is none.

The count is still of *places* rather than of errors, so one node with two
problems correctly reads "2 errors in 1 step".

### One problem is stated once, and every sample is named

`check::render` groups errors whose text is identical once the scope is
removed, prints the first in full, and adds one line naming the rest:

```text
  The same problem affects 3 other samples: SRR32217715, SRR32217716, SRR32217726.
```

Named, not counted. A user who has to expand something to find out which
samples failed has a shorter report rather than a better one, so every name
is listed however many there are — four hundred accessions is a long line and
four hundred identical paragraphs is an unusable report.

Two rules keep the collapsing honest:

- **An error that cannot name itself is never collapsed.** With no scope
  there is nothing to put on the "also affects" line, so hiding one error
  behind an identical-looking one would leave a user unable to tell them
  apart. Those print separately.
- **A sample whose problem differs keeps its own paragraph.** Grouping is on
  the full text of the problem, so a sample that failed for another reason
  can never be folded into the majority.

Collapsing is a property of the rendered report, not of the error list.
`Graph::check` and `pipe.errors()` return one error per failing node, each
with its own scope, because that is the granularity a program wants —
counting affected samples, or retrying one of them in M3.

## Intervals, and the order that is not an order

`DataKind::Intervals` was held back from M1 on purpose: the reference demo
does not use intervals, and the `coordinates` dimension has nothing to do
until they arrive, since SAM, BAM and VCF are all 1-based closed.

Parsing is hand-rolled, as FASTA is. Only the first column is needed, and
`noodles`' BED reader is generic over the field count — materialising nine
typed columns per record to read one of them would buy nothing and would put
a pre-1.0 API in a hot loop. A GFF stops at `##FASTA`, because a parser that
kept going would report a contig named `>chr1`.

Contigs come from the records, so:

- **no length, no checksum.** A BED records neither. Containment
  ([D10](../decisions.md)) already compares only what is present, which is
  what makes these files checkable against a reference at all — by name,
  which catches a `chr`-prefixed BED against a bare reference and an interval
  on a contig the reference does not have. A GFF's `##sequence-region` gives
  a length, but only when the region starts at base 1: a directive describing
  part of a contig says nothing about how long the contig is.
- **no dictionary order.** This is the trap. `relate` compares the positions
  of matched contigs and reports `OrderDiffers`, because GATK requires
  sequence dictionaries to agree. A BED's records have an order too, but it
  is the order the intervals happen to be sorted in, and comparing it would
  report a contig order mismatch for a perfectly ordinary BED that lists
  contig 2 before contig 1.

So `ReferenceId` now records whether its contig list is a `Dictionary` or
merely `Observed`, and order is compared only between two dictionaries.

### What intervals require

`required_pair` demands only `coordinates` of a pair involving intervals —
never `reference`. An interval file's contigs come from its records, so an
empty one knows none, and a user has no way to declare them: `build=` is a
display label and cannot carry contigs. Requiring something nobody can supply
is an unsatisfiable check, which the catalogue already treats as worse than a
missed one (the same reasoning that keeps `quality_encoding` out of the
table). It is still *checked* whenever both sides state it, which is every
non-empty file.

`coordinates`, by contrast, is always known on both sides, because it is a
property of the format. It is a demand the user can always satisfy and the
one mismatch no amount of checksum comparison would catch.

## A miss this milestone fixed

Found while testing propagation, because the graph compares derived datasets
with each other constantly where M1 mostly compared data against a reference.

Under `Relation::Compatible` — two datasets, neither of which is the
reference — contigs the other side lacks are ignored. So a `chr1`-named VCF
against a bare-named VCF shared *nothing*, agreed vacuously, and reported no
error: there was nothing left in common to disagree about. The naming
dimension did not catch it either, because naming is deliberately suppressed
when both sides have a known reference, on the grounds that the reference
comparison subsumes it.

A comparison pass that matched nothing can no longer claim agreement. It
returns a third outcome, `Disjoint`, which is never reported and exists only
to send the comparison into its second, normalised pass — where the overlap
is found and the result is the naming error it always should have been. Two
datasets that share nothing even normalised still agree, because a
chr20-only VCF and a chr21-only VCF are both correct and saying otherwise
would be a false alarm.

## Read pairing lives in Rust

`lg.samples()` globs in Python, with `glob` from the standard library, and
groups in Rust. The split is not arbitrary: globbing is a filesystem
convenience, while deciding that two files are mates of the same sample is
correctness-critical. Pairing two samples' files by accident produces results
for a mixture of two people and nothing crashes.

So a file whose mate cannot be identified is an error rather than a guess, a
lone mate is an error rather than a single-end sample, and two files claiming
the same mate is an error rather than a silent overwrite. The marker is
matched anywhere in the stem rather than only at the end, and everything from
it onwards is dropped, so Illumina's `SAMPLE_R1_001.fastq.gz` gives the
sample `SAMPLE` rather than `SAMPLE_001`.

## The CLI is Python

`ligase check` has to import the user's pipeline module — that is how a
Python pipeline declares itself, the way a Snakefile does — so it cannot be a
Rust binary. `argparse` keeps it dependency-free.

The module is executed; the step bodies are not, because the decorator defers
them. The acceptance test proves that rather than asserting it: every body in
the test pipeline raises, so a passing check is evidence that nothing ran.

Pipelines are found by scanning the imported module's namespace for
`Pipeline` instances. The alternative — a global registry that `Pipeline`
writes itself into — would find pipelines built inside functions too, but at
the cost of hidden global state that leaks between imports. Scanning requires
the pipeline be assigned at the top level, which the error message says
plainly.

The summary line is still worded in the core, so every surface reports a
check the same way.

## Trade-offs and known gaps

- **The handle is not really the type it claims to be.** `lg.handle()` exists
  because of it, and a user who stores a step's output somewhere typed as
  `Alignment` and expects to read its contigs will find they must build the
  pipeline first. The payoff is static checking of pipeline wiring, which
  seems worth it.
- **Propagation picks the first stating input.** Where two inputs disagree,
  the output takes one of them. The disagreement is reported at that step, so
  nothing is hidden, but a downstream error may name a value that came from
  the side you did not expect.
- **Plain parameters are recorded and otherwise ignored.** `min_freq` is in
  the signature and in `StepOutput.arguments`, and does nothing until it
  joins the cache key in M3.
- **Nothing executes.** `pipe.run()`, `pipe.methods()`, `lg.sh()` and
  `ligase run` are later milestones. The parser deliberately does not offer
  subcommands that would not work.
- **Requirements are still per-step at heart.** The graph decides *where* to
  report a requirement and attributes it to its origin, but what counts as
  required still comes from `required_pair` over the kinds a step joins. A
  step cannot yet say "I need the sample set" on its own behalf.
- **One graph per pipeline.** Two pipelines over the same inputs build two
  graphs and load nothing twice, but check the shared inputs twice over.
- **A scope is only ever a sample.** `Scope::kind` is a `&'static str` so
  that other discriminators could be added, but nothing produces one yet. A
  fan-out node whose sample set is unknown therefore has no name, and its
  errors are reported separately rather than collapsed — correct, but less
  readable than it could be.

## Related

- [Pipelines and the graph](../concepts/pipelines.md) — the user-facing view
- [Type system architecture](type-system.md) — dimensions, and the checker
  the graph drives
- [Decisions](../decisions.md) — D12, D13 and D14 were taken here
