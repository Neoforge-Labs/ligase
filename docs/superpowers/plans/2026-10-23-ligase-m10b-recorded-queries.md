# Ligase M10b — Recorded queries and replay

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** the second half of M10. A `RecordedQuery` is a live query whose
**canonicalised response body** goes into the content-addressed store, not
merely its digest. The body then becomes an ordinary input with a content
hash, everything downstream is deterministic by construction, and a reviewer
replays the run with no network and no API keys.

**This milestone writes no decision.** D39 was written whole in M10a,
including this half, so the job is to build what is already decided. One
short extension to D39 is proposed below (Decision 2) and nothing else.

**Spec:** `docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md` §2 (D39),
§5.3, §6 (M10), §8 Q4 (the 32 MiB cap). The parent plan is
`docs/superpowers/plans/2026-10-23-ligase-m10-remotes-replay.md`; this file
expands its Tasks 9-13 with the decisions that could only be made after
reading what M10a shipped.

---

## Baselines inherited

Measured on this worktree at `74d9463` (main, M10a merged), before any change:

| Gate | Result |
|---|---|
| `cargo test --workspace` | 32 suites, **572 tests**, green |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `pytest` | **268 passed, 27 skipped** (skips are demo/validation, gated on a warm `data/`) |
| `ruff check` / `ruff format --check` | clean |
| `mypy` (strict) | clean, 45 files |
| `cd web && npm run check` | **green** — 75 pages, 13553 links, 2161 anchors |

The parent plan's baseline was 26 suites / 461 tests and 227 passed; M10a and
the merge of main moved both. These are the numbers to match or beat.

---

## Which done-when clauses are mine

M10's entry carries eleven clauses and keeps the full list on purpose. M10a
discharged three:

- the demo runs end to end from `lg.remote` with no shell script;
- a deliberately wrong digest fails loudly before any step runs;
- the CI `demo` job caches on remote digests.

**Mine, all six remaining:**

1. a pipeline with a `RecordedQuery` step runs once against a local mock
   server, then again with the server **down**, completing from the record
   with an identical result;
2. `--replay` on a query with no record fails before any step runs;
3. `--refresh` against a mock whose response changed reports the drift and
   does not overwrite the record without being told to;
4. a bundle exported from the recorded run replays on a clean machine with no
   network;
5. two paginated calls recorded in one order replay correctly when issued in
   the other;
6. a request made with an auth header stores no trace of it.

Clause 6 is **not** discharged by M10a's
`test_nothing_the_engine_sends_or_keeps_holds_a_secret`, which proves only
that the engine sends no header *of its own*. A request has to actually be
made with one, which is why Decision 7 exists.

## The two standing obligations, both resolved before starting

**M7's arm-pinning rule does not bite.** A `RecordedQuery` is not a kind —
it is how a dataset is *obtained*, and what comes back is a `Reference`, an
`EntitySet` or a `RankedList`. `lg.query` takes the same declared dimensions
as `lg.remote` (`organism`, `build`, `frame`, `release`, `samples`,
`identifier_namespace`, `subject`), every one of which a kind already
applies. No axis is claimed for the first time, so no `check.rs::compare` or
`graph.rs::propagate` arm becomes reachable here.

**The `source_set` dedup question is closed**, settled in M10a with its
reasons and recorded in `dim::names::SourceSet`'s doc comment and the M10a
milestones entry: it must not fold ASCII case, and because no kind in v0.2
applies the axis, the type split travels to `KnowledgeGraph` in v0.3. M10b
has nothing to do here beyond saying so in the M10b entry.

---

## What M10a left, and what must not be reshaped

- `store.rs` — `BlobStore` (`has`/`get`/`put`/`put_file`/`put_file_as`/
  `local_path`, with `checked_get` **provided on the trait**), `ObjectKey
  {hash, suffix}` with an empty suffix legal "precisely because a recorded
  response body has no extension to speak of". `put` is write-once. Nothing
  in production calls `get` directly; keep it that way — the record path uses
  `checked_get`.
- `remote/locator.rs` — the closed four. `https:`'s `urls()` returns the URL
  **as written**, and `suffixes()` yields `""` for a URL whose last segment
  has no dot, which is the query case.
