# M18 — Cluster execution on the shared store

*Written against v0.2 at commit `0c005b0` (27 Oct 2026). The last engine
milestone of v0.3. Read alongside `CLAUDE.md`, §5.7 and §7 of
`docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md`,
`web/content/project/architecture/store.mdx`,
`web/content/project/architecture/execution.mdx`, D42, D43 and M14's entry in
`web/content/project/milestones.mdx`.*

---

## 1. Intent

Deliver the thing the shared store was built for. D43 closes with the
sentence this milestone is answerable to:

> It is also the precondition for cluster execution in v0.3: workers need a
> shared store before they can share work.

v0.2's out-of-scope list is just as precise about what was *not* delivered:
"Cluster execution (the shared store is its precondition, not its delivery)."
So there is a store, there is a scheduler that assumes one machine
(`architecture/execution.mdx` says so under **What is deliberately absent**),
and nothing joins them.

There is no written specification beyond that sentence. This document is it.

**The claim this milestone exists to make, and to prove.** Several machines
can run one pipeline between them with **no coordinator, no scheduler
integration and no lock** — because the step key is a content hash and the
store is write-once, so two workers that compute the same key produce the
same answer. Duplicated work is a cost; it is never a wrong result. Section 4
states that argument precisely, section 4.4 says where it breaks, and the
engine's behaviour at the break is a done-when clause rather than a footnote.

---

## 2. What "cluster" means here, concretely

Three readings were available. The choice is the first question, because
every other decision follows from it.

### 2.1 Rejected: a local worker pool over independent nodes

This already exists and has since M3. `exec::run` builds a rayon pool sized
to `--jobs`, walks a ready queue, and runs independent nodes at once.
Shipping it again under a new name would be a release note, not a milestone.

### 2.2 Rejected: submission to a scheduler (Slurm, SGE, Kubernetes)

The obvious reading of "cluster execution", and the wrong boundary.

A research group that has a cluster has **Slurm** — SGE is in retreat and
Kubernetes is an industry answer, not an academic one. But a site's Slurm is
its own: partitions, accounts, QoS, time limits, a module system, a local
scratch convention, and an MPI or container story. A submitter is therefore
either wrong at most sites or endlessly configurable at all of them, and
either way Ligase would own job lifecycle — submission, polling, requeue,
cancellation, log scraping — which is the scheduler's job and is precisely
where workflow tools accumulate their weight.

It also mistakes what the group is missing. A group with a cluster already
knows how to start twenty of something: `sbatch --array=1-20`. What they do
not have is twenty processes that can divide one pipeline between them
without a coordinator. That is the gap, and it is the gap content addressing
closes.

### 2.3 Chosen: a coordinator-free worker that any launcher can start N of

`ligase run --worker[=NAME]` makes the process a **worker**. A worker builds
the whole graph, as every run does, and then computes only the steps it can
claim through the store; everything else it takes from the store, or waits
for. N workers started however the group starts things — a Slurm array, a
`for` loop over `ssh`, a few workstations, a container per node — converge on
one complete run and then exit.

Ligase submits nothing, knows nothing about Slurm, and holds no cluster
state. Its whole contribution is that the workers **need no coordinator**.

This serves both of the deployments a research group actually has:

| What the group has | How they start workers | What the store is |
|---|---|---|
| A Slurm cluster | `sbatch --array=1-20 … --worker=$SLURM_ARRAY_TASK_ID` | `s3://…`, or a `file://` path on Lustre/GPFS |
| A few workstations | `ssh node0N ligase run --worker=node0N` | `s3://…` |
| One big machine | `ligase run --jobs 64` — **not** workers | the local tier |

That last row is a rule, not an aside. **`--jobs` is for one machine and
`--worker` is for several.** Workers coordinate through a store at a cost per
step; threads coordinate through a mutex at no cost. Running eight workers on
one host is paying network prices for a local problem, and §5.5's
cache-directory default is shaped so the mistake is at least not a
correctness one.

