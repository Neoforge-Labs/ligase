# Bioconda recipe

`meta.yaml` here is the recipe for publishing `ligase-bio` to
[Bioconda](https://bioconda.github.io). It is kept in this repository so it
can be reviewed and updated alongside a release, but **Bioconda builds from
its own repository, not this one** — this copy is the source, not the thing
that gets built.

Bioconda matters because conda, not pip, is how much of computational biology
installs software: it is where tool recommendations point, and a package
absent from it is effectively invisible to a large part of the field.

## Submitting it

Recipes are added by pull request to
[bioconda/bioconda-recipes](https://github.com/bioconda/bioconda-recipes):

1. Fork that repository.
2. Copy `meta.yaml` to `recipes/ligase-bio/meta.yaml` on a branch.
3. Open a pull request. Bioconda's CI builds the recipe on Linux and macOS and
   a human reviews it; expect iteration rather than a merge on the first try.

Read their [contributing
guide](https://bioconda.github.io/contributor/index.html) first — the
conventions are theirs and they change.

## Why the recipe looks like this

**A C compiler as well as Rust.** `rusqlite` is built with its `bundled`
feature, so SQLite is compiled from C source. A Rust-only build environment
is not enough.

**`cargo-bundle-licenses`.** Rust links its dependencies statically, so the
built artifact carries their licence terms. Bioconda and conda-forge expect
those to be collected into `THIRDPARTY.yml` and shipped, which is why it is
both a build dependency and a `license_file`.

**`skip: true  # [py<311]`.** The extension is built against Python's stable
ABI from 3.11, which is also the floor the package declares.

**A test that the rulesets survived.** The vendored Stanford HIVdb algorithm
files live inside the package, and the engine cannot score without them. A
build that dropped them would install cleanly and fail at a user's first
interpretation, so the recipe asserts `hivdb.ALGORITHMS` is non-empty.

## Keeping it current

On each release, update `version` and `sha256`, and reset `build.number` to
`0`. The checksum is the sdist's, from PyPI:

```bash
curl -sL "https://pypi.org/packages/source/l/ligase-bio/ligase_bio-<version>.tar.gz" | shasum -a 256
```

Bioconda's bots often open the version-bump pull request themselves once the
package is in the channel, so after the first submission this is usually a
review rather than a chore.
