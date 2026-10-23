# M11 — Stochastic steps

*Plan, written against `main` at `174fa7e` (23 Oct 2026). Implements §5.4 and
D40 of `docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md`.*

A step whose tool draws on randomness declares itself, names the parameter
that seeds it, and is refused at decoration time if it does not. The payoff
is in the audit vocabulary: `Deterministic` becomes unreachable for such a
step, so the word keeps meaning what it says.

---

## 1. The public API change, stated

`@lg.step` gains two keyword arguments. CLAUDE.md asks that an API change be
stated rather than slipped in; §5.4 sanctions this one, so it is stated and
taken.

```python
@lg.step(tool="iqtree", stochastic=True, seed="seed")
def tree(aln: Alignment, seed: int = 42) -> Tree: ...
```

```python
def step(
    function: Callable[P, R] | None = None,
    /,
    *,
    tool: str | Sequence[str] | None = None,
    coordinates: str | None = None,
    cite: str | Sequence[str] | None = None,
    conversion: str | None = None,
    stochastic: bool = False,          # new
    seed: str | None = None,           # new
) -> ...
```

`Step` gains two attributes of the same names. Nothing else on the surface
changes: `AuditReport`, `StepAudit` and `Methods` gain fields but lose none,
and no existing verdict string is renamed.

---

## 2. The four open questions, settled

### 2.1 `stochastic=` and `seed=` are one declaration in two parts

Both or neither. Each half alone is a `StepDefinitionError` at decoration.

`stochastic=True` with no `seed=` is the case the spec names. `seed="x"` with
no `stochastic=True` is the case it does not, and the temptation is to read it
as a shorthand — a user declaring determinism-by-seeding on a tool the engine
has no opinion about. Rejected, for one reason: it would make `stochastic=True`
ceremony. Two spellings of one meaning is how a codebase acquires a second
code path that nobody tests, and D40's whole content is that a stochastic step
*declares itself*. A declaration with an optional half is not one.

So there is exactly one spelling, and the other direction gets an error that
teaches it rather than a silent reinterpretation. The engine then represents
the pair as a single `Option<String>` — the seed parameter's name, `None` for
an ordinary step — which makes "stochastic with no seed" unrepresentable below
the decorator rather than merely unreached.

Three further refusals fall out of "names its seed parameter", and all are
decoration-time:

- a `seed=` naming a parameter the function does not take;
- a `seed=` naming a parameter annotated as anything but `int` (§5.4: "must
  exist and be typed `int`");
- a `seed=` naming one of the step's *data* parameters, which the annotation
  test catches on its own.

### 2.2 The seed is already in the cache key, and will be pinned there

`key.rs` says so in its module docs already — "a seed is an ordinary parameter
of a step, so it is hashed here like any other". Verified: `graph.py::_plan`
puts every plain parameter into `Task.parameters` as `(name, repr(value))`,
after `apply_defaults()`, and `Key::compute` hashes that list under a
length-prefixed `parameters` field.

So there is nothing to build, and the risk is the other one: an unpinned
property is a property that can be lost. A test asserts that two pipelines
differing only in the seed have different keys, and that the same seed hits
the cache — because without the first, two seeds collide and the second run is
served the first one's answer, in silence.

### 2.3 Audit has no first-run problem, and `run` makes no claim

`audit` is *defined* as two runs of its own, into scratch files, after an
ordinary run that gives the step its inputs. There is never one observation to
judge. The surface that does see a single run — `ligase run` — reports an
`Outcome` (`ran`, `cached`, `skipped`), which is a fact about the cache and
not a claim about repeatability, and that is the right answer rather than a
gap: a verdict from one run would be a guess.

Nothing to change. Recorded in D40 so that the next reader does not go looking
for the missing case.

### 2.4 A stochastic step is never `Deterministic` — and the report must not lie about why

The subtle one, and the fix is not a relabel.

Today `twice()` returns `Deterministic` on byte equality. A step that is
stochastic but correctly seeded *will* be byte-identical, so it reaches
`Deterministic` today, which D40 forbids.

The reason it is forbidden is worth stating, because it is a claim about what
the verdict means. `Verdict` is a statement about the **step**, not about the
pair of files in front of it: `ligase audit` exists to answer "if I ran this
again, would I get the same answer". Said of a step that draws random numbers,
"deterministic" is false even when the bytes match — the bytes match *because
the seed was held still*, and move the seed and they do not. A reader who sees
`deterministic` concludes the step has no randomness in it. That conclusion
would be wrong, and it is exactly the class of engine-produced false statement
the project exists not to make.

So:

| canonical forms | bytes | declared stochastic | verdict |
|---|---|---|---|
| differ | — | — | `not reproducible` |
| agree | agree | no | `deterministic` |
| agree | agree | **yes** | **`reproducible`** |
| agree | differ | — | `reproducible` |

`Deterministic` is reachable only when the bytes match *and* nothing declared
itself stochastic. A declaration does not add a verdict; it removes one from
reach.

**And the text has to follow.** Relabelling alone would make the report lie by
a different route: `Reproducible`'s existing wording is "the bytes differ /
what they mean does not", which is false of a byte-identical seeded step. So
`text()` grows branches that say what actually happened, and the most useful
of them is the one for a seeded step that *still* disagreed — a step whose
declaration is a lie, which the report should name as such rather than filing
under ordinary irreproducibility.

### 2.5 Which scale the verdict is on

M10b put `RemoteVerdict` in its own type because "a remote's verdict is about
where bytes came from, not about whether a step repeats". The argument does
not transfer, and the test is whether the new fact is on the audit's own axis.

It is. "This step draws on randomness" is a statement about whether the step
repeats — the very question `Verdict` answers. It changes what two runs can
tell you about that question, rather than answering a different question
beside it. A separate `StochasticVerdict` would force a reader to combine two
words to learn one fact, and would leave `Verdict::Deterministic` free to go
on being wrong.

Confirmation from the other direction: `prov::bundle::Match::Identical`
*stays* correct for a seeded stochastic step, and does not need any of this.
`Match` compares one run against a record — a question about two artefacts, to
which "identical" is a true answer however the step got there. The two enums
differ in subject, not in strictness, and that is the line D40 draws.

---

## 3. No `Tree` kind — D35 refuses it

§5.4's example returns a `Tree`, which does not exist. It is not being added.

D35: a kind earns its place only if it brings a dimension whose mismatch is a
documented silent failure, and a kind that is a file format with a schema is a
parser. A phylogenetic tree's tip labels are identifiers, on the
`identifier_namespace` axis `EntitySet` already claims and `RankedList`
already shares; its organisms are `organism`, set-valued since D37; a
reference tree's version is `release`. There is no axis a `Tree` brings that
nothing has. What is left is Newick with a schema — a parser, which D35 says
no to however convenient it would be for the example in a spec.

So the standing requirement from M7 does not land on this milestone: no kind
is added, no axis is newly applied, and no `compare`/`propagate` arm comes
within reach that was not already pinned. Stated in the milestone entry so
that the absence is visible rather than merely true.

## 4. What the tests run on, and why nothing skips

§5.4 allows "IQ-TREE on a small alignment, or a seeded Python step where the
tool is absent". The seeded Python step is chosen, on both halves of the
reason:

- IQ-TREE is not installed in CI and is not in the `demo` job's three tools,
  so an IQ-TREE test would skip on every machine the project has. The brief's
  standard — "a test that silently skips in CI is worth much less than one
  that runs" — rules it out, and `test_demo.py`'s own docstring says the same
  thing about why its properties are *also* tested without the tools.
- It would need the `Tree` kind D35 has just refused.

The vehicle is a `RankedList` produced by a seeded Python shuffle. It is the
one existing kind whose canonical form preserves order (D41), which makes the
seed's effect visible in the hash rather than hidden behind a sort — so a test
that the seed matters can assert on the engine's own comparison instead of on
the step's internals. Two bodies are needed:

- `shuffled(…, seed: int)` — seeds `random.Random(seed)`. Byte-identical
  across two runs with one seed. The clause-2.4 subject.
- `unseeded_shuffle(…, seed: int)` — declares a seed and ignores it, drawing
  on `random.SystemRandom`. `not reproducible`, and the step whose declaration
  is false.

## 5. Changes, by file

**Mine. None of these is M13's (`python/ligase/steps/`, `examples/`,
`prov/cite.rs`) or M14's (`store.rs`, `prov/bundle.rs`, `cli.py`,
`Cargo.toml`).**