### 2.4 And one ordinary run that collects

A worker reports what *it* saw. The authoritative report, the methods
paragraph and the reproducibility bundle come from one final **ordinary**
`ligase run` with no flags, which finds every step in the store, computes
nothing, and writes up the whole graph.

This is not an extra mechanism — it is M14's published rows doing exactly
what they were built for, and it is already demonstrated by
`tests/s3.rs::a_second_machine_takes_the_work_from_the_bucket`. Making it the
*designed* collection step rather than a happy accident is what gives
§6's clause 10 and clause 11 their shape, and it is what makes a distributed
run produce one write-up instead of N partial ones (§5.6).

So the deployment is **N workers, then one collector**, and the collector is
a command the user already knows.

---

## 3. Goals, non-goals, anti-goals

**Goals**

1. `ligase run --worker[=NAME]`: a worker that claims work through the store.
2. An advisory lease, so N workers divide work instead of duplicating it.
3. Lock-free correctness: the argument in §4, proved against a real bucket.
4. A lost or failed worker cannot stall a run or corrupt one.
5. Resumability: a partially complete graph completes without recomputing
   what landed.
6. One methods paragraph and one bundle for a distributed run — including
   the honesty fix distribution forces (§5.6).

**Non-goals, with reasons**

- **Job submission.** §2.2.
- **Lease extension / heartbeats.** §5.4 explains the mechanism that would
  carry them and why the generation scheme is built so they can be added
  later without a format change.
- **`ligase store gc`.** §8.
- **Cross-worker failure consensus.** §5.7.
- **Moving the SQLite index anywhere.** D43 forbids it and §5.5 checks the
  reading against the two cases that could have argued otherwise.

**Anti-goals** — things that would be worse than not doing them

- **A lock that pretends to be one.** The store affords write-once blobs, no
  listing and no compare-and-swap. A "lock" built on that is a lock that
  fails silently under exactly the conditions it exists for. The lease is
  advisory and the documentation says so in those words.
- **A worker that waits inside the rayon pool.** D27 is the scheduler's
  defining invariant and this milestone must not spend it. §5.3.
- **Silence on divergence.** Two workers computing different answers for one
  key is the one event here a user has to *read*. §4.4.

---

## 4. The argument: correctness needs no lock

### 4.1 What the premises already give

Four facts, all of them already true and none of them added by this
milestone:

1. **A step key is a content hash** of code, parameters, input *canonical*
   hashes and tool versions (`key.rs::Ingredients`). It says nothing about
   where the cache lives or which machine computed it.
2. **An object's name is the hash of its bytes** and the store is
   **write-once**: `BlobStore::put` keeps an object already held, and
   `checked_get` hashes everything on arrival (`store/mod.rs`).
3. **A published row is write-once too**, and `published::fetch` refuses a
   row that restates a table or key other than the one it was found under.
4. **A downstream key is built on the *canonical* hash of its inputs**, not
   on their bytes (`INPUT_POLICY = "canonical"`, D4). `exec.rs` keys every
   `StepResult` twice for exactly this reason, and
   `architecture/execution.mdx` records what forced it: `minimap2` writes its
   command line into `@PG`, that command line holds the staging path, and the
   staging path is named after the step's own key — so a step's *bytes* moved
   whenever its key did, and early cutoff was unearnable until the canonical
   hash became the thing below it was keyed on.

### 4.2 The argument

Let two workers both compute key `K`.

- They agreed on `K`, so they agreed on the code, the parameters, the input
  canonical hashes and the tool versions. There is no sense in which they are
  doing different work.
- Each stores its output under the hash of the bytes it got. If the bytes
  agree, both wrote the same object, and write-once makes the second write a
  no-op (M14 proved this is benign rather than assuming it:
  `two_writers_of_one_object_do_not_trip_over_each_other`, and against a real
  bucket, `many_machines_writing_one_object_is_benign`).
