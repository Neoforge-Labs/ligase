# Cache

How `.ligase/` is laid out, how a key is composed, and what each choice
costs. The user-facing account is
[Running, and not running](../concepts/caching.md).

Everything here lives in `crates/ligase-core/src/`: `hash.rs`, `key.rs` and
`cache.rs`. None of it knows about Python.

## The directory

```text
.ligase/
├── cache.sqlite          metadata: keys, outputs, file hashes
├── store/                content-addressed objects
│   └── 3c/73431de6...eeaa85.tsv
└── tmp/                  staging, for steps that are still running
```

Three properties are deliberate.

**The store is fanned out over the first byte of the hash.** A cohort of
four hundred samples through five steps is two thousand objects; a flat
directory of those is a directory no filesystem enjoys. One level of 256
buckets is enough for the sizes this tool is for.

**An object keeps its suffix.** `samtools sort -o x.bam` writes BAM and
`x.sam` writes SAM: tools read the format off the name, so an object called
`3c73…` with no extension is an object no tool will read correctly.
Everything after the first dot survives, which is what keeps `.vcf.gz`
intact.

**Staging is inside the cache directory.** Moving a finished output into the
store is then a rename rather than a copy — a 24 GB BAM copied once more per
run would be a real cost — and a crashed run leaves its debris somewhere a
user can delete wholesale.

Stored objects are made read-only. The store is the cache's evidence: a step
that wrote back through one of its inputs would invalidate every key derived
from it without anything noticing.

## The schema

```sql
CREATE TABLE meta  (name TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE files (path TEXT PRIMARY KEY,
                    size INTEGER NOT NULL,
                    mtime_ns INTEGER NOT NULL,
                    hash TEXT NOT NULL);

CREATE TABLE steps (key TEXT PRIMARY KEY,
                    step TEXT NOT NULL,
                    output_hash TEXT NOT NULL,
                    suffix TEXT NOT NULL,
                    tools TEXT NOT NULL,
                    duration_ms INTEGER NOT NULL,
                    created_at TEXT NOT NULL);
```

