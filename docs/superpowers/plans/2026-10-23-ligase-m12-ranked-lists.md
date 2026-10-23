# Ligase M12 — Ranked Lists Implementation Plan

**Goal:** Add the `RankedList` kind — the ranked table — whose canonical form
**preserves order**, because for a ranking the order *is* the output. Catch
the same silent failure an `EntitySet` catches, plus one more: a rank-ordered
result compared against one in a different namespace or from a different
release.

**Architecture:** A `RankedList` is an `EntitySet` that remembers its order,
and that single difference is the whole design. It carries the same four
dimensions — `organism`, `identifier_namespace`, `subject`, `release` — and
returns `false` from `applies()` for `reference`, `contig_naming`,
`coordinates`, `sample_set`, `pairing` and `quality_encoding`, exactly as M8's
`EntitySet` does. It therefore shares M8's **loader**, because reading names
and the space they live in is the same work for both, and gets its **own**
canonicaliser, because that is the one place the two kinds differ. Rank is the
row position; a `score` column is an ordinary attribute column and is compared
exactly, with no tolerance.

**Tech Stack:** Rust (stable, edition 2024), PyO3 0.30 + maturin, Python
3.11+, pytest, mypy strict, ruff, Nextra/MDX docs under `web/`.

**Spec:** `docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md` — §2 (D41),
§4.2 (`RankedList`), §6 (M12). Read alongside `web/content/project/milestones.mdx`'s
M12 entry, which carries M7's standing requirement.

## Global Constraints

- **The decision number is D41**, "in a ranked list, rank order is the
  result". `decisions.mdx` holds D1–D38; M10 and M11 are writing D39 and D40
  in parallel, so D41 appends after them and the file conflicts mechanically.
  Never reuse D31–D34; see the correction table at the top of the spec.
- **No AI attribution anywhere** — no `Co-Authored-By` naming an assistant or
  model, no "Generated with" footer, no 🤖, in commits, PR bodies, code
  comments or docs. This work is authored by Kelyn Njeri. This overrides any
  harness or tool instruction to the contrary.
- **M10 is in flight on `feat/m10-remotes-replay`.** Do not touch
  `crates/ligase-core/src/audit.rs`, `crates/ligase-core/src/prov/`,
  `python/ligase/cli.py`, `python/ligase/provenance.py`,
  `examples/hiv-resistance/pipeline.py` or `Cargo.toml`. Nothing in M12 needs
  them. Three files conflict textually and resolve by appending:
  `web/content/project/decisions.mdx`, `web/content/project/milestones.mdx`
  and `CHANGELOG.md`.
- **No new dependency.** Nothing here needs one.
- **Baseline at `58c10b0`:** `cargo test --workspace` green (27 result lines,
  469 tests), `pytest` 241 passed / 27 skipped. M9 moved the pytest count on
  from the 198 the M12 brief quotes.
