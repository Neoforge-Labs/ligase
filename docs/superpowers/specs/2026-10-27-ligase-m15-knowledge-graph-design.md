# M15 — `KnowledgeGraph`

*Written against v0.2 at commit `19c911a` (27 Oct 2026). The first milestone
of v0.3. Read alongside `CLAUDE.md`, §4.3 of
`docs/superpowers/specs/2026-10-22-ligase-v0-2-design.md`,
`web/content/project/decisions.mdx` and
`web/content/project/milestones.mdx`.*

---

## 1. Intent

Give Ligase the `KnowledgeGraph` kind: typed nodes and edges with per-edge
provenance, read from JSON, carrying `organism` (set), `identifier_namespace`,
`source_set`, `subject` and `release`.

Three obligations recorded in v0.2 land on this milestone and are discharged
here, not deferred again:

1. **M7's standing requirement.** The first kind to claim an axis pins that
   axis's `check.rs::compare` and `graph.rs::propagate` arms with named tests.
2. **`source_set`'s dedup rule.** D36 reserved the right to give `source_set`
   its own value type for the milestone that first applies the axis. This is
   that milestone.
3. **A JSON-reading kind.** `architecture/remotes.mdx` records that no kind in
   v0.2 reads JSON, so `response.rs`'s JSON branch is exercised by unit tests
   and never end to end.

## 2. The D35 reckoning, first

> A kind earns its place only if it brings a dimension whose mismatch is a
> documented silent failure in real research. A kind that is a file format
> with a schema is a parser, and Ligase does not need more parsers.

**The dimension it brings: `source_set`.** No kind in Ligase applies it. M8
declined it, M12 declined it with reasons, and M10a settled its comparison
rule while applying it to nothing. A knowledge graph is the first kind in
Ligase that is *assembled* rather than measured: its content is a function of
which evidence sources contributed to it, and that list is not visible in any
single node or edge.

**The silent failure a mismatch causes.** In a knowledge graph built for
target prioritisation, an association's score, and whether an association
exists at all, is a function of which sources were folded in. Add DisGeNET to
a graph that had Open Targets and the literature-mined set, and gene–disease
edges appear, neighbourhoods widen and every centrality- or
propagation-derived ranking moves. Nothing about the resulting file says which
sources went into it, so:

- two graphs that differ *only* in source composition join, diff and score
  against each other without complaint, and the difference between them reads
  as a biological or methodological result;
- a benchmark scored against a graph assembled from a different source list
  measures the assembly, not the method, and comes out plausible;
- an edge present in one and absent in the other is read as evidence of
  absence rather than as absence of a source.

This is the same shape as `identifier_namespace`'s failure, which M8 admitted
an `EntitySet` for: the join succeeds, the result is plausible, and the cause
is bookkeeping. It differs in that the bookkeeping is per-edge provenance the
graph itself carries, which is why the engine can read the dimension off the
file rather than only accept a declaration.

**Two further axes come alive natively, not as padding.** D37 names "a disease
knowledge graph joins human genes to evidence from model organisms" as one of
the four cases that made `organism` set-valued; this is the first kind where
that set is the normal case rather than the host–pathogen exception. And a
graph drawn from a versioned resource has a `release`, compared by equality
(D36) — Open Targets 24.06 against 24.09 is the hg19/hg38 failure in another
costume.

**The honest caveat, recorded rather than smoothed over.** There is no
standard knowledge-graph file format, so this milestone defines the node/edge
JSON shape Ligase reads. Defining a shape is exactly what D35 says is not
sufficient grounds for a kind, and it is not the grounds here: the shape is
the cheapest envelope that can carry per-edge provenance, and the kind is
admitted on `source_set`. A loader for Open Targets' own Parquet dumps, or for
Neo4j or RDF exports, is **not** in this milestone and is named as a gap on
the architecture page rather than implied.

**Verdict: it passes.** It brings `source_set`, which no kind applies, whose
mismatch is silent, plausible and attributable to biology by a reader who
cannot see it.

## 3. Goals, non-goals, anti-goals

**Goals.** The kind, its loader, its canonical form, its dimensions and its
Python surface. `source_set` given its own value type and its own equality.
Both axis arms pinned. The JSON branch exercised end to end. Docs in the same
change.

**Non-goals.** Claim binding (the next v0.3 milestone). A conversion for
graphs — `map_identifiers` over a graph has to rewrite edge endpoints as well
as node ids, which is a different operation from renaming a column, and
registering the existing conversion for the kind would make a false claim
about data. Loaders for third-party graph formats. Graph algorithms of any
kind: Ligase checks what flows along an edge, it does not traverse graphs.

