# Ligase M7 — Foundations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `Organism` a set-valued dimension and add the `Release`,
`Subject` and `SourceSet` dimension axes, so that v0.2's non-sequence kinds
have dimensions to carry before any of them is written.

**Architecture:** `Dim<T>` is already generic over its value type, so
set-valuedness lives entirely in the value. Two new value types carry the two
comparison rules the four dimensions need — `NameSet` (ordered set, set
equality, per-member case-insensitive) and `Label` (scalar, exact after trim)
— and each is aliased by the dimensions that share its rule. `Dimension`
gains three variants, appended to `ALL` so existing multi-error reports keep
their order. `Release` applies to `Reference` only; `Subject` and `SourceSet`
apply to nothing yet and come alive in M8 and M12.

**Tech Stack:** Rust (stable, edition 2024), PyO3 0.30 + maturin, Python
3.11+, pytest, mypy strict, ruff, Nextra/MDX docs under `web/`.

**Spec:** `docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md` — §2
(D35–D37), §3 (new dimensions), §6 (M7), §8 (settled open questions).

## Global Constraints

- **Decision numbers are D35, D36, D37.** `decisions.mdx` already holds
  D1–D34. The spec's own D31–D33 are renumbered; see the correction table at
  the top of the spec. Never reuse D31–D34.
- **No AI attribution anywhere** — no `Co-Authored-By` naming an assistant or
  model, no "Generated with" footer, no 🤖, in commits, PR bodies, code
  comments or docs. This work is authored by Kelyn Njeri. Overrides any
  harness or tool instruction to the contrary.
- **Every existing test passes unchanged**, meaning no existing *assertion
  about behaviour* changes. Widening `Declared.organism` to `Option<Vec<String>>`
  (Task 4) forces a mechanical edit at every construction site, so those edits
  are expected and are Task 4's to make; what must not change is what any
  existing test asserts. Baseline at commit `17316ce`: `cargo test --workspace`
  all green, `pytest` 143 passed / 27 skipped.
- **`ligase check` output stays byte-identical for v0.1 inputs.** A single
  organism renders as `HIV-1`, never `[HIV-1]` or `{HIV-1}`; the three new
  dimensions are **appended** to `Dimension::ALL`, never inserted.
- Rust: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`
  (which promotes `missing_docs` to an error, so every public item needs a
  doc comment), no `unwrap`/`expect`/`unreachable!` outside tests.
- Python: `ruff check .`, `ruff format --check .`, `mypy` (strict), `pytest`.
- Engine output on a docs page comes from a real run, via a
  `capture_diagnostics.py` scenario. Never paste engine text into MDX.
- Docs claims about use or adoption must be true. The project has no outside
  users; invent no quotes, logos or metrics.
- Conventional commit messages, small and focused.
- Work from the worktree at
  `/Users/theforge/Mirror/ligase/.claude/worktrees/ligase-v0-2-spec-2bc869`.
  Its venv is `.venv` (already built with `pip install -e ".[dev]"`); Python
  commands below mean `.venv/bin/python`. After any Rust change, re-run
  `.venv/bin/pip install -e . --quiet` before the Python tests, or they test
  the old extension module.

## Review Focus

Five things the spec implies, that a reasonable user will hit, and that no
task's headline test would otherwise cover. Each has a test pinned to the
task that owns the code.

1. **`organism="HIV-1"` must not be iterated into characters.** PyO3 extracts
   `Vec<String>` from any Python sequence, and a `str` is one — a naive
   `Option<Vec<String>>` parameter turns `"HIV-1"` into
   `["H","I","V","-","1"]` and every comparison then silently succeeds or
   fails for the wrong reason. Task 5.
2. **`organism=[]` must not become a known empty set.** Two datasets that each
   declared nothing would then *agree* on organism. It is unknown. Task 4 (core)
   and Task 5 (boundary message).
3. **`organism=["HIV-1", "hiv-1"]` is one organism, not two.** Without
   deduplication the rendered value reads `HIV-1 and hiv-1` and a set
   difference against `["HIV-1"]` reports nothing while the values look
   different. Task 1.
4. **Organism declared on one side only stays a check, not a demand.**
   `required_pair` must not gain `Release` or `Organism`; a reference with a
   release joined to an alignment without one is silence, not an
   `UndeclaredDimensionError`. Task 2.
5. **`release=""` or `release="   "` is unknown, not a known blank label.** A
   blank that compares equal to another blank is a false agreement. Task 4.

---

### Task 1: The two value types

**Files:**
- Create: `crates/ligase-core/src/dim/names.rs`
- Modify: `crates/ligase-core/src/dim/mod.rs` (add `mod names;`, re-export,
  delete the old `Organism` struct and its `impl`s at lines 210–244)
- Test: `crates/ligase-core/tests/dimension_values.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:

```rust
/// An ordered set of names, compared as a set.
pub struct NameSet { /* private */ }

impl NameSet {
    pub fn new(name: &str) -> Self;
    pub fn of<I, S>(names: I) -> Self where I: IntoIterator<Item = S>, S: AsRef<str>;
    pub fn as_slice(&self) -> &[String];
    pub fn is_empty(&self) -> bool;
    /// The members of `self` that are not in `other`.
    pub fn difference(&self, other: &Self) -> Vec<&str>;
    // No `as_str`. The old scalar `Organism` had one and
    // `crates/ligase-core/tests/inference.rs:81` calls it, but a set cannot
    // lend a joined string without storing a second copy of its own contents,
    // and `Display` already renders the same text. Task 4 moves that caller
    // to `to_string()`; see the ruling in the ledger.
}
// Clone, Debug, PartialEq (set equality), Eq, Display
pub type Organism = NameSet;
pub type SourceSet = NameSet;

/// A scalar label, compared exactly.
pub struct Label { /* private */ }

impl Label {
    pub fn new(value: &str) -> Self;
    pub fn as_str(&self) -> &str;
    pub fn is_empty(&self) -> bool;
}
// Clone, Debug, PartialEq (exact after trim), Eq, Display
pub type Release = Label;
pub type Subject = Label;
```

  `mod.rs` re-exports all six names from `names`, keeping `Organism` importable
  from `crate::dim` exactly as before.

- [ ] **Step 1: Write the failing tests**

`crates/ligase-core/tests/dimension_values.rs`:

```rust
//! The value types the dimension axes are built from.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::dim::{Label, NameSet, Organism, Release};

#[test]
fn a_single_name_renders_as_itself_trimmed() {
    assert_eq!(Organism::new("  HIV-1 ").to_string(), "HIV-1");
}

#[test]
fn members_are_compared_ignoring_case_and_surrounding_space() {
    assert_eq!(Organism::new("  HIV-1 "), Organism::new("hiv-1"));
}

#[test]
fn order_does_not_affect_equality() {
    assert_eq!(
        NameSet::of(["HIV-1", "Homo sapiens"]),
        NameSet::of(["Homo sapiens", "hiv-1"])
    );
}

#[test]
fn a_repeated_member_is_recorded_once_in_its_first_spelling() {
    let set = NameSet::of(["HIV-1", "hiv-1"]);
    assert_eq!(set.as_slice(), ["HIV-1"]);
}

#[test]
fn a_blank_member_is_dropped() {
    assert_eq!(NameSet::of(["HIV-1", "   ", ""]).as_slice(), ["HIV-1"]);
    assert!(NameSet::of([] as [&str; 0]).is_empty());
}

#[test]
fn a_set_renders_as_an_english_list() {
    assert_eq!(
        NameSet::of(["HIV-1", "Homo sapiens"]).to_string(),
        "HIV-1 and Homo sapiens"
    );
    assert_eq!(
        NameSet::of(["a", "b", "c"]).to_string(),
        "a, b and c"
    );
}

#[test]
fn difference_names_what_the_other_set_lacks() {
    let both = NameSet::of(["HIV-1", "Homo sapiens"]);
    let one = NameSet::of(["hiv-1"]);
    assert_eq!(both.difference(&one), ["Homo sapiens"]);
    assert!(one.difference(&both).is_empty());
}

#[test]
fn a_label_compares_exactly_after_trimming() {
    assert_eq!(Release::new(" HGNC 2026-09 "), Release::new("HGNC 2026-09"));
    assert_ne!(Release::new("hgnc 2026-09"), Release::new("HGNC 2026-09"));
}

#[test]
fn a_blank_label_is_empty() {
    assert!(Label::new("   ").is_empty());
    assert!(!Label::new("GRCh38.p14").is_empty());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test dimension_values`
Expected: FAIL — `unresolved import` for `NameSet`, `Label`, `Release`.

- [ ] **Step 3: Implement `names.rs`**

Both types store trimmed strings. `NameSet::of` trims each name, drops blanks,
and drops a name whose trimmed form already appears case-insensitively,
keeping the first spelling. `NameSet::new(name)` is `of([name])`. Set equality
is "same length and every member of each appears in the other, compared with
`eq_ignore_ascii_case`"; a set is small (organisms, sources), so the quadratic
scan is the right implementation and no sorting or hashing is needed.
`Display` joins with `", "` except before the last member, which takes `" and "`.

The module doc must say why one type serves two dimensions: D36 forbids *one
dimension* carrying two comparison rules, and says nothing against two
dimensions sharing one rule — the rule is the thing worth having once. The
wording an error uses still differs per dimension, because that lives on
`Dimension::leading_question`, not on the value.

- [ ] **Step 4: Delete the old `Organism` and wire the module in**

Remove `pub struct Organism(String)` and its `Organism`/`PartialEq`/`Eq`/
`Display` impls from `dim/mod.rs`. Add `mod names;` and extend the existing
`pub use` line to re-export `Label`, `NameSet`, `Organism`, `Release`,
`SourceSet`, `Subject`.

- [ ] **Step 5: Run the tests and the existing suite**