- Each publishes a `steps` row under `K`. Write-once: the first wins, the
  second is dropped. The row names an object that is in the store, because
  whoever published it had stored it first.
- Every third machine therefore reads one row, fetches one object, hashes it
  against its name, and proceeds.

**No step in that sequence needs mutual exclusion.** A lock would prevent
the duplicated *computation*; it is not what makes the *result* right.

So the lease (§5.4) is an efficiency device, and the engine is correct with
it switched off, mis-tuned, or failing. That is the property worth having on
a machine nobody is watching at 3 a.m.: the thing that can break is the
thing whose breakage costs CPU.

### 4.3 Where the bytes diverge and it still holds

The common case is that two workers' bytes **differ**. `@PG` again, plus
timestamps, plus thread-count-dependent record order. So:

- Worker A stores object `Oa`, worker B stores `Ob`, both under key `K`.
- One `steps` row wins — say A's, naming `Oa`.
- B's publish is dropped. **B adopts the published entry** (§5.4) and uses
  `Oa` from here on, discarding its own result's standing. `Ob` stays in the
  store, unreferenced; §8 counts that against garbage collection.
- B's downstream steps key on `canonical(Oa)`. A's key on `canonical(Oa)`.
  They agree.

And if B had *not* adopted, it would still have been fine, because
`canonical(Oa) == canonical(Ob)` — which is the whole point of
canonicalisation. **The canonical form is what makes lock-free distribution
safe.** The property M4 introduced to make early cutoff earnable is the same
property that lets two machines disagree about bytes without disagreeing
about results. Adoption is belt and braces: it keeps the store's answer
authoritative, so a third machine, A and B all name one object.

### 4.4 Where it breaks, and what the engine does

The argument needs `canonical(Oa) == canonical(Ob)`. Two cases break it.

**(a) A kind whose canonical form does not fix the result.** `canon/` defines
a canonical form per kind; where it strips less than the tool varies, two
runs of one key differ canonically. This is not hypothetical — it is the
residual that `ligase audit` exists to measure, and `architecture/audit.mdx`
already distinguishes *deterministic* from *reproducible* for this reason.

**(b) A stochastic step under D40.** D40 makes an unseeded stochastic step a
`StepDefinitionError` at decoration time, so it cannot reach a run, and a
seed is a hashed parameter — two workers with the same key have the same
seed. That is necessary and not sufficient: IQ-TREE, RAxML and BEAST are
deterministic given a seed **and a thread count**, and two cluster nodes with
different core counts are the normal case, not the exception. Same key, same
seed, different tree.

In both cases the two workers hold genuinely different answers under one key,
and everything below them would diverge.

**What the engine does.** It reports it, and does not try to resolve it.

A `steps` row gains the output's **canonical** hash (§5.2 — one field, one
`ROW_VERSION` bump). A worker whose publish is dropped therefore learns, for
free and with no extra read, whether the row it lost to means the same thing
as what it computed:

- same canonical hash → the ordinary byte divergence of §4.3, nothing said;
- different → the run report carries a **divergence**, naming the step and
  both canonical hashes, and the published entry is used.

Not a failure, because the step ran and its result stands: the fix is in the
pipeline, where the user cannot make it without being told (D26).

**And not a `Note` either**, which was the first shape tried and is wrong. A
`Note` is recorded with the cache entry and read back on every later hit
(`Cache::record` takes `&[Note]`, `Entry::notes` returns them), because a note
is a fact about *the output* — a gene barely read stays barely read. A
divergence is a fact about **this run on this worker**: another machine got
there first and disagreed. Persisting it would attach it to a key for good and
report it to everyone who ever hits that key, which is both false and
unhelpful. So it goes where `RunReport::refused` already goes, and for the
reason that field records — it is a store event a user has to *read* rather
than count. `RunReport::diverged` carries the node, so the report can render it
against its step.

