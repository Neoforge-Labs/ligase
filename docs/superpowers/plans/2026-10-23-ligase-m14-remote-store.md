# M14 — Remote store: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans
> to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A second tier behind M10a's `BlobStore` trait — a local store that
consults a remote on a miss and writes through on a put — so a lab points
every machine at one bucket and everybody else's `ligase run` hits the cache.

**Architecture:** `store.rs` becomes a `store/` module: the trait and
`ObjectKey` in `mod.rs`, `FsStore` in `fs.rs`, a `TieredStore` in `tiered.rs`
that holds a local `FsStore` and an optional remote, and a feature-gated
`object_store` backend in `remote.rs`. The cache above it changes in one
place — `Cache::objects()` returns the tiered store — plus one new idea: the
rows that make a cache hit possible are **published into the store as
objects**, so a machine with an empty SQLite can learn them back. Nothing
read from the remote is believed: an object is hashed on arrival, and a
published row must restate the key it was found under.

**Tech Stack:** Rust (stable, edition 2024), `object_store` 0.14 behind the
`remote-store` cargo feature, Python 3.11+ (`tomllib` for `ligase.toml`),
MinIO in CI.

**Spec:** `docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md` — §2
(D43), §5.7, §6 (M14), §7 (out of scope), §8 question 5 (no eviction).

## Global Constraints

- Stable Rust; `cargo fmt --all --check`; `cargo clippy --workspace
  --all-targets -- -D warnings` **with the feature on and off**; workspace
  lints make `missing_docs` an error and deny `unwrap`/`expect` outside
  tests.
- Python: `ruff check`, `ruff format --check`, `mypy --strict`, `pytest`.
- **No AI attribution anywhere** — commits, PR body, comments, docs.
- Credentials come from each provider's standard chain. **There is no way to
  put a secret in `ligase.toml`**, and no Ligase-owned credential code.
- Objects are written once and never overwritten. No eviction, no garbage
  collection, no bucket management beyond reading and writing objects
  (§7, §8 q5).
- The default feature set must not pull `object_store`, proven by a test
  rather than asserted in prose.
- Docs: every feature ships its `concepts/`, `guide/` and
  `project/architecture/` pages, each in its directory's `_meta.ts`, plus
  `CHANGELOG.md`. Engine output on a page comes from a real run. No claim
  about use or adoption: the project has no outside users.

## Review Focus

Five ways a two-tier store is *slowly* wrong, which the done-when clauses do
not name. Each gets its test in the task that owns the code.

1. **An unreachable remote silently becomes a cold cache.** A lab whose
   bucket is behind a dropped VPN recomputes for an hour and is told
   nothing. → Task 3: a remote whose every call fails is a miss, not a
   failure, **and the run report says so**.
2. **A corrupt remote object poisons the local tier.** Promotion into the
   trusted local tier must verify *before* it installs, or the mismatch is
   detected once and then never again. → Task 3.
3. **A `has()` that is not a `get()`.** A cache decision taken on `has`
   alone can be a hit that cannot be served. → Task 3: the hit path
   materialises and verifies; `has` is never the last word.
4. **A thin bundle that quietly drops the no-network replay guarantee**
   M10b bought. → Task 7: thin says so in its README and manifest, and
   seeding one without store access fails before any step runs, naming the
   store.
5. **A cheap number read as an exact one.** "How many objects are local
   only" is a diff between two stores. → Task 6: the count is exact below a
   bound and sampled above it, and the output says which.

---

## File structure

| File | Responsibility |
|---|---|
| `crates/ligase-core/src/store/mod.rs` | `ObjectKey`, `StoreError`, `BlobStore`, `suffix_of`, `Backend` parsing of a store URL |
| `crates/ligase-core/src/store/fs.rs` | `FsStore`, moved verbatim |
| `crates/ligase-core/src/store/tiered.rs` | `TieredStore`, promotion, write-through, `StoreStats` |
| `crates/ligase-core/src/store/published.rs` | rows published into the store as objects, and their verification |
| `crates/ligase-core/src/store/remote.rs` | `#[cfg(feature = "remote-store")]` `object_store` backend |
| `crates/ligase-core/src/cache.rs` | `objects()` returns `&TieredStore`; `lookup`/`record`/`remote`/`record_remote` publish and read back |
| `crates/ligase-core/src/exec.rs` | the store's line in the run report |
| `crates/ligase-core/src/prov/bundle.rs` | `--thin`: `store.tsv`, a README that says what is no longer promised, and `seed` that fetches bodies from the store |
| `python/ligase/cli.py` | `ligase store status\|push\|pull`, `--thin` |
| `python/ligase/config.py` *(new)* | `ligase.toml`'s `store` key, with `LIGASE_STORE` winning |
| `.github/workflows/ci.yml` | the `minio` job |