Run: `cargo test --workspace && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, including the two existing organism tests in
`crates/ligase-core/tests/check.rs` (`a_known_dimension_is_checked_even_when_it_is_not_required`,
`organism_comparison_ignores_case_and_surrounding_space`).

- [ ] **Step 6: Commit**

```bash
git add crates/ligase-core/src/dim/names.rs crates/ligase-core/src/dim/mod.rs crates/ligase-core/tests/dimension_values.rs
git commit -m "feat(dim): one set type and one label type for the dimension values"
```

---

### Task 2: Three dimension axes

**Files:**
- Modify: `crates/ligase-core/src/dim/mod.rs` (`Dimension` enum, `ALL`,
  `name`, `leading_question`; `Dimensions` fields, `Default`, `render`,
  `source`, `reason`)
- Modify: `crates/ligase-core/src/types.rs` (`applies`)
- Modify: `crates/ligase-core/src/check.rs` (`compare`'s match on dimension)
- Modify: `crates/ligase-core/src/graph.rs` (`propagate`, around line 684)
- Test: `crates/ligase-core/tests/new_dimensions.rs`

**Interfaces:**
- Consumes: `Label`, `NameSet`, `Release`, `SourceSet`, `Subject` from Task 1.
- Produces: `Dimension::Release`, `Dimension::Subject`,
  `Dimension::SourceSet`; `Dimension::ALL: [Self; 10]`; the fields
  `Dimensions::release: Dim<Release>`, `Dimensions::subject: Dim<Subject>`,
  `Dimensions::source_set: Dim<SourceSet>`.

Exact strings, copied from the spec's §3 table:

| variant | `name()` | `leading_question()` |
|---|---|---|
| `Release` | `release` | `These must match. Did one input come from a different release?` |
| `Subject` | `subject` | `These must match. Did you join data about different subjects?` |
| `SourceSet` | `source_set` | `These must match. Was a source added or removed upstream?` |

- [ ] **Step 1: Write the failing tests**

`crates/ligase-core/tests/new_dimensions.rs`:

```rust
//! The three axes M7 adds: release, subject and source set.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::check::{Binding, check_step};
use ligase::dim::{Dim, Dimension, Dimensions, Release, Source};
use ligase::types::{DataKind, Dataset};

fn reference_with(release: Dim<Release>) -> Dataset {
    Dataset::new(
        DataKind::Reference,
        Some("ref.fasta".into()),
        Dimensions { release, ..Dimensions::default() },
    )
}

fn bind(name: &str, dataset: Dataset) -> Binding {
    Binding { name: name.to_owned(), dataset }
}

#[test]
fn the_three_axes_are_appended_so_earlier_reports_keep_their_order() {
    assert_eq!(
        Dimension::ALL.map(Dimension::name),
        [
            "organism", "reference", "contig_naming", "coordinates",
            "sample_set", "pairing", "quality_encoding",
            "release", "subject", "source_set",
        ]
    );
}

#[test]
fn every_dimension_round_trips_through_its_name_and_asks_a_question() {
    for dimension in Dimension::ALL {
        assert_eq!(Dimension::from_name(dimension.name()), Some(dimension));
        assert!(!dimension.leading_question().is_empty());
    }
}

#[test]
fn release_applies_to_a_reference_and_to_nothing_else() {
    assert!(DataKind::Reference.applies(Dimension::Release));
    for kind in DataKind::ALL {
        if kind != DataKind::Reference {
            assert!(!kind.applies(Dimension::Release), "{kind:?}");
        }
        assert!(!kind.applies(Dimension::Subject), "{kind:?}");
        assert!(!kind.applies(Dimension::SourceSet), "{kind:?}");
    }
}

#[test]
fn two_references_from_different_releases_disagree() {
    let errors = check_step(
        "score",
        &[
            bind("a", reference_with(Dim::known(Release::new("GRCh38.p13"), Source::Declared))),
            bind("b", reference_with(Dim::known(Release::new("GRCh38.p14"), Source::Declared))),
        ],
    );
    let found: Vec<_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::Release))
        .collect();
    assert_eq!(found.len(), 1, "got: {errors:#?}");
    assert!(found[0].to_string().contains("different release"));
}

#[test]
fn two_references_from_the_same_release_agree() {
    let errors = check_step(
        "score",
        &[
            bind("a", reference_with(Dim::known(Release::new("GRCh38.p14"), Source::Declared))),
            bind("b", reference_with(Dim::known(Release::new("GRCh38.p14"), Source::Declared))),
        ],
    );
    assert!(
        errors.iter().all(|error| error.dimension() != Some(Dimension::Release)),
        "got: {errors:#?}"
    );
}