| File | Change |
|---|---|
| `python/ligase/graph.py` | `step()` and `Step` take `stochastic=` / `seed=`; `_check_seed` validates; `_plan` puts the seed name in the task |
| `python/ligase/audit.py` | `StepAudit.seed` — the parameter and the value it was held at, or `None` |
| `python/ligase/_core.pyi` | `Task` and `StepAudit` tuples gain their field |
| `crates/ligase-py/src/lib.rs` | `PyTask`/`PyStepAudit` gain theirs; `recipes()` carries it through |
| `crates/ligase-core/src/exec.rs` | `Task.seed: Option<String>` |
| `crates/ligase-core/src/audit.rs` | verdict rule, `StepAudit.seed`, `text()` branches, module note |
| `crates/ligase-core/src/prov/methods.rs` | `Group.seed`; the sentence says *seeded with*; the seed is never dropped as per-sample |
| `crates/ligase-core/tests/audit.rs` | verdict and wording tests |
| `crates/ligase-core/tests/keys.rs` | the seed moves the key |
| `crates/ligase-core/tests/provenance.rs` | the paragraph names the seed as a seed |
| `tests/test_step.py` | the five decoration refusals |
| `tests/test_stochastic.py` | new — the milestone's clauses end to end |
| `web/scripts/capture_diagnostics.py` | `unseededStochasticStep` scenario |
| docs | as §7 |

### 5.1 Why the declaration travels on `Task` and not on `StepSpec`

`StepSpec` holds what the graph needs to *check* a pipeline — output kind,
coordinates, conversion — and stochasticity is not a dimension, does not
propagate, and no check turns on it.

`Task` already carries exactly this shape of thing: `citations`, documented
as deliberately outside the cache key, read only by the methods generator.
The seed declaration belongs beside the `parameters` whose value it names,
because the name alone is not the fact — "`seed`, held at 42" is, and
splitting the two halves across two structs would make every reader join them
back up.

By the same argument as `citations`, the declaration is **not** written into a
provenance `Record`: a record that stored it would go stale the moment the
decorator line was edited, and a methods paragraph takes it from the pipeline
in front of it.

### 5.2 The methods paragraph says *seeded*, not just *seed = 42*

A seed is a plain `int` parameter, so the paragraph already prints
`, with seed = 42` today, indistinguishable from `min_freq = 0.01`. That
passes the clause's letter and fails its purpose: a reader cannot tell from it
that the step is stochastic or that this is what makes it repeat. The sentence
will read `, seeded with seed = 42`, which is what a methods section has to
say about a stochastic tool and is the form a reader can check.

### 5.3 One guard that is wrong the moment a seed exists

`methods.rs::restating` drops any parameter whose value equals the node's own
scope, so that `sample_id = 'SRR8525904'` does not appear beside a sentence
already naming four samples. The rule is right and its catch-all is now too
wide: a seed caught by it would vanish from the paragraph, and D40 requires
the paragraph to state it. The exclusion is principled rather than defensive —
a seed never *merely restates* the scope; it is the one parameter the
paragraph is obliged to carry — so the declared seed is exempted, with a
comment saying which rule wins and why.

---

## 6. Tests, in the order they are written

Failing first, in both languages.

**Decoration — `tests/test_step.py`**

1. `test_a_stochastic_step_must_name_its_seed` — `stochastic=True`, no
   `seed=`. Names the missing `seed=`, and suggests the `int` parameter it can
   see.