The divergence names the two canonical hashes and the step, and **not** the
worker that published first. The lease row holds a worker name, but by the time
a divergence is seen that lease may have been relinquished or superseded, so
the name there is not reliably the publisher's. Naming a machine on evidence
that weak is worse than naming none.

And the published entry is used rather than the local one because §4.3's
adoption rule has to hold whether the divergence was benign or not — the
alternative is a run where each worker believes its own answer, which is the
silent failure this project exists to delete.

**What is deliberately not done**: pinning stochastic steps to a single
worker. It sounds safer and is worse. It would make the engine's answer
depend on how many workers happened to be running, hide the nondeterminism
that is a real property of the pipeline, and leave the user with a tree they
cannot reproduce and no sign that anything happened. Reporting it is what
`audit` is for, and this makes a distributed run report it too.

---

## 5. What gets built

### 5.1 `store/lease.rs` — the advisory lease

A new module beside `published.rs`, using the same write-once row machinery
and adding no object class the store does not already understand.

```rust
/// Advisory leases: which worker is computing which step key.
pub const LEASES: &str = "leases";

/// What probing a step key's leases found.
pub enum Lease {
    /// Nobody holds it, or the holder's deadline has passed.
    Free { generation: u32 },
    /// A worker holds it until `deadline`.
    Held { worker: String, deadline: i64, generation: u32 },
}

/// What this worker got.
pub enum Claimed {
    /// This worker holds the key until its deadline.
    Acquired { generation: u32 },
    /// Another worker got there first.
    Held { worker: String, deadline: i64 },
}
```

Rows live at `published::key_of(LEASES, &format!("{key}/{generation}"))` and
hold `(worker, started_at, deadline)`.

**Why generations.** A lease row cannot be deleted, rewritten or refreshed —
objects are write-once. One row per step key would therefore mean a lease
that can be taken exactly once in the lifetime of the store, which is wrong
in three ways at once: a stealing worker could not claim it, a relinquishing
worker could not release it, and a long-running holder could not extend it.
A generation counter gives all three on one mechanism:

- **`probe`** reads generations upward from 0 until it misses, bounded at
  `MAX_GENERATIONS` (16). The **highest occupied** generation is the current
  state of the key.
- **`acquire`** probes; if the highest generation is held and live by someone
  else, it returns `Held`. Otherwise it publishes at `highest + 1` and
  **reads the row back**. Write-once means the first writer wins, so the
  read-back is what settles a race between two workers that both probed and
  both saw nothing. S3, GCS and Azure are all read-after-write consistent for
  a new object, and a filesystem trivially is, so this is a check the store
  can actually honour — unlike a compare-and-swap, which it cannot.
- **`relinquish`** publishes the next generation with `deadline = now`, i.e.
  an already-expired lease. A release is just a lease nobody holds, so
  giving up needs no second object class and no new reader.
- **Extension**, when it is built, is `relinquish`'s mirror: publish the next
  generation with a deadline further out. The format carries it already,
  which is why §3 can defer it without deferring a format change.

Exhausting 16 generations means a step has been claimed and given up fifteen
times; the key is then treated as free by everyone, which degrades to §4's
lock-free correctness rather than to a stall.

### 5.2 The `steps` row gains a canonical hash

`ROW_VERSION` goes 1 → 2, and a `steps` row becomes
`(output_hash, suffix, duration_ms, notes, canonical_hash)`.

