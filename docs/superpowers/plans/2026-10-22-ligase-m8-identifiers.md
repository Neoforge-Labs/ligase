# Ligase M8 — Identifiers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the commonest silent bug in biology — a gene list in one
identifier space joined to data in another — a refused connection, and build
the conversion registry that the only legal way across sits in.

**Architecture:** `identifier_namespace` is a new `Dimension` whose value
reuses M7's `Label` (scalar, exact after trimming), because D36 permits two
dimensions to share one comparison rule and forbids only one dimension
carrying two. A `KnownNamespace` catalogue holds the inference patterns, in
the Rust loader so that `ligase inspect` (M9) gets them for free (spec §8 Q2).
`EntitySet` is the first non-sequence kind: it applies `organism`,
`identifier_namespace`, `subject` and `release`, and returns `false` from
`applies()` for `reference`, `contig_naming`, `coordinates`, `sample_set`,
`pairing` and `quality_encoding`. `map_identifiers` is the first entry in a
**conversion registry**: a conversion declares the axis it moves and reads its
target from a parameter named `to`, so `liftover` (axis `reference`) and
`rename_contigs` (axis `contig_naming`) are later entries rather than later
redesigns. The registry overrides one axis of a step's propagated output,
which is the mechanism `coordinates` already uses for intervals.

**Tech Stack:** Rust (stable, edition 2024), PyO3 0.30 + maturin, Python
3.11+, pytest, mypy strict, ruff, Nextra/MDX docs under `web/`.

**Spec:** `docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md` — §1 (D35,
the scope rule), §2 (D38), §3 (new dimensions), §4.1 (`EntitySet`), §5.1
(`map_identifiers` and the registry), §6 (M8), §8 Q2 (inference lives in Rust).
Read alongside `web/content/project/milestones.mdx`'s M8 entry, which carries
M7's standing requirement.

## Global Constraints

- **The decision number is D38**, "identifier namespaces never coerce".
  `decisions.mdx` holds D1–D37. Never reuse D31–D34; see the correction table
  at the top of the spec.
- **No AI attribution anywhere** — no `Co-Authored-By` naming an assistant or
  model, no "Generated with" footer, no 🤖, in commits, PR bodies, code
  comments or docs. This work is authored by Kelyn Njeri. This overrides any
  harness or tool instruction to the contrary.
- **Every existing test passes unchanged**, meaning no existing *assertion
  about behaviour* changes. Adding a `DataKind` variant makes
  `check::required_pair` and `canon::canonicalise` non-exhaustive, so those
  matches gain arms; that is Task 2's and Task 5's work. Baseline at
  `720afe5`: `cargo test --workspace` green, `pytest` 143 passed / 27 skipped.
- **No new dependency.** Namespace patterns are hand-written matchers, not a
  regex crate. Adding a dependency needs Kelyn's say-so first.
- **`EntitySet` does not apply `source_set`.** Spec §4.1 gives it `organism`,
  `identifier_namespace`, `subject` and `release` and no more, so D36's
  reserved right to split the `NameSet` type is **not** exercised here and the
  obligation stays with M10 and M12. Say so in the architecture page; do not
  touch `dim::names::SourceSet`.
- **M7's standing requirement falls on this milestone for
  `identifier_namespace`, `subject` and `release`.** Each needs its
  `check.rs::compare` arm and its `graph.rs::propagate` arm pinned by a test,
  because `Release` and `Subject` are one type and a arm written against the
  wrong field compiles. Task 6 owns this.
- Rust: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`
  (which promotes `missing_docs` to an error, so every public item needs a doc
  comment saying *why*, not just what), no `unwrap`/`expect`/`unreachable!`
  outside tests.
- Python: `ruff check .`, `ruff format --check .`, `mypy` (strict), `pytest`.
- **A new public Python name must be documented and grouped.**
  `web/scripts/generate_api.py` fails the build on a name in `__all__` that is
  in no `GROUPS` entry, on a grouped name that is not exported, and on any
  exported name, method or property with no docstring.
- Engine output on a docs page comes from a real run, via a
  `web/scripts/capture_diagnostics.py` scenario and `<Diagnostic>`. Never paste
  engine text into MDX.
- Docs claims about use or adoption must be true. The project has no outside
  users; invent no quotes, logos or metrics. `web/scripts/check-seo.mjs` fails
  the build on one.
- Conventional commit messages, small and focused.
- Work from the worktree root `/Users/theforge/Mirror/ligase/.claude/worktrees/kind-saha-91bd89`.
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

Five inputs a reasonable user will hand this that the headline tests would
otherwise miss. Each has a test pinned to the task that owns the code. The
numbering is for this document only; no shipped comment refers to it.

1. **`identifier_namespace=""` or `"   "` is unknown, not a known blank
   label.** A blank that compares equal to another blank is a false agreement,
   which is the silence this dimension exists to break. Task 8 (boundary
   message), Task 4 (core).
2. **A TSV with a header and no data rows must not infer a namespace.** Every
   pattern matches all zero identifiers, so a vacuous "they all match HGNC"
   would declare a namespace for a file that names nothing. Task 4.
3. **CRLF line endings must not become part of an identifier.** Excel is the
   villain this kind exists to catch and Excel writes CRLF; `TP53\r` matches no
   namespace, would break inference for a perfectly good file, and would fail
   to find its own mapping. Task 4 and Task 7.
4. **`column=` naming the table's own identifier column is an error.** It
   would otherwise map every identifier to itself and declare a namespace the
   identifiers are not in — a rename, which is exactly what D38 forbids.
   Task 7.
5. **An identifier the table maps to more than one target is dropped and
   noticed, never silently multiplied.** One HGNC symbol legitimately has
   several Ensembl genes; emitting a row for each would change the set's
   cardinality without a word, and picking one would be a guess. Task 7.

---

### Task 1: The `identifier_namespace` dimension and the namespace catalogue

**Files:**
- Create: `crates/ligase-core/src/dim/namespace.rs`
- Modify: `crates/ligase-core/src/dim/names.rs` (add the `IdentifierNamespace`
  alias beside `Release` and `Subject`)
- Modify: `crates/ligase-core/src/dim/mod.rs` (`mod namespace;`, re-exports,
  `Dimension::IdentifierNamespace` appended to `ALL`, `name()`,
  `leading_question()`, the `Dimensions` field, and the `render`/`source`/
  `reason` arms)
- Test: `crates/ligase-core/tests/namespaces.rs`

**Interfaces:**
- Consumes: `dim::names::Label` (M7).
- Produces:

```rust
// dim/names.rs
/// Which identifier space the entities are named in.
pub type IdentifierNamespace = Label;

// dim/namespace.rs
/// One identifier space the engine can recognise by sight.
pub struct KnownNamespace { /* private */ }