SQLite because [D6](../decisions.md#d6-sqlite-metadata-content-addressed-store)
says so: zero-config, embedded, portable, and — the part that matters for a
reproducibility tool — openable with `sqlite3` when a result needs
explaining. A faster key-value store that can only be read by the program
that wrote it would be a worse trade here.

`rusqlite` is used with the `bundled` feature, which compiles SQLite from
source. That costs a C compiler at build time, which is a real concession
given that [D11](../decisions.md#d11-noodles-for-format-io-and-the-msrv-follows-it)
chose pure-Rust format parsing precisely to avoid one. It buys one identical
SQLite everywhere, which an on-disk format that outlives a single run needs
more than it needs a shorter build.

`duration_ms` and `tools` are not used for cache decisions. They are there
because the next question after "was this cached?" is "how long did it take
when it did run, and with what?" — and because M5 will want them.

## Versioning, and why nothing is migrated

Two version numbers sit in `meta`.

`schema_version` describes the directory layout. A directory written by
another version has its tables dropped.

`key_version` describes how keys are composed
([`key::KEY_VERSION`](#the-key)). A mismatch deletes every row of `steps`,
and keeps `files`.

That asymmetry is the whole point. A file's hash is a fact about the file,
true regardless of how keys are made. A step's entry is a claim that *these
ingredients, combined this way* produced that output — so a key from another
format is not stale data to be migrated, it is a claim about something else.
Reading it would be the one failure mode a cache must never have.

Nothing is migrated because nothing needs to be: every byte in here is
derived, and the cost of being wrong is enormously higher than the cost of
recomputing.

## The prefilter

`Cache::content_hash` consults `files` by `(path, size, mtime_ns)`. A hit
returns the stored hash without opening the file; a miss hashes it and
records the result.

Three details keep this honest.

**A timestamp can only ever save work.** It never contributes to a hash and
never to a key. The direction of the bet matters: a stale timestamp causes a
rehash (a cost), never a wrong answer — except in the one case below.

**The file is re-stat'd after hashing.** A file written *while* we were
hashing it would otherwise be remembered under the size and mtime it had
before, and every later run would trust that hash. If the stat moved, the
hash is used for this run and not recorded.

**The miss case is a file edited in place**, keeping its exact length,
within one timestamp granularity. This is the residual risk of
[D3](../decisions.md#d3-blake3-with-sizemtime-prefilter), it is documented
rather than hidden, and `--force` is the answer. The alternative — hashing
every input on every run — makes looking at a 40 GB BAM cost minutes, which
turns content addressing from a feature into something users route around.

`Cache::stats()` counts hits and misses, which is where `report.hashed` and
`report.prefiltered` come from. It exists so the prefilter is *testable*:
asserting that a second run read nothing is how you find out it is working,
and no amount of staring at output tells you that.

## The key

```rust
pub struct Ingredients {
    pub step: String,                      // the step's name
    pub code: String,                      // its source, as written
    pub recipe: String,                    // the command, or "callback"
    pub output: DataKind,                  // what it declares it produces
    pub parameters: Vec<(String, String)>, // (name, value)
    pub inputs: Vec<(String, ContentHash)>,// (parameter, content hash)
    pub tools: Vec<(String, String)>,      // (name, version)
}
```

Hashed with BLAKE3, every field length-prefixed. The length prefixes are not
decoration: concatenating would make a parameter `x` with value `yz`
indistinguishable from a parameter `xy` with value `z`, and a cache key that
can confuse two different inputs is a wrong answer waiting for the right
coincidence. There is a test for exactly that pair.

Names bind, order does not. Inputs, parameters and tools are sorted by name
before hashing, so the order they were collected in cannot change a key —
but *which* name a value is bound to very much does, because the same two
files swapped between `aln` and `ref` is a different command.

The composition is pinned by a test that asserts a literal key for a fixed
set of ingredients. That makes an accidental change to the layout a failing
test rather than a silently cold cache for every user of the release; it is
meant to be updated deliberately, together with `KEY_VERSION`.

`code` is the function's source as written, read with `inspect.getsource`.
The consequence is worth stating plainly: a comment-only edit reruns the
step. That is the safe direction — too cold costs time, too warm produces a
wrong answer — and
[early cutoff](../concepts/caching.md#why-outputs-are-named-after-their-contents)
stops the rerun going any further. For a function with no source to read (a
REPL, an `exec`), the compiled code object stands in.

## What an input's hash is a hash of

Inputs enter as content hashes tagged with the policy that produced them.
Since M4 that is `INPUT_POLICY = "canonical"`: the hash of what a file
*means* rather than of its bytes
([D4](../decisions.md#d4-semantic-hashing)) — sorted records, volatile
headers such as `@PG` lines and timestamps stripped, as
[canonicalisation](canonicalisation.md) defines per type.

It arrived exactly as this page said it would, which is worth recording
because it was the point of putting the tag in before needing it: the policy
name changed, `KEY_VERSION` went to 2, the old entries retired themselves,
and nothing in the shape of `Ingredients` moved. `Runner::key` asks
`Cache::semantic_hash(kind, path)` where it asked `Cache::content_hash`.

Three things sit in `meta` now rather than two, and they are cleared
differently. A layout change drops the tables; a key-format change deletes
the step entries; a change to the canonicalisation rules
(`CANON_VERSION`) deletes the canonical hashes **and** the step entries
built from them. The `files` table survives all three, because a file's byte
hash is a fact about the file however keys are made.

The **store** still names objects after their bytes, deliberately: an
object's name is a claim about its contents that anyone can check with
`b3sum`. The canonical hash is a derived fact held beside it, in the
`canonical` table, keyed on the byte hash rather than on a path — so a file
moved, copied, or produced identically by another step is parsed once in its
life.

## What is not here

Provenance (M5). This layer records that a key produced an output, with the
tool versions and the duration, because it had them. It does not record
*why* a result is what it is — references, citations, the methods paragraph
— and a cache is the wrong place to grow that: the question "has this been
computed?" and the question "what would I write in a paper?" want different
lifetimes and different storage.