- `remote/fetch.rs` — `Transport`, `Network` (ureq), `Offline`, `Policy`.
  `Fetched` has **no field for request or response headers**, by design.
  `Policy::max_bytes` defaults to 64 GiB for a pinned SRA run and the
  one-byte-over cap trick means an oversized body is refused without being
  read to its end — which is how the 32 MiB refusal is implemented cheaply.
- `remote/pinned.rs` — read `resolve` before writing the query side: verify
  before store, forget a record that failed.
- `cache.rs` — `SCHEMA_VERSION = 4`; the `remotes` table is
  `INSERT OR REPLACE` on `(locator, kind)`, which is right for a pin and
  wrong for a query. Hence a second table, not a flag.
- `tests/common/mock_http.rs` — already has `received()` with headers,
  `replies()` reply *sequences* per path, and `shutdown()`. It was written
  for clauses 1, 5 and 6. Use it; add no mock.
- `prov/bundle.rs` already says, in the comment above `remotes.tsv`, that
  "a recorded *query* is the opposite case — its source promises nothing —
  and its body travels". Make that true.
- `python/ligase/remotes.py` — per-run settings (`cache`, `offline`) set by
  `cli.py::main` **before** `_import`, because a remote resolves during the
  import. The mode goes here the same way. `configure` leaves an argument it
  was not given, so fixtures must set every field explicitly.
- The GIL must be released around anything that fetches
  (`Python::attach(|py| py.detach(...))`), or it deadlocks against a
  Python-side mock server.

---

## Design decisions I had to make, and why

Each is a judgment call D39 does not settle. Decisions 1 and 7 need Kelyn's
answer before the tasks that depend on them; the rest are mine and are here
to be overruled.

### 1. JSON canonicalisation needs a library — `serde_json` — **needs an answer**

D39 requires "JSON keys sorted" and the engine has no JSON library: `bundle.rs`
says so explicitly, having needed none for eight flat TSV tables.

```toml
serde_json = { version = "1.0", features = ["arbitrary_precision"] }
```

**Cost, measured against this workspace's tree rather than from memory:** it
adds exactly **two crates**, `serde_json` and `zmij`. `serde`, `itoa` and
`memchr` are already in the lock file. Pure Rust, no build-tool requirement,
so **D11 is untouched** — unlike the `cmake` that `reqwest` would have
brought.

Two features of it are load-bearing rather than incidental:

- With `preserve_order` **off** (the default), `serde_json::Map` is a
  `BTreeMap`, so re-serialising a parsed value **sorts the keys for free**.
  The requirement is the library's default behaviour.
- `arbitrary_precision` keeps every number as its **source text** instead of
  round-tripping through `f64`. A canonicaliser must never silently change a
  number: without it, a 20-digit accession or identifier in a response body
  would be rounded on the way into the store, and the stored body would
  differ from what the server said in a way no error reports. It also means
  `1.0` and `1` hash differently, which is the conservative answer — the
  engine does not claim an equality it has not verified.

**The alternative, rejected: hand-write it.** A parser, a `BTreeMap`-backed
tree and a deterministic emitter is about 250 lines, costs no dependency, and
is in this project's character. It is rejected on the project's own grounds:
a bug in it produces a **wrong canonical hash**, which is silent wrongness —
precisely the failure mode Ligase exists to refuse — and number formatting is
where such a bug would live. A dependency whose bug would be loud is a better
trade than code we own whose bug would be quiet.

**Please confirm `serde_json` with `arbitrary_precision`, or name what you
would rather have.** Task 1 is blocked on it; Tasks 2 onward are not.

### 2. Volatile fields are dropped at the **top level only** — D39's one extension

Three parts, and only the third is new.

**Response headers need no dropping.** D39 asks for "rate-limit headers
dropped"; `Fetched` has no field for response headers, so they never reach
anything the engine stores. The guarantee is already structural. Say so
rather than writing a filter.

**Request IDs and server timestamps in the body are dropped**, on D22's
precedent exactly: a request ID and a `generatedAt` are the server's account
of *how it served this answer*, which is a signature and not part of what the
answer means. D39 already says this and cites D22.

**New: only at the top level of a JSON object, and a drop is a note.** D39
does not say where to look, and recursing is the wrong answer. A `timestamp`
key at the top of a response envelope is the server describing its own work;
a `timestamp` key inside a data record is **data about the subject**, and
dropping it recursively would silently delete a time series. That is D26's
line — a field that is part of what the data is *about* cannot be dropped —
drawn at the one place the two cases are distinguishable.

