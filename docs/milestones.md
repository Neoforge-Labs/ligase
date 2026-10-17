# Milestones and Acceptance Tests

One milestone per session. Plan → write tests → implement → write docs.

## M0 — Scaffold
Cargo workspace (`ligase-core`, `ligase-py`), maturin, `python/ligase`, CI running fmt, clippy, ruff, mypy, cargo test, pytest.

**Done when:** `pip install -e .` works and `import ligase` succeeds in CI.

## M1 — Types and dimensions
**Done when:**
- HXB2 FASTA and a sample BAM are loaded with dimensions inferred from headers.
- A BAM aligned to a different reference fails `check()` with a clear `DimensionError`.
- A `chr1` vs `1` mismatch raises a distinct error.

## M2 — Graph
**Done when:** the golden example builds a graph without executing anything, and `ligase check` reports all type errors in one pass.

## M3 — Execution and cache
**Done when:**
- The demo runs end to end on 3–4 runs from PRJNA517147.
- A second run completes entirely from cache.
- Changing `min_freq` reruns only `call` and `resistance`.
- Adding one sample processes only that sample.

## M4 — Semantic hashing and early cutoff
**Done when:** rerunning alignment with a different thread count produces the same canonical hash and downstream steps are skipped.

## M5 — Provenance
**Done when:** `ligase methods` outputs a methods paragraph with tool versions, parameters, reference accession and citations, plus a bundle that reproduces the results on a clean machine.

## M6 — Validation
**Done when:** resistance calls match published results for PRJNA517147 and the Sanger ground truth for PRJNA1218827, with discrepancies explained.