---

### Task 1: `store.rs` becomes `store/`

Mechanical. No behaviour changes, so the existing `tests/store.rs` is the
test.

**Files:**
- Create: `crates/ligase-core/src/store/mod.rs`, `crates/ligase-core/src/store/fs.rs`
- Delete: `crates/ligase-core/src/store.rs`

**Interfaces:**
- Produces: every name `store.rs` exports today, at the same path
  (`ligase::store::{BlobStore, FsStore, ObjectKey, StoreError, suffix_of}`).

- [ ] **Step 1: Move `FsStore` and its free functions to `store/fs.rs`; keep
      `ObjectKey`, `StoreError`, `BlobStore`, `suffix_of` and the module
      documentation in `store/mod.rs`, re-exporting `FsStore`.**
- [ ] **Step 2: `cargo test --workspace`** — Expected: PASS, nothing changed.
- [ ] **Step 3: `cargo clippy --workspace --all-targets -- -D warnings`** — Expected: clean.
- [ ] **Step 4: Commit** — `refactor(store): split the blob tier into a module`

---

### Task 2: a store URL names a backend

`LIGASE_STORE` is parsed before anything connects, so an unusable value is
an error with a list rather than a failure at the first miss. `file://` is
served by `FsStore` and needs no feature, which is what lets every tiering
test below run in the default build.

**Files:**
- Modify: `crates/ligase-core/src/store/mod.rs`
- Test: `crates/ligase-core/tests/store.rs`

**Interfaces:**
- Produces:
  ```rust
  pub enum Backend { Local(PathBuf), Remote { url: String } }
  impl Backend { pub fn parse(url: &str) -> Result<Self, StoreError> }
  pub fn configured() -> Option<String>            // reads LIGASE_STORE
  pub const SCHEMES: [&str; 4] = ["file", "s3", "gs", "az"];
  ```
  and `StoreError::Unsupported { url: String, reason: String }`.

- [ ] **Step 1: Write the failing tests** in `crates/ligase-core/tests/store.rs`:

```rust
#[test]
fn a_file_url_is_a_local_directory() { /* Backend::parse("file:///tmp/x") == Local("/tmp/x") */ }

#[test]
fn an_unknown_scheme_names_the_four_that_work() {
    let error = Backend::parse("ftp://host/bucket").unwrap_err().to_string();
    assert!(error.contains("ftp"), "{error}");
    for scheme in ["file", "s3", "gs", "az"] {
        assert!(error.contains(scheme), "{error}");
    }
}

#[test]
fn a_bare_path_is_not_a_store_url() {
    // "/tmp/x" and "s3:/bucket" are both refused: a scheme is required, so a
    // typo cannot quietly become a directory in the current working tree.
}
```

- [ ] **Step 2: Run them and watch them fail** — `cargo test -p ligase --test store` — Expected: FAIL, `Backend` not found.
- [ ] **Step 3: Implement `Backend`, `configured` and `SCHEMES`** in
      `store/mod.rs`. `s3`/`gs`/`az` parse in every build and only *open*
      needs the feature, so a default build's error says "this build has no
      remote-store support" rather than "unknown scheme" — a user with the
      wrong wheel must not be told their URL is wrong.
