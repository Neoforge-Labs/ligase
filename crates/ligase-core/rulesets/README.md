# Vendored resistance algorithms

Two files per algorithm live here:

- `HIVDB_<version>.xml` — Stanford's ASI2 algorithm definition, **unmodified**,
  pinned by SHA-256 in `PROVENANCE.tsv`.
- `HIVDB_<version>.ruleset` — the same algorithm in the small tab-separated
  format the engine parses, **derived** from the XML by
  `scripts/vendor_hivdb_algorithm.py`.

Both are committed. The XML is what you audit against upstream; the
`.ruleset` is what Ligase reads. `tests/test_rulesets.py` re-derives every
`.ruleset` from its XML on every test run, so the two can never drift apart
unnoticed.

## Why these are vendored rather than fetched

`CLAUDE.md` requires remote data to be pinned by accession, version and
checksum, and `web/content/project/decisions.mdx` D8 puts public-data
fetching out of scope for v0.1. More to the point, an interpretation that reaches the network to
learn what a mutation means is not reproducible: the answer depends on the
day you ran it, and a cache key cannot see that.

Stanford operates a web service (Sierra) that does exactly this
interpretation. Ligase does not call it, at run time or in CI. The one
place Stanford's service appears in this repository is
`crates/ligase-core/tests/fixtures/hivdb/`, where expected scores for a set
of mutation patterns were obtained once, recorded with the date and the
algorithm version, and committed — an oracle for the scoring engine that
is not derived from the same XML the engine reads.

## Licence

The algorithm files come from Stanford's own repository,
[`hivdb/hivfacts`](https://github.com/hivdb/hivfacts), which carries the
**Unlicense** — a dedication of the work to the public domain:

> This is free and unencumbered software released into the public domain.
>
> Anyone is free to copy, modify, publish, use, compile, sell, or
> distribute this software, either in source code form or as a compiled
> binary, for any purpose, commercial or non-commercial, and by any means.

Nothing in Stanford's Terms of Use restricts copying or redistribution, and
the ASI exchange format was designed in 2003 precisely so that genotypic
interpretation systems would not be locked inside proprietary formats.

Two neighbouring repositories are deliberately **not** used here.
`hivdb/sierra`, the web service, is GPL-3.0, and Ligase is MIT OR
Apache-2.0, so no code is taken from it. `hivdb/asi_interpreter`, the Java
reference implementation, is Apache-2.0; it was read as a specification for
what the rule language means, and no code was copied.

## Citation

Stanford asks that the general review be cited:

> Rhee SY, Gonzales MJ, Kantor R, Betts BJ, Ravela J, Shafer RW. Human
> immunodeficiency virus reverse transcriptase and protease sequence
> database. *Nucleic Acids Research* 2003;31(1):298–303.

`ligase methods` emits this citation for any pipeline whose steps
interpret resistance, from the curated table described in D24.

## Updating to a newer algorithm

1. Download the file from `hivdb/hivfacts`, unmodified, into this
   directory.
2. Add a row to `PROVENANCE.tsv` with its SHA-256, the commit it came
   from, and the `ALGNAME`/`ALGVERSION`/`ALGDATE` inside it.
3. Run `scripts/vendor_hivdb_algorithm.py --write`.
4. Run the test suite. The validation in `tests/test_validation.py` is
   pinned to 9.8, because that is the version the published results it
   checks against were produced with; a newer algorithm is expected to
   disagree with them, and that disagreement is science, not a regression.