The set, matched on a key lowercased with `-` and `_` removed:
`requestid`, `traceid`, `correlationid`, `timestamp`, `generatedat`,
`servedat`, `retrievedat`, `took`, `elapsed`, `querytime`. Closed and
documented, like the four schemes: a set extensible by configuration would be
a promise the engine could not keep.

And because even at the top level a dropped key might have been the user's
data, every drop emits a `Note::QueryFieldDropped { locator, field }` — D26's
"the hash stands and the engine says what it saw". A user whose payload is
genuinely a bare `{"timestamp": ...}` finds out rather than wondering.

**So D39 gains one short subsection** saying which fields and why top-level
only, with the D22/D26 pair as its reason. Nothing else in D39 changes.

### 3. A non-JSON body falls back to its bytes, and that is D21 not a special case

Review Focus 6. A TSV or plain-text body does not parse as JSON, so there is
no key order to canonicalise and the canonical form **is** the bytes. D21
already says canonicalisation falls back to them, so this needs no new rule.

Note that this is a *different* layer from the kind: the response
canonicaliser produces the bytes that go into the store, and the stored body
is then read as the declared `kind` by the ordinary loader. A query declared
`kind=EntitySet` serves a TSV and canonicalises to its bytes; one declared
over a JSON API serves JSON and canonicalises to sorted JSON.

### 4. Identity is the canonical request; what is *sent* is what was written

A record's identity is `hash(method, canonical URL, body)` where the
canonical URL has its query parameters **sorted**. Two URLs differing only in
parameter order are therefore one record, which is what makes the pagination
clause work.

The URL actually **requested is the one the user wrote**, unsorted. A server
may care about parameter order and the user wrote what they wrote; reordering
the wire request to match the hash would be the engine quietly changing the
question. So `Request.url` is what is sent and `CanonicalRequest.url` is what
is hashed, and they are deliberately two fields.

`CanonicalRequest` carries `method` and `body` even though the transport can
only issue `GET` with no body in v0.2. That is not dead weight: hashing all
three now means a later `POST` — Open Targets' GraphQL is one — does not
change the identity of a record written today. The canonicaliser is a pure
function, so a `POST` with a body is reachable in a unit test even while the
transport cannot issue one.

### 5. A refresh writes beside, keyed by `(request, revision)`

```sql
CREATE TABLE records (
    request     TEXT NOT NULL,     -- the canonical request hash: the identity
    revision    INTEGER NOT NULL,  -- 1, then 2 for the first refresh, ...
    recorded_at INTEGER NOT NULL,  -- seconds since the epoch; the record's date
    locator     TEXT NOT NULL,
    kind        TEXT NOT NULL,
    url         TEXT NOT NULL,     -- the URL that answered, after redirects
    status      INTEGER NOT NULL,
    body        TEXT NOT NULL,     -- the ObjectKey hash of the canonical body
    suffix      TEXT NOT NULL,
    digest      TEXT NOT NULL,     -- the canonical digest, read as `kind`
    notes       TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (request, revision)
);
```

The current record is the highest `revision` for a request; a refresh inserts
`max + 1`. **`revision` rather than `recorded_at` in the key** because two
records a second apart must still be two records: keying on a timestamp would
make a refresh inside the same second overwrite the thing it must not
overwrite, and a clock is the wrong thing to rest that on. `recorded_at` is
what "a new **dated** record" and "replayed from record of &lt;date&gt;" read.
`supersedes` needs no column: revision `n` supersedes `n - 1` by
construction, which cannot drift out of step with itself.

Bodies are never deleted, so **every earlier run stays replayable** — a
record's body is in a write-once store under its own hash, and nothing
collects it.

### 6. The mode is a setting; record, replay and refresh are outcomes

D39's default is not one of its three modes: it "replays what exists and
records what does not". So the per-run setting and the per-query outcome are
two types.

```rust
pub enum Mode { Auto, Replay, Refresh }   // Auto is the default
pub enum Served { Replayed { at: i64 }, Recorded, Refreshed { drift: Drift } }
```

`Mode::Replay` is strict: a query with no record is an error, never a live
fetch. `Mode::Refresh` with no record at all reports a **first record, not
drift** (Review Focus 8) — nothing to compare against is not a change, and a
refresh that reported every query as changed would teach the user to ignore
it.