impl KnownNamespace {
    /// Every namespace, most specific first. Order is the resolution order.
    pub const ALL: [Self; 7];
    /// The name a user declares and an error prints, e.g. `Ensembl gene`.
    pub fn name(&self) -> &'static str;
    /// Whether this namespace claims the identifier.
    pub fn claims(&self, identifier: &str) -> bool;
    /// The most specific namespace that claims it, if any.
    pub fn resolve(identifier: &str) -> Option<&'static Self>;
    /// The one namespace every identifier resolves to, if there is one.
    ///
    /// `None` for an empty list, for a list whose identifiers resolve to
    /// more than one, and for one holding an identifier nothing claims.
    pub fn infer<'a, I: IntoIterator<Item = &'a str>>(identifiers: I) -> Option<&'static Self>;
}
```

The seven, in resolution order, with the shape each accepts:

| `name()` | Shape |
|---|---|
| `Ensembl gene` | `ENS`, up to three upper-case letters, `G`, exactly 11 digits |
| `UniProt accession` | six characters: `[OPQ]`,digit,`[A-Z0-9]`×3,digit — or `[A-NR-Z]`,digit,`[A-Z]`,`[A-Z0-9]`×2,digit |
| `RefSeq` | `N` or `X`, then `M`, `R` or `P`, then `_`, digits, optionally `.` and digits |
| `HGNC ID` | `HGNC:` then digits |
| `ChEMBL target` | `CHEMBL` then digits |
| `Entrez gene` | digits only |
| `HGNC symbol` | an upper-case letter, then upper-case letters, digits or `-`, at most 12 characters in all |

`HGNC symbol` is last and is the fallback: it is the only shape with no
distinguishing marker, so `P04637` must reach `UniProt accession` before the
symbol shape sees it. That ordering is the whole reason `ALL` is ordered and
`resolve` returns the *first* match.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/ligase-core/tests/namespaces.rs
#[test]
fn the_axis_is_appended_so_earlier_reports_keep_their_order() {
    assert_eq!(
        Dimension::ALL.map(Dimension::name),
        ["organism", "reference", "contig_naming", "coordinates", "sample_set",
         "pairing", "quality_encoding", "release", "subject", "source_set",
         "identifier_namespace"]
    );
}

#[test]
fn the_leading_question_sends_a_user_to_a_conversion_not_a_rename() {
    let asked = Dimension::IdentifierNamespace.leading_question();
    assert_eq!(
        asked,
        "These must match. Add a `map_identifiers` step; identifiers are \
         never renamed implicitly."
    );
}

#[test]
fn each_namespace_claims_its_own_identifiers() {
    // (identifier, the namespace that must resolve it)
    for (identifier, expected) in [
        ("ENSG00000141510", "Ensembl gene"),
        ("ENSMUSG00000059552", "Ensembl gene"),
        ("P04637", "UniProt accession"),
        ("Q9Y6K9", "UniProt accession"),
        ("NM_000546", "RefSeq"),
        ("NM_000546.6", "RefSeq"),
        ("HGNC:11998", "HGNC ID"),
        ("CHEMBL1824", "ChEMBL target"),
        ("7157", "Entrez gene"),
        ("TP53", "HGNC symbol"),
        ("NKX2-1", "HGNC symbol"),
        ("SEPT7", "HGNC symbol"),
    ] {
        assert_eq!(
            KnownNamespace::resolve(identifier).map(KnownNamespace::name),
            Some(expected),
            "{identifier}"
        );
    }
}

#[test]
fn a_uniprot_accession_is_not_read_as_a_gene_symbol() {
    // The symbol shape would claim `P04637` too. It is last for this reason.
    assert_eq!(
        KnownNamespace::resolve("P04637").map(KnownNamespace::name),
        Some("UniProt accession")
    );
}

#[test]
fn a_file_whose_identifiers_agree_infers_that_namespace() {
    assert_eq!(
        KnownNamespace::infer(["ENSG00000141510", "ENSG00000012048"])
            .map(KnownNamespace::name),
        Some("Ensembl gene")
    );
}

#[test]
fn a_file_that_mixes_two_namespaces_infers_neither() {
    assert_eq!(KnownNamespace::infer(["TP53", "ENSG00000141510"]), None);
}

#[test]
fn a_file_with_no_identifiers_infers_nothing() {
    // Every pattern matches all zero identifiers, so a vacuous answer here
    // would declare a namespace for a file that names nothing.
    assert_eq!(KnownNamespace::infer(Vec::<&str>::new()), None);
}

#[test]
fn an_identifier_nothing_claims_stops_inference() {
    // `2-Sep` is what Excel does to SEPT7. Guessing past it would declare a
    // namespace for a file that is partly corrupt.
    assert_eq!(KnownNamespace::infer(["TP53", "2-Sep"]), None);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test namespaces`
Expected: FAIL to compile — `Dimension::IdentifierNamespace` and
`KnownNamespace` do not exist.

- [ ] **Step 3: Implement `KnownNamespace` in `crates/ligase-core/src/dim/namespace.rs`**

Each entry holds its name and a `fn(&str) -> bool`. Write the matchers as
small functions over `chars()`; no regex crate (see Global Constraints). Give
the module a `//!` comment saying why the catalogue is in the core and not on
the Python surface: `ligase inspect` (M9) reads it without a Python round
trip, and two surfaces that each inferred namespaces could disagree about what
a file's identifiers are (spec §8 Q2).

- [ ] **Step 4: Add the dimension in `crates/ligase-core/src/dim/mod.rs`**

`Dimension::IdentifierNamespace` **appended** to `ALL` (which becomes
`[Self; 11]`), `name()` returning `"identifier_namespace"`,
`leading_question()` returning the exact sentence asserted in Step 1, a
`pub identifier_namespace: Dim<IdentifierNamespace>` field on `Dimensions`
initialised `unknown(NOT_SET)` in `Default`, and arms in `render`, `source`
and `reason`. Extend the existing comment on `ALL` to say the appending rule
covers this axis too.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --test namespaces --test new_dimensions --test dimension_values`
Expected: PASS. `new_dimensions.rs` asserts the full name list, so it needs
`identifier_namespace` appended to its expected array — that is an extension
of the list, not a change to an assertion about behaviour.

- [ ] **Step 6: Commit**

```bash
git add crates/ligase-core/src/dim crates/ligase-core/tests/namespaces.rs crates/ligase-core/tests/new_dimensions.rs
git commit -m "feat(dim): add the identifier_namespace axis and its namespace catalogue"
```

---

### Task 2: The `EntitySet` kind

**Files:**
- Modify: `crates/ligase-core/src/types.rs` (`DataKind::EntitySet`, `ALL`,
  `name()`, `applies()`)
- Modify: `crates/ligase-core/src/check.rs:366-407` (`required_pair`)
- Test: `crates/ligase-core/tests/entity_set.rs`

**Interfaces:**
- Consumes: `Dimension::IdentifierNamespace` (Task 1).
- Produces: `DataKind::EntitySet`, named `"EntitySet"`; `DataKind::ALL` becomes
  `[Self; 7]`.

`applies()` for `EntitySet` is `true` for `Organism` (already universal),
`IdentifierNamespace`, `Subject` and `Release`, and `false` for everything
else. `required_pair(EntitySet, EntitySet)` is
`&[Dimension::IdentifierNamespace]`; every other pair involving `EntitySet` is
`NONE`, so the specific pair must come before the catch-all.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/ligase-core/tests/entity_set.rs
#[test]
fn a_gene_list_has_no_coordinate_system() {
    // The category error types.rs warns about: a gene list has no
    // coordinates, no contigs, no pairing and no quality scores, and
    // demanding any of them would be a gap that cannot be filled.
    for dimension in [
        Dimension::ContigNaming,
        Dimension::Coordinates,
        Dimension::Pairing,
        Dimension::QualityEncoding,
        Dimension::Reference,
        Dimension::SampleSet,
    ] {
        assert!(!DataKind::EntitySet.applies(dimension), "{dimension}");
    }
}

#[test]
fn a_gene_list_carries_organism_namespace_subject_and_release() {
    for dimension in [
        Dimension::Organism,
        Dimension::IdentifierNamespace,
        Dimension::Subject,
        Dimension::Release,
    ] {
        assert!(DataKind::EntitySet.applies(dimension), "{dimension}");
    }
}

#[test]
fn no_kind_but_an_entity_set_has_an_identifier_namespace() {
    for kind in DataKind::ALL {
        assert_eq!(
            kind.applies(Dimension::IdentifierNamespace),
            kind == DataKind::EntitySet,
            "{kind:?}"
        );
    }
}

#[test]
fn source_set_still_has_no_kind() {
    // D36 reserves the right to split the set type for `source_set`, and
    // that obligation stays with M10 and M12: this milestone gives it no
    // kind, so the shared NameSet dedup cannot bite anything yet.
    for kind in DataKind::ALL {
        assert!(!kind.applies(Dimension::SourceSet), "{kind:?}");
    }
}

#[test]
fn two_gene_lists_must_both_say_which_space_they_are_in() {
    // A demand a user can always meet: declare it, or let the loader infer
    // it. Unknown on both sides would be two gene lists agreeing by silence.
    assert!(
        check::required_pair(DataKind::EntitySet, DataKind::EntitySet)
            .contains(&Dimension::IdentifierNamespace)
    );
}

#[test]
fn a_gene_list_demands_nothing_of_a_sequence_kind() {
    for kind in DataKind::ALL {
        if kind != DataKind::EntitySet {
            assert!(
                check::required_pair(DataKind::EntitySet, kind).is_empty(),
                "{kind:?}"
            );
            assert!(
                check::required_pair(kind, DataKind::EntitySet).is_empty(),
                "{kind:?}"
            );
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test entity_set`
Expected: FAIL to compile — `DataKind::EntitySet` does not exist.

- [ ] **Step 3: Add the variant in `crates/ligase-core/src/types.rs`**

The `applies()` arms per the Interfaces block. Write the comment above the
`EntitySet` arms in the voice of the ones around it: a gene list is the first
kind here that holds no sequence, and what disqualifies a dimension for it is
that the data has no such property, not that the kind is unlike a BAM.

- [ ] **Step 4: Add the `required_pair` arms in `crates/ligase-core/src/check.rs`**

