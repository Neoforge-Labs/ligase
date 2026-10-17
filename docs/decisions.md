# Decision Records

## D1 — Rust core, Python surface
Scientists live in Python; correctness-critical logic (types, hashing, graph, cache) needs speed and safety. PyO3 + maturin give a single `pip install`.

## D2 — Content hashing, not timestamps
Timestamps lie (copies, clones, touched files). Cache keys hash code, parameters, input content and tool versions.

## D3 — BLAKE3 with size+mtime prefilter
BLAKE3 is fast enough for multi-GB BAMs; the prefilter avoids rehashing unchanged files.

## D4 — Semantic hashing
Outputs are canonicalised before hashing (sorted records, volatile headers stripped) so scientifically identical results share a hash.

## D5 — Dimensions as types (units analogy)
Errors are caught before compute. Conversions are typed functions in a conversion graph, enabling v2 auto-fixing via path search.

## D6 — SQLite metadata, content-addressed store
Zero-config, embedded, portable; no server required.

## D7 — HIV-1 reference pipeline
One real, validated pipeline drives every feature and ties Ligase to SAWA.

## D8 — Out of scope for v0.1
Auto-fixing, public-data fetching, cluster execution, GUI.
