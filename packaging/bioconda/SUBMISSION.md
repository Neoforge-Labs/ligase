# The Bioconda submission, ready to send

This is the pull request that has **not** been opened. It is written out so
that what gets approved is a concrete thing rather than an intention.
Everything below was checked on 30 October 2026 against `meta.yaml` in this
directory at version 0.4.0.

## What would be done

1. Fork `bioconda/bioconda-recipes` to the account that will own the PR.
2. Branch from its `master`: `add-ligase-bio`.
3. Copy this directory's `meta.yaml` to `recipes/ligase-bio/meta.yaml`. One
   file; nothing else is added. The recipe carries its build commands as a
   list under `build/script` rather than in a separate `build.sh`, which
   `recipes/igua/meta.yaml` in the channel already does with the same two
   commands — a `pip install` and a `cargo-bundle-licenses`. Many Rust
   recipes do use a `build.sh`, and a reviewer may ask for one; it would be
   a mechanical change.
4. Open the pull request with the body below.
5. Bioconda's CI builds on Linux and macOS and a human reviews. Expect
   iteration rather than a merge on the first try.

The fork and the pull request are outward-facing, on someone else's
repository, under a named GitHub account. They need Kelyn's explicit word
before either happens.

## Proposed pull request title

```
Add ligase-bio 0.4.0
```

## Proposed pull request body

```markdown
Adds a recipe for `ligase-bio` 0.4.0, a Rust engine with a Python API for
computational biology pipelines that are checked before they run: biological
datasets carry their dimensions — organism, reference build, coordinate
convention, contig naming, sample set — and the engine refuses to connect
two steps whose dimensions disagree, at graph-build time, before anything
computes.

- Source: https://github.com/Neoforge-Labs/ligase
- Docs: https://ligase.neoforgelabs.tech
- PyPI: https://pypi.org/project/ligase-bio/
- Licence: MIT OR Apache-2.0 (dual, Rust convention; both files shipped)

Notes for the reviewer, all of them reasons the recipe looks the way it does:

- **A C compiler as well as Rust.** `rusqlite` is built with its `bundled`
  feature, so SQLite is compiled from C source; a Rust-only build environment
  is not enough. `{{ stdlib('c') }}` accompanies `{{ compiler('c') }}`.
- **`cargo-bundle-licenses`.** Rust links statically, so the artifact carries
  its dependencies' terms. They are collected into `THIRDPARTY.yml` and
  shipped via `license_file`.
- **`skip: true  # [py<311]`.** The extension is built against Python's
  stable ABI from 3.11, which is also the floor the package declares
  (`requires-python >=3.11`).
- **`run_exports` pins to the minor, not the major.** The package is pre-1.0
  and a minor release can deliberately refuse a pipeline the release before
  it accepted, so a downstream package built against 0.4 should not be
  assumed to work against 0.5.
- **The last test command is not boilerplate.** The vendored Stanford HIVdb
  algorithm definitions ship inside the package and the engine cannot score
  without them. A build that dropped them would install cleanly and fail at
  a user's first interpretation, so the recipe asserts
  `hivdb.ALGORITHMS` is non-empty.

Research-use note, since the package scores HIV-1 drug resistance: the
resistance calls are not for clinical use, and the README and documentation
say so.
```

## What was verified before writing this, and how

| Checked | How | Result |
|---|---|---|
| sha256 of the sdist | downloaded from PyPI and hashed | `7d75be04…2daa1b`, matches the recipe |
| the `url` template resolves | `curl` followed it to `files.pythonhosted.org` | HTTP 200, 708,465 bytes |
| `LICENSE-MIT`, `LICENSE-APACHE` exist in the sdist | listed the tarball | both present at the root |
| `Cargo.lock` is in the sdist | listed the tarball | present, so the Rust build is reproducible |
| the HIVdb rulesets are in the sdist | listed the tarball | `HIVDB_9.8` and `HIVDB_10.2`, with `PROVENANCE.tsv` |
| the build backend matches `host:` | read the sdist's `pyproject.toml` | `build-backend = "maturin"`, `maturin>=1.15,<2.0` |
| every `test:` command | ran all five verbatim against an installed 0.4.0 | all pass; `ALGORITHMS` is `['HIVDB_10.2', 'HIVDB_9.8']` |
| the recipe renders | Jinja2 with stubbed conda macros, then YAML parse | parses; build number 0, `stdlib` present |
| new recipes still use `meta.yaml` | 60 most recent merged recipe PRs in `bioconda-recipes` | 60/60 `meta.yaml`, none `recipe.yaml` |
| the Rust licence-bundling guidance | Bioconda's contributor guidelines | still `cargo-bundle-licenses` into `THIRDPARTY.yml` |
| `stdlib` and `run_exports` | Bioconda's linting page | `compiler_needs_stdlib_c` and `missing_run_exports` are real checks; both were missing and are now added |

## What was not verified

**The recipe has not been built.** There is no `conda`, `conda-build`,
`rattler-build` or `bioconda-utils` on this machine, and acquiring the
Bioconda build image plus a Rust toolchain is a multi-gigabyte download. So
the evidence above is that every input the recipe names is real and every
test it declares passes against an installed 0.4.0 — not that a conda build
of it succeeds. The first real build will be Bioconda's CI.

The two lint fixes were made from the published list of checks and from
existing recipes in the channel — `pybigtools` and `snapatac2` for the
maturin build, and `igua` for the list-form script and the identical
`run_exports` line — not from a linter run.