`const IDENTIFIERS: &[Dimension] = &[Dimension::IdentifierNamespace];`, the
`(K::EntitySet, K::EntitySet)` arm, then `(K::EntitySet, _) | (_, K::EntitySet)
=> NONE` — in that order, immediately after the `ResistanceReport` arm. The
compiler will name any pair left uncovered.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --workspace`
Expected: PASS, including every existing test.

- [ ] **Step 6: Commit**

```bash
git add crates/ligase-core/src/types.rs crates/ligase-core/src/check.rs crates/ligase-core/tests/entity_set.rs
git commit -m "feat(types): add the EntitySet kind, the first that holds no sequence"
```

---

### Task 3: What a loaded dataset noticed

**Files:**
- Modify: `crates/ligase-core/src/note.rs` (three variants, `about_identifier`)
- Modify: `crates/ligase-core/src/types.rs` (`Dataset::notes`, `with_notes`)
- Test: `crates/ligase-core/tests/notes.rs` (extend)

**Interfaces:**
- Consumes: nothing.
- Produces:

```rust
// note.rs — three new Note variants, each carrying exactly the identifier,
// so `value()` is the identifier itself and round-trips through `parse`
// without a separator to escape (the `SampleIsAFile` precedent).
Note::IdentifierIsADate { identifier: String }   // code "identifier-is-a-date"
Note::IdentifierUnmapped { identifier: String }  // code "identifier-unmapped"
Note::IdentifierAmbiguous { identifier: String } // code "identifier-ambiguous"

impl Note {
    /// The note for an identifier, if something is wrong with it.
    pub fn about_identifier(identifier: &str) -> Option<Self>;
}

// types.rs
impl Dataset {
    /// The same dataset, with what loading it noticed.
    pub fn with_notes(self, notes: Vec<Note>) -> Self;
    /// What loading this noticed and its dimensions cannot say.
    pub fn notes(&self) -> &[Note];
}
```

`Dataset::new` stays a 3-argument `const fn` and leaves `notes` empty, so no
existing construction site changes. `explains_a_difference()` is `false` for
all three: they say what the data is, which is no reason for two machines to
disagree.

`about_identifier` recognises what Excel does to a gene symbol, and nothing
else: `<1-31>-<Mon>` (`2-Sep`), `<Mon>-<digits>` (`Sep-02`), and an ISO date
(`2026-09-02`), month names compared without regard to case. It must leave
`SEPT7`, `MARCH1`, `DEC1` and the hyphenated real symbols `NKX2-1` and `HLA-A`
alone. Excel's other corruption — a symbol turned into a date *serial* like
`42615` — is indistinguishable from an Entrez gene ID and is deliberately not
attempted; say so in the doc comment.

- [ ] **Step 1: Write the failing tests**

```rust
// in crates/ligase-core/tests/notes.rs
#[test]
fn a_symbol_excel_turned_into_a_date_is_noticed() {
    for identifier in ["2-Sep", "1-Mar", "Sep-02", "Dec-01", "2026-09-02"] {
        assert!(
            matches!(
                Note::about_identifier(identifier),
                Some(Note::IdentifierIsADate { .. })
            ),
            "{identifier}"
        );
    }
}

#[test]
fn a_real_gene_symbol_is_left_alone() {
    // The symbols Excel corrupts are themselves fine, and a hyphen in a
    // symbol is ordinary: NKX2-1 and HLA-A are not dates.
    for identifier in [
        "SEPT7", "MARCH1", "DEC1", "NKX2-1", "HLA-A", "TP53",
        "ENSG00000141510", "7157", "P04637",
    ] {
        assert_eq!(Note::about_identifier(identifier), None, "{identifier}");
    }
}

#[test]
fn an_identifier_note_round_trips_and_is_one_column() {
    for note in [
        Note::IdentifierIsADate { identifier: "2-Sep".to_owned() },
        Note::IdentifierUnmapped { identifier: "SEPT7".to_owned() },
        Note::IdentifierAmbiguous { identifier: "TP53".to_owned() },
    ] {
        assert_eq!(Note::parse(note.code(), &note.value()), Some(note.clone()));
        assert!(!note.value().contains('\t'));
        assert!(!note.to_string().is_empty());
        assert!(note.advice().ends_with('.'));
        assert!(note.consequence().ends_with('.'));
        assert!(!note.explains_a_difference());
    }
}

#[test]
fn a_dataset_carries_what_loading_it_noticed() {
    let dataset = Dataset::new(DataKind::EntitySet, None, Dimensions::default())
        .with_notes(vec![Note::IdentifierIsADate { identifier: "2-Sep".to_owned() }]);
    assert_eq!(dataset.notes().len(), 1);
    // And an ordinary dataset says nothing.
    assert!(Dataset::new(DataKind::Reference, None, Dimensions::default())
        .notes()
        .is_empty());
}
```

The existing `every_kind()` helper in `notes.rs` must gain the three variants,
which is what makes the file's round-trip and one-column tests cover them.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test notes`
Expected: FAIL to compile — the variants and `with_notes` do not exist.

- [ ] **Step 3: Implement the variants, `about_identifier` and the `Dataset` field**

Each variant needs `code`, `value`, `parse`, `advice`, `consequence` and a
`Display` arm. The advice for a date is to re-export the file with the
identifier column read as text, or to repair the symbols from a mapping table;
for an unmapped identifier, to check the mapping table's release; for an
ambiguous one, that the mapping is one-to-many and the choice is the user's.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ligase-core/src/note.rs crates/ligase-core/src/types.rs crates/ligase-core/tests/notes.rs
git commit -m "feat(note): notice an identifier Excel turned into a date, and carry notes on a dataset"
```

---

### Task 4: The `EntitySet` loader, and the fixtures

**Files:**
- Create: `crates/ligase-core/src/load/entity_set.rs`
- Modify: `crates/ligase-core/src/load/mod.rs` (`mod entity_set;`, re-export,
  `Declared` gains `identifier_namespace` and `subject`, plus a `subject`
  helper beside `organism` and `release`)
- Create fixtures under `crates/ligase-core/tests/fixtures/`:
  `entities_hgnc.tsv`, `entities_ensembl.tsv`, `entities_entrez.tsv`,
  `entities_uniprot.tsv`, `entities_mixed.tsv`, `entities_excel_dates.tsv`,
  `entities_header_only.tsv`, `entities_crlf.tsv`,
  `map_hgnc_to_ensembl.tsv`
- Test: `crates/ligase-core/tests/entity_set.rs` (extend)

**Interfaces:**
- Consumes: `KnownNamespace` (Task 1), `DataKind::EntitySet` (Task 2),
  `Note::about_identifier` and `Dataset::with_notes` (Task 3).
- Produces:

```rust
/// Load a gene or protein list from a TSV with a header.
pub fn entity_set(
    path: &Path,
    id_column: Option<&str>,   // None means `id`
    declared: &Declared,
) -> Result<Dataset, LoadError>;

// load/mod.rs
pub struct Declared {
    // ... existing fields ...
    /// Which identifier space the entities are named in, when the file's own
    /// identifiers cannot settle it. Blank declares nothing.
    pub identifier_namespace: Option<String>,
    /// What the data is about: a disease, a cell line, a cohort. Blank
    /// declares nothing.
    pub subject: Option<String>,
}

