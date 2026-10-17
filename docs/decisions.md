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

## D9 — Coordinates are semantics, not encoding

The `coordinates` dimension records what a step sees, not the on-disk
convention. SAM and BAM both present 1-based closed alignment records, so
re-encoding between them changes nothing. BED (0-based half-open) and GFF
(1-based closed) differ *semantically*, and that is the difference worth
catching.

The rejected alternative — BAM 0-based because its binary encoding is — would
make `samtools view -b` appear to change a dimension. False alarms are not a
safe failure mode: a checker that cries wolf gets bypassed, and then the real
hg19/hg38 mixup goes through unchallenged.

## D10 — References relate by containment

Two references compared with each other must be identical. Data checked
*against* a reference need only be contained in it: every contig the data
mentions must appear in the reference with a matching length and MD5, while
extra contigs in the reference are fine.

Strict equality everywhere would reject a VCF whose header lists only the
contigs it has variants on — correct input, rejected. Contig order still gets
its own error, because htslib matches by name but GATK requires matching
sequence dictionary order.

## D11 — `noodles` for format I/O, and the MSRV follows it

Format parsing uses [`noodles`](https://github.com/zaeleus/noodles) rather than
`rust-htslib`. It is pure Rust, so wheels build without a C htslib on the
machine — which is the difference between `pip install ligase-bio` working and
not. The cost is that `noodles` is a pre-1.0 crate that bumps minor versions
often, and its MSRV sets ours (currently Rust 1.91).

MD5 arrives via `md-5`. That is not a security choice: MD5 is what the SAM
specification prescribes for the `@SQ M5` contig checksum, so we must compute
exactly that digest to compare against it. Cache keys use BLAKE3 (D3); the two
are unrelated.