- [ ] **Step 4: `cargo test -p ligase --test store`** — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(store): parse the store URL before anything connects`

---

### Task 3: `TieredStore`

The heart. Everything here is tested with a second `FsStore` standing in for
the remote, which exercises the same tiering code the object store will run
under and needs no container.

**Files:**
- Create: `crates/ligase-core/src/store/tiered.rs`
- Create: `crates/ligase-core/tests/tiered.rs`

**Interfaces:**
- Consumes: Task 2's `Backend`.
- Produces:
  ```rust
  pub struct TieredStore { /* local: FsStore, remote: Option<Box<dyn BlobStore + Send + Sync>>, … */ }
  impl TieredStore {
      pub fn open(local: &Path, remote: Option<&str>) -> Result<Self, StoreError>;
      pub fn local(&self) -> &FsStore;
      pub fn remote_url(&self) -> Option<&str>;
      pub fn stats(&self) -> StoreStats;
      pub fn path_of(&self, key: &ObjectKey) -> PathBuf;   // as FsStore's
  }
  impl BlobStore for TieredStore { … }

  #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
  pub struct StoreStats {
      pub fetched: u64,      // objects promoted from the remote
      pub pushed: u64,       // objects written through to it
      pub unreachable: u64,  // calls that failed
      pub rejected: u64,     // objects refused for a hash mismatch
  }
  ```
  The remote backend is **private to the module**: nothing outside can hold
  one, so nothing outside can read one without verification.

- [ ] **Step 1: Write the failing tests** in `crates/ligase-core/tests/tiered.rs`:

```rust
#[test] fn a_local_hit_never_touches_the_remote() {}
#[test] fn a_local_miss_is_served_from_the_remote_and_promoted() {
    // after get(), the object is in the LOCAL tier and stats().fetched == 1
}
#[test] fn a_put_is_written_through_to_the_remote() {}
#[test] fn a_corrupt_remote_object_is_refused_and_does_not_poison_the_local_tier() {
    // corrupt in place in the remote directory, then:
    //   get() -> Err(StoreError::Mismatch) naming both hashes
    //   local tier still does not hold the key
    //   stats().rejected == 1
}
#[test] fn local_path_promotes_and_verifies_before_it_installs() {
    // same corruption, through local_path rather than get
}
#[test] fn an_unreachable_remote_is_a_miss_and_is_counted() {
    // remote pointed at a path that cannot be read:
    //   has() -> Ok(false), get() -> Ok(None), stats().unreachable > 0
}
#[test] fn an_unreachable_remote_does_not_fail_a_put() {
    // the object is local, the run is correct, stats().unreachable > 0
}
#[test] fn two_threads_putting_identical_bytes_both_succeed() {
    // 8 threads, same key and bytes, through one shared remote: every put is
    // Ok and the object reads back correct. Content addressing makes the race
    // benign; this is the proof rather than the assumption.
}
#[test] fn a_store_with_no_remote_behaves_exactly_as_the_local_one_did() {}
```

- [ ] **Step 2: Run them and watch them fail** — `cargo test -p ligase --test tiered` — Expected: FAIL, `TieredStore` not found.
- [ ] **Step 3: Implement `TieredStore`.** The rules, all four of them:
  - `get`: local, else `remote.checked_get` — **the verified read, never
    plain `get`** — then `local.put` the verified bytes and count `fetched`.
  - `local_path`: local, else promote exactly as `get` does, then the local
    path.
  - `put` / `put_file` / `put_file_as`: local first (so the run is correct
    whatever the network does), then push and count `pushed`.
  - Any remote error that is **not** `Mismatch` counts `unreachable` and is
    swallowed into a miss. A `Mismatch` counts `rejected` and is returned,
    because an object that is not what its name says is a finding, not a
    miss. The caller turns it into a rerun.
- [ ] **Step 4: `cargo test -p ligase --test tiered`** — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(store): a local tier that consults a remote and writes through`

---

### Task 4: rows published into the store

A blob tier alone cannot satisfy "a second checkout completes the demo
entirely from cache": a clean machine computes the right step key and has
nowhere to learn what it produced. **The claim in D43 that the index derives
from content hashes is false as written** — `steps.key` is a hash of
ingredients that no object carries, and `remotes` is keyed by a locator. So
the rows that make a hit possible are published into the store as objects
named for their key, and read back on an index miss.

This is the one object class whose name is not the hash of its contents, and
it is verified differently: the body restates the key, and a body that does
not restate the key it was found under is refused. What it cannot verify is
the *claim* — that key K produced output O — which is why the trust boundary
is documented rather than hidden. The output itself is still verified byte
for byte.

**Files:**
- Create: `crates/ligase-core/src/store/published.rs`
- Modify: `crates/ligase-core/src/cache.rs`
- Create: `crates/ligase-core/tests/published.rs`

**Interfaces:**
- Consumes: Task 3's `TieredStore`.
- Produces:
  ```rust
  pub struct Row { pub table: &'static str, pub key: ContentHash, pub fields: Vec<String> }
  pub fn publish(store: &dyn BlobStore, row: &Row) -> Result<(), StoreError>;
  pub fn fetch(store: &dyn BlobStore, table: &'static str, key: ContentHash)
      -> Result<Option<Vec<String>>, StoreError>;
  pub const STEPS: &str = "steps";
  pub const REMOTES: &str = "remotes";
  ```
  Body is the same escaped TSV a bundle writes; first two fields are the
  table and the key, and `fetch` refuses a body whose first two fields are
  not the ones asked for.