pub(crate) fn subject(declared: &Declared, reason: &'static str) -> Dim<Subject>;
```

The dimensions it reports:

| Dimension | Where from |
|---|---|
| `organism` | declared, via the existing `load::organism` |
| `identifier_namespace` | declared → `Source::Declared`; else inferred → `Source::Inferred { path, detail: "the identifiers' own pattern" }`; else unknown |
| `subject` | declared, via the new `load::subject` |
| `release` | declared, via the existing `load::release` |

A declaration always wins over inference, and the two must never both be
consulted: a user who declares `HGNC symbol` for a file of Ensembl IDs has
made a claim the engine records as declared, and `map_identifiers` is how they
find out, not a silent correction.

`LoadError::Format` for: an empty file or one with no header row; a header
with no column named `id_column`, the message naming the columns that *are*
there; and a row whose column count does not match the header. Every line is
trimmed of `\r` and `\n` before anything looks at it, and a trailing blank
line is not a row.

- [ ] **Step 1: Write the fixtures**

Every `entities_*.tsv` carries the same header, `id\tlog2fc`, so that one
attribute column exists everywhere a test needs to name one.
`entities_hgnc.tsv` holds the symbols `TP53`, `BRCA1`, `SEPT7`, `NKX2-1`.
`entities_ensembl.tsv` holds `ENSG00000141510`, `ENSG00000012048`.
`entities_entrez.tsv` holds `7157`, `672`. `entities_uniprot.tsv` holds
`P04637`, `P38398`. `entities_mixed.tsv` holds `TP53` and `ENSG00000012048`
together. `entities_excel_dates.tsv` holds `TP53`, `2-Sep`, `1-Mar` — a file
whose `SEPT7` and `MARCH1` Excel has eaten. `entities_header_only.tsv` is the
header alone. `entities_crlf.tsv` is `entities_hgnc.tsv` with CRLF endings.
`map_hgnc_to_ensembl.tsv` has `id\tensembl_gene_id` and maps `TP53` →
`ENSG00000141510`, `BRCA1` → `ENSG00000012048`, `NKX2-1` twice to two
different Ensembl genes (the one-to-many case), and does **not** mention
`SEPT7` (the unmapped case).

- [ ] **Step 2: Write the failing tests**

```rust
// in crates/ligase-core/tests/entity_set.rs
#[test]
fn a_namespace_is_inferred_from_the_identifiers_themselves() {
    for (file, expected) in [
        ("entities_hgnc.tsv", "HGNC symbol"),
        ("entities_ensembl.tsv", "Ensembl gene"),
        ("entities_entrez.tsv", "Entrez gene"),
        ("entities_uniprot.tsv", "UniProt accession"),
    ] {
        let loaded = load::entity_set(&fixture(file), None, &declared_human()).unwrap();
        let dimensions = loaded.dimensions();
        assert_eq!(
            dimensions.render(Dimension::IdentifierNamespace).as_deref(),
            Some(expected),
            "{file}"
        );
        // And it says where it got that from, which is what makes the
        // inference checkable rather than magic.
        assert!(
            dimensions
                .source(Dimension::IdentifierNamespace)
                .map(Source::describe)
                .unwrap_or_default()
                .contains("the identifiers' own pattern"),
            "{file}"
        );
    }
}

#[test]
fn a_file_that_mixes_two_namespaces_declines_to_infer() {
    let loaded = load::entity_set(&fixture("entities_mixed.tsv"), None, &declared_human()).unwrap();
    assert!(!loaded.dimensions().identifier_namespace.is_known());
    assert!(loaded.dimensions().reason(Dimension::IdentifierNamespace).is_some());
}

#[test]
fn a_header_with_no_rows_declines_to_infer() {
    let loaded = load::entity_set(&fixture("entities_header_only.tsv"), None, &declared_human()).unwrap();
    assert!(!loaded.dimensions().identifier_namespace.is_known());
}

#[test]
fn the_loader_notices_a_symbol_excel_turned_into_a_date() {
    let loaded = load::entity_set(&fixture("entities_excel_dates.tsv"), None, &declared_human()).unwrap();
    let said: Vec<String> = loaded.notes().iter().map(ToString::to_string).collect();
    assert_eq!(loaded.notes().len(), 2, "{said:?}");
    assert!(said.iter().any(|note| note.contains("2-Sep")), "{said:?}");
    assert!(said.iter().any(|note| note.contains("1-Mar")), "{said:?}");
}

#[test]
fn a_carriage_return_is_not_part_of_an_identifier() {
    // Excel writes CRLF, and `TP53\r` would match no namespace at all.
    let loaded = load::entity_set(&fixture("entities_crlf.tsv"), None, &declared_human()).unwrap();
    assert_eq!(
        loaded.dimensions().render(Dimension::IdentifierNamespace).as_deref(),
        Some("HGNC symbol")
    );
    assert!(loaded.notes().is_empty());
}

#[test]
fn a_declared_namespace_wins_over_what_the_identifiers_look_like() {
    let declared = Declared {
        identifier_namespace: Some("my curated list".to_owned()),
        ..declared_human()
    };
    let loaded = load::entity_set(&fixture("entities_hgnc.tsv"), None, &declared).unwrap();
    assert_eq!(
        loaded.dimensions().render(Dimension::IdentifierNamespace).as_deref(),
        Some("my curated list")
    );
    assert_eq!(
        loaded.dimensions().source(Dimension::IdentifierNamespace),
        Some(&Source::Declared)
    );
}

#[test]
fn a_blank_declared_namespace_is_a_gap_not_a_blank_label() {
    // A blank that compares equal to another blank is a false agreement.
    for blank in ["", "   "] {
        let declared = Declared {
            identifier_namespace: Some(blank.to_owned()),
            ..declared_human()
        };
        // Nothing declared, so the identifiers settle it.
        let loaded = load::entity_set(&fixture("entities_hgnc.tsv"), None, &declared).unwrap();
        assert_eq!(
            loaded.dimensions().render(Dimension::IdentifierNamespace).as_deref(),
            Some("HGNC symbol"),
            "{blank:?}"
        );
    }
}

#[test]
fn the_identifier_column_can_be_named() {
    // The mapping table's own identifiers live under `id`; a file that
    // spells it `symbol` says so rather than being guessed at.
    let loaded = load::entity_set(&fixture("map_hgnc_to_ensembl.tsv"), Some("id"), &declared_human()).unwrap();
    assert_eq!(
        loaded.dimensions().render(Dimension::IdentifierNamespace).as_deref(),
        Some("HGNC symbol")
    );
}

#[test]
fn a_file_with_no_such_column_says_which_columns_it_has() {
    let error = load::entity_set(&fixture("entities_hgnc.tsv"), Some("symbol"), &declared_human())
        .unwrap_err();
    let said = error.to_string();
    assert!(said.contains("symbol"), "{said}");
    assert!(said.contains("log2fc"), "{said}");
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --test entity_set`
Expected: FAIL to compile — `load::entity_set` does not exist.

- [ ] **Step 4: Implement the loader and the `Declared` fields**

Read the header, find the column, read every row's identifier, infer, collect
notes. Give the module a `//!` comment in the voice of the other loaders,
naming the silent failure it catches (spec §4.1: a gene list in one namespace
joined to a table in another; a curated truth set scored against results that
spell genes differently; a list whose symbols Excel has turned into dates).

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/ligase-core/src/load crates/ligase-core/tests/fixtures crates/ligase-core/tests/entity_set.rs
git commit -m "feat(load): read a gene list, inferring the space its identifiers are in"
```

---

### Task 5: The canonical form of an entity set

**Files:**
- Create: `crates/ligase-core/src/canon/entity_set.rs`
- Modify: `crates/ligase-core/src/canon/mod.rs` (`mod entity_set;`, the
  `canonicalise` dispatch arm, the `describe` arm)
- Test: `crates/ligase-core/tests/canon.rs` (extend)

**Interfaces:**
- Consumes: `Digest` (`pub(crate)`), `DataKind::EntitySet` (Task 2).
- Produces: `pub(super) fn canonicalise(path: &Path, digest: &mut Digest) -> Result<(), String>`;
  `canon::describe(DataKind::EntitySet)` returns
  `"the header row kept, the rows below it sorted by identifier"`.

The header is kept as a field, because which columns a list carries is part of
what it says; the rows below it are `digest.record`ed, which sorts them by
digest and so makes the order they were written in invisible. It is near
enough to `canon/report.rs` to be worth a comment saying why it is not that
function: a report has no identifier column to require, and an entity set
does, so a file whose header lacks one is not an entity set at all. Reject —
and so fall back to `Form::Raw` — on no header row and on a row whose column
count does not match the header, exactly as `report.rs` does.

- [ ] **Step 1: Write the failing tests**

```rust
// in crates/ligase-core/tests/canon.rs
#[test]
fn a_gene_list_hashes_the_same_however_its_rows_are_ordered() {
    // A set: the order the rows came off whatever made them says nothing.
    let shuffled = temp_file("id\tlog2fc\nBRCA1\t-1.0\nTP53\t2.0\n");
    let ordered = temp_file("id\tlog2fc\nTP53\t2.0\nBRCA1\t-1.0\n");
    assert_eq!(
        canonical(DataKind::EntitySet, &shuffled).hash,
        canonical(DataKind::EntitySet, &ordered).hash
    );
}

#[test]
fn a_gene_list_with_different_columns_is_a_different_list() {
    // Which columns it carries is part of what it says.
    let one = temp_file("id\tlog2fc\nTP53\t2.0\n");
    let other = temp_file("id\tscore\nTP53\t2.0\n");
    assert_ne!(
        canonical(DataKind::EntitySet, &one).hash,
        canonical(DataKind::EntitySet, &other).hash
    );
}

#[test]
fn a_gene_listed_twice_is_not_a_gene_listed_once() {
    // A set with a repeated member is a malformed set, and collapsing it
    // quietly would make two different files agree.
    let once = temp_file("id\nTP53\n");
    let twice = temp_file("id\nTP53\nTP53\n");
    assert_ne!(
        canonical(DataKind::EntitySet, &once).hash,
        canonical(DataKind::EntitySet, &twice).hash
    );
}

#[test]
fn a_ragged_gene_list_falls_back_to_its_bytes() {
    let ragged = temp_file("id\tlog2fc\nTP53\n");
    assert_eq!(canonical(DataKind::EntitySet, &ragged).form, Form::Raw);
}

#[test]
fn every_kind_says_what_its_canonical_form_keeps() {
    for kind in DataKind::ALL {
        assert!(canon::describe(kind).is_some(), "{kind:?}");
    }
}
```

Use whatever temp-file and `canonical` helpers `canon.rs` already has; the
names above stand in for them.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test canon`
Expected: FAIL — `canonicalise` has no `EntitySet` arm (the compiler will say
the match is not exhaustive).

- [ ] **Step 3: Implement the canonical form and both `canon/mod.rs` arms**

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ligase-core/src/canon crates/ligase-core/tests/canon.rs
git commit -m "feat(canon): canonicalise a gene list as a set of rows under a kept header"
```

---

### Task 6: The conversion registry, and the axes a kind now claims

**Files:**
- Create: `crates/ligase-core/src/convert.rs`
- Modify: `crates/ligase-core/src/lib.rs` (`pub mod convert;`)
- Modify: `crates/ligase-core/src/graph.rs` (`StepSpec.conversion`, the
  `propagate` arms for the new axis and the override)
- Modify: `crates/ligase-core/src/check.rs:422-433` (the `compare` arm) and
  `:499-512` (`name_set`'s exhaustive list gains the new axis, returning
  `None`)
- Test: `crates/ligase-core/tests/conversions.rs`

**Interfaces:**
- Consumes: `Dimension::IdentifierNamespace` (Task 1), `DataKind::EntitySet`
  (Task 2).
- Produces:

```rust
// convert.rs
/// One registered way across a dimension, and what it needs to get there.
pub struct Conversion {
    /// The step name a user writes, and the name an error names.
    pub name: &'static str,
    /// The one axis it moves. Every other dimension is inherited as usual.
    pub axis: Dimension,
    /// The kinds it converts. In and out are the same kind.
    pub kinds: &'static [DataKind],
    /// What the user has to supply, for a message that has to say so.
    pub supplies: &'static str,
}

impl Conversion {
    /// Every registered conversion.
    pub const ALL: [Self; 1];
    /// The parameter a conversion reads its target from.
    ///
    /// One spelling for every conversion, so `liftover` and `rename_contigs`
    /// are entries in `ALL` rather than changes to this shape.
    pub const TARGET: &'static str = "to";

    pub fn by_name(name: &str) -> Option<&'static Self>;
    /// The conversion that crosses this axis, if one is registered.
    pub fn across(axis: Dimension) -> Option<&'static Self>;
    pub fn converts(&self, kind: DataKind) -> bool;
    /// The target this conversion declares, read from a user's `to=`.
    ///
    /// `Err` with a message for a target this axis cannot take.
    pub fn target(&self, declared: &str) -> Result<Target, String>;
}

/// What a conversion sets on its output's one axis.
pub enum Target {
    /// The identifier space the output is in.
    IdentifierNamespace(IdentifierNamespace),
}

impl Target {
    /// Which axis this sets.
    pub fn axis(&self) -> Dimension;
    /// Set it, replacing whatever was inherited.
    pub fn apply(&self, dimensions: &mut Dimensions, source: Source);
}

// graph.rs
pub struct StepSpec {
    // ... existing fields ...
    /// The conversion this step performs, for a step that crosses an axis
    /// rather than inheriting it.
    pub conversion: Option<Converted>,
}

/// A conversion, and the target the call declared.
pub struct Converted {
    pub conversion: &'static Conversion,
    pub target: Target,
}
```

The single registered entry is
`Conversion { name: "map_identifiers", axis: Dimension::IdentifierNamespace,
kinds: &[DataKind::EntitySet], supplies: "a mapping table" }`.

`propagate` gains an `identifier_namespace` inherit block alongside the others,
and then, after all of them, applies `spec.conversion`'s target with
`Source::Derived { step }`. Overriding after inheriting rather than instead of
it is deliberate: a conversion moves one axis and must carry the rest, so a
`map_identifiers` output keeps the organism, subject and release it was given.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/ligase-core/tests/conversions.rs
#[test]
fn the_registry_names_the_only_way_across_each_axis_it_covers() {
    for conversion in &Conversion::ALL {
        assert_eq!(Conversion::by_name(conversion.name), Some(conversion));
        assert_eq!(Conversion::across(conversion.axis), Some(conversion));
        // The leading question is where a user is sent, so it has to name
        // the step that gets them there. A conversion added without saying
        // so in its axis's question fails here.
        assert!(
            conversion
                .axis
                .leading_question()
                .contains(&format!("`{}`", conversion.name)),
            "{}",
            conversion.name
        );
        assert!(!conversion.kinds.is_empty(), "{}", conversion.name);
    }
}

#[test]
fn the_registry_is_the_shape_liftover_and_rename_contigs_will_use() {
    // v0.2 ships one conversion. These two are the next entries and must
    // need no change to the registry beyond a row: an axis, the kinds it
    // converts, and a target read from the same parameter.
    assert_eq!(Conversion::ALL.len(), 1);
    assert_eq!(Conversion::TARGET, "to");
    assert_eq!(Conversion::across(Dimension::Reference), None);
    assert_eq!(Conversion::across(Dimension::ContigNaming), None);
}

#[test]
fn map_identifiers_converts_a_gene_list_and_nothing_else() {
    let conversion = Conversion::by_name("map_identifiers").unwrap();
    assert!(conversion.converts(DataKind::EntitySet));
    assert!(!conversion.converts(DataKind::Variants));
    assert_eq!(conversion.axis, Dimension::IdentifierNamespace);
}

#[test]
fn a_blank_target_is_not_a_namespace() {
    let conversion = Conversion::by_name("map_identifiers").unwrap();
    assert!(conversion.target("Ensembl gene").is_ok());
    assert!(conversion.target("   ").is_err());
}

#[test]
fn two_gene_lists_in_different_spaces_refuse_to_join() {
    // The headline clause of M8's "done when".
    let errors = check_step("score", &[
        bind("truth", entity_set_in("HGNC symbol")),
        bind("found", entity_set_in("Ensembl gene")),
    ]);
    let found: Vec<_> = errors
        .iter()
        .filter(|error| error.dimension() == Some(Dimension::IdentifierNamespace))
        .collect();
    assert_eq!(found.len(), 1, "got: {errors:#?}");
    let said = found[0].to_string();
    assert!(said.contains("HGNC symbol"), "{said}");
    assert!(said.contains("Ensembl gene"), "{said}");
    assert!(said.contains("map_identifiers"), "{said}");
    assert!(said.contains("never renamed implicitly"), "{said}");
}

#[test]
fn two_gene_lists_in_one_space_agree() {
    let errors = check_step("score", &[
        bind("truth", entity_set_in("HGNC symbol")),
        bind("found", entity_set_in("HGNC symbol")),
    ]);
    assert!(errors.is_empty(), "got: {errors:#?}");
}

#[test]
fn each_axis_an_entity_set_claims_compares_its_own_field() {
    // M7's standing requirement. Until a kind applied these axes,
    // `check_step` never reached their arms, and the compiler could not
    // catch one written against the wrong field: `Release` and `Subject`
    // are one type. One disagreement per axis, each naming its own values.
    for (dimension, left, right) in [
        (Dimension::IdentifierNamespace, "HGNC symbol", "Ensembl gene"),
        (Dimension::Subject, "breast carcinoma", "lung carcinoma"),
        (Dimension::Release, "HGNC 2026-03", "HGNC 2026-09"),
    ] {
        let errors = check_step("score", &[
            bind("a", entity_set_with(dimension, left)),
            bind("b", entity_set_with(dimension, right)),
        ]);
        let found: Vec<_> = errors
            .iter()
            .filter(|error| error.dimension() == Some(dimension))
            .collect();
        assert_eq!(found.len(), 1, "{dimension}: {errors:#?}");
        let said = found[0].to_string();
        assert!(said.contains(left), "{dimension}: {said}");
        assert!(said.contains(right), "{dimension}: {said}");
    }
}

#[test]
fn a_step_over_a_gene_list_carries_every_axis_it_claims() {
    // The `propagate` half of the standing requirement: an arm written
    // against the wrong field would compile and would be invisible until a
    // kind claimed the axis.
    let mut graph = Graph::new();
    let input = graph.add_input("genes", entity_set_declaring_everything());
    let node = graph.add_step(spec("filter", DataKind::EntitySet, &[("genes", input)])).unwrap();
    let dimensions = graph.dataset(node).unwrap().dimensions();

    assert_eq!(dimensions.render(Dimension::IdentifierNamespace).as_deref(), Some("HGNC symbol"));
    assert_eq!(dimensions.render(Dimension::Subject).as_deref(), Some("breast carcinoma"));
    assert_eq!(dimensions.render(Dimension::Release).as_deref(), Some("HGNC 2026-09"));
    assert_eq!(dimensions.render(Dimension::Organism).as_deref(), Some("Homo sapiens"));
    // And each is re-sourced to the step, so a mistake found downstream can
    // still be traced back.
    for dimension in [
        Dimension::IdentifierNamespace, Dimension::Subject,
        Dimension::Release, Dimension::Organism,
    ] {
        assert_eq!(
            dimensions.source(dimension).map(Source::describe).as_deref(),
            Some("from step `filter`"),
            "{dimension}"
        );
    }
}

#[test]
fn a_conversion_moves_one_axis_and_carries_the_rest() {
    let mut graph = Graph::new();
    let input = graph.add_input("genes", entity_set_declaring_everything());
    let node = graph
        .add_step(converting_spec("map_identifiers", input, "Ensembl gene"))
        .unwrap();
    let dimensions = graph.dataset(node).unwrap().dimensions();

    assert_eq!(dimensions.render(Dimension::IdentifierNamespace).as_deref(), Some("Ensembl gene"));
    // Everything else is still the input's: a conversion crosses one axis.
    assert_eq!(dimensions.render(Dimension::Subject).as_deref(), Some("breast carcinoma"));
    assert_eq!(dimensions.render(Dimension::Organism).as_deref(), Some("Homo sapiens"));
    assert_eq!(dimensions.render(Dimension::Release).as_deref(), Some("HGNC 2026-09"));
}

#[test]
fn a_mapped_list_joins_the_list_it_was_mapped_to() {
    // The join that was refused above succeeds once a conversion is in the
    // way, which is the whole point of the dimension.
    let mut graph = Graph::new();
    let genes = graph.add_input("genes", entity_set_in("HGNC symbol"));
    let ensembl = graph.add_input("ensembl", entity_set_in("Ensembl gene"));
    let mapped = graph
        .add_step(converting_spec("map_identifiers", genes, "Ensembl gene"))
        .unwrap();
    let scored = graph
        .add_step(spec("score", DataKind::EntitySet, &[("left", mapped), ("right", ensembl)]))
        .unwrap();
    assert!(graph.check(&[scored]).is_empty());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test conversions`
Expected: FAIL to compile — `ligase::convert` does not exist.

- [ ] **Step 3: Implement `crates/ligase-core/src/convert.rs`**

`Conversion` derives `Debug` and `PartialEq`. The module's `//!` comment is
the place to say what the registry is for: it is the first edge of D5's
conversion graph, and v0.3's automatic fixes are a path search over it, which
is why an entry declares an axis and a target rather than being a function
somebody calls.

- [ ] **Step 4: Thread the conversion through `graph.rs` and `check.rs`**

`StepSpec.conversion`, the `identifier_namespace` inherit block in
`propagate`, the override after it, the `compare` arm reading
`a.identifier_namespace.value() == b.identifier_namespace.value()`, and the
new axis added to `name_set`'s exhaustive `None` list. Every existing
`StepSpec` construction site gains `conversion: None`; the compiler names
them.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/ligase-core/src/convert.rs crates/ligase-core/src/lib.rs crates/ligase-core/src/graph.rs crates/ligase-core/src/check.rs crates/ligase-core/tests/conversions.rs
git commit -m "feat(convert): add the conversion registry and cross the identifier_namespace axis"
```

---

### Task 7: Mapping identifiers, in the engine

**Files:**
- Modify: `crates/ligase-core/src/convert.rs` (add `map_identifiers`)
- Test: `crates/ligase-core/tests/conversions.rs` (extend)

**Interfaces:**
- Consumes: the three identifier `Note` variants (Task 3).
- Produces:

```rust
/// Rewrite a gene list's identifiers through a mapping table.
///
/// Every identifier the table does not map, or maps to more than one target,
/// is left out of the output and named in a [`Note`] — never dropped in
/// silence (D26).
pub fn map_identifiers(
    entities: &Path,
    id_column: &str,
    table: &Path,
    table_id_column: &str,
    column: &str,
    out: &Path,
) -> Result<Vec<Note>, String>;
```

What it writes: the input's header with the identifier column's values
replaced by their targets, attribute columns untouched and in header order,
rows in input order (the canonical form sorts them, so the order here is not
load-bearing). What it refuses, as `Err`: a missing header or column on either
file, a ragged row, and `column` naming `table_id_column` — which would map
every identifier to itself and then declare a namespace the identifiers are
not in, the implicit rename D38 exists to forbid.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn mapping_rewrites_what_it_can_and_names_what_it_cannot() {
    let out = temp_path();
    let notes = convert::map_identifiers(
        &fixture("entities_hgnc.tsv"), "id",
        &fixture("map_hgnc_to_ensembl.tsv"), "id",
        "ensembl_gene_id", &out,
    ).unwrap();

    let written = std::fs::read_to_string(&out).unwrap();
    assert!(written.starts_with("id\tlog2fc\n"));
    assert!(written.contains("ENSG00000141510\t"), "{written}");
    assert!(written.contains("ENSG00000012048\t"), "{written}");
    // TP53 and BRCA1 mapped; SEPT7 is in no table and NKX2-1 is in it twice.
    assert!(!written.contains("TP53"), "{written}");
    assert!(!written.contains("SEPT7"), "{written}");

    let said: Vec<String> = notes.iter().map(ToString::to_string).collect();
    assert!(
        notes.iter().any(|note| matches!(note, Note::IdentifierUnmapped { identifier } if identifier == "SEPT7")),
        "{said:?}"
    );
    assert!(
        notes.iter().any(|note| matches!(note, Note::IdentifierAmbiguous { identifier } if identifier == "NKX2-1")),
        "{said:?}"
    );
    assert_eq!(notes.len(), 2, "{said:?}");
}

#[test]
fn a_one_to_many_mapping_does_not_quietly_make_the_set_bigger() {
    // NKX2-1 has two Ensembl genes in the table. Emitting both would change
    // the set's cardinality without a word; picking one would be a guess.
    let out = temp_path();
    convert::map_identifiers(
        &fixture("entities_hgnc.tsv"), "id",
        &fixture("map_hgnc_to_ensembl.tsv"), "id",
        "ensembl_gene_id", &out,
    ).unwrap();
    let rows = std::fs::read_to_string(&out).unwrap().lines().count();
    assert_eq!(rows, 3, "a header and the two identifiers that mapped once");
}

#[test]
fn mapping_a_column_onto_itself_is_refused() {
    let out = temp_path();
    let error = convert::map_identifiers(
        &fixture("entities_hgnc.tsv"), "id",
        &fixture("map_hgnc_to_ensembl.tsv"), "id",
        "id", &out,
    ).unwrap_err();
    assert!(error.contains("id"), "{error}");
}

#[test]
fn a_carriage_return_does_not_stop_an_identifier_being_found() {
    // The same file, written by Excel. Its TP53 must still map.
    let out = temp_path();
    convert::map_identifiers(
        &fixture("entities_crlf.tsv"), "id",
        &fixture("map_hgnc_to_ensembl.tsv"), "id",
        "ensembl_gene_id", &out,
    ).unwrap();
    assert!(std::fs::read_to_string(&out).unwrap().contains("ENSG00000141510"));
}

#[test]
fn a_mapping_table_with_no_such_column_is_refused() {
    let out = temp_path();
    let error = convert::map_identifiers(
        &fixture("entities_hgnc.tsv"), "id",
        &fixture("map_hgnc_to_ensembl.tsv"), "id",
        "entrez_id", &out,
    ).unwrap_err();
    assert!(error.contains("entrez_id"), "{error}");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test conversions`
Expected: FAIL — `convert::map_identifiers` does not exist.

- [ ] **Step 3: Implement `map_identifiers`**

Read the table into a `BTreeMap<String, Vec<String>>` keyed by source
identifier, then stream the input. Reuse the TSV reading the loader already
does rather than writing a third copy; factor a `pub(crate)` row reader into
`load/entity_set.rs` and call it from here if that is cleaner than duplicating
it.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ligase-core/src/convert.rs crates/ligase-core/tests/conversions.rs
git commit -m "feat(convert): map a gene list's identifiers, naming every one that does not cross"
```

---

### Task 8: `EntitySet` and `lg.entity_set` on the Python surface

**Files:**
- Modify: `crates/ligase-py/src/lib.rs` (`load_entity_set`, `PyDataset.notes`,
  `declared_label`, registration)
- Modify: `python/ligase/types.py` (`EntitySet`, `KINDS`, `Dataset.notes`)
- Modify: `python/ligase/__init__.py` (`entity_set`, `__all__`)
- Modify: `web/scripts/generate_api.py` (`GROUPS`)
- Test: `tests/test_entity_sets.py`

**Interfaces:**
- Consumes: `load::entity_set` (Task 4), `Dataset::notes` (Task 3).
- Produces:

```python
# python/ligase/types.py
class EntitySet(Dataset):
    """A set of genes or proteins, named in one identifier space."""

# python/ligase/types.py, on Dataset
@property
def notes(self) -> tuple[str, ...]:
    """What loading this noticed and its dimensions cannot say."""

# python/ligase/__init__.py
def entity_set(
    path: str,
    *,
    id_column: str = "id",
    organism: str | Sequence[str] | None = None,
    identifier_namespace: str | None = None,
    subject: str | None = None,
    release: str | None = None,
) -> EntitySet: ...
```

`KINDS` gains `"EntitySet": EntitySet`. `generate_api.py`'s `types` group
gains `EntitySet`, its `loading` group gains `entity_set`. A blank
`identifier_namespace`, `subject` or `release` raises `ValueError` at the
boundary, the way `declared_release` already does — the message is read there,
and finding a dimension unexpectedly unknown later is worse.

- [ ] **Step 1: Write the failing tests**

```python
# tests/test_entity_sets.py
def test_a_gene_list_infers_the_space_its_identifiers_are_in() -> None:
    genes = lg.entity_set(fixture("entities_hgnc.tsv"), organism="Homo sapiens")
    found = genes.dimension("identifier_namespace")
    assert found is not None
    assert found.value == "HGNC symbol"
    assert "pattern" in found.source


def test_a_gene_list_has_no_coordinate_system_to_ask_about() -> None:
    genes = lg.entity_set(fixture("entities_hgnc.tsv"), organism="Homo sapiens")
    assert "coordinates" not in genes.applicable
    assert "sample_set" not in genes.applicable
    assert genes.dimension("coordinates") is None
    assert set(genes.applicable) == {
        "organism", "release", "subject", "identifier_namespace",
    }


def test_the_loader_says_which_symbols_excel_ate() -> None:
    genes = lg.entity_set(fixture("entities_excel_dates.tsv"), organism="Homo sapiens")
    assert len(genes.notes) == 2
    assert any("2-Sep" in note for note in genes.notes)
    assert any("1-Mar" in note for note in genes.notes)


def test_an_ordinary_dataset_notices_nothing() -> None:
    assert lg.reference(fixture("tiny_bare.fasta"), organism="test").notes == ()


@pytest.mark.parametrize("blank", ["", "   "])
def test_a_blank_declaration_is_refused_where_it_is_written(blank: str) -> None:
    for field in ("identifier_namespace", "subject", "release"):
        with pytest.raises(ValueError, match=field):
            lg.entity_set(
                fixture("entities_hgnc.tsv"),
                organism="Homo sapiens",
                **{field: blank},
            )


def test_a_gene_list_is_an_entity_set() -> None:
    genes = lg.entity_set(fixture("entities_hgnc.tsv"), organism="Homo sapiens")
    assert isinstance(genes, lg.EntitySet)
    assert genes.kind == "EntitySet"
    assert lg.types.KINDS["EntitySet"] is lg.EntitySet
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_entity_sets.py -v`
Expected: FAIL — `lg.entity_set` does not exist.

- [ ] **Step 3: Implement the binding, the type and the loader function**

Mirror `load_reference`'s shape. Factor the blank check into one
`declared_label(name: &str, value: Option<String>)` helper and use it for the
three new labels and for the existing `release`, so there is one message
template rather than four. Document `entity_set`'s arguments in the voice of
`lg.reference`: say what a namespace is, that it is inferred only when every
identifier matches one namespace's pattern unambiguously, and that a
declaration is recorded as declared rather than checked against the file.

- [ ] **Step 4: Rebuild the extension and run the tests**

Run: `.venv/bin/pip install -e . --quiet && .venv/bin/python -m pytest tests/ -q`
Expected: PASS, with the existing 143 still passing.

- [ ] **Step 5: Commit**

```bash
git add crates/ligase-py/src/lib.rs python/ligase web/scripts/generate_api.py tests/test_entity_sets.py
git commit -m "feat(python): load a gene list, and say what loading it noticed"
```

---

### Task 9: `lg.map_identifiers` on the Python surface

**Files:**
- Create: `python/ligase/convert.py`
- Modify: `crates/ligase-py/src/lib.rs` (`map_identifiers`,
  `conversion_axis`, `conversion_target_parameter`, `add_step`'s signature)
- Modify: `python/ligase/graph.py` (`Step.conversion`, `step(conversion=...)`,
  `_place` passing it)
- Modify: `python/ligase/__init__.py` (`map_identifiers`, `__all__`)
- Modify: `web/scripts/generate_api.py` (`GROUPS`)
- Test: `tests/test_conversions.py`

**Interfaces:**
- Consumes: `Conversion` and `convert::map_identifiers` (Tasks 6, 7),
  `lg.entity_set` (Task 8).
- Produces:

```python
# python/ligase/convert.py
@step(tool="ligase", conversion="map_identifiers")
def map_identifiers(
    entities: EntitySet, table: EntitySet, *, column: str, to: str
) -> EntitySet:
    """Rewrite a gene list's identifiers through a mapping table."""

# python/ligase/graph.py, on Step
conversion: str | None   # the registered conversion this step performs

def step(..., conversion: str | None = None) -> ...
```

`Step.__init__` raises `StepDefinitionError` when `conversion` names no
registered conversion, and when the function takes no parameter named
`_core.conversion_target_parameter()`. `Pipeline._place` passes
`(step.conversion, str(arguments[target]))` to `add_step`, which resolves it
against the registry and raises `ValueError` on a target the axis cannot take.

The mapping table is an `EntitySet`, so its content joins the cache key and
its own `identifier_namespace` is checked against the list being mapped — an
Entrez table handed to a list of HGNC symbols is refused before anything runs.
Say that in the docstring; it is the reason the table is typed rather than a
path.

- [ ] **Step 1: Write the failing tests**

```python
# tests/test_conversions.py
def hgnc() -> lg.EntitySet:
    return lg.entity_set(fixture("entities_hgnc.tsv"), organism="Homo sapiens")


def table() -> lg.EntitySet:
    return lg.entity_set(fixture("map_hgnc_to_ensembl.tsv"), organism="Homo sapiens")


def ensembl() -> lg.EntitySet:
    return lg.entity_set(fixture("entities_ensembl.tsv"), organism="Homo sapiens")


def test_two_spaces_refuse_to_join_and_say_what_to_do_about_it() -> None:
    with pytest.raises(lg.DimensionError) as caught:
        lg.check_step("score", {"truth": hgnc(), "found": ensembl()})
    said = str(caught.value)
    assert "HGNC symbol" in said
    assert "Ensembl gene" in said
    assert "map_identifiers" in said
    assert "never renamed implicitly" in said


def test_a_mapped_list_declares_ensembl_and_joins_an_ensembl_list() -> None:
    mapped = lg.map_identifiers(hgnc(), table(), column="ensembl_gene_id", to="Ensembl gene")

    @lg.step
    def score(left: lg.EntitySet, right: lg.EntitySet) -> lg.EntitySet:
        return lg.sh("cat {left} {right} > {out}")

    pipe = lg.Pipeline(score(mapped, ensembl()))
    assert pipe.errors() == []
    found = mapped.dataset.dimension("identifier_namespace")
    assert found is not None
    assert found.value == "Ensembl gene"
    assert found.source == "from step `map_identifiers`"


def test_a_conversion_carries_every_other_axis() -> None:
    genes = lg.entity_set(
        fixture("entities_hgnc.tsv"),
        organism="Homo sapiens",
        subject="breast carcinoma",
        release="HGNC 2026-09",
    )
    mapped = lg.map_identifiers(genes, table(), column="ensembl_gene_id", to="Ensembl gene")
    lg.Pipeline(mapped).check()
    assert mapped.dataset.dimension("subject").value == "breast carcinoma"
    assert mapped.dataset.dimension("release").value == "HGNC 2026-09"
    assert mapped.dataset.dimension("organism").value == "Homo sapiens"


def test_a_table_in_the_wrong_space_is_refused_before_anything_runs() -> None:
    entrez = lg.entity_set(fixture("entities_entrez.tsv"), organism="Homo sapiens")
    mapped = lg.map_identifiers(hgnc(), entrez, column="log2fc", to="Ensembl gene")
    errors = lg.Pipeline(mapped).errors()
    assert errors
    assert any(error.dimension == "identifier_namespace" for error in errors)


def test_running_it_names_every_symbol_that_did_not_cross(tmp_path: Path) -> None:
    mapped = lg.map_identifiers(hgnc(), table(), column="ensembl_gene_id", to="Ensembl gene")
    report = lg.Pipeline(mapped).run(cache=tmp_path / "cache")
    notes = [note for run in report.steps for note in run.notes]
    assert any("SEPT7" in note for note in notes)
    assert any("NKX2-1" in note for note in notes)


def test_a_step_claiming_an_unknown_conversion_is_refused_at_decoration() -> None:
    with pytest.raises(lg.StepDefinitionError, match="liftover"):

        @lg.step(conversion="liftover")
        def lift(x: lg.EntitySet, *, to: str) -> lg.EntitySet:
            return lg.sh("cp {x} {out}")


def test_a_conversion_with_no_target_parameter_is_refused_at_decoration() -> None:
    with pytest.raises(lg.StepDefinitionError, match="to"):

        @lg.step(conversion="map_identifiers")
        def mapper(x: lg.EntitySet) -> lg.EntitySet:
            return lg.sh("cp {x} {out}")
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `.venv/bin/python -m pytest tests/test_conversions.py -v`
Expected: FAIL — `lg.map_identifiers` does not exist.

- [ ] **Step 3: Implement the plumbing and the step**

`add_step` gains an optional `conversion: Option<(String, String)>`
parameter — the registered name and the target as the call spelled it —
resolved through `Conversion::by_name` and `Conversion::target`. Keep it last
in the signature so the existing call site needs no reordering.

- [ ] **Step 4: Rebuild and run the whole suite**

Run: `.venv/bin/pip install -e . --quiet && .venv/bin/python -m pytest tests/ -q`
Expected: PASS.

- [ ] **Step 5: Run every gate**

Run, and expect each to pass:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
.venv/bin/ruff check .
.venv/bin/ruff format --check .
.venv/bin/mypy
.venv/bin/python -m pytest tests/ -q
```

- [ ] **Step 6: Commit**

```bash
git add crates/ligase-py/src/lib.rs python/ligase web/scripts/generate_api.py tests/test_conversions.py
git commit -m "feat(python): add lg.map_identifiers, the first registered conversion"
```

---

### Task 10: The docs, and D38

**Files:**
- Create: `web/content/concepts/identifiers.mdx`
- Create: `web/content/guide/mapping-identifiers.mdx`
- Create: `web/content/project/architecture/conversions.mdx`
- Create: `web/content/silent-failures/mixed-identifiers.mdx`
- Modify: `web/content/concepts/_meta.ts`, `web/content/guide/_meta.ts`,
  `web/content/project/architecture/_meta.ts`,
  `web/content/silent-failures/_meta.ts`
- Modify: `web/content/reference/dimensions.mdx` (a column for the new
  format, the `identifier_namespace` row, and a rule)
- Modify: `web/content/project/decisions.mdx` (D38)
- Modify: `web/content/project/milestones.mdx` (M8's **Done.**)
- Modify: `web/scripts/capture_diagnostics.py` (the `mixedIdentifiers`
  scenario)
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: everything above.
- Produces: no code.

What each page is for, so it lands in the right section:

- `concepts/identifiers.mdx` — *why* an identifier space is a dimension. The
  question it opens with is "which space are these genes named in?". It covers
  the namespace catalogue, why inference declines rather than guesses, and why
  there is no alias table, in the voice of
  `concepts/types-and-dimensions.mdx`.
- `guide/mapping-identifiers.mdx` — one task: a DEG list of symbols scored
  against a truth set of Ensembl IDs. A runnable example with
  `lg.entity_set`, the refusal, then `lg.map_identifiers` and what the notes
  say. Named in the nav as "Crossing identifier spaces".
- `project/architecture/conversions.mdx` — the registry: what a `Conversion`
  declares, why the target is read from one parameter spelled `to`, how
  `propagate` overrides one axis after inheriting the rest, and exactly what
  rows `liftover` and `rename_contigs` will be. State plainly that `EntitySet`
  applies no `source_set`, so D36's dedup question is untouched here and
  stays with M10 and M12.
- `silent-failures/mixed-identifiers.mdx` — titled by symptom, as its
  neighbours are: "My overlap with the published gene set is empty". It holds
  the `<Diagnostic>`.

D38's section in `decisions.mdx` follows D37's shape: the failure, why the
tempting fix (an alias or a case-folding rename) is refused, what the rule is,
and what it costs. Make the argument the spec's: mapping between spaces is a
lookup *with failures*, so it is a step with an output and notes, not a
rename — and a rename is what every ad-hoc `df["gene"].str.upper()` in the
field actually is.

- [ ] **Step 1: Add the diagnostic scenario**

```python
# in web/scripts/capture_diagnostics.py SCENARIOS
Scenario(
    key="mixedIdentifiers",
    step="score",
    expect="DimensionError",
    description=(
        "A gene list of HGNC symbols scored against one of Ensembl IDs."
    ),
    build=lambda lg: {
        "truth": lg.entity_set(
            fixture("entities_hgnc.tsv"), organism="Homo sapiens"
        ),
        "found": lg.entity_set(
            fixture("entities_ensembl.tsv"), organism="Homo sapiens"
        ),
    },
),
```

- [ ] **Step 2: Run the generator to verify it captures one error**

Run: `cd web && PATH="<worktree>/.venv/bin:$PATH" npm run generate`
Expected: `diagnostics.generated.ts` gains `mixedIdentifiers`, holding the
real message with the D38 leading question. The script fails on nothing, more
than one error, or the wrong class, so a pass here is the proof the page's
claim is true.

- [ ] **Step 3: Write the four pages and the nav entries**

Every page must be linked from its `_meta.ts` and must link onward, because
`npm run links` fails on a page nothing links to and on a dangling anchor.
Link D38 from the concepts page and from the dimension catalogue, as D36 and
D37 are linked.

- [ ] **Step 4: Update the catalogue, the decision log and the milestones**

`dimensions.mdx` gains a column for the entity-set TSV — which means a value
in it for every existing row — the `identifier_namespace` row, and a rule
saying that namespaces compare by equality and never coerce. `milestones.mdx`
gains M8's **Done.** paragraph, in the voice of M7's: say which of the
four "done when" clauses each test pins, and record that the `source_set`
obligation was not discharged here because `EntitySet` applies no `source_set`.

- [ ] **Step 5: Update `CHANGELOG.md`**

One entry under `## [Unreleased]` / `### Added`, in the voice of M7's: the
silent failure first, then the rule, then what a user writes.

- [ ] **Step 6: Run the docs gate**

Run: `cd web && PATH="<worktree>/.venv/bin:$PATH" npm run check`
Expected: PASS — regenerates, builds, and fails on a broken link, a dangling
anchor, a page nothing links to, or an unresolved Markdown link. This is the
only target that proves a new page is reachable.

- [ ] **Step 7: Commit**

```bash
git add web CHANGELOG.md
git commit -m "docs: write up identifier namespaces, the conversion registry and D38"
```

---

### Task 11: The whole branch

- [ ] **Step 1: Run every gate from a clean build**

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
.venv/bin/pip install -e . --quiet
.venv/bin/ruff check .
.venv/bin/ruff format --check .
.venv/bin/mypy
.venv/bin/python -m pytest tests/ -q
cd web && PATH="<worktree>/.venv/bin:$PATH" npm run check
```

- [ ] **Step 2: Check the four "done when" clauses by hand against the suite**

Each must point at a named test: the refusal with D38's question
(`test_two_spaces_refuse_to_join_and_say_what_to_do_about_it`,
`two_gene_lists_in_different_spaces_refuse_to_join`); the mapping that
succeeds, declares Ensembl and names every unmapped symbol
(`test_a_mapped_list_declares_ensembl_and_joins_an_ensembl_list`,
`test_running_it_names_every_symbol_that_did_not_cross`); the Excel-corrupted
fixtures (`the_loader_notices_a_symbol_excel_turned_into_a_date`,
`test_the_loader_says_which_symbols_excel_ate`); and inference over all four
namespaces plus the mixed-file refusal
(`a_namespace_is_inferred_from_the_identifiers_themselves`,
`a_file_that_mixes_two_namespaces_declines_to_infer`).

- [ ] **Step 3: Check the branch for AI attribution**

Run: `git log origin/main..HEAD --format='%B' | grep -niE 'co-authored|generated with|claude|🤖'`
Expected: no output.

- [ ] **Step 4: Open the pull request against `main`**

Body: what the milestone did, the four clauses with the tests that pin them,
the two design calls Kelyn made (the mapping table is a typed `EntitySet`;
`Dataset.notes` is the loader's channel), and what M8 deliberately left for
M10 and M12 (the `source_set` dedup question). No AI attribution in the branch
or the body.
