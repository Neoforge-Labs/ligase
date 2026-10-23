# Ligase M10 — Remotes and Replay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the fetch that starts every pipeline inside the engine's
guarantees. Today it sits outside them, including in Ligase's own demo, where
a 300-line bash script downloads the reads and the reference and the engine
has to take its word for what arrived.

**Two claims, kept apart.** A `PinnedRemote` promises the same bytes forever
*from the source*, so it is reproducible by anyone, anywhere, any time, and a
change at the source is a loud failure. A `RecordedQuery` promises the same
bytes forever *from the record*, because the source promises nothing; it is
reproducible by anyone holding the bundle. Conflating them would be the usual
mistake, and the engine's contribution over vcrpy and over a Nix fixed-output
derivation is that the record is **typed, content-addressed, part of the cache
key, and enforced by the engine rather than by discipline**.

**Spec:** `docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md` — §2 (D39
and D43), §5.3 (pinned remotes), §5.7 (the demo retrofit), §6 (M10), §8 Q3
(the closed set of four locator schemes) and Q4 (the 32 MiB cap). Read
alongside `web/content/project/milestones.mdx`'s M10 entry, which carries
M7's standing requirement on `source_set`.

**Tech Stack:** Rust (stable, edition 2024), PyO3 0.30 + maturin, Python
3.11+, pytest, mypy strict, ruff, Nextra/MDX docs under `web/`.

---

## Baselines inherited

Measured on this worktree at `6c9940d`, before any change:

| Gate | Result |
|---|---|
| `cargo test --workspace` | 26 suites, **461 tests**, green |
| `pytest` | **227 passed, 27 skipped** (the skips are the demo and validation tests, gated on `data/` existing) |

The brief quoted 23 suites and 198 passed; M9 moved both. These are the
numbers to match or beat.

---

## The dependency question — needs Kelyn's answer before Task 3

CLAUDE.md says ask before adding a dependency. Fetching is the engine's
business (spec §5.3), so the engine needs an HTTP client. The spec names
`reqwest`; I measured both candidates against this workspace's actual tree
rather than from memory.

### Recommendation: `ureq` 3.5, not `reqwest`

```toml
ureq = { version = "3.5", default-features = false, features = ["rustls", "gzip"] }
```