- [ ] **Step 1: Write the failing tests** in `crates/ligase-core/tests/published.rs`:

```rust
#[test] fn a_published_row_reads_back() {}
#[test] fn a_row_that_restates_another_key_is_refused() {
    // write a body under key A whose own key field says B: fetch(A) is an error
    // naming both, not a row
}
#[test] fn a_row_published_under_another_table_is_not_found() {}
```
and in `crates/ligase-core/tests/execution.rs` (the clause itself):
```rust
#[test] fn deleting_the_index_rebuilds_it_from_the_store() {
    // run a step; delete cache.sqlite; reopen; lookup(key) returns the Entry,
    // the output is where it was, and the step is NOT rerun
}
#[test] fn an_entry_whose_output_is_only_in_the_remote_is_materialised() {
    // the clean-machine clause in miniature: empty local tier and index,
    // a remote holding both the entry and the object
}
```

- [ ] **Step 2: Run them and watch them fail** — `cargo test -p ligase --test published --test execution`.
- [ ] **Step 3: Implement `published.rs`.** The key is
      `ObjectKey::new(blake3("ligase.published-row" || table || key), ".row")`,
      so a row can never collide with an object: an object's name is the hash
      of its own bytes and a row's is not.
- [ ] **Step 4: Wire it into `Cache`.** `record` publishes a `STEPS` row
      after the SQLite insert; `lookup` on a SQLite miss fetches one, and on a
      hit whose object is absent locally asks the store rather than forgetting
      the entry. `record_remote` and `remote` do the same with `REMOTES`.
      `lookup` writes what it learned back into SQLite, so the rebuild is lazy
      and costs one store read per step per cold machine.
- [ ] **Step 5: `cargo test --workspace`** — Expected: PASS.
- [ ] **Step 6: Commit** — `feat(cache): publish the rows a cache hit needs, and read them back`

---

### Task 5: the `object_store` backend

**Files:**
- Create: `crates/ligase-core/src/store/remote.rs`
- Modify: `Cargo.toml`, `crates/ligase-core/Cargo.toml`, `pyproject.toml`
- Create: `crates/ligase-core/tests/default_build.rs`

**Interfaces:**
- Produces: `TieredStore::open` accepting `s3://`, `gs://` and `az://` when
  the feature is on, and `StoreError::Unsupported` naming the feature when
  it is off. Nothing new is public.

- [ ] **Step 1: Write the failing test** — `crates/ligase-core/tests/default_build.rs`:

```rust
/// D6 and D43: the default install is zero-config, which means it does not
/// acquire a dependency on an object store. A sentence in the docs would not
/// notice a default feature added later; this does.
#[test]
#[cfg_attr(feature = "remote-store", ignore = "the feature is on by request")]
fn the_default_feature_set_pulls_no_object_store() {
    let tree = std::process::Command::new(env!("CARGO"))
        .args(["tree", "--package", "ligase", "--edges", "normal", "--prefix", "none"])
        .output().unwrap();
    let tree = String::from_utf8_lossy(&tree.stdout);
    for unwanted in ["object_store", "aws-lc-sys", "tokio", "reqwest", "hyper"] {
        assert!(!tree.contains(unwanted), "default build pulled {unwanted}:\n{tree}");
    }
}
```

- [ ] **Step 2: Run it** — `cargo test -p ligase --test default_build` — Expected: PASS already, and it must keep passing after step 3.
- [ ] **Step 3: Add the dependency, optional, with the argument in the
      manifest comment** — `object_store = { version = "0.14", optional = true,
      default-features = false, features = ["aws", "gcp", "azure"] }` plus
      `tokio` for a runtime, and `[features] remote-store = ["dep:object_store",
      "dep:tokio"]`. The comment records what was weighed: `aws`/`gcp`/`azure`
      each hard-enable `aws-lc-rs`, whose `aws-lc-sys` has a non-optional
      build-dependency on `cmake`, so **enabling this feature adds a cmake
      requirement**; the `ring` feature is additive and cannot remove it; and
      this is the exception to the architecture note on `ureq`, which says the
      executor never touches an async runtime.