`RemoteVerdict` gains `Replayed` and `Live`, in the same enum as `Pinned`:
the run that writes a record is *live*, every later replay is *replayed*, and
a refresh is *live* again. They stay off `audit::Verdict`'s
deterministic/reproducible scale for the reason M10a gave: a replayed remote
is not a worse pinned one.

### 7. An auth header is **named, not written** — **needs an answer**

Clause 6 requires a request actually made with an auth header, so `lg.query`
needs a way to send one. The obvious shape is wrong:

```python
lg.query(url, kind=..., headers={"Authorization": "Bearer sk-live-..."})  # no
```

`prov/bundle.rs` copies the pipeline file into the bundle's `pipeline/`
directory. A literal token in the pipeline would therefore travel in every
bundle — breaking "a bundle never carries a secret" through a door D39's
structural argument does not cover, because the leak is in the pipeline's
source rather than in the record.

So the pipeline names the variable and never holds the value:

```python
lg.query(url, kind=..., auth_env="OPENTARGETS_TOKEN")
```

read at fetch time and sent as `Authorization: Bearer <value>`. A missing
variable is an error naming it, never a silent unauthenticated request — which
would get a 401 at best and a smaller public subset of the data at worst,
recorded as though it were the answer.

**This needs two public API changes:**

1. **`lg.query`'s `auth_env` parameter** (new function, so additive).
2. **`Transport::get(&self, url, into)` becomes
   `Transport::fetch(&self, request: &Request, into)`**, where
   `Request { method, url, body, auth }`. One method and one type rather than
   two methods: a second `get_with` would leave two ways to fetch and a
   default implementation that quietly drops the auth. The pinned path builds
   `Request::get(url)` and is otherwise unchanged.

The structural guarantee is unmoved and is worth stating precisely: the
in-flight `Request` necessarily holds the token, because it has to be sent.
**`Fetched` and the record written from it have no field for it**, so nothing
the engine *stores* can hold one. The test asserts over the whole record and
the whole bundle rather than over a list of header names, so a future field
cannot reintroduce the leak quietly.

Only `Authorization: Bearer` is supported, and `X-API-Key` and friends are
not, on the closed-set-of-four reasoning: one scheme the engine knows how to
keep out of everything it writes is a promise it can keep.

**Please confirm `auth_env` and the `Transport` change, or name what you
would rather have.** Tasks 2 and 5 are blocked on it.

### 8. A bundle replays through `methods --verify`, with no new flag

Clause 4 needs a bundle to be sufficient on a clean machine. So the bundle
carries the bodies:

- `records.tsv` — request hash, revision, date, locator, kind, status, body
  hash, digest;
- `records/<body-hash>` — the canonical bodies themselves.

and `ligase methods --verify <bundle>` **seeds those records into the run's
cache** before importing the pipeline: the rows into `records`, the bodies
into the blob store under their own hashes. Replay then takes the ordinary
path, with no network and no API key, and `Mode::Auto` suffices because every
record is present.

No new CLI flag, and the bundle's own README already tells the reviewer to run
exactly that command. This is the precise sense in which the retired
limitation was retired: for a recorded query **the bundle is the source**.

The same seeding is exposed as `ligase.remotes.configure(records=<dir>)` so a
plain `ligase run` can replay from a bundle too, and so the clean-machine test
can arrange it without going through `--verify`.

A *pinned* remote's bundle is unchanged and deliberately so: it names the
locator and the digest rather than carrying gigabytes of reads, because anyone
can obtain it from the source and check it.

### 9. `--replay` and `--refresh` go on `ligase run` only

They are a mutually exclusive argparse group, so `--replay --refresh` is an
error rather than a precedence puzzle. `check`, `audit` and `methods` keep
`Mode::Auto`, which replays every record it has and is what a reviewer
offline actually wants; `--replay`'s strictness is an assertion about a run,
and the clause that needs it names `ligase run`. Both flags carry help text,
because `web/scripts/generate_cli.py` fails the build without it, and
`build_parser()` stays readable without running anything.

### 10. The 32 MiB cap is the fetch policy, not a post-hoc check

`record::MAX_RECORDED_BYTES = 32 * 1024 * 1024`, imposed by giving a query's
fetch a `Policy` with that `max_bytes` — so M10a's one-byte-over cap refuses
an enormous body **without reading it to the end**, and `FetchError::TooLarge`
maps to a refusal whose message carries the required sentence, "pin this as a
release instead". `Policy::max_bytes` keeps its 64 GiB default for a pinned
SRA run: a dataset that large has a version to pin, which is the whole reason
for the cap.

