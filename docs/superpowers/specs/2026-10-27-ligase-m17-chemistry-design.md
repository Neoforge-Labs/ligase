# M17 — The chemistry extra

*Written against v0.3 at commit `0c005b0` (27 Oct 2026). The third milestone
of v0.3, after M15 (`KnowledgeGraph`) and M16 (claim binding). Read alongside
`CLAUDE.md`, §4.4 of
`docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md`,
`web/content/project/decisions.mdx` and
`web/content/project/milestones.mdx`.*

---

## 1. Intent

Give Ligase two chemistry kinds — `MoleculeSet` and `MoleculeRanking` — and
the dimension that admits them: `representation`, which says what a name in
such a file *is* and what standardisation produced it. RDKit arrives as an
optional Python extra, `pip install ligase-bio[chem]`, so that the genomics
install acquires nothing.

The milestone rests on a measurement rather than on §4.4's say-so, and the
measurement is in §3. §4.4 asked for it before committing to InChIKey as a
canonical form; it was run first, it settles the canonical form, and it
contradicts §4.4 on one point.

Two obligations land here and are discharged rather than deferred:

1. **M7's standing requirement.** `representation` is a new axis, so its
   `check.rs::compare` and `graph.rs::propagate` arms are pinned by named
   tests.
2. **D35's unwritten second limb.** The rule as written does not admit
   `RankedList`, which the project shipped in M12. §2.4 writes down the limb
   that did admit it, because `MoleculeRanking` is the second kind to need
   it.

## 2. The D35 reckoning, first

> A kind earns its place only if it brings a dimension whose mismatch is a
> documented silent failure in real research. A kind that is a file format
> with a schema is a parser, and Ligase does not need more parsers.

§4.4 names three things. They are not three kinds, and the first job of this
milestone is to say what each one is.

### 2.1 `Representation` is the dimension, not a kind

§4.4 describes it as a record of "canonicalisation source and standardisation
state". That is a *value*, and the thing it is a value of is an axis. It
brings no file of its own and holds no data; it is what the two kinds below
are admitted for. Treating it as a kind would have been the category error
the dimension catalogue exists to prevent.

**The silent failure a mismatch causes.** The same substance has different
InChIKeys depending on what was done to it before the key was computed. Not
occasionally: in the thirty-molecule set of §3, **twelve** molecules — forty
per cent, all of them ordinary drugs — get a different key under a different
standardisation state. Aspirin gets *three* keys. So:

- two compound sets standardised differently overlap in **nothing** for the
  affected substances, and a virtual screen's hit list scored against a
  library prepared by somebody else reads as poor enrichment rather than as a
  mismatch;
- the overlap is rarely zero, which is what makes it the worst shape of bug:
  the unaffected sixty per cent match, so the answer is small, non-zero and
  plausible — the same shape as D38's gene-set overlap;
- merging two libraries standardised differently puts the **same substance in
  one set twice** under two keys, so anything counting it double-counts, and
  a set that is not a set has stopped being what its kind claims.

**That this is documented, not conceivable.** According to PubMed:

- Fourches, Muratov & Tropsha, *Trust, but verify: on the importance of
  chemical structure curation in cheminformatics and QSAR modeling research*,
  J Chem Inf Model 50:1189–1204 (2010),
  [DOI](https://doi.org/10.1021/ci100176x). Names the exact operations
  `representation` records — removal of counterions, salts and mixtures,
  normalisation of specific chemotypes, **curation of tautomeric forms**, and
  the deletion of duplicates — as mandatory steps, and gives case studies
  where curating the raw database is what made the modelling work or
  materially changed its accuracy.
- Fourches, Muratov & Tropsha, *Trust, but Verify II: A Practical Guide to
  Chemogenomics Data Curation*, J Chem Inf Model 56:1243–1252 (2016),
  [DOI](https://doi.org/10.1021/acs.jcim.6b00129). The same argument six
  years on, framed as a reproducibility problem.
- Bento *et al.*, *An open source chemical structure curation pipeline using
  RDKit*, J Cheminform 12:51 (2020),
  [DOI](https://doi.org/10.1186/s13321-020-00456-1). ChEMBL's own
  standardiser, and its stated reason for existing is this failure in
  Ligase's own words: incoming compounds are not standardised consistently,
  so standardisation is what makes it possible "to easily compare and
  integrate data on the same compound from different sources".

The third is the one that settles it. A database with nearly two million
compounds built a three-stage standardiser *because* comparing and
integrating compounds from different sources does not otherwise work. That is
not a conceivable mismatch; it is infrastructure somebody built to avoid one.

**Verdict: the axis is real.** It is `identifier_namespace`'s failure in a
different costume — the join succeeds, the result is plausible, and the cause
is bookkeeping — and it differs in being *invisible in the names themselves*.
Two gene lists in different spaces look different at a glance: `TP53` is not
`ENSG00000141510`. Two compound sets standardised differently look identical:
both are columns of twenty-seven-character InChIKeys, and
`BSYNRYMUTXBXSQ-UHFFFAOYSA-M` against `BSYNRYMUTXBXSQ-UHFFFAOYSA-N` is one
character of difference in the block nobody reads. There is no glance that
catches this one, which is the strongest case for a dimension that Ligase has
yet had.

### 2.2 `MoleculeSet` passes

It brings `representation`, which no other kind brings and no other kind
can: a gene list has no standardisation state, and asking it for one is the
category error the catalogue's `n/a` cells exist for.

**The honest caveat, recorded rather than smoothed over.** Unlike knowledge
graphs, chemistry *has* standard formats — SDF, MOL, SMILES files — and
RDKit reads them properly, which is precisely why Ligase will not. D35's
first clause is that a kind which is a file format with a schema is a parser,
and this milestone writes no chemistry parser at all. What it reads is a
tab-separated table with an identity column, which is the cheapest envelope
that can carry an InChIKey, a representation line and whatever else a
screening table holds. The shape is not the grounds; the axis is. Take
`representation` away and this kind should not exist.

### 2.3 `BindingScores` brings no dimension — and is admitted anyway, on a
### different limb, renamed

The brief's own suspicion is correct on the facts. Examined axis by axis, it
brings nothing:

- its **target** is `subject`, which both named-list kinds already carry;
- its **compounds** carry `representation`, which `MoleculeSet` brings;
- its **release** — ChEMBL 34 against ChEMBL 35 — is `release`;
- its **scoring function and the software that ran it** are provenance, not
  a dimension. D32 has a step say what its own work rested on, and D42 puts
  the tool's version into the cache key, so swapping Vina for Glide is
  already a different key and a different methods paragraph. Making it a
  dimension as well would be the same fact in two places;
- its **score column** is a column. D41 settled that one too: compared
  exactly, with no tolerance, because no drift has been measured and a
  guessed tolerance would be baked into every cache key.

So under D35 as written, `BindingScores` earns no place. But D35 as written
does not admit `RankedList` either, and M12 shipped it — on the dimensions
page's own account, `RankedList` carries *exactly* the dimensions an
`EntitySet` carries and differs only in its canonical form. Either that was
outside the rule, or the rule has a second limb nobody wrote down.

**It has a second limb, and this is the milestone that writes it: D48.** A
kind also earns its place if it brings a **canonical form whose absence makes
the cache serve a wrong answer**. That is not a loosening. It is the same
standard — a silent failure, in the engine instead of in the data — and D41
states it in full for the one case that already existed: a ranking whose
canonical form sorted its rows would declare two different answers identical,
and a cache keyed on that form would serve one when the pipeline asked for
the other, in silence. A ranking of ten thousand docked compounds is that
failure exactly, with more at stake than a ranking of genes, because the
number of compounds anybody orders is the length of the prefix the order
decides.

So the kind is admitted, and it is admitted on a stated limb rather than by
being waved through beside its sibling.

**Renamed, and the rename is not cosmetic.** `BindingScores` would be a false
name on most of its contents. The same file shape holds docking scores,
QSAR-predicted activities, predicted solubilities and synthetic-accessibility
scores, and none of the last three is a binding score. A kind named for one
of the things it holds is a kind whose dimension table and error messages
describe something the user is not holding — the objection `load::named_list`
already records against sharing reasons between kinds. The kind is
**`MoleculeRanking`**, parallel to `MoleculeSet` so that the shared-loader,
separate-canonical-form relationship reads off the two names, and "ranking"
is D41's own word for the thing whose order is its answer.

### 2.4 What this milestone ships, counted

Two kinds, one axis, one optional extra. `Representation` is reclassified
from kind to dimension; `BindingScores` is admitted under D48 and renamed.

## 3. The measurement

§4.4: "Measure InChIKey stability across RDKit versions before committing to
it as a canonical form." Run first, before any engine code.

**Method.** Six RDKit releases spanning four years — 2022.09.5, 2023.09.6,
2024.09.6, 2025.09.6, 2026.03.6, 2026.09.1 — each in its own ephemeral
Python 3.11 environment. Thirty input SMILES: marketed drugs, four salt
forms, three tautomer pairs, a zwitterion, a charged/neutral pair, a
pentavalent-nitro spelling, a metal complex, and both ibuprofen enantiomers.
Six standardisation states per molecule: raw, `Cleanup` only, salt-stripped
(`FragmentParent`), tautomer-normalised (`TautomerEnumerator().Canonicalize`),
charge-neutralised (`Uncharger`), and all three together. 1,080 InChIKeys in
total. The probe is `tests/chem/inchikey_stability.py` in this change; the
raw run is in the session notes.

### 3.1 InChIKey survives the test

**Zero disagreements in 1,080 cells.** Every (state, molecule) pair produced
the same InChIKey on all six releases, including under tautomer
normalisation, which was the state most likely to drift. InChIKey is a sound
canonical form, and §4.4's instinct was right.

### 3.2 Canonical SMILES did not survive it

In the same span, RDKit's canonical SMILES for cisplatin changed:

| RDKit | Canonical SMILES | InChIKey |
|---|---|---|
| 2022.09.5 – 2024.09.6 | `Cl[Pt]Cl.N.N` | `LXZZYRPGZAFOLE-UHFFFAOYSA-L` |
| 2025.09.6 – 2026.09.1 | `N.N.[Cl][Pt][Cl]` | `LXZZYRPGZAFOLE-UHFFFAOYSA-L` |

One compound in thirty, and it is the whole argument for §4.4's "InChIKeys,
not SMILES strings". A canonical form over SMILES would have changed the
content hash of every file holding that compound on an RDKit upgrade that
changed no chemistry, and everything downstream would have rerun — a cache
that is too cold, which D41 calls the affordable failure, but for no reason
at all. The InChIKey did not move. This is now measured rather than assumed,
and it is why `representation` distinguishes the two identity sources instead
of treating them as interchangeable.

### 3.3 The standardisation state is what moves the key

Twelve of thirty molecules, within a single RDKit version:

| Substance | What produced the difference | Keys |
|---|---|---|
| aspirin, from its sodium salt | nothing / salt-stripped / salt-stripped and neutralised | `JZLOKWGVGHYBKD-…-M`, `BSYNRYMUTXBXSQ-…-M`, `BSYNRYMUTXBXSQ-…-N` |
| warfarin | keto and enol forms, raw against tautomer-normalised | two keys become one |
| tetracycline | tautomer normalisation | `NWXMGUDVXFXRIG-YCRLQHIDSA-N` → `MUHBBHLZPHKTTR-ZXGQWNGHSA-N` |
| imatinib mesylate | salt stripping | `YLMAHDNUQAMNNX-…-N` → `KTUFNOKKBVMGRW-…-N` |
| metformin hydrochloride, choline chloride, sodium valproate, cisplatin | salt stripping | different in the first block or the last |
| histidine, glycine | charge neutralisation | the stereo or charge block moves |
| phosphoric acid | charge neutralisation | `…-UHFFFAOYSA-K` → `…-UHFFFAOYSA-N` |

Aspirin is the one to read twice. One substance, three identities, and the
difference between the second and the third is the final character — `M`
against `N` — which is the charge block. Nothing about a column of those
strings tells a reader which standardisation produced them.

**Salt-stripping alone is not enough to match a neutral set**, which is the
practical trap: `FragmentParent` leaves the deprotonated carboxylate, so a
salt-stripped aspirin is `BSYNRYMUTXBXSQ-UHFFFAOYSA-M` and a plain aspirin is
`BSYNRYMUTXBXSQ-UHFFFAOYSA-N`. Two sets that both say "salts removed" still
fail to overlap if only one of them was also neutralised.

### 3.4 §4.4 is wrong about stereochemistry, and this is the finding

§4.4 describes the standardisation state as "salt-stripped,
tautomer-normalised, charge-neutralised, **stereo kept**". Stereo is not
kept. RDKit's tautomer canonicaliser discards it:

```
ibuprofen-S  C[C@@H](c1ccc(CC(C)C)cc1)C(=O)O  →  HEFNNWSXXWATRW-JTQLQIEISA-N
ibuprofen-R  C[C@H](c1ccc(CC(C)C)cc1)C(=O)O   →  HEFNNWSXXWATRW-SNVBAGLBSA-N
                                    both, tautomer-normalised
                                                 →  HEFNNWSXXWATRW-UHFFFAOYSA-N
```

The canonical tautomer's SMILES is `CC(C)Cc1ccc(C(C)C(=O)O)cc1`: the `[C@@H]`
is gone. This is RDKit behaving as documented — stereo at an sp3 centre
inside a tautomeric system is removed, and a carboxylic acid's α-carbon is
inside one — but the consequence is that **a tautomer-normalised compound set
cannot tell one enantiomer from the other**. The same happens to histidine
and to tetracycline, whose stereo block changes outright.

For a drug set that is not a nicety. Two enantiomers are routinely one active
and one inactive, and sometimes one drug and one toxin.

So `representation` does not record an intention. It records **what was
actually done, stereo loss included**, as an operation applied:
`stereo-discarded`. And because a user asking for tautomer normalisation
cannot be expected to predict it, the standardisation step **checks the
declaration against what it observed**, asymmetrically — see §5.5. That
asymmetry is D47.

## 4. Approaches considered

### 4.1 What the canonical form hashes

- **(a) The sorted set of InChIKeys and nothing else**, which is §4.4's
  letter. *Rejected.* Two screening tables holding the same compounds with
  different activity columns would hash identically, so the cache would serve
  one for the other — a false agreement, which is the thing the engine exists
  to refuse. `canon::entity_set` settled the same question for a gene list:
  the attribute columns are part of what the list says, because "a list of
  symbols with a fold change is not the same claim as the same symbols with a
  p-value".
- **(b) The header as a field, the representation line as a field, and the
  rows as records.** *Chosen.* The ordinary set-shaped form, identical in
  shape to `canon::entity_set`. §4.4's substance is kept where it belongs: the
  **identity** a row is compared by is the InChIKey column, named by the
  `representation` dimension, and a set whose identity is SMILES is a
  recognisably weaker thing by §3.2's measurement rather than a silently
  equivalent one.

**The representation line is hashed as a field, and that is load-bearing.**
A step key is hash(code, parameters, input content hashes, tool versions) —
dimensions are not in it. Two files with byte-identical InChIKey columns and
different standardisation lines mean different things, so without the line in
the content hash the cache would serve one for the other. This is the one
place in the design where omitting a field would be a correctness bug rather
than a cold cache.

### 4.2 Where the representation comes from

- **(a) Declared only.** *Rejected*, for D44's reason: a file written by the
  standardiser knows its own answer, and asking for something the engine is
  holding is what D44 refused for a graph's sources.
- **(b) Read from a line the standardiser wrote, with a declaration
  overriding it.** *Chosen.* `#ligase representation: …` ahead of the header,
  read as `Source::Inferred`; a declaration wins and is recorded; a step that
  standardised is `Source::Derived`. A file with InChIKeys and no such line
  leaves the axis **unknown with a reason**, never "as given" — assuming "as
  given" would be the engine inventing the most dangerous of the possible
  answers, since a ChEMBL download *is* standardised and a vendor file may
  not be.

A declared representation is **recorded, not verified**, exactly as D38 has
a declared namespace be, and for the same reason: the engine cannot read a
standardisation off a column of keys, and a demand it could not check would
be unusable for in-house sets. What is new here is that the *step* can check
it, because the step did the work — §5.5.

### 4.3 Where RDKit lives

- **(a) In the Rust core, so the engine computes InChIKeys itself.**
  *Rejected.* There is no InChI implementation in the core's dependency set,
  the InChI library is C, and D6's zero-config install plus D43's
  keep-cmake-out rule are what `default_build.rs` exists to defend. A core
  that linked an InChI library would end the claim that
  `pip install ligase-bio` needs no toolchain beyond Rust.
- **(b) In one Python module, imported lazily, behind the extra.** *Chosen,
  and it is D49.* The **engine core knows no chemistry at all** — no new
  crate, so `cargo tree` is unchanged and `default_build.rs` needs no new
  entry. `representation` is a value the core compares and propagates, and
  computing one is somebody else's job. RDKit is imported inside
  `ligase.chem`, by exactly one function, and its absence is reported by
  name.

This is a better shape than `remote-store`'s, and worth saying why. That
extra is empty because a Python extra cannot reach a cargo feature. This one
is not empty: RDKit *is* a Python package, so `chem = ["rdkit>=2024.3.1"]`
does the thing an extra is for.

### 4.4 Standardisation is a conversion, not a step on its own

`convert.rs` exists for "the registered ways across a dimension", and D5
wants its entries to be rows rather than redesigns. Standardisation is one:
it crosses `representation` and nothing else, it reads its destination from
the same `to=` every conversion reads, and it is **an operation that can
fail** — a SMILES that does not parse, a mixture with no single parent, a
structure the standardiser rejects. `convert`'s own module comment draws
exactly that line: "A conversion that could not fail would be a rename, and a
rename across a dimension is what D38 forbids."

So `lg.chem.standardize` is registered in `Conversion::ALL`, which makes it
the **third edge of the conversion graph** after `map_identifiers` and ahead
of `liftover` and `rename_contigs`, and the first whose body is Python rather
than Rust. That the registry takes a Python-bodied entry without changing
shape is worth knowing before `liftover` arrives. Its failures are named and
counted in notes, never dropped — D26, by the same contract
`Note::IdentifiersDidNotCross` already keeps. This is D50.

## 5. What gets built

### 5.1 The file shape

A tab-separated table with a header, read by the existing `load::table`
reader, optionally preceded by `#` lines of which one may be the
representation:

```
#ligase representation: inchikey; salt-stripped, charge-neutralised
inchikey	smiles	name	activity_nm
BSYNRYMUTXBXSQ-UHFFFAOYSA-N	CC(=O)Oc1ccccc1C(=O)O	aspirin	120
HEFNNWSXXWATRW-JTQLQIEISA-N	C[C@@H](c1ccc(CC(C)C)cc1)C(=O)O	ibuprofen	85
```

A `MoleculeRanking` is the same file with the row order being the answer. The
table needs an `inchikey` column or a `smiles` column; neither is an error
naming both, as `named_list` already errors naming the column it wanted and
the columns the file has. Every other column is carried without being read,
a score column included.

Writing no chemistry parser is the point of §2.2, so: no `.sdf`, no `.mol`,
no `.smi`.

### 5.2 The dimension

```rust
pub struct Representation { identity: Identity, applied: Standardisations }
pub enum Identity { InChIKey, Smiles }
pub enum Standardisation { SaltStripped, TautomerNormalised,
                           ChargeNeutralised, StereoDiscarded }
```

A closed set of operations rather than free text, on `CoordinateSystem`'s
grounds: a value the engine cannot render is a value it cannot explain, and a
file naming an operation the engine does not know is a format error naming
the four it knows. The operations are phrased as **what was applied**, so
`stereo-discarded` is present when stereo was lost and absent when it was
not.

Compared by equality of `identity` and set equality of `applied`, with the
difference **named** in the detail line — "`library` also lists
tautomer-normalised" — through `english_list`, so this axis is punctuated
exactly as `organism` and `source_set` are. It is not a `NameSet<M>`: the
members are a closed enum, not names, so there is no case rule to choose and
D36's question does not arise.

Rendered as `InChIKey, salt-stripped and charge-neutralised`, or
`InChIKey, as given` where nothing was applied.

### 5.3 Dimensions of the two kinds

| Axis | Where from |
|---|---|
| `organism` | declared (a compound table does not record it) |
| `representation` | read from the representation line; declared overrides; derived from a `standardize` step |
| `subject` | declared — the target, the assay, the therapeutic area |
| `release` | declared — `ChEMBL 34` |

Everything else returns `false` from `applies()`. Both kinds answer
identically, as D41 requires of a set and its ranked sibling: they differ in
their canonical form and in nothing else.

**`identifier_namespace` is deliberately not claimed.** An InChIKey is an
identifier space, and `representation.identity` already says which one this
file is in. Two axes carrying one fact is D36's objection in mirror image,
and it would also make `required_pair` demand two things where one will do.

**`source_set` is declined, with reasons, as M8, M10 and M12 each declined
it.** A compound library merged from three vendors is assembled, and the
temptation is real. It is declined because this milestone can read nothing:
which library a compound came from is a column — an attribute — not a fact
about the set, and an axis the loader cannot fill is the unexercised value
type M7's standing requirement exists to prevent. Named as a gap in §8.

`representation` is **demanded** between any two of the molecule kinds, in
all four combinations written out — M12's lesson about the mixed pair,
applied before it can become the same bug a third time, and scoring a ranking
against a library is the commonest reason to join these two kinds at all.
Against any other kind they share only `organism`, so nothing is demanded.
The demand is one a user can always meet: declare it, or standardise and let
the step write it.

### 5.4 Canonical forms

`canon::molecule_set` records rows unordered; `canon::molecule_ranking`
writes them in order. They share the reader and nothing else, exactly as
`entity_set` and `ranked_list` do, and `canon/molecule_ranking.rs` carries
the why-it-does-not-sort comment at the top of the module that D41 requires
of any such file. The pinning test asserts the **contrast** over the same two
files, not a bare `assert_ne!`, for D41's stated reason: a reader finding only
an inequality would take it for a bug and fix the code to match.

A score column is compared exactly, with no tolerance, on D41's grounds.

### 5.5 `lg.chem.standardize`

```python
clean = lg.chem.standardize(library, to="inchikey; salt-stripped, charge-neutralised")
```

A registered conversion across `representation`. It reads the structures,
applies the operations the target names, computes InChIKeys, and writes a
molecule table with its own representation line.

Three things it must get right, each from §3:

- **Molecules that do not standardise are named and counted**, never dropped
  in silence (D26), capped at twenty named with a total, exactly as
  `map_identifiers` caps its unmapped identifiers.
- **Stereo loss is observed, and the check is asymmetric.** Declaring
  `stereo-discarded` when nothing was discarded is *safe* — it can only
  refuse a join a user can fix by declaring one thing consistently — so it is
  a note. Not declaring it when stereo *was* discarded is *dangerous*: two
  sets would agree while one of them has lost the difference between an
  active enantiomer and an inactive one, which is the false agreement the
  engine refuses. So that is an error, and the message says how many
  molecules lost stereo and what to do about it. The asymmetry is the one
  M10a settled for case folding, applied to a second axis.
- **RDKit's version joins the cache key**, so an upgrade reruns the
  standardisation it could have changed. §3.1 says the keys have not moved in
  four years and §3.2 says something else did, which is exactly why this is
  not left to trust.

### 5.6 Surfaces

Rust: `DataKind::MoleculeSet`, `DataKind::MoleculeRanking`,
`Dimension::Representation`, `dim::{Representation, Identity,
Standardisation, Standardisations}`, `load::{molecule_set,
molecule_ranking}`, `canon::{molecule_set, molecule_ranking}`,
`convert::Target::Representation`, a `standardize` row in `Conversion::ALL`,
`exec::output_suffix` → `.tsv`, `canon::describe`, two new `Note` variants.

Python: `lg.molecule_set(...)`, `lg.molecule_ranking(...)`, the
`MoleculeSet` and `MoleculeRanking` dataset classes and their `KINDS`
entries, a frozen `Representation(identity, applied)` dataclass in
`ligase.types`, `lg.chem.standardize`, the `load_molecule_set` and
`load_molecule_ranking` bindings, `_core.pyi`, and two `generate_api.py`
groups.

`ligase inspect` dispatches on the header, not a new extension: a `.tsv`
whose header holds an `inchikey` column is a molecule set, and otherwise an
entity set. A ranking is never inferred from a file — which is already true
of `RankedList` and is the same limitation, not a new one.

`pyproject.toml`: `chem = ["rdkit>=2024.3.1"]`. The floor is the first series
with clean NumPy 2 wheels; the measurement covers two series below it, so a
lower floor is defensible and would need `numpy<2` said somewhere an extra
cannot say it.

### 5.7 Proving the default install is unburdened

`default_build.rs` asserts a claim about the Rust dependency graph, and the
Rust dependency graph does not change here — which is itself the strongest
form of the claim, and §6 clause 10 asserts it rather than assuming it. The
Python half needs its own test, because that is where the dependency is:

- importing `ligase` imports no `rdkit` — asserted against `sys.modules`,
  which is what notices the day somebody moves the import to the top of
  `chem.py`;
- `lg.chem.standardize` without RDKit raises an error naming
  `pip install ligase-bio[chem]`, on the precedent of `ligase store status`
  telling a user which build they have;
- the **docs build needs no RDKit**. The guide's runnable example checks two
  pre-standardised fixtures, and the `capture_diagnostics.py` scenarios use
  fixtures too, so `npm run check` passes on a machine with no chemistry
  toolkit. A docs site that could not build without the optional extra would
  have made the extra mandatory in everything but name.

Every chemistry test **skips** where RDKit is absent, and the skip is not
silent: the suite says how many were skipped and why.

## 6. Done when

1. Both kinds exist with their loader, their canonical forms, their
   dimensions and their Python surface.
2. Two molecule sets standardised differently refuse to join, naming the
   standardisation one side lists and the other lacks, pinned in Rust and in
   Python.
3. `representation` is demanded between any two of the molecule kinds in all
   four combinations, and two sets that each state nothing are refused rather
   than agreeing by silence.
4. `representation`'s `check::compare` and `graph::propagate` arms are each
   pinned by a named test, M7's standing requirement discharged by name.
5. A set's representation is read off the line the standardiser wrote; a file
   with InChIKeys and no such line leaves the axis unknown with a reason
   rather than assuming "as given"; a declaration overrides and is recorded.
6. Reordering a molecule set's rows leaves its canonical hash unchanged while
   reordering a ranking's changes it — asserted as the contrast over the same
   two files.
7. Changing only the representation line changes the content hash, with the
   InChIKey column byte-identical.
8. The measurement is in the repository and reproducible: where RDKit is
   installed, a named test asserts that each standardisation state produces
   the keys §3 records — aspirin's three, warfarin's collapse, and ibuprofen's
   two enantiomers becoming one — and skips cleanly where it is not.
9. `standardize` crosses the axis as a registered conversion; molecules that
   do not standardise are named and counted; stereo discarded without being
   declared is an error naming the count, and declared without being
   discarded is a note.
10. The default install is proved unburdened: `cargo tree` for a default
    build is unchanged, importing `ligase` imports no `rdkit`,
    `lg.chem.standardize` without RDKit names the extra, and `npm run check`
    passes with no chemistry toolkit installed.
11. Docs ship in the same change: concepts, guide, architecture, a
    silent-failures page titled by symptom, `_meta.ts` entries, the dimension
    catalogue's new row and two new columns, D47–D50, the M17 entry, and
    `CHANGELOG.md` under `[Unreleased]`.
12. All gates green: `cargo fmt --all --check`, `cargo clippy --workspace
    --all-targets -- -D warnings`, `cargo test --workspace`, `ruff check .`,
    `ruff format --check .`, `mypy`, `pytest`, and `npm run check` in `web/`.

## 7. Left to the builder

- The exact spelling of the representation line. `#ligase representation:` is
  proposed because Ligase owns the prefix and a `#` line is already dropped
  by the interval loader, but a `##`-style header on the VCF model is equally
  defensible.
- Whether `Standardisations` gets its own file in `dim/` or sits in
  `dim/names.rs` beside the other value types. It is not a `NameSet`, so
  `names.rs` is a weaker fit than it looks.
- How the Python `Representation` dataclass crosses the binding — as a pair
  of `(str, list[str])` or as a single parsed string. The parsed string is one
  spelling for the file, the declaration and the conversion target, which is
  an argument for it.
- Fixture size. Small: these are read whole, and the existing fixtures are
  tiny on purpose.

## 8. Gaps named, not papered over

Each of these goes on `architecture/molecules.mdx`:

- **No chemistry format is read.** No SDF, no MOL, no SMILES file, and no
  `.cdx`. RDKit reads those and is better at it, which is D35's first clause
  applied to this milestone rather than argued around.
- **`source_set` is not claimed**, though a merged compound library is
  genuinely assembled. Claiming it would ship an axis the loader cannot fill.
  Closing it means deciding whether a per-row source column can be read as a
  set, which is a different question from a graph's per-edge provenance and
  is not answered here.
- **The engine cannot verify a declared representation.** It has no
  chemistry, by §4.3, so a wrong declaration on a hand-written file passes —
  D38's position exactly, and its limits are the same. The step can check its
  own work and does; nothing checks a user's claim about a file somebody else
  standardised.
- **One standardisation state, one toolkit.** `representation` records the
  operations, not which toolkit applied them. Two sets standardised to the
  same state by RDKit and by OpenEye would compare equal, and §3's
  measurement says nothing about whether they should: it measured RDKit
  against itself across time, not RDKit against another toolkit. Recording
  the toolkit would be a wider claim than the evidence supports, and
  measuring it is the next experiment rather than a line in this milestone.
- **The whole table is read into memory by the canonical form**, as a gene
  list's is, and for the same reason. A library larger than memory is not
  readable today and nothing pretends otherwise.