- [ ] **Step 4: Implement the backend.** One `tokio` multi-thread runtime
      owned by the backend, entered with `block_on` from rayon workers —
      multi-thread rather than current-thread because several workers call it
      at once. `has` is `head`, `get` is `get`, `put` is `put` for a small
      object and `put_multipart` above 16 MiB so a multi-gigabyte BAM is not
      read into memory. A `404` is a miss; every other error is an error, and
      `TieredStore` is what decides a non-`Mismatch` error is a miss.
- [ ] **Step 5: `cargo clippy --workspace --all-targets --features remote-store -- -D warnings`**
      and again with no features — Expected: clean both ways.
- [ ] **Step 6: `cargo test -p ligase --test default_build`** — Expected: PASS.
- [ ] **Step 7: Commit** — `feat(store): an object_store backend behind the remote-store feature`

---

### Task 6: `ligase store`, and the config

**Files:**
- Create: `python/ligase/config.py`
- Modify: `python/ligase/cli.py`, `crates/ligase-py/src/lib.rs`
- Test: `tests/test_store_cli.py`

**Interfaces:**
- Produces: `ligase store status|push|pull`, and
  `ligase.config.store_url(start: Path) -> tuple[str, str] | None` returning
  the URL and where it came from (`"LIGASE_STORE"` or the `ligase.toml`
  path), environment winning.
- A `_core.store_status(cache, url)` returning configured/reachable/counts,
  and `_core.store_push(cache)` / `_core.store_pull(cache)`.

- [ ] **Step 1: Write the failing tests** in `tests/test_store_cli.py`:

```python
def test_status_says_when_the_build_has_no_remote_support() -> None:
    # the done-when clause: a default build says so, in words that name the
    # feature and the extra rather than only failing
def test_status_names_where_the_url_came_from(tmp_path: Path) -> None:
    # ligase.toml's [store] url, then LIGASE_STORE overriding it
def test_status_counts_local_only_and_says_how_it_counted(tmp_path: Path) -> None:
    # "exact" below the bound; the word "sampled" and the sample size above it
def test_a_secret_in_ligase_toml_is_refused(tmp_path: Path) -> None:
    # a `secret_key`/`access_key`/`token` key under [store] is an error naming
    # the provider's own chain. D43: credentials never come from our config.
def test_push_then_pull_moves_objects(tmp_path: Path) -> None:
```

- [ ] **Step 2: Run them** — `pytest tests/test_store_cli.py` — Expected: FAIL.
- [ ] **Step 3: Implement `config.py`** with `tomllib` (stdlib on 3.11+, so
      no dependency), walking up from the pipeline's directory for
      `ligase.toml`. Any key under `[store]` other than `url` is an error that
      names the provider's standard chain — the one place a user would try to
      put a secret is the one place that refuses it.
- [ ] **Step 4: Implement the three subcommands**, each with help text, since
      `generate_cli.py` fails the docs build without it. `status` prints the
      URL and its source, whether this build has the feature, whether the
      store answered, the local object count, and the local-only count —
      **exact at or below 1000 local objects, sampled over 1000 above it, and
      the line says which**. `push` publishes every object and row this
      cache's index references that the remote lacks; `pull` fetches what the
      index references and the local tier lacks. Their help says plainly that
      they move what *this* index knows about, and that a cold machine is
      warmed by `ligase run`, which is what knows the keys it needs.
- [ ] **Step 5: `pytest tests/test_store_cli.py` and `mypy`, `ruff`** — Expected: PASS.
- [ ] **Step 6: Commit** — `feat(cli): ligase store status, push and pull`

---

### Task 7: thin bundles

**Files:**
- Modify: `crates/ligase-core/src/prov/bundle.rs`, `crates/ligase-py/src/lib.rs`, `python/ligase/cli.py`, `python/ligase/provenance.py`
- Test: `tests/test_methods.py`

**Interfaces:**
- Produces: `bundle::Request { thin: bool, .. }`, a `store.tsv` naming the
  store URL, and `seed` that fetches a missing body from the store.

- [ ] **Step 1: Write the failing tests** in `tests/test_methods.py`:

```python
def test_a_thin_bundle_carries_no_bodies_but_says_what_it_no_longer_promises() -> None:
    # records.tsv is there, records/ is not, README.md and store.tsv name the
    # store, and the README says the no-network replay guarantee is gone
def test_a_thin_bundle_replays_where_the_store_is_reachable(tmp_path: Path) -> None:
def test_a_thin_bundle_without_store_access_fails_before_any_step_runs(tmp_path: Path) -> None:
    # the message names the store URL and how many bodies are missing, and no
    # step has run
def test_thin_warns_about_the_recorded_bodies_it_is_leaving_out() -> None:
    # loudly, not quietly: M10b's guarantee is the thing being traded away
```

- [ ] **Step 2: Run them** — `pytest tests/test_methods.py` — Expected: FAIL.
- [ ] **Step 3: Implement `--thin`.** It omits `records/` and every copied
      body, writes `store.tsv`, and the README states the trade in a
      sentence. `seed` reads `store.tsv`, and for a body it cannot find in
      the bundle asks the store; what it cannot get at all it collects, and
      fails **once** with every missing hash and the store URL rather than
      per-record.
- [ ] **Step 4: `pytest tests/test_methods.py`** — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(prov): thin bundles, and what they stop promising`

---

### Task 8: the run report says what the store did

**Files:**
- Modify: `crates/ligase-core/src/exec.rs`, `crates/ligase-py/src/lib.rs`, `python/ligase/run.py`
- Test: `tests/test_run.py`

- [ ] **Step 1: Write the failing tests** in `tests/test_run.py`:

```python
def test_the_report_says_when_objects_came_from_the_store(tmp_path: Path) -> None:
def test_the_report_says_when_the_store_was_unreachable(tmp_path: Path) -> None:
    # Review Focus 1. A run that fell back to computing because a bucket was
    # briefly away is not wrong, and a user who is not told is misled.
def test_the_report_says_when_an_object_was_refused(tmp_path: Path) -> None:
```

- [ ] **Step 2: Run them** — Expected: FAIL.
- [ ] **Step 3: Add `StoreStats` to what `render` prints**, as its own line
      after the step lines, and nothing at all when no store is configured —
      a report must not grow a line for a feature the run did not use.
- [ ] **Step 4: `pytest tests/test_run.py`** — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(exec): the run report says what the store did`

---

### Task 9: MinIO in CI

**Files:** Modify `.github/workflows/ci.yml`.

- [ ] **Step 1: Add a `minio` job** — `ubuntu-24.04`, a
      `minio/minio` service container, `AWS_ACCESS_KEY_ID`,
      `AWS_SECRET_ACCESS_KEY`, `AWS_ENDPOINT_URL` and `AWS_REGION` in the
      environment (the provider's own chain, nothing Ligase-specific), a
      bucket created with `mc`, then `cargo test --workspace --features
      remote-store` and `pytest` with `LIGASE_STORE=s3://ligase-ci`.
- [ ] **Step 2: Run the suite locally against a MinIO container first**, and
      record in the plan file what breaks. **If a test fails because another
      test's entry was served from the shared bucket, that is a finding about
      the clause, not a licence to edit the test** — report it.
- [ ] **Step 3: Commit** — `ci: run the suite against a MinIO container`

---

### Task 10: docs

**Files:**
- Create: `web/content/concepts/remote-store.mdx`, `web/content/guide/remote-store.mdx`, `web/content/project/architecture/store.mdx`
- Modify: the three `_meta.ts`, `web/content/project/decisions.mdx` (D43),
  `web/content/project/milestones.mdx` (M14 done), `CHANGELOG.md`

- [ ] **Step 1: Write the three pages.** Concepts: the two tiers, what each
      promises, and the trust boundary — a shared cache is as trustworthy as
      write access to the bucket, and content addressing protects the bytes
      rather than the claim. Guide: pointing a lab at a bucket, with the
      provider's own credential chain and nothing in `ligase.toml` but a URL.
      Architecture: the trait, promotion, write-through, published rows, why
      the index stays local, the `object_store` dependency and its cmake
      cost, and what was deliberately left out.
- [ ] **Step 2: Write D43** into `decisions.mdx`, including the correction:
      the index is rebuildable **because the rows are published**, not
      because every column derives from a content hash, which it does not.
- [ ] **Step 3: Mark M14 done; add the `CHANGELOG.md` entry.** Both files are
      being appended to by two other sessions — expect a mechanical conflict
      and keep both sides.
- [ ] **Step 4: `cd web && PATH="<worktree>/.venv/bin:$PATH" npm run check`** — Expected: PASS.
- [ ] **Step 5: Commit** — `docs: the two-tier store, and D43`