**Anti-goals** — things that would make this a failure even if it worked:

- A kind admitted because JSON was convenient. If `source_set` were dropped
  from it, this kind would be a parser and should not exist.
- `source_set` shipped as an unexercised value type. That is the precise
  condition M7's standing requirement exists to prevent, and M10a refused to
  ship it for that reason.
- A canonical form that reads the parts it understands and skips the rest.
  `canon`'s own module comment calls that the dangerous kind of lenient.
- An inferred source set that can be *empty and known*. Two graphs that each
  recorded no sources would then agree, which is the false agreement the
  engine exists to refuse.

## 4. Approaches considered

### 4.1 How `source_set` gets its own equality

The rule is already settled by M10a and recorded in `dim::names::SourceSet`'s
doc comment: **it must not fold ASCII case.** Folding drops a member and turns
two sets that differ into two that agree; not folding can only add a member
and turn agreement into a loud refusal. What is open is the implementation.

- **(a) Copy `NameSet` into a second struct.** Rejected. D36's own argument
  for sharing the type is that "a rule implemented twice is a rule that will
  eventually be implemented differently", and the set algebra — insertion-order
  dedup, difference, English rendering, length-plus-containment equality — is
  the part worth having exactly once. Only the member comparison differs.
- **(b) A `fold_case: bool` field on `NameSet`.** Rejected. This is the shape
  `Digest::record` / `ordered_record` was deliberately split to avoid: two
  meanings behind one call, chosen by an argument a caller can get wrong
  silently. It would also let one dimension carry two rules at runtime, which
  is the thing D36 forbids outright.
- **(c) `NameSet<M>`, generic over how two members are compared.** *Chosen.*
  `NameSet<FoldsAsciiCase>` is `Organism`, `NameSet<Exactly>` is `SourceSet`.
  The algebra is written once, the two rules are each written once, and the
  two are different types, so a `compare` arm reading the wrong field no
  longer compiles. The default type parameter is the folding one, so existing
  code and tests naming `NameSet` keep their meaning.

(c) costs one trait with one method and makes the compiler enforce what M7's
standing requirement currently asks a test to catch. That is the trade worth
taking.

**What moves with it.** `check.rs::name_set` returns `Option<&NameSet>` for
both axes today and cannot once they are two types. The set-difference detail
in `compare` is shared by `Organism | SourceSet`, and must stay shared, since
it is the sentence a user reads. So `name_set` is replaced by a helper that
returns the *difference clauses* rather than the set: one formatting path, two
typed reads, and no arm able to read the other axis's field.

### 4.2 Where the source set comes from

- **(a) Declared only.** Rejected. The sources are in the file, on the edges;
  refusing to read them would make the engine ask for something it is holding.
- **(b) Inferred as the union of the sources named on nodes and edges, with a
  declaration overriding it.** *Chosen.* Same shape as namespace inference for
  a gene list: declared wins and is recorded rather than checked, inference is
  the fallback, and an inference that cannot be made leaves the dimension
  unknown with a reason. A graph whose nodes and edges name no sources leaves
  `source_set` unknown, never known-and-empty.

This is the decision worth a number, and it is **D44**.

### 4.3 The JSON gap

- **(a) Name it as a gap again.** Rejected. The gap exists because no kind
  read JSON; this kind reads JSON. Deferring it now would be deferring for no
  reason, and M13 and M14's habit is to name gaps that are *out* of reach, not
  ones the milestone walks past.
- **(b) Close it.** *Chosen.* A `RecordedQuery` declared `KnowledgeGraph`
  against a local mock server, replayed with the server down, over the
  existing `mock_http.rs` and `tests/test_queries.py` infrastructure. The
  architecture page then records it as exercised rather than as a gap.

## 5. What gets built

### 5.1 The file shape

JSON, one object, two required arrays:

```json
{
  "nodes": [
    { "id": "ENSG00000141510", "type": "gene",
      "ids": ["HGNC:11998", "P04637"], "sources": ["open-targets"] }
  ],
  "edges": [
    { "source": "ENSG00000141510", "target": "EFO_0000305",
      "type": "associated_with", "sources": ["open-targets", "disgenet"],
      "score": 0.82 }
  ]
}
```

A node needs a string `id`; an edge needs string `source`, `target` and
`type`. Everything else on either is carried without being read, exactly as a
gene list's attribute columns are. `sources` on a node or an edge is the
per-edge provenance §4.3 asks for and is what `source_set` is read from; a
top-level `sources` array is read too, for a graph that records its assembly
in one place. A file that is not this shape does not parse, which by `canon`'s
existing rule makes it hash as its own bytes rather than as the canonical form
of something it is not.