A build that reads version 1 ignores a version 2 row and recomputes, which
`published.rs` already documents as the right cost ("a row from a format this
build does not read is *ignored*, costing one recomputation, rather than
half-read"). v0.2 shipped four days ago and the project has no outside users,
so the bump costs one recomputation per key in whatever buckets exist.

**And one thing the bump uncovered, which has to be fixed before the bump is
safe.** `published::key_of` hashes the domain, the table and the key — and
*not* `ROW_VERSION`. So every version of a row competes for one object
address, and objects are write-once. A bucket holding a version 1 row would
therefore refuse every version 2 row written to that key: the `put` is
dropped, the read finds a version the build cannot parse, `fetch` answers
`None`, and that step key can never be published again by anyone. Not one
recomputation — **a permanently unpublishable key**, for as long as the bucket
exists.

The bug is latent in M14 rather than new here: it is unreachable until the
first format bump, and this is the first format bump. The fix is to put
`ROW_VERSION` into `key_of`'s hash, so each format addresses its own rows,
old rows are never read instead of being read wrongly, and the *next* bump is
free as well. The lesson generalises past this project: a format version that
is not in the name of the thing it versions can only be discovered by the
reader, which is already too late in a write-once store.

This field is what makes §4.4's divergence check free rather than a fetch and
a canonicalisation of another machine's output, which for a 24 GB BAM is not
a check anyone would leave switched on.

### 5.3 The scheduler: an outer loop, and D27 kept intact

`exec.rs`'s invariant today is that *the run is over exactly when the scope
is empty*, which holds because every node that will ever reach the ready
queue is put there by a step that is still running. A remote lease breaks it:
the scope can empty while work is outstanding on a machine this process
cannot talk to.

So `run` gains an outer loop around the scope:

```
loop {
    pool.scope(|scope| runner.start(scope));     // drain everything takeable
    if nothing deferred { break }
    if nothing can ever become takeable { break }
    sleep (backoff);                             // <- no rayon worker in flight
    re-arm `ready` from `deferred`;
}
```

**This does not spend D27.** D27 says a worker that waits is a worker rayon
cannot use, and the two tests in `tests/execution.rs` pin it by making a step
queue work on its own pool. The wait here happens *between* scopes, with the
scope drained and **zero** rayon workers in flight, so there is no worker to
starve and nothing queued from inside a step to strand. A node whose key is
leased elsewhere returns `Attempt::Deferred` and hands its worker straight
back, which is the same shape as `Attempt::Parked` and for the same reason.

The two kinds of waiting stay separate, and the distinction is worth stating
because it is the design in one line:

| | What it is | Why |
|---|---|---|
| `running` / `parked` | a **lock**, in-process | a process *can* hold one correctly, so it should |
| a lease | **advisory**, cross-process | the store cannot offer one, so nothing may depend on it |

`attempt()` gains one step, between the in-process claim and the compute:

1. `cache.lookup(key)` — hit? done. (Already consults the store.)
2. in-process `claim(key)` — taken? `Parked`.
3. **`lease::acquire`** — held by another worker? `Deferred`.
4. compute, store, record, publish entry; `relinquish` on failure.

Backoff is internal — 1 s doubling to 30 s — rather than a flag. Each
re-arm costs one store read per deferred node (`published_entry`), so the cap
is what keeps a worker idling on a wide cohort from becoming a load
generator. One flag is enough for a user to get wrong; two is a tuning
exercise nobody asked for.

### 5.4 Lease lifetime, stealing, and giving up

One flag: `--lease SECONDS`, default 3600.

A worker claims each step until `now + lease`. A worker with no takeable work
waits and re-probes; a lease whose deadline has passed is free, so it is
taken. There is no separate steal rule and no `--steal-after`: a worker
always prefers unleased ready work, so "steal only when idle" is a
consequence of the ready queue rather than a policy to implement.

**The two ways to get `--lease` wrong, and what each costs.** This is the
sentence the guide has to carry:

- **Too short** — a worker steals a step whose holder is still working. Cost:
  duplicated computation. Not a wrong answer, by §4.
- **Too long** — a worker that died holding a lease keeps others waiting
  until its deadline. Cost: idle time.

Both are costs; neither is a wrong result. Set it above your longest step.

**Failure relinquishes.** A worker whose step fails publishes a relinquish
(§5.1), so another worker retries it at once instead of idling to the
deadline. For a node-local failure — a full scratch disk, a missing module, a
bad GPU — a retry elsewhere is what the user wants. For a deterministic
failure it costs one retry per worker, which is bounded by the user reading
the first report.

### 5.5 Where the index lives — D43 checked, not assumed

D43 fixes the SQLite index as local and rebuildable. "Each worker has its own
local index" is the obvious reading; the task was to check it against the two
things that could have argued otherwise.

**Against "a second machine completes from cache".** Already demonstrated:
`tests/s3.rs::a_second_machine_takes_the_work_from_the_bucket`. A cold index
is not a cold cache, because `Cache::lookup` falls through to
`published_entry` and learns the row back. That is M14's whole point and it is
what makes a per-worker index sufficient rather than merely permitted.

**Against the `FsStore::put` race.** M14 found that staging every write of a
key at one path made two concurrent writers of one object trip over each
other, and fixed it by putting the process and a counter in the staged name.
That fix is what makes the `file://` deployment in §2.3 — a shared Lustre or
GPFS store, which is most of HPC — safe for N workers rather than two
threads. The milestone inherits a fixed race; it does not need to find it
again. It does need to *extend* the test to N processes rather than N
threads, which §6's clause 7 does.

**What a per-worker index forces, and the default that follows.** Two
processes must not share a `cache.sqlite` across machines: SQLite over NFS is
the known failure D43 names, and a Slurm array whose tasks all default to
`$HOME/.ligase` walks straight into it. So:

- `--worker=NAME` **defaults its cache directory to `<cache>/workers/NAME`**.
  The right thing is the default rather than merely the documented thing.
- An explicit `--cache` is still honoured, and a **lock file**
  (`worker.lock`, created `O_EXCL`, holding the worker name, pid and start
  time, removed on clean exit) refuses a second worker in one directory and
  names the holder.

A stale lock after a crash blocks that directory and the error says to delete
it. That is a real wart; it is accepted because the alternatives are a
dependency on `flock` (unreliable on NFS, which is the case being guarded)
or a liveness probe that is not portable, and because a stale lock costs a
deletion rather than a wrong answer.

### 5.6 Provenance under distribution

**The mechanism is already there, and the milestone's job is to prove it.**
M14 found that provenance did not travel with a shared cache entry, so runs
completed perfectly and could not be written up; D43 fixed it. `exec.rs`
publishes a record beside every output it computes
(`Runner::remember` → `prov.publish`), and `cached_result` adopts one for any
hit this machine never computed (`prov.adopt`).

It follows that **any run that sees the graph to completion holds a complete
provenance database** — every step it computed it recorded, every step it did
not it adopted. The collector of §2.4 is therefore not a new mechanism: it is
the existing one, used deliberately. One methods paragraph, one bundle, from
the one run that saw everything.

**And one honesty bug that distribution exposes.** `prov::methods` takes the
engine version and the platform from `groups.first()`:

```rust
let (engine, platform) = groups.first().map_or_else(
    || (crate::version().to_owned(), super::platform()),
    |group| (group.record.engine.clone(), group.record.platform.clone()),
);
```

and renders `"Analyses were run with Ligase {engine} on {platform}."`. Every
record carries its own `engine` and `platform`, and before this milestone
they were always the same because one machine ran everything. A distributed
run is the first time they are not: a mixed cluster, an ARM workstation
beside x86 nodes, or one worker on an older wheel. The paragraph then names
**one** platform for a run that spanned several, in a methods section, which
is the one place in this project where a quiet half-truth costs the most.

So `Methods` carries every distinct engine and platform its groups used, and
the sentence names them. This is a public API change (§7) and it is the
clause this milestone would be least honest to skip: a reproducibility tool
that cannot say where a result was computed has lost the argument it is
making.

**Claim binding (M16).** Not merged as this is written, so nothing here
integrates with it and no test asserts against it. The mechanism it will need
is the one above: a claim binds to a step key, and a record travels with its
entry, so a claim bound to a step computed on another machine binds for the
same reason `ligase methods` works. Asserting it belongs to whichever of M16
and M18 lands second, and §9 records it as an obligation rather than a hope.

### 5.7 What the user sees

`ligase run --worker=node03` prints the ordinary report with the worker
named, and the report gains a line for what distribution did — in the shape
`store_lines` already uses, so a run that used no workers prints nothing new:

```
node03: 7 steps: 4 ran, 1 from cache, 2 waited for other workers
```

A worker reports what it saw: steps it computed, steps it took from the
store, steps it waited for, and its own failures. **Cross-worker failure
consensus is a non-goal**: a worker that failed a step reports the failure and
its dependents do not run on that worker, even if another worker later
succeeds at it. The authoritative answer is the collector's, which is one
command and is where the write-up comes from anyway.

---

## 6. Done when

Each clause is pinned by a **named** test. Clauses 2–8 are the distributed
claims and run against a real bucket in CI's `store` job as well as over a
`file://` store in every build — the latter is not a stand-in but the
shared-filesystem deployment of §2.3.

1. **`--worker` exists and names itself.** `ligase run --worker[=NAME]` and
   `--lease SECONDS` carry help text (`generate_cli.py` fails without it),
   and a worker's name appears in its run report.
2. **Two workers divide one graph.** Two workers over one store and one
   pipeline complete the graph between them, and the union of what they
   computed is the whole graph.
3. **Neither duplicates the other's work.** The sets of steps the two workers
   *computed* are disjoint.
4. **A worker waits rather than duplicating.** A worker whose only ready
   steps are leased by another worker computes nothing, waits, and takes them
   from the store when they land.
5. **A lost worker cannot stall a run.** A worker that dies holding leases
   does not block the graph: once the deadline passes, an idle worker
   computes those steps and the run completes.
6. **A failure relinquishes at once.** A worker whose step fails publishes a
   relinquish, and another worker retries that step without waiting for the
   deadline.
7. **A partially complete graph is resumable.** A second invocation takes
   what landed from cache and computes only what did not — including after
   N *processes* wrote one object concurrently, extending M14's thread-level
   race test.
8. **Divergence is reported, not hidden.** Two workers computing one key to
   byte-different but canonically-equal outputs agree silently; two computing
   it to canonically-**different** outputs do not — the loser adopts the
   published entry and the run report names the step and both canonical
   hashes. And a row of one format version never occupies another's address,
   so no key is left unpublishable by the bump (§5.2).
9. **Two workers never share a cache directory.** `--worker=NAME` defaults
   its cache to `<cache>/workers/NAME`, and a second worker in one explicit
   `--cache` is refused, naming the holder.
10. **A run that computed nothing can still write it up.** The collector
    produces a complete methods paragraph and a reproducibility bundle for a
    graph every step of which was computed elsewhere.
11. **A methods paragraph names every platform and engine it used.** A run
    whose records carry two platforms, or two engine versions, says both
    rather than naming the first.
12. **The default build acquires nothing.** Cluster execution needs no new
    dependency and no cargo feature; `tests/default_build.rs` keeps the
    default build clean, and `ligase run --worker` works in it.

Plus the standing gates: `cargo fmt`, `cargo clippy -D warnings` with no
`unwrap` outside tests, `cargo test`, `ruff check .`,
`ruff format --check .` (which formats Python inside Markdown, so a code
block in this file is held to it too), `mypy`, `pytest`, and `npm run check`
in `web/`.

---

## 7. Public API, and the dependency question

**No new dependency.** Deliberately checked rather than assumed, and it
shaped §5: worker identity is a name the user supplies (`--worker=NAME`,
defaulting to a short id derived from the pid and the clock) precisely so
that nothing needs a hostname, for which there is no stdlib call and which
would have cost a crate. Time is `std::time`. The lease rides on
`published.rs`. `tests/default_build.rs` asserts it.

**Public API changes, which `CLAUDE.md` says to raise before implementing:**

| Surface | Change |
|---|---|
| CLI | `ligase run --worker[=NAME]`, `--lease SECONDS` |
| Python | `Pipeline.run(worker=…, lease=…)` |
| Rust | `exec::Options` gains `worker` and `lease` |
| Rust | new `store::lease` module (`Lease`, `Claimed`, `probe`, `acquire`, `relinquish`) |
| Rust | `store::published::publish` returns what it found rather than `()`, so a dropped write is distinguishable from a fresh one — §4.4 needs that distinction and it belongs in the store rather than in `exec.rs` |
| Rust | `Cache::record` takes the output's canonical hash and returns whether the store superseded it |
| Rust | `exec::RunReport` gains `diverged` (§4.4) |
| Rust | `prov::methods::Methods::engine`/`platform` become plural (§5.6) |
| Store format | `ROW_VERSION` 1 → 2, and the version enters `published::key_of`'s hash so each format addresses its own rows; `steps` rows gain a canonical hash (§5.2) |

Every new public name needs a docstring and a group in `generate_api.py` or
the docs build fails.

---

## 8. `ligase store gc` — its own milestone, and why

v0.2 deferred store garbage collection to v0.3 as unsized (§8.5 of the v0.2
spec, and `architecture/store.mdx` under **What is deliberately not here**).
It does not belong here, for three reasons that get stronger in that order.

1. **It is orthogonal.** Distribution is about who computes; gc is about what
   is kept. Neither needs the other.
2. **This milestone makes gc harder, not easier.** Before: one machine, one
   local tier, one process. After: N workers writing concurrently, plus two
   new classes of garbage — the unreferenced loser objects of §4.3 and a
   lease row per step key per claim, forever. A collector that cannot tell a
   live claim from a dead one deletes the object a running worker is about to
   publish a row for.
3. **It is still unsized, and now more so.** v0.2 deferred it for want of a
   measurement to size an LRU with. This milestone is what will produce that
   measurement, because it is the first thing that fills a shared store at
   cluster rates. Sizing gc from data M18 generates is the right order;
   guessing it inside M18 is not.

So: **its own milestone**, and this one leaves it two more things to collect
and a reason to be designed for concurrent writers. Said on the architecture
page rather than left for a reviewer to find.

---

## 9. Gaps named, not papered over

- **Lease extension.** A holder cannot extend past its deadline, so a step
  longer than `--lease` is stolen. The generation format carries extension
  (§5.1) so adding it needs no format change. Until then `--lease` is the
  knob and §5.4's two costs are the documentation.
- **Lease rows are never collected.** §8.
- **The loser's object is never collected.** §4.3, §8.
- **No cross-worker failure consensus.** §5.7.
- **No scheduler integration.** §2.2. The guide gives a worked `sbatch
  --array` recipe, which is documentation rather than code, and that is the
  boundary on purpose.
- **A stale `worker.lock` blocks its directory.** §5.5.
- **Claim binding is not asserted across machines.** §5.6. The obligation
  travels to whichever of M16 and M18 lands second, recorded here in M15's
  style so it cannot be quietly dropped.
- **The NeoRx seven-disease experiment is not in this repository.** The v0.3
  roadmap names it as v0.3's validation case, "ported across". Nothing of it
  is discoverable here: `docs/design/neorx-expansion-notes.md` is absent and
  the v0.2 spec already records its absence, and the only other mentions are
  a colour in the identity spec and an `identifier_namespace` value in a
  guide example. There is therefore nothing to port, and porting it is not
  this milestone — it is a validation milestone of its own, needing the
  experiment's own data and code from wherever they live. §6 carries no
  clause for it.