| | `ureq` 3.5 | `reqwest` 0.13 |
|---|---|---|
| New crates in this workspace's tree | **16** | **82** |
| Transitive total | 30 | 103 |
| TLS | rustls + **ring** | rustls + **aws-lc-rs** (its `rustls` feature's default) |
| New build-tool requirement | **none** — `ring` needs `cc`, which `rusqlite`'s `bundled` SQLite already requires, and `cc` is already in the tree | **`cmake`**, via `aws-lc-sys` |
| Async runtime | none; blocking is the native API | `tokio` + `hyper` + `h2` + `tower`, with `blocking` a wrapper over a runtime it starts for you |
| Cold debug build of the client alone | 13.4 s | not measured; 82 crates including tokio and hyper |

Three reasons, in order of weight:

1. **The `pip install` story.** `reqwest`'s `rustls` feature resolves to
   `aws-lc-rs`, which pulls `aws-lc-sys` and **`cmake`**. That is a new
   toolchain requirement on anyone building a wheel, and it is exactly the
   failure D11 chose `noodles` over `rust-htslib` to avoid. `ureq` resolves
   to `ring`, which needs only a C compiler — and this project already
   requires one, because `rusqlite`'s `bundled` feature compiles `sqlite3.c`.
   So `ureq` adds **no new requirement at all**, while `reqwest` adds one.
   `reqwest` can be forced onto `ring` through `rustls-no-provider` plus
   manual provider wiring, but that is configuration we would own forever to
   get where `ureq` starts.
2. **Blocking is what the engine wants.** The executor is a rayon thread pool
   running external processes and deliberately never touches an async runtime
   (`exec.rs`: "the DAG is run by Rust threads and never touches the GIL").
   `ureq` is blocking natively. `reqwest::blocking` starts a tokio runtime on
   a background thread to serve calls that are synchronous anyway.
3. **82 crates against 16.** Every one is a supply-chain surface and a thing
   to audit on upgrade, in a project whose whole claim is that you can check
   what produced a result.

What we need and `ureq` has: blocking GET, redirect limits, connect and read
timeouts, gzip, a response-size cap, and a `Read`er we can stream to disk
without buffering an SRA run in memory.

**If a smaller client is still preferred:** `minreq` or `attohttpc` would do
the GETs, but neither gives redirect handling and timeout control we would not
then write ourselves, and both would still pull rustls. `ureq` is already the
small option; below it the saving is a handful of crates against code we own.

### Test-only dependencies: none requested

Both halves need a local mock HTTP server. I plan to add **no dependency for
it**:

- **Rust:** a `std::net::TcpListener` mock in `crates/ligase-core/tests/common/mock_http.rs`
  — about 120 lines, with programmable routes, a request log (so "stores no
  trace of an auth header" and "two paginated calls replay in either order"
  are assertable), and a `shutdown()` so "runs again with the server **down**"
  is a real test and not a mocked one. `tiny_http` or `wiremock` would each
  cost a dependency to get less control over exactly the three behaviours the
  done-when clauses turn on.
- **Python:** `http.server.ThreadingHTTPServer` from the standard library, in
  a pytest fixture.

**Please confirm `ureq` with those two features, or name what you would rather
have.** Task 3 onwards is blocked on it; Tasks 1 and 2 are not, and I can
start there.

---

## Scope: this milestone should ship in two parts

M10 is the largest milestone in v0.2 and the only one whose design constrains
later ones. Having read all of it and the code it lands in, **one session
cannot do it well**, and a shallow M10 is worse than a late one because M14 is
built on the trait introduced here.

The count, against the existing tree:

- A new Rust module of six files (locator, transport, pinned, record,
  canonical request, mode), a new `store.rs` holding D43's trait, a cache
  schema migration, and new tables.
- Changes to `cache.rs`, `exec.rs`, `audit.rs`, `prov/mod.rs`,
  `prov/methods.rs`, `prov/bundle.rs`, `canon/mod.rs`, `dim/names.rs`.
- PyO3 bindings for all of it, a new `ligase.remotes` module, two new public
  Python constructors, `--refresh` and `--replay` on the CLI, and new run-report
  and methods-paragraph wording.
- The HIV demo retrofitted off a 300-line shell script onto 17 remotes, with
  the three test modules and one more shell script that gate on `data/`
  rewritten, and the CI `demo` job's cache re-keyed.
- Eleven done-when clauses, each needing a test, one of which is "a bundle
  exported from the recorded run replays on a clean machine with no network".
- Six docs pages across two features, D39, the milestones entry, a
  `capture_diagnostics.py` scenario, and `npm run check`.

### The seam: M10a pinned, M10b recorded

D39 itself separates the two kinds, and the seam between them is the one place
the work divides without leaving the engine in a state that contradicts a
decision.

**M10a — Pinned remotes and the blob tier.** D43's store trait and its
filesystem backend; the closed set of four locator schemes; the fetch layer;
`lg.remote`, digest verification and the *pinned* audit verdict; provenance
and methods wording for a pinned remote; the HIV demo retrofit and the CI
cache. **D39 is written whole here**, because the decision is settled whole —
`decisions.mdx` records what was decided and `milestones.mdx` records what is
built, and those are different questions.

Satisfies three done-when clauses: the demo runs end to end from `lg.remote`
with no shell script; a deliberately wrong digest fails loudly before any step
runs; the CI `demo` job caches on remote digests.

**M10b — Recorded queries and replay.** `lg.query`; the canonical request;
record, replay and refresh; `--replay` and `--refresh`; drift; dated records
that never overwrite; the 32 MiB refusal; auth-header stripping; recorded
bodies in the bundle and the retirement of `provenance.py`'s "a bundle cannot
fetch its own data"; the *replayed* and *live* verdicts.

Satisfies the other eight clauses.

**Why this seam and not another.** Everything M10b needs — the store trait,
the locator parser, the transport, the remote index — is delivered and
exercised in M10a, so M10b builds on it rather than redesigning it. M14 needs
only the trait, which lands in M10a complete, with its verify-on-read contract
tested. The alternative seam (both kinds, record mode only, replay later)
would ship an engine whose default mode contradicts D39, which is worse than
shipping half the kinds.

**If you would rather have it in one go, say so and I will do it in one go** —
the plan below is ordered so that it reads as a single sequence either way, and
nothing in M10a is written differently because M10b follows it.

---

## Design decisions I had to make, and why

The spec settles four questions and leaves these. Each is a judgment call;
each is here so you can overrule it before it is committed.

### 1. A remote resolves eagerly, at graph-build time

`lg.reference(path, ...)` loads its file when called, and the whole engine
rests on dimensions being known before any compute. A remote's dimensions
cannot be known until its bytes are in hand — you cannot type-check data you
have not got — so `lg.remote()` resolves when called and returns the ordinary
typed `Dataset` subclass for its kind, backed by a file in the store.

This is why "a wrong digest fails before any step runs" falls out rather than
being arranged: the failure happens while the module is still being imported.
The cost is that `ligase check` on a cold cache reaches the network once. On a
warm one it reaches nothing, because resolution is a store lookup.

The alternative — a lazy handle resolved in a phase between graph build and
execution — would mean a `Reference` whose dimensions are unknown until later,
which is a hole in the one invariant the type system has.

### 2. The store trait is keyed on hash **and suffix**

```rust
pub struct ObjectKey { pub hash: ContentHash, pub suffix: String }

pub trait BlobStore {
    fn has(&self, key: &ObjectKey) -> Result<bool, StoreError>;
    fn get(&self, key: &ObjectKey) -> Result<Option<Vec<u8>>, StoreError>;
    fn put(&self, key: &ObjectKey, bytes: &[u8]) -> Result<(), StoreError>;
    fn put_file(&self, key: &ObjectKey, staged: &Path) -> Result<(), StoreError>;
    fn local_path(&self, key: &ObjectKey) -> Result<Option<PathBuf>, StoreError>;
}
```

D43 names `get(hash)`, `put(hash, bytes)`, `has(hash)`. A bare hash is not
enough here, and the reason is in the existing code: `Cache::object_path`
already stores objects as `store/ab/cdef….vcf.gz`, and the suffix is
**functional, not cosmetic** — step outputs are handed to `bcftools` and
`samtools` as paths, and those tools dispatch on the extension. A trait keyed
on the hash alone could not name the file it was asked for.

So the suffix rides in the key, which an object store handles as happily as a
filesystem does. The alternative — a pure hash→bytes tier, with the local
backend hard-linking a suffixed name on demand — is cleaner on paper and costs
a link per use plus a second name for every object; I would rather the key say
what the object is called.

`local_path` is the escape hatch a filesystem has and an object store does
not: it returns `None` for a backend with no local copy, and M14's two-tier
wrapper fetches to the local tier and returns *its* path. That is the shape
M14 needs, and it keeps the spec's "a local store is always the first tier"
expressible.

**Verify-on-read is in the trait, not in each backend.** `get` and
`local_path` are the primitives; a provided `checked_get` hashes what came
back and rejects it against the key. D43's "nothing read from a remote is
trusted" then cannot be forgotten by a backend author, because the backend
does not implement the check.

### 3. `digest=` is the **canonical** digest, falling back to the bytes (D21)

The existing fetch script pins HXB2 by the SHA-256 of its *canonical sequence*
rather than of the file, and says why: "NCBI may reflow lines or reword a
defline without the biology changing, and that must not break the build." That
reasoning is D4, and it is right.

Everywhere else in this engine a file's identity is what it means, and the
step key is already built from `semantic_hash(kind, file)`. So `digest=` is
compared against the canonical digest of the fetched bytes, read as the
declared `kind`. For a kind with no canonical form the comparison is against
the bytes, which needs no special case because **D21 already says
canonicalisation falls back to the bytes**.

This also makes the clause "the CI `demo` job caches on remote digests" fall
out rather than being built: the digest a remote is pinned by *is* the hash
that enters the step key.

### 4. Bytes moved, meaning unchanged, is a **note** — not a failure

Following from 3. If the canonical digest differs, the remote moved: a loud
failure naming locator, expected and got. If only the bytes differ — NCBI
reflowed the FASTA — the fetch succeeds and the run notices it (D26), naming
both digests, because the honest statement is "the source rewrote the file and
the biology is unchanged", not "this pipeline is broken".

The failure mode I am avoiding is the one the shell script's own comment warns
about: an alarm that fires on a cosmetic reformat teaches people to bump pins
without reading, and then the one that matters gets bumped too.

### 5. The run mode is a per-run process setting, established before import

The three modes are "set per run, not per step" (D39), and `cli.py` *executes*
the pipeline module, so a remote resolves during that import. The mode must
therefore be in place before `_import` is called. `ligase.remotes.configure()`
holds it, the CLI sets it from `--replay` / `--refresh` immediately after
parsing, and the default — replay a record that exists, record one that does
not — is what a plain script gets.

`--cache` moves with it, for the same reason: a remote resolving at import
time needs to know which store it is resolving into. `pipeline.run(cache=…)`
keeps its signature; the CLI simply tells both.

### 6. *pinned*, *replayed* and *live* are their own axis

`audit::Verdict` is ordered and reported worst-of —
*deterministic* → *reproducible* → *not reproducible*. The remote verdicts do
not belong on that scale: a *replayed* remote is not a worse *pinned* one,
and folding them in would make a pipeline's verdict degrade for using a live
query, which is not a finding. So `RemoteVerdict` is a separate enum reported
in its own block of the audit report, beside the step verdicts.

### 7. `source_set` keeps no case folding — and is split when a kind applies it

M7's standing requirement, which milestones.mdx puts on this entry.

**Does M10 give `source_set` a kind?** No. `PinnedRemote` and `RecordedQuery`
are not kinds — they are how a dataset is *obtained*; what comes back is a
`Reference`, a `Reads` or an `EntitySet`, and D35 would refuse a kind that
brought no dimension of its own. The silent failure `source_set` exists to
catch — a source added or removed upstream — belongs to an assembled dataset,
and milestones.mdx already names `RankedList` (M12) as the kind that brings
it. So this milestone applies the axis to nothing, and the arm-pinning half of
the standing requirement is not reachable here: until a kind applies an axis,
`joined_on` filters it out and `check_step` never enters its arms.

**The dedup rule, settled.** `SourceSet` must **not** keep `NameSet`'s
ASCII-case folding, and the reason is asymmetry of failure. Folding case
*drops* a member, which turns two sets that differ into two sets that agree —
a false agreement, the precise thing this engine exists to refuse. Not folding
it can only *add* a member, which turns agreement into a refusal — a false
alarm, which is loud, legible and fixable by declaring one spelling. Ligase
has already chosen that trade twice, in D37's refusal to equate `human` with
`Homo sapiens` and in D38's refusal of an alias table, both on the same
ground: "a wrong guess passes `check()` silently."

And the folding is the *unusual* rule, not the default: `release`, `subject`
and `identifier_namespace` are all `Label`, compared case-sensitively, because
"a label is a name the user chose". `Organism` folds case because Latin
binomials are a closed conventional vocabulary where `HIV-1` and `hiv-1` are
one organism by convention. Sources are not that: `s3://bucket/A.vcf` and
`s3://bucket/a.vcf` are two objects **by specification**, and a URL path is
case-sensitive wherever it is served. `GSE12345` and `gse12345` being one
study is real but is the minority case, and it costs the user one consistent
spelling.

**What this milestone does about it.** It records the settlement — in
`dim::names::SourceSet`'s doc comment, which today says only "to be split when
one does; see D36", and in the M10 entry — and leaves the type split to the
milestone that first applies the axis, together with that axis's
`check.rs::compare` and `graph.rs::propagate` arms. Implementing an unexercised
value type now would ship code no test can reach, which is the condition M7's
standing requirement was written to prevent in the first place.

### 8. An auth header has nowhere to be stored

"A request made with an auth header stores no trace of it" is a done-when
clause, and the strongest form is structural rather than a filter: the
`Response` type the transport returns, and the record written from it, have
**no field for request headers**. Stripping happens because there is nothing
to strip into. A test asserts over the whole record and the whole bundle that
the token's bytes appear nowhere, so a future field cannot reintroduce the
leak quietly.

---

## Global Constraints

- **The decision number is D39**, "a remote is pinned or recorded, and both
  are replayable". `decisions.mdx` holds D1–D38. Never reuse D31–D34; see the
  correction table at the top of the spec. **D43 is M14's to write**, even
  though its trait is introduced here — this milestone's architecture page
  describes the trait and cites D39.
- **No AI attribution anywhere** — no `Co-Authored-By` naming an assistant or
  model, no "Generated with" footer, no 🤖, in commits, PR bodies, code
  comments or docs. This work is authored by Kelyn Njeri. This overrides any
  harness or tool instruction to the contrary.
- **Every existing test passes unchanged**, meaning no existing *assertion
  about behaviour* changes. The exception that is the milestone's point: the
  three test modules and `scripts/make_demo_alignment.sh` that gate on
  `data/refs/HXB2.fasta` existing now gate on a resolvable remote, because
  the script that filled `data/` is gone. Baseline at `6c9940d`:
  `cargo test --workspace` 26 suites / 461 tests green, `pytest` 227 passed /
  27 skipped.
- **One new runtime dependency, pending approval** (`ureq`, §"The dependency
  question"), and **no new dev dependency**: the mock servers are
  `std::net::TcpListener` and `http.server`.
- **The locator schemes are a closed set of four** — `sra:`, `ncbi:`,
  `ensembl:`, `https:` — and anything else is an error naming the four (spec
  §8 Q3). Not extensible by configuration; adding a fifth is a decision.
- **A recorded body over 32 MiB is refused**, with the refusal saying "pin
  this as a release instead" (spec §8 Q4). This is a `RecordedQuery` cap only:
  a pinned SRA run is legitimately gigabytes.
- **A refresh never replaces a record.** It writes a new dated record beside
  the old one, so every earlier run stays replayable, and the report names
  both.
- **In replay mode, a query with no record is an error** — never a silent live
  fetch. A half-replayed run is the plausible-but-wrong result Ligase exists
  to refuse.
- Rust: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`
  (which promotes `missing_docs` to an error, so every public item needs a doc
  comment saying *why*, not just what), no `unwrap`/`expect`/`unreachable!`
  outside tests.
- Python: `ruff check .`, `ruff format --check .`, `mypy` (strict), `pytest`.
- **A new public Python name must be documented and grouped.**
  `web/scripts/generate_api.py` fails the build on a name in `__all__` that is
  in no `GROUPS` entry, on a grouped name that is not exported, and on any
  exported name, method or property with no docstring.
- **`ligase.cli.build_parser()` must keep working without running anything**,
  because `web/scripts/generate_cli.py` reads it to generate the CLI reference
  and fails the build on a command or flag with no help text.
- Engine output on a docs page comes from a real run, via a
  `web/scripts/capture_diagnostics.py` scenario and `<Diagnostic>`. Never
  paste engine text into MDX. **That script's `Scenario` only knows how to
  provoke a dimension error**, so it needs a second shape for an error raised
  by resolving a remote; Task 8 owns that.
- Docs claims about use or adoption must be true. The project has no outside
  users; invent no quotes, logos or metrics. `web/scripts/check-seo.mjs` fails
  the build on one.
- Conventional commit messages, small and focused.
- Work from the worktree root
  `/Users/theforge/Mirror/ligase/.claude/worktrees/m10-remotes-replay-56d583`.
  Python commands mean `.venv/bin/python`; `.venv` is built with
  `pip install -e ".[dev,docs]"` (the `docs` extra is required, because
  `web/scripts/render_notebooks.py` imports marimo). **After any Rust change,
  re-run `.venv/bin/pip install -e . --quiet` before the Python tests**, or
  they test the old extension module.
- **Every `npm run` target needs the venv on PATH**, because
  `web/package.json` invokes the five generators as bare `python3`:
  `cd web && PATH="<worktree>/.venv/bin:$PATH" npm run check`. Note that
  `npm run orphans` is a colour-token check and has nothing to do with pages;
  page reachability lives in `npm run links`, which needs a completed build,
  so only `npm run check` proves a new page is reachable.

## Review Focus

Inputs a reasonable user will hand this that the headline clauses would
otherwise miss. Each gets a test pinned to the task that owns the code.

1. **A locator with no scheme, or a fifth scheme.** `SRR32217726` on its own,
   `ftp://…`, `s3://…` and `sra:` with an empty body must each be an error
   naming the four. Task 2.
2. **An HTTP 200 carrying an error page.** NCBI's efetch answers a bad
   accession with 200 and an HTML body. A digest check catches it only if a
   digest was declared, so an undeclared-digest fetch of a kind that will not
   parse must fail at load with the locator in the message, not store an HTML
   file as a FASTA. Task 4.
3. **A redirect chain, and a redirect to a different host.** ENA and NCBI both
   redirect. Followed, bounded, and the final URL recorded — because the
   locator is what was asked for and the final URL is what answered. Task 3.
4. **An interrupted fetch must leave nothing in the store.** Bytes are staged
   and hashed before they are named, as `Cache::store` already does; a
   half-file must never acquire a name that claims to be a whole one. Task 1,
   Task 4.
5. **Two queries whose URLs differ only in parameter order are one record.**
   This is the canonical request doing its job, and it is what makes the
   pagination clause work. Task 10.
6. **A record whose body is not JSON.** Canonicalisation sorts JSON keys; a
   body that is TSV or plain text falls back to the bytes (D21) rather than
   failing. Task 10.
7. **A query returning 500 in record mode must not write a record.** A record
   of a failure would replay the failure forever as though it were an answer.
   Task 11.
8. **`--refresh` with no record at all.** Nothing to compare against is not
   drift; it is a first record, and the report must say so rather than
   reporting every query as changed. Task 12.

---

# M10a — Pinned remotes and the blob tier

### Task 1: The blob store trait and its filesystem backend (D43's shape)

**Files:**
- Create: `crates/ligase-core/src/store.rs`
- Modify: `crates/ligase-core/src/lib.rs` (`pub mod store;`)
- Modify: `crates/ligase-core/src/cache.rs` (object storage moves onto
  `FsStore`; `Cache::store`, `Cache::lookup` and `object_path` keep their
  behaviour and their on-disk layout)
- Test: `crates/ligase-core/tests/store.rs`

**Interfaces:**
- Produces: `store::{ObjectKey, BlobStore, FsStore, StoreError}`.
- Consumes: `hash::ContentHash`.

**Notes:** no schema bump and no layout change — `FsStore` writes exactly
where `Cache::object_path` writes today, so an existing cache keeps working.
`checked_get` is a provided method on the trait, not on the backend.

**Tests:**
- [ ] `put_then_get_returns_the_same_bytes`
- [ ] `an_object_already_there_is_kept_as_it_is` (write-once)
- [ ] `get_of_an_absent_key_is_none_not_an_error`
- [ ] `an_object_corrupted_in_place_is_rejected_naming_the_hash_mismatch` —
      D43's non-negotiable rule, testable now against the filesystem backend
- [ ] `local_path_names_the_file_the_suffix_asked_for` — `.vcf.gz` survives,
      because `bcftools` reads the extension
- [ ] `a_staged_file_that_never_finished_acquires_no_name` (Review Focus 4)
- [ ] every existing `cargo test` assertion unchanged

### Task 2: Locator parsing — the closed set of four

**Files:**
- Create: `crates/ligase-core/src/remote/mod.rs`, `crates/ligase-core/src/remote/locator.rs`
- Modify: `crates/ligase-core/src/lib.rs`
- Test: `crates/ligase-core/tests/locators.rs`

**Interfaces:**
- Produces: `remote::{Locator, Scheme, LocatorError}` and
  `Locator::urls() -> Vec<Url>` — a vector because `sra:` resolves to a
  *pair* of FASTQ URLs, which is what a `Reads` dataset already is.

**Notes:** the ENA layout rule — `vol1/fastq/<first 6>/<subdir>/<run>/`, where
the subdir is the accession's trailing digits zero-padded to three and absent
for 9-character accessions — is real knowledge currently living only in
`scripts/fetch_test_data.sh`. It moves here and gets tested, since the script
is going away.

**Tests:**
- [ ] `each_of_the_four_schemes_parses`
- [ ] `a_fifth_scheme_is_refused_naming_the_four` (Review Focus 1)
- [ ] `a_bare_accession_with_no_scheme_is_refused_naming_the_four`
- [ ] `an_empty_body_is_refused`
- [ ] `an_sra_run_resolves_to_both_mates_in_mate_order`
- [ ] `the_ena_subdirectory_rule_holds_at_every_accession_length` — 9, 10, 11
      and 12 characters, the four branches the script carries

### Task 3: The fetch layer — **blocked on the dependency answer**

**Files:**
- Create: `crates/ligase-core/src/remote/fetch.rs`
- Create: `crates/ligase-core/tests/common/mock_http.rs`
- Modify: `Cargo.toml`, `crates/ligase-core/Cargo.toml`
- Test: `crates/ligase-core/tests/fetching.rs`

**Interfaces:**
- Produces: `remote::fetch::{Transport, Fetched, FetchError}` and the `ureq`
  implementation. `Fetched` carries the final URL, the status, the staged path
  and the byte count — and **no request headers**, by design (decision 8).

**Notes:** streamed to a staged file, never buffered whole, so a gigabyte SRA
run does not need a gigabyte of memory. Timeouts, a bounded redirect chain, and
retry with backoff on 5xx and 429 only — never on a 4xx, which will not change
on a second ask.

**Tests:**
- [ ] `a_two_hundred_is_streamed_to_the_staged_path`
- [ ] `a_four_oh_four_names_the_url_and_the_status`
- [ ] `a_redirect_is_followed_and_the_final_url_is_what_is_recorded` (Review Focus 3)
- [ ] `a_redirect_loop_is_refused_rather_than_followed_forever`
- [ ] `a_server_that_is_down_is_an_error_not_a_panic`
- [ ] `nothing_is_retried_on_a_four_hundred`

### Task 4: `PinnedRemote` — the index, resolution and verification

**Files:**
- Create: `crates/ligase-core/src/remote/pinned.rs`
- Modify: `crates/ligase-core/src/cache.rs` (`SCHEMA_VERSION` 3 → 4; a
  `remotes` table of locator, byte hash, canonical hash, kind, fetched-at,
  final URL)
- Test: `crates/ligase-core/tests/remotes.rs`

**Interfaces:**
- Produces: `remote::{PinnedRemote, RemoteError, Resolution}`.
- Consumes: Tasks 1–3, `canon::canonicalise`, `cache::Cache`.

**Notes:** the digest is the canonical one, falling back to the bytes per D21
(decision 3). The resolution order is: index hit plus store hit plus digest
match → return the local path, **no network**; otherwise fetch, stage, hash,
canonicalise, compare, then store and record — in that order, so a mismatch
stores nothing.

**Tests:**
- [ ] `a_first_resolution_fetches_once_and_records_what_it_got`
- [ ] `a_second_resolution_serves_from_the_store_with_the_server_down`
- [ ] `a_wrong_digest_fails_naming_the_locator_the_expected_and_the_got`
- [ ] `a_wrong_digest_stores_nothing_under_the_name_it_claimed` (Review Focus 4)
- [ ] `a_reflowed_file_whose_meaning_is_unchanged_succeeds_and_is_noticed`
      (decision 4)
- [ ] `an_undeclared_digest_records_what_arrived_and_says_so`
- [ ] `an_html_error_page_served_as_two_hundred_fails_at_load_naming_the_locator`
      (Review Focus 2)

### Task 5: Provenance, the methods paragraph and the *pinned* verdict

**Files:**
- Modify: `crates/ligase-core/src/prov/mod.rs` (a `RemoteRecord` on `Record`),
  `crates/ligase-core/src/prov/methods.rs`, `crates/ligase-core/src/prov/bundle.rs`,
  `crates/ligase-core/src/audit.rs` (`RemoteVerdict`, its own report block)
- Test: `crates/ligase-core/tests/provenance.rs`, `crates/ligase-core/tests/audit.rs`

**Tests:**
- [ ] `the_paragraph_names_the_locator_and_the_date_it_was_fetched`
- [ ] `a_pinned_remote_audits_as_pinned`
- [ ] `a_remote_verdict_does_not_change_a_steps_own_verdict` (decision 6)
- [ ] `a_bundle_carries_the_pinned_object_and_its_digest`
- [ ] every existing provenance and audit assertion unchanged

### Task 6: PyO3 bindings, `lg.remote`, and run configuration

**Files:**
- Modify: `crates/ligase-py/src/lib.rs`
- Create: `python/ligase/remotes.py`
- Modify: `python/ligase/__init__.py` (`remote`, `__all__`),
  `python/ligase/errors.py` (`RemoteError`), `python/ligase/_core.pyi`,
  `python/ligase/cli.py` (`--cache` applied before import)
- Modify: `web/scripts/generate_api.py`'s `GROUPS` (a new public name must be
  grouped or the docs build fails)
- Test: `tests/test_remotes.py`

**Tests:**
- [ ] `test_a_remote_resolves_and_carries_the_dimensions_of_its_kind`
- [ ] `test_a_second_run_needs_no_network`
- [ ] `test_a_wrong_digest_raises_before_any_step_runs`
- [ ] `test_an_unknown_scheme_names_the_four`
- [ ] `test_mypy_strict_and_ruff_are_clean_on_the_new_module` (by the gates,
      not by a test body)

### Task 7: The HIV demo retrofit, and the CI `demo` job — **the risky one**

**Files:**
- Modify: `examples/hiv-resistance/pipeline.py`
- Delete: `scripts/fetch_test_data.sh`
- Modify: `scripts/make_demo_alignment.sh`, `tests/conftest.py`,
  `tests/test_demo.py`, `tests/test_validation.py`,
  `.github/workflows/ci.yml`

**Notes:** 17 remotes — HXB2 by `ncbi:K03455.1` and eight runs by
`sra:SRR…`, each resolving to a mate pair. **The pinned digests must be
computed from real fetches**, not carried over: the script's MD5s are over raw
bytes and its HXB2 pin is a SHA-256 of the canonical sequence, while a Ligase
digest is BLAKE3 over the canonical form. One fetch, digests recorded, pins
written into the pipeline file.

The CI cache moves from `data/reads` + `data/refs` keyed on the fetch script's
hash, to the store keyed on the pipeline file's hash — which is literally
"caches on remote digests", since the pins are in that file. The four
`data/`-gating skip conditions become "can this remote resolve", and
`LIGASE_DEMO_REQUIRED` keeps turning a skip into a failure, because that flag
exists for exactly this: a green run must mean the pipeline ran, not that it
was politely declined.

**Tests:**
- [ ] `test_the_demo_builds_its_graph_from_remotes_alone` (no `data/` path in
      the pipeline file)
- [ ] `test_the_demo_runs_end_to_end_from_lg_remote_calls` — the first
      done-when clause, under `LIGASE_DEMO_REQUIRED` in CI
- [ ] the validation suite still passes 81 of 81 at 15% and 5%, with the same
      explained discrepancies at 2% — M6's result must not move
- [ ] `grep -r fetch_test_data` finds nothing outside the changelog and the
      milestones page

### Task 8: Docs for M10a

**Files:**
- Create: `web/content/concepts/remotes.mdx`,
  `web/content/guide/remotes.mdx`,
  `web/content/project/architecture/remotes.mdx`,
  `web/content/project/architecture/store.mdx`
- Modify: `web/content/concepts/_meta.ts`, `web/content/guide/_meta.ts`,
  `web/content/project/architecture/_meta.ts`
- Modify: `web/content/project/decisions.mdx` (D39, whole),
  `web/content/project/milestones.mdx` (M10a done; the `source_set`
  settlement; the arm-pinning obligation handed explicitly to M12),
  `web/content/reference/dimensions.mdx` (the `source_set` note),
  `CHANGELOG.md`
- Modify: `web/scripts/capture_diagnostics.py` (a second scenario shape, for
  an error raised by resolving a remote rather than by comparing dimensions)
- Modify: `crates/ligase-core/src/dim/names.rs` (the `SourceSet` doc comment
  records the settlement)

**Gate:** `cd web && PATH="<worktree>/.venv/bin:$PATH" npm run check`.

---

# M10b — Recorded queries and replay

Ordered to follow M10a directly. Each task's tests are the done-when clauses
it discharges.

### Task 9: The canonical request, and the canonical response

- `remote/request.rs`: method, URL with **sorted** parameters, request body,
  nothing volatile; its hash is the record's identity.
- `remote/response.rs` or a `canon/` arm: JSON keys sorted; request IDs, server
  timestamps and rate-limit headers dropped; non-JSON falls back to the bytes
  (D21, Review Focus 6). The `@PG`-line precedent is D22.
- [ ] `two_urls_differing_only_in_parameter_order_are_one_request` (Review Focus 5)
- [ ] `two_responses_that_mean_the_same_thing_hash_the_same`
- [ ] `a_server_timestamp_is_not_part_of_what_a_response_means`
- [ ] `a_body_that_is_not_json_falls_back_to_its_bytes`

### Task 10: The record store and the three modes

- A `records` table keyed on the request hash, holding the body hash, the
  locator, the status, the fetched-at date, and which record supersedes which.
- `Mode::{Record, Replay, Refresh}` and the default.
- [ ] `replay_is_the_default_where_a_record_exists_and_record_where_it_does_not`
- [ ] `replay_mode_with_no_record_is_an_error_before_any_step_runs`
- [ ] `a_body_over_thirty_two_mebibytes_is_refused_saying_pin_this_as_a_release_instead`
- [ ] `a_five_hundred_in_record_mode_writes_no_record` (Review Focus 7)
- [ ] `twenty_paginated_calls_are_twenty_records_keyed_by_request_not_by_sequence`

### Task 11: Refresh, drift, and dated records

- [ ] `a_refresh_writes_a_new_dated_record_beside_the_old_one`
- [ ] `an_earlier_run_is_still_replayable_after_a_refresh`
- [ ] `a_refresh_against_a_changed_response_reports_the_drift_and_names_both_records`
- [ ] `a_refresh_with_no_record_reports_a_first_record_not_drift` (Review Focus 8)

### Task 12: The Python surface, the CLI, and the bundle

- `lg.query`, `--replay` and `--refresh` on `ligase run`, the run report's
  "replayed from record of <date>" wording, the *replayed* and *live* verdicts,
  recorded bodies travelling in the bundle, and the removal of
  `python/ligase/provenance.py`'s "a bundle cannot fetch its own data" — with
  the surrounding sentence rewritten rather than the clause merely deleted.
- [ ] `test_a_recorded_pipeline_runs_again_with_the_server_down_to_an_identical_result`
- [ ] `test_replay_on_a_query_with_no_record_fails_before_any_step_runs`
- [ ] `test_two_paginated_calls_recorded_in_one_order_replay_in_the_other`
- [ ] `test_an_auth_header_leaves_no_trace_in_the_record_or_the_bundle` —
      asserted over the whole record and the whole bundle (decision 8)
- [ ] `test_a_bundle_replays_on_a_clean_machine_with_no_network` — a temp
      directory, a fresh store, no transport configured at all
- [ ] `test_the_methods_paragraph_says_what_was_fetched_and_when`

### Task 13: Docs for M10b

- `web/content/concepts/record-and-replay.mdx`,
  `web/content/guide/recorded-queries.mdx`,
  `web/content/project/architecture/records.mdx`, their `_meta.ts` entries,
  the M10b milestones entry, and `CHANGELOG.md`.
- D39 needs no second write; it was written whole in M10a.

---

## Done means

- The spec's eleven done-when clauses demonstrably met, each by a named test.
- All gates green: `cargo test --workspace`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `pytest`,
  `ruff check`, `ruff format --check`, `mypy`, and `npm run check` in `web/`.
- The CI `demo` job passing on the retrofitted demo, with
  `LIGASE_DEMO_REQUIRED` set, and its cache keyed on the pins.
- D39 written into `decisions.mdx` in the house format; the `source_set`
  settlement recorded with its reason and the arm-pinning obligation handed to
  M12 by name.
- The docs CLAUDE.md requires present and in their `_meta.ts`; `CHANGELOG.md`
  updated; the milestones entry marked done the way M7, M8 and M9 are.
- A pull request against `main`, with no AI attribution in the branch name or
  the body.
