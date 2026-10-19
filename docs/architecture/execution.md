# Execution

How a checked graph becomes parallel work. The cache it consults is
[described separately](cache.md); this is about who runs what, where, and
why Python is not in the loop.

Lives in `crates/ligase-core/src/exec.rs`, with the Python side in
`python/ligase/graph.py` (`Pipeline.run`, `Pipeline._plan`) and the binding
in `crates/ligase-py/src/lib.rs`.

## Recipes are extracted before anything runs

A step's body is Python. A run is a few dozen external processes over tens of
gigabytes. Those two facts pull in opposite directions: if the scheduler had
to call into Python to find out what each step does, the GIL would sit in
the hot path of every parallel run.

So it does not. The body is called **once, before the run starts**, and what
it returns is a recipe:

```python
return lg.sh("minimap2 -ax sr {ref} {reads.r1} {reads.r2} | samtools sort -o {out}")
```

That is a `Shell` holding a template — a pure value, no paths in it, nothing
read. `Pipeline._plan` collects one per step node, together with the
function's source, its plain parameters and its tools, and hands the core a
complete description of the work. The core then owns the whole run with the
GIL released, and a cohort of shell steps runs at full width.

This is only possible because bodies were already deferred
([D12](../decisions.md#d12-steps-defer-the-graph-is-built-from-signatures)):
a body that *computed* something could not be called before its inputs
existed. A body that *describes* work can.

The one exception is `lg.write`, whose work genuinely is Python. The core
schedules it like any other step and takes the GIL to call it, through a
`Host` trait so that nothing in the core knows Python exists:

```rust
pub trait Host: Sync {
    fn call(&self, id: u64, out: &Path, inputs: &[(String, Vec<PathBuf>)])
        -> Result<(), String>;
}
```

Those steps serialise against each other. For an interpretation step that is
the right trade; for anything doing real work it is the wrong one, which the
guide says out loud.

## The scheduler

Index order is dependency order — a node can only name earlier ones
([the graph](graph.md)) — so there is no topological sort to do. What is left
is a ready queue:

- `remaining[node]` counts unfinished upstream *steps* (input nodes are
  already there).
- A node whose count reaches zero joins `ready`.
- A step runs, then decrements its dependents and starts whichever of them
  that emptied.

The state sits behind one `Mutex`, and each step is one rayon job on a pool
sized to `--jobs`. A job that finishes starts the steps it released before it
ends, so the run is over exactly when the scope is empty: it cannot empty
while anything can still start.

Rayon rather than Python threads is the direct consequence of recipes being
extracted up front: there is no interpreter to re-enter, so the pool is just
a pool. It is also already in the tree for BLAKE3's multi-threaded hashing of
large files.

### Nothing waits for work

No worker loops looking for the next step and none blocks waiting for one.
That is not a style preference; it is what keeps the pool safe to reuse from
inside a step.

A step hashes its own inputs, and BLAKE3 hashes a large file across whatever
pool it is called from
([D3](../decisions.md#d3-blake3-with-sizemtime-prefilter)) — so a step queues
work on the executor's own pool as a matter of course. Rayon can only give
that work to a worker it believes is idle. The first version of this
scheduler held every spare worker blocked on a `Condvar` of its own, inside a
`scope.spawn` job, which rayon counts as busy: hashing inside a step was left
with no one able to help it, and under heavy load a run stopped for good
([D27](../decisions.md#d27-a-worker-that-waits-is-a-worker-rayon-cannot-use)).

A worker can still block where rayon cannot see it — on a step's own child
process, or on the GIL inside a callback. The invariant is narrower than "a
worker never blocks": a worker must never block on something that needs the
pool to make progress. A child process and the GIL do not; the ready queue
and a claimed key did.

Two tests in `tests/execution.rs` pin this. Each has a step queue work on the
pool it is running on and wait for it with a timeout, once while another step
is waiting its turn and once while a twin key is waiting — the two places a
worker used to block. They fail in about ten seconds against the old
scheduler rather than hanging, so a regression is a red test and not a
`cargo test` that never returns.

## One key, one piece of work

Two nodes can share a key — the same step over identical data, which happens
more often in a cohort than you would expect. Running both would be the same
work twice, and under a thread pool the second would not even see the first's
cache entry.

So a key is *claimed* for the duration of a step. A node that wants a
claimed key is *parked* on it — recorded under that key and abandoned, with
its worker handed straight back to rayon. When the holder drops the claim it
puts everything parked on its key back on the ready queue, and those nodes
start again from the top: they recompute the key, find it in the cache this
time, and report a hit.

Parking rather than waiting is the same rule as above. A worker blocked on a
claimed key is a worker rayon counts as busy, and therefore one that cannot
help the very step it is waiting for
([D27](../decisions.md#d27-a-worker-that-waits-is-a-worker-rayon-cannot-use)).
Parking also costs no `--jobs` slot: a waiting node used to occupy a worker
doing nothing.

The alternative — letting both run and relying on the store to deduplicate
identical bytes — is correct but wasteful, and makes "how many times did this
run?" unanswerable. For a tool whose job is to not redo work, that is the
wrong default.

## Filling in a command

`{name}`, `{name.r1}`, `{name.r2}` and `{out}` are resolved against the
step's own parameters; `{{` and `}}` are literal braces.

Two decisions here are about failure modes rather than features.

**Unknown placeholders are an error.** `{refrence}` is a typo, and a shell
handed the literal text would either fail obscurely much later or — with
`echo` — succeed. The cost is that an `awk '{print $1}'` has to be written
`'{{print $1}}'`, which is [Snakemake's
convention](https://snakemake.readthedocs.io) and so already familiar to
the people this is for. Anything between braces that is not an identifier
is reported as a brace needing escaping rather than as a mistyped
placeholder, because that is the thing the user has to change.

**Every substituted value is one shell word**, single-quoted, with embedded
quotes escaped. A reference under `/my refs/` is one path, not two
arguments, and a value cannot end the quoting and continue as code.

## Failure is per node, and independent branches continue

A step that fails is recorded against its node, with the tool's own stderr
and the exit status. Then:

- Independent branches keep running. Losing three good samples to one bad
  FASTQ is the overnight-run problem this project exists to remove.
- Steps *below* a failure do not run, and are in neither list: one cause,
  one failure, rather than a cascade of "input missing" noise.
- Nothing is recorded in the cache, so the staged output of a half-finished
  step can never be served as a result.

`pipe.run()` then raises `RunError` carrying the whole report, the same shape
`pipe.check()` uses: every problem found before anything is raised, and the
successes still reachable.

## Why the report is worded in Rust

`exec::summarise` and `exec::render` build the text, exactly as the check
report is built in the core. Two surfaces print it — `ligase run` and
`report.text` — and one wording for both is the only way they cannot drift.

Two rules carry over from
[D15](../decisions.md#d15-an-error-identifies-its-node-not-just-its-step),
for the same reasons:

- A step that ran gets its own line with its own cost, and the costs line up,
  because reading down a column is how you find the step that took the
  night.
- Steps that did not run are grouped by outcome and their samples are
  **named**, not counted. A user who has to expand something to learn which
  four samples were cached has a shorter report, not a better one.

Results are sorted by node before rendering. Workers finish in whatever
order the machine allows, and a reproducibility tool that printed two
different reports for two identical runs would be an odd thing.

## What `skipped` means, and why it is not `cached`

Both are cache hits. The difference is whether anything above them ran.

Each finished node carries a flag: did this step, or anything above it, run
in this run? A cache hit whose upstream flag is set is `skipped` — early
cutoff — and one whose flag is clear is `cached`.

That distinction costs one boolean and answers the question a user actually
has after an edit: *did my change reach anything?* "Everything cached" and
"the step reran and changed nothing, so the rest was unnecessary" are very
different pieces of news.

"Changed nothing" means the output's **canonical** form is unchanged, not
its bytes ([canonicalisation](canonicalisation.md)). With bytes, `skipped`
was a verdict almost no real tool could earn: `minimap2` records its command
line in `@PG`, that command line holds the output path, and the output path
is the staging file named after the step's own key — so a step's bytes moved
whenever its key did. Every `StepResult` therefore carries both hashes, and
the one below it is keyed on the second.

## What a run records besides its answer

A step that runs writes a [provenance](provenance.md) record naming the
command that ran, with its placeholders filled in. It has to happen here:
`fill_template` is the only thing that renders that command, and since M4 it
is nowhere else — not in the cache, and not in the output, whose `@PG` line
is dropped before hashing
([D22](../decisions.md#d22-a-tools-signature-is-not-part-of-its-result)).
A step served from the cache writes nothing, because the record from the run
that did the work is still there.

`exec::produce` therefore returns an `Execution` — the duration and the
command — and `exec::run` takes a `Stores { cache, prov }`, so that "a run
records provenance" shows up in the type rather than hiding in a
constructor.

## What is deliberately absent

**Turning those records into prose.** A methods paragraph and a bundle are
`prov`'s business, built after a run rather than during one.

**Running a step twice** to find out whether it is reproducible is
`audit.rs`, which borrows `exec::produce` and `exec::inputs_of` rather than
repeating them ([canonicalisation](canonicalisation.md#audit-mode)).

**Cluster execution** ([D8](../decisions.md#d8-out-of-scope-for-v01)). The
scheduler assumes one machine and a shared filesystem. The ready-queue shape
would survive a change of executor, but nothing has been built for one.