### 5.2 Dimensions

| Axis | Where from |
|---|---|
| `organism` | declared (a graph does not record it) |
| `identifier_namespace` | declared, else inferred from the node ids as for a named list |
| `source_set` | inferred from the `sources` on nodes, edges and the top level; declared overrides |
| `subject` | declared |
| `release` | declared |

Everything else returns `false` from `applies()`: a graph has no contigs, no
coordinate system, no pairing, no quality scores and no sample set.

`identifier_namespace` is **demanded** where two graphs are joined, and where
a graph is joined to either named list — M12's lesson about the mixed pair,
applied before it can become a gap. `source_set`, `subject` and `release` stay
compared-not-demanded, as the catalogue requires.

### 5.3 Canonical form

Nodes as unordered records keyed on their id; edges as unordered records keyed
on `(source, target, type)`; the `ids` and `sources` arrays sorted *within* a
node or edge; every remaining key carried with JSON object keys sorted at
every depth, which `serde_json`'s `BTreeMap` does on re-serialisation.
Top-level fields other than `nodes` and `edges` are hashed as fields, with
`response.rs`'s closed volatile set dropped and a note per drop — the same set
and the same D22/D26 reasoning, and at the top level only, for D26's reason: a
`timestamp` inside a record is what the record says.

Response order is therefore excluded by sorting, and fetch timestamps and
cache paths by the volatile drop, which is what §4.3 asks for.

### 5.4 Surfaces

`DataKind::KnowledgeGraph`, `load::knowledge_graph`,
`canon::knowledge_graph`, `exec::output_suffix` → `.json`,
`canon::describe`, a `KnowledgeGraph` dataset class and `KINDS` entry in
Python, `lg.knowledge_graph(...)`, the `load_knowledge_graph` binding,
`.json` in the CLI's `inspect` loader table, and the two `generate_api.py`
groups.

## 6. Done when

1. The kind exists with its loader, canonical form, dimensions and Python
   surface.
2. Two graphs whose `source_set` disagrees refuse to join, naming the source
   one side lists and the other lacks, pinned in Rust and in Python.
3. `source_set` has its own value type and compares **exactly**:
   `s3://bucket/A.vcf` and `s3://bucket/a.vcf` are two sources and refuse to
   join, while `Organism` still folds `HIV-1` and `hiv-1` into one.
4. Every axis the kind claims has its `check::compare` and `graph::propagate`
   arm pinned by a named test, M7's standing requirement for `source_set`
   discharged by name.
5. A graph's source set is read off its own edges, and a graph naming no
   sources leaves the axis unknown with a reason rather than known and empty.
6. Reordering a graph's nodes or edges does not change its canonical hash;
   adding a source to one edge does.
7. A `RecordedQuery` declared `KnowledgeGraph` runs against a mock server and
   replays with the server down to an identical result, closing the JSON gap
   `architecture/remotes.mdx` names.
8. Docs ship in the same change: concepts, guide, architecture, a
   silent-failures page titled by symptom, `_meta.ts` entries, the dimension
   catalogue, `CHANGELOG.md` under `[Unreleased]`, D44, and the M15 entry with
   its own account of what was met and what was found.
9. All gates green: `cargo fmt --all --check`, `cargo clippy --workspace
   --all-targets -- -D warnings`, `cargo test --workspace`, `ruff check .`,
   `ruff format --check .`, `mypy`, `pytest`, and `npm run check` in `web/`.

## 7. Left to the builder

- The exact wording of the unknown-reason sentences, following the pattern the
  named-list loaders set.
- Whether the volatile-key predicate is lifted out of `remote::response` or
  restated; lifting it is preferred if it costs no visibility the crate does
  not already grant.
- How large the JSON fixtures are. Small: these are read whole, and the
  fixtures that exist are tiny on purpose.

## 8. Gaps named, not papered over

Each of these goes on `architecture/knowledge-graphs.mdx`:

- **No conversion crosses a graph.** `map_identifiers` renames a column; a
  graph's identifiers are node ids *and* edge endpoints, and a conversion that
  rewrote one and not the other would produce a graph whose edges point at
  nothing. Registering the existing conversion for the kind would be the false
  claim D38 refuses.
- **No third-party graph format is read.** Open Targets' Parquet, Monarch's
  dumps, Neo4j and RDF exports are all outside this milestone.
- **The whole file is parsed into memory.** Bounded like a recorded body is
  and for the same reason: sorting keys at every depth needs the document
  whole. A graph larger than memory is not readable today and nothing pretends
  otherwise.
