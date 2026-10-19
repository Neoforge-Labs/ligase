# Provenance

`crates/ligase-core/src/prov/`. No pyo3; exposed through `ligase-py` and
reached from Python as `pipe.methods()` and `ligase methods`.

Three pieces, each answering a different question:

| Module | Question |
|---|---|
| `prov/mod.rs` | What was recorded about how a step ran? |
| `prov/methods.rs` | What would I write in a paper? |
| `prov/bundle.rs` | Would someone else get the same thing? |
| `prov/cite.rs` | Who do I credit for this tool? |

## The store

`provenance.sqlite`, in the cache directory and **not** in `cache.sqlite`.
One row per step key in `produced`, with four child tables —
`produced_parameters`, `produced_inputs`, `produced_references`,
`produced_tools` — rather than JSON blobs in one row, so that a question
about a paper can be answered with `sqlite3` and no parser:

```console
$ sqlite3 .ligase/provenance.sqlite 'select tool, version from produced_tools'
minimap2|2.31-r1302
samtools|samtools 1.24...
```

That is the same bet the cache makes
([D6](../decisions.md#d6-sqlite-metadata-content-addressed-store)), and it
is why the engine needs no JSON library.

### Why a separate file

`Cache::open` drops its tables when `KEY_VERSION` or `CANON_VERSION` moves,
because an entry written under another format is a claim about ingredients
that were combined differently and must never be read
([D19](../decisions.md#d19-a-cache-from-another-version-is-discarded-not-migrated)).
None of that reasoning applies to a record of what a command did. A separate
file in the same directory survives every such bump for free, and
`PROV_VERSION` tracks this schema on its own
([D23](../decisions.md#d23-provenance-has-its-own-store-and-its-own-lifetime)).

### What is keyed, and what is not

A record is filed under the step key, which is already the identity of the
computation: the same code, parameters, input contents and tool versions
*are* the same step, and a second run of it has nothing new to say.

Two things are deliberately absent.

**The scope.** The sample label a report puts beside a step's name depends on
the pipeline the step was reached from, not on the step. `Graph::scopes` can
work it out whenever it is needed, and storing it would be a fact about one
run masquerading as a fact about the computation.

**Citations.** A citation cannot change a result, so it is not a key
ingredient — and storing one in a record would go stale the moment it was
edited, since editing it does not rerun the step. `prov::methods` takes them
from the pipeline in front of it, which is always current
([D24](../decisions.md#d24-a-citation-is-curated-or-declared-never-guessed)).

## The write, and where it happens

In `exec.rs`, inside `Runner::attempt`, after the output is stored:

```rust
let produced = produce(&task.recipe, &inputs, &task.parameters, &staging, self.host)?;
// ...
self.remember(&Made { spec, task, key, inputs: &inputs,
                      command: produced.command, /* … */ })?;
```

It has to be there. `exec::produce` now returns an `Execution` carrying the
command it ran, because `fill_template` is the only thing that ever renders
it and the rendered command exists nowhere else — not in the cache, and
since M4 not in the output either
([D22](../decisions.md#d22-a-tools-signature-is-not-part-of-its-result)).

A step served from the cache writes nothing. That is not an omission: the
record from the run that did the work is still there, and that is the whole
point of keeping it outside the cache.

`Stores { cache, prov }` is passed to `exec::run` and `audit::audit` so that
"a run records provenance" is visible in the type rather than hidden in a
constructor. A provenance write that fails fails the step, exactly as a
cache metadata write already does: a result the engine cannot account for is
not one it should serve.

### Cost

The hashes a record needs are the ones the key was just built from, so
`remember` makes cache lookups rather than reads — the `files` prefilter and
the memoised `canonical` table both answer from SQLite. One extra
`content_hash` per input file is taken alongside the canonical hash, because
a record names the file by its bytes as well as by its meaning.

Where that hashing happens matters more than what it costs. It runs inside
the step's own worker, and BLAKE3 hashes a large file across whatever thread
pool it is called from — so writing a record, like building a key, queues
work on the executor's own pool. That is safe only because no worker in that
pool is ever blocked waiting for scheduler work
([D27](../decisions.md#d27-a-worker-that-waits-is-a-worker-rayon-cannot-use),
[the scheduler](execution.md#nothing-waits-for-work)). A `ligase methods`
run on the demo is what found this out, by hanging.

## Pinning a reference

`prov::pin` reads `ReferenceId`, which `load::reference` already filled:

- **accession** — the contig name, where it looks like one. The check is
  `is_accession`: one to six uppercase letters, an optional underscore, five
  to ten digits, an optional `.version`. `K03455.1` and `NC_001802.1` pass;
  `chr1` and `1` do not.
- **checksum** — the per-contig MD5 the loader computes over the uppercased
  sequence with whitespace removed, which is the digest the SAM
  specification prescribes for `@SQ M5`
  ([D11](../decisions.md#d11-noodles-for-format-io-and-the-msrv-follows-it)).
  It is given for a single-sequence reference; a multi-sequence one has no
  single checksum to name.
- **fingerprint** — always present, so a reference with no accession is
  still told apart from any other.

Nothing here is declared, and nothing is guessed. That is the whole reason
the accession requirement cost no new API: both halves of "accession plus
checksum" were already being read off the file in M1.

## The paragraph

`prov::methods` takes a graph, its roots, the tasks, **a run report** and
the store. The run report is where the keys come from, which is why
`pipe.methods()` runs the pipeline first.

### Grouping

One written step becomes one node per sample, and four identical sentences
with a different accession in each would be unreadable. So nodes are grouped
by a signature of everything that could make their sentences differ — step
name, command template, parameters, tool versions, reference fingerprints —
and each group gets one sentence saying how many nodes it covers.

Where two nodes of a step genuinely were run differently they fall into
different groups and get a sentence each. That is not a special case; it is
the same rule, and it puts the difference in front of the reader.

### Wording

Fixed sentence forms, assembled in `Group::sentence` and wrapped at 76
columns. Two details worth knowing:

- `describe_tool` does not say a tool's name twice. `samtools --version`
  prints `samtools 1.24`, while `minimap2 --version` prints `2.31-r1302` and
  nothing else, so the name is prepended only when the version does not
  already start with it. Only the first line reaches a sentence; the rest —
  the htslib a samtools was built against — is in the bundle.
- The **template** is printed, not the filled command. A filled command
  names a staging file inside the cache, which is true and of no use in a
  paper. The filled one is in the record, and `commands.tsv` prints it.

### Gaps

A node whose key has no record becomes a `Gap`, reported by name with
`ligase run --force` as the remedy. Nothing can reconstruct such a command,
which is exactly the trade D22 made.

## The bundle

`prov::bundle::write` takes a `Request` and produces a directory. Seven flat
TSV tables, the source of every step, whatever files the caller asked to
carry along (the CLI passes the pipeline file), and a `MANIFEST` hashing all
of it with BLAKE3.

Fields are escaped — `\\`, `\t`, `\n`, `\r` — so a command containing a tab
cannot shift a column, and `unescape` reads them back.

### What "a clean machine" means

A bundle cannot carry the reference pipeline's inputs: they are gigabytes,
and fetching them is out of scope
([D8](../decisions.md#d8-out-of-scope-for-v01)). So the claim is narrower
and checkable: *a run with these inputs, on a machine with these tools,
lands on these canonical hashes*. To this engine a clean machine is an empty
cache — nothing computed, nothing remembered, every tool probed again — and
that is what the acceptance test uses
([D25](../decisions.md#d25-a-bundle-states-a-claim-and-can-be-checked)).

### The four verdicts

`verify` matches the bundle's rows to a fresh run's results by `(step,
scope)` rather than by key, because the key is the thing that may legitimately
have moved:

| Verdict | Key | Result |
|---|---|---|
| `Identical` | same | same |
| `Reproduced` | **different** | same |
| `Differs` | either | different |
| `Missing` | — | this pipeline produced nothing for it |

`Reproduced` is the verdict that justifies the whole mechanism. Keys are
content-addressed, so a bundle checked on a machine with the same data and
the same tool versions comes out `Identical` — and one checked where the
aligner is a different build comes out `Reproduced`, which is a stronger
statement than the cache could ever make.

Inputs are checked by content as well, and reported only when something
moved: a run of four hundred samples should not print four hundred lines to
say nothing changed.

## The citation table

Four entries' worth of promise: `minimap2` (Li 2018) and `samtools` /
`bcftools` (Danecek et al. 2021, one paper for both). It is short on
purpose — every entry is a commitment to keep a reference correct, and a
long list of half-checked citations is worth less than a short list of
checked ones plus an honest gap. Anything else comes from `cite=` on the
step, and a tool neither covers is *named as uncited*.

## What is not here

**Validation.** Whether the resistance calls agree with the published ones
is [M6](../milestones.md). Provenance says how a number was produced; it
says nothing about whether it is right.

**Fetching pinned data.** A bundle names its inputs by content hash and its
references by accession and checksum, and checks the ones it finds. Going
and getting them is out of scope for v0.1 (D8).

**A history of runs.** One row per step key, replaced if the same key is
produced again. The key *is* the computation, so the second run has nothing
new to say about it except the machine and the moment, where the later
answer is the better one. A pipeline wanting an append-only log of every
invocation would want a different table, not a different key.

**Protection from a tool that names a sample after a file.** `bcftools`
names a VCF's sample column after its input BAM's *path* when the BAM
carries no `@RG SM`, and a Ligase run hands it a path inside the
content-addressed store. The canonical form of a VCF includes its sample
names — correctly, because a sample set is a dimension
([D15](../decisions.md#d15-an-error-identifies-its-node-not-just-its-step))
— so such a VCF's canonical hash depends on where the cache is, and a
bundle written from one cache cannot verify against another.

This is D22's pattern in a field that cannot be dropped, and it is handled
in the reference pipeline rather than in the engine: `align` writes its read
group, so the sample is the run accession it should always have been. The
engine does not yet notice when a tool hands it a path as a sample
identifier, and a pipeline whose alignments lack `@RG SM` will meet this.
Refusing or normalising a path-shaped sample name is a canonicalisation
change, which belongs to M4's rules and to a `CANON_VERSION` bump.

**Provenance for a step that failed.** `pipe.methods()` refuses a pipeline
with a failure rather than describing the steps that worked, because a
paragraph covering most of a pipeline reads exactly like a paragraph
covering all of it.