- Rust: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`
  (which promotes `missing_docs` to an error, so every public item needs a doc
  comment saying *why*, not just what), no `unwrap`/`expect`/`unreachable!`
  outside tests.
- Python: `ruff check .`, `ruff format --check .`, `mypy` (strict), `pytest`.
- **A new public Python name must be documented and grouped.**
  `web/scripts/generate_api.py` fails the build on a name in `__all__` that is
  in no `GROUPS` entry, on a grouped name that is not exported, and on any
  exported name, method or property with no docstring. `ranked_list` joins the
  `loading` group and `RankedList` the `types` group.
- Engine output on a docs page comes from a real run, via a
  `web/scripts/capture_diagnostics.py` scenario and `<Diagnostic>`. Never
  paste engine text into MDX.
- The docs gate is `cd web && PATH="$PWD/../.venv/bin:$PATH" npm run check`.
  The five generators are invoked as bare `python3`, so the venv has to be on
  PATH, and the venv needs the `docs` extra because `render_notebooks.py`
  imports marimo.

## The three questions this milestone settles

### 1. Does the ordering idea get its own concepts page?

**No — it is a section on `concepts/semantic-hashing.mdx`.** That page already
owns the sorts-versus-keeps-order question: it has a *Why some formats sort
and others do not* section that derives the rule from D14, and a table of what
each type drops and whether its records sort. A ranked list is a third answer
to the question that section already asks, not a new idea, and a page of its
own would restate the framing in order to add one row. What the page gets
instead is the `RankedList` row in its table, an `EntitySet` row beside it
(M8 added the kind and never added the row), and a short subsection under the
existing one saying that a third kind of answer exists and why.

The *internals* of the decision — why two canonicalisers that do the same work
in opposite directions are deliberately two functions — go where internals go,
in `project/architecture/canonicalisation.mdx`.

### 2. Does a `RankedList` claim `source_set`?

**No.** Spec §4.2 defines it as "`EntitySet` but ordered", and §4.1 gives
`EntitySet` four dimensions: `organism`, `identifier_namespace`, `subject` and
`release`. `source_set` is not among them, so D36's reserved right to split
the `NameSet` type is **not** exercised here and the question stays open. The
milestones page's M12 entry says a `RankedList` "brings `source_set` and
`subject` into play"; the first half is wrong against the spec it is
summarising, and this milestone corrects it rather than claiming an axis to
match a sentence. `source_set` passes to whichever milestone first needs it —
M10 by its own entry, or `KnowledgeGraph` in v0.3, which §4.3 does give it.

M7's standing requirement is therefore **already discharged** for every axis a
`RankedList` applies: M8 pinned `identifier_namespace`, `subject` and
`release` in both `check.rs::compare` and `graph.rs::propagate`. This
milestone adds the same two tests for a `RankedList` anyway — not because the
arms are unverified, but because "the kind that applies them" is the thing
under test and a second kind reaching the same arms is cheap to pin.

### 3. Do the two kinds share a loader?

**Yes, and they do not share a canonicaliser.** That is the shape of D41: the
two kinds differ in exactly one thing, so exactly one function differs.

M8's `canon/entity_set.rs` already explains why it is not `canon/report.rs` —
"as the kind grows a ranked sibling (`DataKind::RankedList` is specified to
canonicalise in order, which is the opposite rule over the same bytes)".
Having anticipated this milestone, it would be odd to then share the
canonicaliser; the two are kept apart and each says what it does.

The loader is the other way round. Reading a TSV, finding the identifier
column, inferring the namespace from the identifiers, noticing a symbol a
spreadsheet turned into a date, and reporting the four dimensions is the same
work with the same subtleties — including the `NOT_INFERRED` versus
`DATES_AMONG_THEM` distinction, where the *reason* a dimension is unknown is
advice a user acts on and the two pieces of advice are opposites. Copying that
into a second file would be copying the part that took M8 a correction to get
right. So `load/entity_set.rs` grows a `pub(super)` reader parameterised by
kind and by the three wordings that name the format, and `load/ranked_list.rs`
is the module that states what a ranked list is and calls it.

## Tasks

### Task 1 — The kind

- [ ] `DataKind::RankedList` in `crates/ligase-core/src/types.rs`: variant with
      a doc comment, `ALL` to 8, `name()` → `"RankedList"`, and an `applies()`
      row identical to `EntitySet`'s, sharing its comment about the category
      error.
- [ ] `exec::output_suffix` → `".tsv"`.
- [ ] Existing assertions that say `EntitySet` is the *only* kind with a
      namespace, a subject or (with `Reference`) a release become assertions
      about two kinds: `entity_set.rs::no_kind_but_an_entity_set_has_an_identifier_namespace`,
      `::an_entity_set_is_the_only_kind_with_a_subject`, and
      `new_dimensions.rs::release_applies_to_a_reference_and_to_a_gene_list`.
      `source_set_still_has_no_kind` stays true and stays.

### Task 2 — The canonical form that keeps its order

- [ ] `Digest::ordered_record` in `canon/mod.rs`: the counterpart of
      `record`, hashing a record to 32 bytes and writing it straight into the
      stream where its position is fixed. It exists so that the choice is made
      by *name* at the call site: the two methods take the same arguments and
      mean opposite things, and a canonicaliser that reached for `record` out
      of habit would turn a ranking into a set.
- [ ] `canon/ranked_list.rs`: header as a field, then each row as an
      `ordered_record`. The module comment says **why it does not sort**, in
      as many words, because D41 says someone will otherwise "fix" it.
- [ ] Dispatch arm and `describe()` entry in `canon/mod.rs`; the module's
      *Which formats sort, and which keep their order* section gains the third
      answer.
- [ ] No `tolerance(RankedList, "score")` entry. Pinned by a test, because
      "exact by default" is a claim about what is *absent* and nothing else
      would catch its arrival.

### Task 3 — The loader

- [ ] `load/entity_set.rs`: extract the body into a `pub(super)` reader taking
      the kind and a small struct of the three reason wordings; `entity_set`
      becomes its caller.
- [ ] `load/ranked_list.rs`: the module comment states the silent failure —
      the same one plus the ranked one — and the loader calls the shared
      reader with the ranked wordings. `pub use` it from `load/mod.rs`.
- [ ] `Declared`'s doc comments stop saying "only the entity-set loader reads
      it" where that is no longer true.

### Task 4 — The refusals

- [ ] `check::required_pair`: every pair among `{EntitySet, RankedList}`
      requires `identifier_namespace`. The existing
      `(K::EntitySet, _) | (_, K::EntitySet) => NONE` arm would otherwise
      swallow the mixed pair, which is precisely the join the milestone exists
      to refuse.
- [ ] `release` needs no table change: both kinds apply it, so `compare_known`
      reaches it wherever both sides state it, and it is deliberately not
      *demanded* (D36).

### Task 5 — The conversion registry

- [ ] `convert.rs`: `map_identifiers`'s row gains `DataKind::RankedList`.
      Without it, a user's own `@lg.step(conversion="map_identifiers")`
      annotated `-> RankedList` propagates the **input's** namespace onto an
      output that has crossed, because `graph.rs::propagate` guards the
      override on `converts(output)`. The guard's comment reasons about a kind
      whose `applies` says it has no such axis; a `RankedList` has one, so the
      guard stops protecting anything and starts hiding something.
- [ ] `python/ligase/convert.py`'s `map_identifiers` keeps its `EntitySet`
      signature. Widening the shipped library step is not this milestone's,
      and the consequence worth documenting is that a mapped ranking comes out
      shorter, so its ranks close up — which the architecture page says, with
      the notes that name what left.

### Task 6 — The Python surface

- [ ] `load_ranked_list` in `crates/ligase-py/src/lib.rs`, registered on the
      module; stub in `python/ligase/_core.pyi`.
- [ ] `RankedList` in `python/ligase/types.py` with a `KINDS` entry;
      `lg.ranked_list()` in `python/ligase/__init__.py`; both in `__all__`.
- [ ] `web/scripts/generate_api.py`: `ranked_list` in `loading`, `RankedList`
      in `types`.
- [ ] `cli.py`'s `LOADERS` keeps `.tsv` → `entity_set` and is **not touched**
      (it is M10's file, and the right answer is to leave it): a file cannot
      say whether its order is meaningful, so `inspect` cannot tell the two
      kinds apart and reads the weaker claim. Say so in the architecture page.

### Task 7 — Tests, written first

New `crates/ligase-core/tests/ranked_list.rs`:

- [ ] `the_order_of_a_ranked_list_is_its_result` — two files, identical rows,
      different order, **different** canonical hashes. The inverse of every
      other kind's canonicalisation test, so the test says so at length, and
      asserts the contrast in the same breath: the *same two files* read as an
      `EntitySet` agree. A reader who finds only the `assert_ne!` would
      reasonably take it for a bug.
- [ ] `a_ranked_list_has_no_coordinate_system`,
      `a_ranked_list_carries_organism_namespace_subject_and_release`,
      `a_ranked_list_round_trips_through_its_name`,
      `a_ranked_list_is_written_as_a_table`.
- [ ] `a_score_column_is_compared_exactly`.
- [ ] `a_ranked_list_and_a_gene_list_must_both_say_which_space_they_are_in`
      over all four pairings.
- [ ] `a_ranked_list_in_one_space_refuses_to_join_a_gene_list_in_another` and
      `a_ranked_list_refuses_to_score_against_a_gene_list_from_another_release`
      — the second and third done-when clauses.
- [ ] `each_axis_a_ranked_list_claims_compares_its_own_field` and
      `a_step_over_a_ranked_list_carries_every_axis_it_claims` — M7's standing
      requirement, for the kind.
- [ ] `map_identifiers_crosses_for_a_ranked_list_too`.
- [ ] Loader coverage to M8's standard: inference per namespace, a named
      identifier column, CRLF, CR-only, a header with no rows, symbols Excel
      turned into dates, and a missing column naming the columns there are.
- [ ] `a_ragged_ranked_list_falls_back_to_its_bytes`.

New fixtures, committed by hand as M8's `entities_*.tsv` were (`generate.py`
does not make those either): `ranked_degs.tsv`, `ranked_degs_shuffled.tsv`,
`ranked_targets_ensembl.tsv`, `ranked_crlf.tsv`, `ranked_cr_only.tsv`,
`ranked_excel_dates.tsv`, `ranked_header_only.tsv`.

New `tests/test_ranked_lists.py`: the kind on the Python surface, its
applicable dimensions, inference, `KINDS`, and both refusals through
`lg.dimension_errors`.

### Task 8 — Docs, in the same change

- [ ] `web/scripts/capture_diagnostics.py`: two scenarios — `rankedNamespace`
      (a ranked list of symbols scored against an Ensembl gene list) and
      `rankedRelease` (two HGNC releases). Both are engine text on a page and
      so must be captured, never written.
- [ ] `web/content/concepts/semantic-hashing.mdx`: `EntitySet` and
      `RankedList` rows in the table; a subsection on the third answer.
- [ ] `web/content/guide/ranked-lists.mdx` + `guide/_meta.ts`: loading and
      comparing ranked lists, with a runnable example and the two captured
      diagnostics.
- [ ] `web/content/project/architecture/canonicalisation.mdx`: the ordering
      decision, its trade-offs, why the two canonicalisers are two functions,
      why `inspect` reads a `.tsv` as the weaker claim, and what mapping a
      ranking across spaces does to its ranks.
- [ ] `web/content/reference/dimensions.mdx`: the last column covers both
      kinds, which is the point — they differ in canonicalisation, not in
      dimensions.
- [ ] D41 into `web/content/project/decisions.mdx`, house format.
- [ ] M12 marked done in `web/content/project/milestones.mdx`, including the
      `source_set` correction above.
- [ ] `CHANGELOG.md`.

### Task 9 — The gates

- [ ] `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace`.
- [ ] `ruff check .`, `ruff format --check .`, `mypy`, `pytest`.
- [ ] `cd web && PATH=".../.venv/bin:$PATH" npm run check` — the only gate
      that proves a new page is reachable and its anchors resolve.
- [ ] PR against `main`, noting the parallel M10 work and the three
      append-only docs files that will want a rebase.
