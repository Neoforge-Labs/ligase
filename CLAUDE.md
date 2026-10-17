# CLAUDE.md — Ligase

## What Ligase is
A Rust-powered, Python-facing framework for computational biology pipelines that are correct and reproducible by construction. Its core idea: biological data types carry dimensions (organism, reference build, coordinate system, sample set), and the engine refuses to connect steps whose dimensions don't match — like units in physics.

## Why it exists
The worst comp bio bugs are silent: hg19 vs hg38 mixups, 0- vs 1-based coordinates, "chr1" vs "1", shuffled sample IDs. Workflow tools (Snakemake, Nextflow) move files but don't understand them. Ligase does.

## MVP scope (v0.1) — build in this order
1. Type system: biological types with dimensions; dimensions inferred from file headers where possible, otherwise declared once by the user. Type check at graph-build time, before any compute.
2. Dependency graph: built from decorated, type-annotated Python functions without executing them.
3. Content-addressed cache: step key = hash(function code, parameters, input content hashes, external tool versions). BLAKE3 hashing; size+mtime prefilter; SQLite metadata; content-addressed output store.
4. Early cutoff (unchanged output → downstream skipped) and per-sample fan-out.
5. Provenance: record versions, params, references, citations; generate a methods-section paragraph and a reproducibility bundle.
Out of scope for v0.1: auto-fixing mismatches, public-data fetching, cluster execution.

## Nondeterminism rules
- Random seeds are explicit, typed, hashed parameters.
- Semantic hashing: canonicalise outputs before hashing (sort records, strip volatile headers like timestamps and @PG lines).
- Numeric types may declare a tolerance for equality.
- Remote data is pinned by accession + version + checksum.
- Audit mode: run a step twice and report whether it is reproducible.

## Architecture
- `crates/ligase-core` (Rust): types, dimension checking, graph, cache, execution, provenance.
- `crates/ligase-py` (Rust, PyO3 + maturin): Python bindings.
- `python/ligase`: user-facing API, decorators, CLI.
- Conversions (e.g. liftover) are typed functions registered in a conversion graph — this enables v2 auto-fixing via path search.

## Reference demo
HIV-1 reads → alignment to HXB2 → variant calling → drug-resistance calls. Every feature must work end to end on this pipeline.

## Engineering rules
- Rust: stable toolchain, `cargo fmt`, `cargo clippy -D warnings`, no `unwrap` outside tests.
- Python: ruff, mypy strict, pytest.
- Every public function gets tests. Every bug fix gets a regression test.
- Small, focused commits with conventional commit messages.
- Ask before adding a dependency or changing a public API.
- Before each milestone, re-read the design docs in `docs/` (`golden-example.md`, `dimensions.md`, `milestones.md`, `decisions.md`). Work one milestone per session: plan first, write failing tests, implement, then write docs.

## Documentation rule (mandatory)
Every feature ships with docs in the same change:
- `docs/concepts/<feature>.md` — what it is and why it exists
- `docs/guide/<feature>.md` — how to use it, with a runnable example
- `docs/architecture/<component>.md` — how it works internally, design decisions, trade-offs
- Update `docs/index.md` and `CHANGELOG.md`.
A feature is not done until its docs are written. Docs use MkDocs Material.