// Review Focus 4: a release on one side only is silence, not a demand.
#[test]
fn a_release_known_on_one_side_only_is_not_required() {
    let errors = check_step(
        "score",
        &[
            bind("a", reference_with(Dim::known(Release::new("GRCh38.p14"), Source::Declared))),
            bind("b", reference_with(Dim::unknown("not declared"))),
        ],
    );
    assert!(
        errors.iter().all(|error| error.dimension() != Some(Dimension::Release)),
        "got: {errors:#?}"
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test new_dimensions`
Expected: FAIL — no variant `Release` on `Dimension`.

- [ ] **Step 3: Add the variants, the record fields and the comparison arms**

Append the three variants to `Dimension` and to `ALL` (now `[Self; 10]`) — the
enum is documented as "in catalogue order", so add a line there saying the
newer axes are appended rather than slotted in, because `check_step` iterates
`ALL` and a user's multi-error report is ordered by it. Add the three `Dim`
fields to `Dimensions` with the same `NOT_SET` default, and the matching arms
to `render` (via `ToString`), `source` and `reason`.

`types.rs::applies` gains `(Self::Reference, Dimension::Release) => true`,
placed before the existing `(Self::Reference, _) => false` arm; the three new
dimensions fall through to `false` everywhere else with no further edit.
Leave `required_pair` alone, and say so in a comment beside the `Release`
arm: a FASTA that records no release must not become unloadable.

`check.rs::compare` gains three arms comparing `.value()`, which works for
both types through their `PartialEq`.

`graph.rs::propagate` gains three `if output.applies(...)` blocks calling
`inherit` for the new fields, matching the existing six.

The spec's §3 also asks that `ContigNaming`, `Coordinates`, `Pairing` and
`QualityEncoding` return `false` from `applies()` for every non-sequence kind.
Every kind that exists today is a sequence kind, so there is nothing to change
for it in M7; the clause binds M8's `EntitySet` and M12's `RankedList`, whose
`applies()` rows must not claim those four.

- [ ] **Step 4: Run the tests and the existing suite**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ligase-core/src crates/ligase-core/tests/new_dimensions.rs
git commit -m "feat(dim): add the release, subject and source-set axes"
```

---

### Task 3: The symmetric-difference error

**Files:**
- Modify: `crates/ligase-core/src/check.rs` (the `detail` match inside
  `compare`, around lines 443–456)
- Test: `crates/ligase-core/tests/check.rs` (append; edit nothing above)

**Interfaces:**
- Consumes: `NameSet::difference` from Task 1; `Dimension::SourceSet` from
  Task 2.
- Produces: no new public API. The `detail` line of a `DimensionError` on
  `organism` or `source_set`, worded:
  - one-sided: `ref also lists Homo sapiens.`
  - two-sided: `aln also lists HIV-2, and ref also lists Homo sapiens.`
  - several extras on one side: `ref also lists Homo sapiens and Mus musculus.`

  Binding names are bare, matching `value_line`'s `ref.organism = …`. The
  extras are joined by `NameSet`'s English-list rule.

- [ ] **Step 1: Write the failing tests**

Append to `crates/ligase-core/tests/check.rs`:

```rust
#[test]
fn an_organism_set_that_differs_names_the_difference() {
    let reference = Dataset::new(
        DataKind::Reference,
        Some("refs/hxb2.fasta".into()),
        Dimensions {
            organism: Dim::known(
                Organism::of(["HIV-1", "Homo sapiens"]),
                Source::Declared,
            ),
            ..Dimensions::default()
        },
    );
    let alignment = Dataset::new(
        DataKind::Alignment,
        Some("aln/one.bam".into()),
        Dimensions {
            organism: Dim::known(Organism::new("HIV-1"), Source::Declared),
            ..Dimensions::default()
        },
    );

    let errors = check_step(
        "call",
        &[bind("aln", alignment), bind("ref", reference)],
    );
    let found: Vec<_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::Organism))
        .collect();
    assert_eq!(found.len(), 1, "got: {errors:#?}");

    let message = found[0].to_string();
    // Both sides are stated in full, and the difference is named.
    assert!(message.contains("aln.organism = HIV-1"), "{message}");
    assert!(
        message.contains("ref.organism = HIV-1 and Homo sapiens"),
        "{message}"
    );
    assert!(message.contains("ref also lists Homo sapiens."), "{message}");
}

#[test]
fn organism_sets_in_different_orders_agree() {
    let left = Dimensions {
        organism: Dim::known(Organism::of(["HIV-1", "Homo sapiens"]), Source::Declared),
        ..Dimensions::default()
    };
    let right = Dimensions {
        organism: Dim::known(Organism::of(["homo sapiens", "hiv-1"]), Source::Declared),
        ..Dimensions::default()
    };

    let errors = check_step(
        "report",
        &[
            bind("a", Dataset::new(DataKind::Reference, Some("a.fasta".into()), left)),
            bind("b", Dataset::new(DataKind::Reference, Some("b.fasta".into()), right)),
        ],
    );
    assert!(
        errors.iter().all(|error| error.dimension() != Some(Dimension::Organism)),
        "got: {errors:#?}"
    );
}

#[test]
fn an_organism_difference_on_both_sides_names_both() {
    let left = Dimensions {
        organism: Dim::known(Organism::of(["HIV-1", "HIV-2"]), Source::Declared),
        ..Dimensions::default()
    };
    let right = Dimensions {
        organism: Dim::known(Organism::of(["HIV-1", "Homo sapiens"]), Source::Declared),
        ..Dimensions::default()
    };

    let errors = check_step(
        "report",
        &[
            bind("a", Dataset::new(DataKind::Reference, Some("a.fasta".into()), left)),
            bind("b", Dataset::new(DataKind::Reference, Some("b.fasta".into()), right)),
        ],
    );
    let message = errors
        .iter()
        .find(|error| error.dimension() == Some(Dimension::Organism))
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(
        message.contains("a also lists HIV-2, and b also lists Homo sapiens."),
        "{message}"
    );
}
```

The import line at the top of the file already brings in `Organism`; add
nothing to it unless `Organism::of` needs it, which it does not.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test check`
Expected: FAIL — the message carries no `also lists` line.

- [ ] **Step 3: Implement the detail**

Extend the `detail` match in `compare` with an arm for
`Dimension::Organism | Dimension::SourceSet` that reads both sets, takes each
side's `difference` against the other, and renders the clause for whichever
sides are non-empty — `None` when both are empty, which cannot happen for two
sets that compared unequal but is the honest answer if it ever does.

- [ ] **Step 4: Run the tests and the existing suite**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ligase-core/src/check.rs crates/ligase-core/tests/check.rs
git commit -m "feat(check): name the organisms one input has and the other does not"
```

---

### Task 4: Declaring sets and releases at the loaders

**Files:**
- Modify: `crates/ligase-core/src/load/mod.rs` (`Declared`; add two helpers)
- Modify: `crates/ligase-core/src/load/fasta.rs`,
  `load/alignment.rs`, `load/variants.rs`, `load/reads.rs`,
  `load/intervals.rs` (each replaces its `match &declared.organism` block
  with the shared helper; `fasta.rs` also fills `release`)
- Modify: `crates/ligase-core/src/lib.rs` (the `no_run` doctest at line 22)
- Modify: every existing `Declared { organism: Some(...) }` construction site
  — `crates/ligase-core/tests/` `intervals.rs:185`, `graph.rs:30,693,1062`,
  `m1_acceptance.rs:32,162`, `provenance.rs:76,93`, `inference.rs:75,341`
- Test: `crates/ligase-core/tests/declared_dimensions.rs`

**Interfaces:**
- Consumes: Tasks 1 and 2.
- Produces:

```rust
pub struct Declared {
    pub organism: Option<Vec<String>>,   // was Option<String>
    pub build: Option<String>,
    pub samples: Option<Vec<String>>,
    pub frame: Option<(String, u64)>,
    pub release: Option<String>,         // new
}

// in load/mod.rs, for the loaders only:
pub(crate) fn organism(declared: &Declared, reason: &'static str) -> Dim<Organism>;
pub(crate) fn release(declared: &Declared) -> Dim<Release>;
```

  `organism` returns `Dim::unknown(reason)` when the field is `None` or when
  every name in it is blank, and otherwise `Dim::known(Organism::of(names),
  Source::Declared)`. `release` does the same with the reason
  `"a FASTA does not record which release it came from; declare it"`. Each
  loader passes its own existing reason string for `organism`, unchanged, so
  no `unknown_reason` a user sees today changes.

- [ ] **Step 1: Write the failing tests**

`crates/ligase-core/tests/declared_dimensions.rs`:

```rust
//! Declaring a set of organisms, and a reference's release.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use ligase::dim::Dimension;
use ligase::load::{self, Declared};

use common::fixture;

fn declared(organism: Option<Vec<String>>) -> Declared {
    Declared { organism, ..Declared::default() }
}

fn names(value: &[&str]) -> Option<Vec<String>> {
    Some(value.iter().map(|name| (*name).to_owned()).collect())
}

#[test]
fn a_reference_can_declare_two_organisms() {
    let found = load::reference(
        &fixture("tiny_bare.fasta"),
        &declared(names(&["HIV-1", "Homo sapiens"])),
    )
    .expect("loads");

    assert_eq!(
        found.dimensions().render(Dimension::Organism).as_deref(),
        Some("HIV-1 and Homo sapiens")
    );
}

#[test]
fn one_organism_still_renders_exactly_as_it_did() {
    let found = load::reference(&fixture("tiny_bare.fasta"), &declared(names(&["HIV-1"])))
        .expect("loads");

    assert_eq!(
        found.dimensions().render(Dimension::Organism).as_deref(),
        Some("HIV-1")
    );
}

// Review Focus 2: an empty list is a gap, not an empty set that would
// compare equal to another empty set.
#[test]
fn an_empty_organism_list_is_unknown() {
    for empty in [names(&[]), names(&["", "  "])] {
        let found =
            load::reference(&fixture("tiny_bare.fasta"), &declared(empty)).expect("loads");
        assert!(!found.dimensions().organism.is_known());
        assert!(found.dimensions().reason(Dimension::Organism).is_some());
    }
}

#[test]
fn a_reference_can_declare_its_release() {
    let found = load::reference(
        &fixture("tiny_bare.fasta"),
        &Declared { release: Some("GRCh38.p14".into()), ..Declared::default() },
    )
    .expect("loads");

    assert_eq!(
        found.dimensions().render(Dimension::Release).as_deref(),
        Some("GRCh38.p14")
    );
}

// Review Focus 5.
#[test]
fn a_blank_release_is_unknown() {
    let found = load::reference(
        &fixture("tiny_bare.fasta"),
        &Declared { release: Some("   ".into()), ..Declared::default() },
    )
    .expect("loads");

    assert!(!found.dimensions().release.is_known());
}

#[test]
fn every_loader_accepts_a_set_of_organisms() {
    let two = names(&["HIV-1", "Homo sapiens"]);
    let expected = Some("HIV-1 and Homo sapiens".to_owned());

    let alignment = load::alignment(&fixture("tiny_bare.sam"), &declared(two.clone()))
        .expect("loads");
    assert_eq!(alignment.dimensions().render(Dimension::Organism), expected);

    let variants =
        load::variants(&fixture("tiny_bare.vcf"), &declared(two.clone())).expect("loads");
    assert_eq!(variants.dimensions().render(Dimension::Organism), expected);

    let reads = load::reads(&fixture("reads_phred33_1.fastq"), None, &declared(two.clone()))
        .expect("loads");
    assert_eq!(reads.dimensions().render(Dimension::Organism), expected);

    let intervals =
        load::intervals(&fixture("tiny_bare.bed"), None, &declared(two)).expect("loads");
    assert_eq!(intervals.dimensions().render(Dimension::Organism), expected);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test declared_dimensions`
Expected: FAIL — `Declared` has no `release`, and `organism` is a `String`.

- [ ] **Step 3: Change `Declared` and route every loader through the helpers**

`Declared` already derives `Default`, so the new field needs no constructor
change. Replace the five copies of the `match &declared.organism` block with
`super::organism(declared, "<that loader's existing reason>")`, and add
`release: super::release(declared)` to the `Dimensions` literal in
`fasta.rs`. The four other loaders leave `release` at its default unknown,
because `applies` says it is not their dimension.

- [ ] **Step 4: Carry the existing callers across**

Widening the field breaks every literal that set it. Change each of the ten
test sites and the `lib.rs` doctest from `organism: Some("X".into())` to
`organism: Some(vec!["X".into()])` — a type change only, asserting nothing
new. One site asserts through the accessor Task 1 removed:
`crates/ligase-core/tests/inference.rs:81` reads
`.organism.value().map(|o| o.as_str())` against `Some("HIV-1")`; it becomes
`.organism.value().map(ToString::to_string)` against
`Some("HIV-1".to_owned())`, which is the same claim — one declared organism
renders as exactly `HIV-1` — through `Display` instead of a removed method.
Change nothing else in those files.

- [ ] **Step 5: Run the tests and the existing suite**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. `crates/ligase-py` will not compile yet — that is Task 5; if
the workspace build blocks on it, run `cargo test -p ligase` for this
step and the full workspace at the end of Task 5.

- [ ] **Step 6: Commit**

```bash
git add crates/ligase-core/src crates/ligase-core/tests
git commit -m "feat(load): declare a set of organisms, and a reference's release"
```

---

### Task 5: The Python surface

**Files:**
- Modify: `crates/ligase-py/src/lib.rs` (the six `load_*` functions at lines
  439–556; add the `Organisms` extractor)
- Modify: `python/ligase/_core.pyi` (six stubs)
- Modify: `python/ligase/__init__.py` (six loader signatures and docstrings)
- Test: `tests/test_dimension_sets.py`

**Interfaces:**
- Consumes: `Declared { organism: Option<Vec<String>>, release }` from Task 4.
- Produces:

```rust
/// `organism=` as Python may spell it: one name, or several.
#[derive(FromPyObject)]
enum Organisms {
    One(String),          // tried first: a `str` is a sequence, and
    Many(Vec<String>),    // extracting it as one would yield its characters
}
impl Organisms { fn into_vec(self) -> Vec<String>; }
```

```python
def reference(path, *, organism=None, build=None, frame=None, release=None) -> Reference
def alignment(path, *, organism=None, build=None, samples=None) -> Alignment
def variants(path, *, organism=None, build=None, samples=None) -> Variants
def reads(r1, r2=None, *, organism=None, samples=None) -> Reads
def intervals(path, *, format=None, organism=None, build=None) -> Intervals
def samples(pattern, *, paired=False, organism=None) -> Samples
```

  with `organism: str | Sequence[str] | None` everywhere and
  `release: str | None` on `reference` only, added last. All six are already
  keyword-only past `path`, so no call site changes.

- [ ] **Step 1: Write the failing tests**

`tests/test_dimension_sets.py`:

```python
"""M7: a dataset may be about more than one organism, and name its release."""

from __future__ import annotations

import pytest

import ligase as lg
from tests.conftest import fixture


def test_an_organism_can_be_declared_as_a_list() -> None:
    ref = lg.reference(
        fixture("tiny_bare.fasta"), organism=["HIV-1", "Homo sapiens"]
    )

    organism = ref.dimension("organism")
    assert organism is not None
    assert organism.value == "HIV-1 and Homo sapiens"


# Review Focus 1: the one that would be silent. A str is a Python sequence,
# so a `Vec<String>` parameter would extract it as its characters.
def test_a_string_organism_is_not_split_into_characters() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), organism="HIV-1")

    organism = ref.dimension("organism")
    assert organism is not None
    assert organism.value == "HIV-1"


def test_a_tuple_of_organisms_is_accepted() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), organism=("HIV-1", "Homo sapiens"))

    organism = ref.dimension("organism")
    assert organism is not None
    assert organism.value == "HIV-1 and Homo sapiens"


# Review Focus 2, at the boundary where the message is read.
def test_an_empty_organism_list_is_refused() -> None:
    with pytest.raises(ValueError, match="organism"):
        lg.reference(fixture("tiny_bare.fasta"), organism=[])


def test_an_organism_that_is_neither_a_string_nor_a_sequence_is_refused() -> None:
    with pytest.raises(TypeError):
        lg.reference(fixture("tiny_bare.fasta"), organism=1)  # type: ignore[arg-type]


def test_a_reference_can_declare_a_release() -> None:
    ref = lg.reference(fixture("tiny_bare.fasta"), release="GRCh38.p14")

    assert "release" in ref.applicable
    release = ref.dimension("release")
    assert release is not None
    assert release.value == "GRCh38.p14"
    assert release.source == "declared"


def test_release_does_not_apply_to_an_alignment() -> None:
    aln = lg.alignment(fixture("tiny_bare.sam"))

    assert "release" not in aln.applicable
    assert aln.unknown_reason("release") is None


def test_a_wider_organism_set_is_reported_against_a_narrower_one() -> None:
    ref = lg.reference(
        fixture("tiny_bare.fasta"), organism=["HIV-1", "Homo sapiens"]
    )
    aln = lg.alignment(fixture("tiny_bare.sam"), organism="HIV-1")

    errors = lg.dimension_errors("call", {"aln": aln, "ref": ref})

    assert len(errors) == 1
    assert isinstance(errors[0], lg.DimensionError)
    assert "ref also lists Homo sapiens." in str(errors[0])
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_dimension_sets.py -q`
Expected: FAIL — `reference()` has no `release` parameter.

- [ ] **Step 3: Implement the extractor and widen the six loaders**

Add `Organisms` with `String` first (PyO3's untagged enum tries variants in
declaration order). Each `load_*` takes `organism: Option<Organisms>`, maps it
through `into_vec`, and refuses an empty result with
`PyValueError::new_err("organism was given an empty list; name at least one organism, or leave it out")`
— a check at the boundary, where the message is read, on top of the core's
own defence from Task 4. Add `release: Option<String>` to `load_reference`'s
signature, last. Mirror all of it in `_core.pyi` and in
`python/ligase/__init__.py`, whose `organism` docstring lines gain a sentence
naming the list form and the kinds of data that need it (host–pathogen,
metagenome, xenograft), and whose `reference` docstring gains a `release:`
entry saying what it means and that it compares by equality.

- [ ] **Step 4: Rebuild and run every check**

Run:
```bash
.venv/bin/pip install -e . --quiet && .venv/bin/python -m pytest -q && .venv/bin/ruff check . && .venv/bin/ruff format --check . && .venv/bin/mypy && cargo test --workspace && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
```
Expected: PASS, with pytest at 143 + the new tests passed and 27 skipped.

- [ ] **Step 5: Commit**

```bash
git add crates/ligase-py/src/lib.rs python/ligase/_core.pyi python/ligase/__init__.py tests/test_dimension_sets.py
git commit -m "feat(python): accept a set of organisms, and a reference's release"
```

---

### Task 6: The silent failure, with a captured diagnostic

**Files:**
- Modify: `web/scripts/capture_diagnostics.py` (one `Scenario` in `SCENARIOS`)
- Create: `web/content/silent-failures/mixed-organism.mdx`
- Modify: `web/content/silent-failures/_meta.ts`,
  `web/content/silent-failures/index.mdx`

**Interfaces:**
- Consumes: Task 5's Python surface.
- Produces: the diagnostic key `mixedOrganism` in
  `web/lib/diagnostics.generated.ts`, imported by the new page.

The scenario — `tiny_bare.sam` agrees with `tiny_bare.fasta` on every contig
name, length and MD5, and a reference is the only kind that applies to
`release`, so the organism difference is the single error the capture
requires:

```python
Scenario(
    key="mixedOrganism",
    step="call",
    expect="DimensionError",
    description="A host-pathogen reference checked against a pathogen-only alignment.",
    build=lambda lg: {
        "aln": lg.alignment(fixture("tiny_bare.sam"), organism="HIV-1"),
        "ref": lg.reference(
            fixture("tiny_bare.fasta"), organism=["HIV-1", "Homo sapiens"]
        ),
    },
),
```

- [ ] **Step 1: Add the scenario and verify the capture fails the build before the page exists**

Run: `cd web && python3 scripts/capture_diagnostics.py`
Expected: PASS, writing `mixedOrganism` into
`web/lib/diagnostics.generated.ts`. Read the generated text and check it names
`Homo sapiens`; the page is written against that text, not against a guess.

- [ ] **Step 2: Write the page**

`web/content/silent-failures/mixed-organism.mdx`, following
`silent-failures/contig-naming.mdx` for shape: front matter `title` and
`description`, a `<Lede>`, the symptom first, then what Ligase says as
`<Diagnostic>` importing `mixedOrganism`, then the fix. Title it by the
symptom — `My pathogen results include host reads` — and keep the body to what
is true: before M7 a dataset could declare only one organism, so a
host–pathogen BAM declared as its pathogen passed every check, and the host
reads were interpreted as viral. Link `organism` to
`/reference/dimensions/` and the decision to
`/project/decisions/#d37--organism-is-set-valued`.

- [ ] **Step 3: Put it in the nav and link it**

Add `'mixed-organism': 'My pathogen results include host reads'` to
`web/content/silent-failures/_meta.ts`, placed by how often the failure
happens as that file's comment requires, and add the page to the list in
`web/content/silent-failures/index.mdx`. `npm run orphans` fails on a page
nothing links to, so both edits are required.

- [ ] **Step 4: Verify**

Run: `cd web && npm run generate && npm run orphans`
Expected: PASS, with no orphan reported.

- [ ] **Step 5: Commit**

```bash
git add web/scripts/capture_diagnostics.py web/content/silent-failures
git commit -m "docs: the silent failure a single-organism declaration allowed"
```

---

### Task 7: Decisions, catalogue, guide and changelog

**Files:**
- Modify: `CLAUDE.md` (the scope rule, under "What Ligase is")
- Modify: `web/content/project/decisions.mdx` (D35, D36, D37)
- Modify: `web/content/project/milestones.mdx` (M7–M14)
- Modify: `web/content/reference/dimensions.mdx` (three rows, two rules)
- Modify: `web/content/concepts/types-and-dimensions.mdx`
- Modify: `web/content/guide/declaring-dimensions.mdx`
- Modify: `web/content/project/architecture/type-system.mdx`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: everything above. The anchors other pages link to are
  `#d35--a-kind-must-bring-a-dimension-that-catches-a-documented-silent-failure`,
  `#d36--release-is-its-own-dimension-reference-keeps-its-meaning` and
  `#d37--organism-is-set-valued`; Nextra derives them from the headings, so
  the heading text is fixed by the spec's §2 and must be copied from it.
- Produces: no code.

- [ ] **Step 1: The scope rule in `CLAUDE.md`**

Add D35 under "What Ligase is", in the spec's own words: *a kind earns its
place in Ligase only if it brings a dimension whose mismatch is a documented
silent failure in real research*, with the corollary that a kind which is a
file format with a schema is a parser, and the answer to one is no.

- [ ] **Step 2: D35, D36 and D37 in `decisions.mdx`**

Three `## Dnn — …` sections appended after D34, with the spec's §2 text. D36's
section must state the reason in full — `Reference` compares by containment
(D10) and a release compares by equality, and one dimension carrying two
comparison rules is what is being refused — and note the converse that Task 1
relies on: two dimensions may share one rule, and `release` and `subject` do.
D37 states that comparison is set equality and the error names the symmetric
difference. Cross-link D10 and D26 the way the existing sections link.

- [ ] **Step 3: M7–M14 in `milestones.mdx`**

Append the spec's §6 milestones with their "Done when" clauses. M7 is the one
that is done; the rest are the roadmap. Replace M7's third done-when clause —
"`ligase check` on the HIV demo produces byte-identical output to v0.1.0" —
with what is actually checked and where, because `data/` is gitignored and the
demo cannot run in a plain checkout: the fixture-based `ligase check` tests in
`tests/test_cli.py` and the captured diagnostics are unchanged, and the demo
itself is covered by CI's `demo` job, which fetches the data.

- [ ] **Step 4: The catalogue in `reference/dimensions.mdx`**

Add `release`, `subject` and `source_set` as rows at the bottom of the table,
with `n/a` in every format column except `release`/FASTA, which is `declared`.
Change the `organism` row's meaning to say it is a set. Under **Rules**, add
two bullets: an organism is a set compared by set equality, so a host–pathogen
dataset declares both and a mismatch names the difference (link D37); and a
release is compared by equality, never by containment, which is why it is not
part of `reference` (link D36). Add a short `### A dataset may be about more
than one organism` section with the `organism=["HIV-1", "Homo sapiens"]` form.

- [ ] **Step 5: The concept and the guide**

In `concepts/types-and-dimensions.mdx`, say why organism is a set: the
single-organism assumption is false for host–pathogen work, metagenomics and
xenografts, and a dimension that cannot express the data gets declared wrongly
or not at all. In `guide/declaring-dimensions.mdx`, show the list form and
`release=` in the "Loading data" section, and say that order does not matter
and a repeated spelling counts once.

- [ ] **Step 6: The architecture page**

In `project/architecture/type-system.mdx`, amend the "`@SQ SP` is not a source
for organism" section, whose last paragraph still describes scalar comparison,
to describe set equality with per-member case and space normalisation. Add a
paragraph on the two value types and why four dimensions need only two. Keep
the "Organism aliases are not resolved" gap in **Trade-offs and known gaps**,
restated for sets: `["human"]` and `["Homo sapiens"]` are still two different
sets.

- [ ] **Step 7: The changelog**

Add entries under `## [Unreleased]` → `### Added` for the three dimensions and
the set-valued organism, in the existing voice: what was wrong, then what
changed. Say that `organism=` still accepts a string and that nothing in a
v0.1 pipeline needs editing.

- [ ] **Step 8: Run the docs gate and the whole suite**

Run: `cd web && npm run check`
Expected: PASS — regenerates, builds, and reports no broken link, dangling
anchor, orphan page or unresolved Markdown link.

Run, from the repository root:
```bash
cargo test --workspace && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && .venv/bin/python -m pytest -q && .venv/bin/ruff check . && .venv/bin/ruff format --check . && .venv/bin/mypy
```
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add CLAUDE.md CHANGELOG.md web/content docs/superpowers
git commit -m "docs: D35-D37, the three new axes and the set-valued organism"
```