2. `test_a_named_seed_must_be_a_parameter_the_step_takes`
3. `test_a_seed_must_be_typed_int`
4. `test_a_seed_cannot_be_a_dataset_parameter`
5. `test_naming_a_seed_without_declaring_stochastic_is_refused`
6. `test_a_seeded_stochastic_step_keeps_what_it_declared`

**Verdicts — `crates/ligase-core/tests/audit.rs`**

7. `a_seeded_stochastic_step_is_reproducible_rather_than_deterministic` —
   byte-identical output, verdict `reproducible`, and `assert_ne!` against
   `Deterministic` with the reason in a comment. The clause that proves 2.4.
8. `a_step_that_is_not_stochastic_is_still_deterministic_on_the_same_bytes` —
   the contrast, over the same command, so a reader cannot mistake 7 for a
   change in what `Deterministic` means.
9. `the_report_does_not_say_the_bytes_differ_when_they_did_not`
10. `a_stochastic_step_whose_result_moves_is_told_its_seed_did_not_hold`

**Key — `crates/ligase-core/tests/keys.rs`**

11. `a_step_run_under_two_seeds_has_two_keys`
12. `the_same_seed_is_the_same_key`

**Methods — `crates/ligase-core/tests/provenance.rs`**

13. `a_stochastic_steps_paragraph_says_it_was_seeded`
14. `a_seed_is_named_even_where_it_reads_like_the_sample` — the §5.3 guard.

**End to end — `tests/test_stochastic.py`**

15. `test_a_seeded_stochastic_step_audits_as_reproducible`
16. `test_a_stochastic_step_is_never_reported_deterministic`
17. `test_a_stochastic_step_that_ignores_its_seed_is_not_reproducible`
18. `test_the_methods_paragraph_names_the_seed`
19. `test_changing_the_seed_reruns_the_step` — through the real cache.

No test in this milestone skips on any machine.

---

## 7. Docs, in the same change

- `web/content/concepts/stochastic-steps.mdx` — what a stochastic step is,
  why declaring it is the thing that makes "reproducible" worth saying, and
  the three verdicts under it. Into `concepts/_meta.ts` after
  `record-and-replay`.
- `web/content/guide/stochastic-steps.mdx` — writing one, with a runnable
  example and the real refusal from `capture_diagnostics.py`. Into
  `guide/_meta.ts` beside `semantic-hashing` ("Checking that a step repeats"),
  which is the task page it continues.
- `web/content/project/architecture/audit.mdx` — how the verdict is decided,
  where the declaration travels and why not on `StepSpec`, why
  `bundle::Match::Identical` needs none of it, and the trade-offs. There is no
  architecture page for the audit yet; this creates it. Into
  `architecture/_meta.ts` after `execution`.
- `decisions.mdx` — D40, between D39 and D41, in the house format.
- `milestones.mdx` — the M11 entry marked **Done**, clause by clause with the
  test that discharges each, plus what the work turned up and the standing
  requirement's absence.
- `CHANGELOG.md`.

`decisions.mdx`, `milestones.mdx` and `CHANGELOG.md` are being appended to by
M13 and M14 in parallel. Expect a mechanical conflict and keep both sides.

Gate: `cd web && PATH="<worktree>/.venv/bin:$PATH" npm run check`.

---

## 8. What this milestone deliberately does not do

- No `Tree` kind (§3).
- No new `Verdict` variant, and no `StochasticVerdict` (§2.5).
- No change to `prov::bundle` — `Match::Identical` is correct as it stands,
  and the file is M14's.
- No `tolerance=`. Numeric tolerance is a separate nondeterminism rule in
  CLAUDE.md and no clause here asks for it.
- No attempt to detect stochasticity. The engine cannot know, which is the
  whole reason a declaration exists.
- `AuditReport::verdict()` still answers `Deterministic` for a report in which
  every step *failed*. Pre-existing, outside these clauses, and already
  handled on the Python surface by `AuditReport.reproducible`, which accounts
  for failures. Noted, not widened.