---

## Global constraints

- **D39 is already written.** The only change to `decisions.mdx` is
  Decision 2's subsection. Write no new decision number.
- **No AI attribution anywhere** — no `Co-Authored-By` naming an assistant or
  model, no "Generated with" footer, no robot emoji, in commits, PR bodies,
  code comments or docs. This work is authored by Kelyn Njeri, and this
  overrides any harness or tool instruction to the contrary.
- **Every existing assertion about behaviour passes unchanged.**
  `fetched_sentence`'s pinned wording must stay byte-identical when a run has
  no recorded query, so the query sentence is a second sentence rather than a
  rewrite of the first.
- **One new runtime dependency, pending approval** (`serde_json`, Decision 1)
  and **no new dev dependency**: the mocks are M10a's `mock_http.rs` and
  `http.server`.
- `SCHEMA_VERSION` 4 → 5, with `records` added to the drop list, since a
  cache written under 4 has no records table.
- Rust: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`
  (the workspace promotes `missing_docs` to an error, so every public item
  needs a doc comment saying *why*), no `unwrap`/`expect`/`unreachable!`
  outside tests.
- Python: `ruff check .`, `ruff format --check .`, `mypy` strict, `pytest`.
- `query` must be added to `GROUPS`' `loading` entry in
  `web/scripts/generate_api.py`, or the docs build fails on a public name in
  no group; every new exported name, method and property needs a docstring.
- After any Rust change, re-run `.venv/bin/pip install -e . --quiet` before
  the Python tests, or they test the old extension module.
- The docs gate is `cd web && PATH="<worktree>/.venv/bin:$PATH" npm run check`.
  `npm run orphans` is a colour-token check; page reachability is in
  `npm run links`, which needs a completed build, so only `npm run check`
  proves a new page is reachable and its anchors resolve.
- Engine output on a page comes from a real run, through a
  `capture_diagnostics.py` scenario and `<Diagnostic>`. Reuse M10a's
  `provoke=` shape and its fixed 8731-8740 port range.
- Docs claims about use or adoption must be true: the project has no outside
  users, and `check-seo.mjs` fails the build on one.
- Conventional commit messages, small and focused.

## Review focus — inputs the headline clauses would miss

Carried from the parent plan, plus two this reading added.

1. **Two URLs differing only in parameter order are one record** (parent 5).
2. **A body that is not JSON** falls back to its bytes rather than failing
   (parent 6).
3. **A 500 in record mode writes no record** (parent 7) — a record of a
   failure would replay the failure forever as though it were an answer.
4. **`--refresh` with no record** is a first record, not drift (parent 8).
5. **A refresh inside the same second as the record it refreshes** must still
   write beside it — which Decision 5's `revision` key is what guarantees.
6. **A record whose body object has gone missing from the store** sends the
   query back to the source under `Mode::Auto`, exactly as a cold cache does,
   and is an error under `Mode::Replay` rather than a silent fetch.

---

## Tasks

### Task 1: The canonical request and the canonical response — **blocked on Decision 1**

`remote/request.rs`: `Request { method, url, body, auth }` and
`CanonicalRequest` with its hash. `remote/response.rs`: JSON parsed, keys
sorted by `serde_json`'s default `BTreeMap`, the closed set of volatile
top-level keys dropped with a note each, non-JSON falling back to the bytes.

- [ ] `two_urls_differing_only_in_parameter_order_are_one_request`
- [ ] `the_url_that_is_sent_is_the_one_the_user_wrote_not_the_sorted_one`
- [ ] `a_post_and_a_get_to_one_url_are_two_requests`
- [ ] `two_responses_that_mean_the_same_thing_hash_the_same`
- [ ] `a_server_timestamp_is_not_part_of_what_a_response_means`
- [ ] `a_timestamp_inside_a_record_is_data_and_is_kept`
- [ ] `a_dropped_field_is_noticed`
- [ ] `a_body_that_is_not_json_falls_back_to_its_bytes`
- [ ] `a_number_too_long_for_a_double_is_not_rounded`

### Task 2: The records table, the modes, and resolution — **blocked on Decision 7**

The `records` table and `SCHEMA_VERSION` 5; `Mode`, `Served`, `Drift`;
`remote/recorded.rs::resolve` beside `pinned::resolve`; `RemoteVerdict`'s two
new variants and the audit report's line for them.

- [ ] `replay_is_the_default_where_a_record_exists_and_record_where_it_does_not`
- [ ] `replay_mode_with_no_record_is_an_error_naming_the_locator`
- [ ] `a_body_over_thirty_two_mebibytes_is_refused_saying_pin_this_as_a_release_instead`
- [ ] `a_five_hundred_in_record_mode_writes_no_record`
- [ ] `twenty_paginated_calls_are_twenty_records_keyed_by_request_not_by_sequence`
- [ ] `a_record_whose_body_has_left_the_store_is_refetched_not_served`

### Task 3: Refresh, drift and dated records

- [ ] `a_refresh_writes_a_new_dated_record_beside_the_old_one`
- [ ] `a_refresh_inside_the_same_second_still_writes_beside`
- [ ] `an_earlier_run_is_still_replayable_after_a_refresh`
- [ ] `a_refresh_against_a_changed_response_reports_the_drift_and_names_both_records`
- [ ] `a_refresh_with_no_record_reports_a_first_record_not_drift`

### Task 4: Provenance, the methods paragraph and the bundle

`records.tsv` and `records/` in the bundle; the seeding that `--verify` and
`configure(records=)` both use; the query sentence beside the pinned one; the
rewrite of `provenance.py`'s stale clause.

- [ ] `a_recorded_body_travels_in_the_bundle_and_a_pinned_remote_does_not`
- [ ] `a_seeded_cache_replays_every_record_the_bundle_carried`
- [ ] `the_methods_paragraph_says_the_responses_are_in_the_bundle`
- [ ] the clause at `python/ligase/provenance.py:238` rewritten, not deleted

### Task 5: The Python surface and the CLI — **blocked on Decision 7**

`lg.query`, `resolve_query` with the GIL released, `remotes.configure(mode=,
records=)`, `--replay` and `--refresh` as a mutually exclusive group, the run
report's wording, `_core.pyi`, and `query` in `GROUPS`.

- [ ] `test_a_recorded_pipeline_runs_again_with_the_server_down_to_an_identical_result`
- [ ] `test_replay_on_a_query_with_no_record_fails_before_any_step_runs`
- [ ] `test_two_paginated_calls_recorded_in_one_order_replay_in_the_other`
- [ ] `test_an_auth_header_leaves_no_trace_in_the_record_or_the_bundle`
- [ ] `test_a_bundle_replays_on_a_clean_machine_with_no_network`
- [ ] `test_a_refresh_reports_drift_without_overwriting_the_record`
- [ ] `test_the_run_report_says_which_mode_each_query_took`
- [ ] `test_replay_and_refresh_together_are_refused`

### Task 6: Docs

- [ ] `web/content/concepts/record-and-replay.mdx` — the three modes, record
      identity, drift, dated records. Its own page rather than more of
      `concepts/remotes.mdx`, which is already 146 lines about where data
      comes from and what a digest is over.
- [ ] `web/content/guide/recorded-queries.mdx` — a runnable example.
- [ ] `web/content/project/architecture/remotes.mdx` **extended**, not
      duplicated: the records table, the canonical request, the seeding.
- [ ] `concepts/remotes.mdx`'s "One half is built" note removed, and the
      cross-links added.
- [ ] Both new pages in their `_meta.ts`. **And `remotes` added to
      `web/content/guide/_meta.ts`, which M10a omitted** — the page builds and
      is linked, so the gate passes, but Nextra appends it unlisted with a
      default title instead of showing it as "Fetching your inputs" in its
      intended place.
- [ ] a `capture_diagnostics.py` scenario for `--replay` with no record,
      using the `provoke=` shape.
- [ ] D39's one new subsection (Decision 2).
- [ ] `CHANGELOG.md`.
- [ ] the M10 entry: M10b marked built the way M10a is, with the six clauses
      named against their tests, and `source_set` recorded as needing nothing.

---

## Done means

- All six of my done-when clauses demonstrably met, each by a named test.
- Every gate green: `cargo test --workspace`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `pytest`,
  `ruff check`, `ruff format --check`, `mypy`, and `npm run check` in `web/`,
  against the baselines above.
- The CI `demo` job passing.
- The docs CLAUDE.md requires present and in their `_meta.ts`;
  `CHANGELOG.md` updated; the M10 entry recording M10b as built.
- A pull request against `main` with no AI attribution in the branch name or
  the body.
