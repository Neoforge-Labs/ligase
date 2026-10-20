# Releasing

Ligase ships twice from one tag: the engine as the **`ligase`** crate on
crates.io, and the Python package as **`ligase-bio`** on PyPI.

Publishing is never automatic. `.github/workflows/release.yml` runs on a
manual trigger whose default uploads nothing, and on a `v*` tag.

## Why wheels matter here

`rusqlite` is built with its `bundled` feature, so building from source
compiles SQLite from C as well as the Rust engine. Someone installing from an
sdist needs a Rust toolchain *and* a C compiler. For the people this is for —
scientists, often in a notebook — that is a wall, so the release builds wheels
and the sdist is only a fallback.

Because `crates/ligase-py` is built against `abi3-py311`, one wheel serves
every Python from 3.11 upwards. The matrix is five builds, not twenty:

| Platform | Target |
|---|---|
| Linux | `x86_64`, `aarch64` (manylinux) |
| macOS | `aarch64`, `x86_64` |
| Windows | `x64` |

A wheel that still needs a compiler would defeat the point, so the
`verify-wheel` job installs the Linux wheel into a fresh virtualenv with
`cargo` removed from `PATH` and fails if `import ligase` or `ligase --help`
does not work. It fails loudly if `cargo` is still reachable, because then the
test would prove nothing.

## One-time setup

Two steps need account access and cannot be done from a workflow or by an
assistant. Do them once.

### 1. PyPI Trusted Publishing

Trusted Publishing lets PyPI verify the workflow's OIDC identity, so **no API
token is ever stored in this repository**. Create a pending publisher at
<https://pypi.org/manage/account/publishing/> with exactly these values:

| Field | Value |
|---|---|
| PyPI project name | `ligase-bio` |
| Owner | `Neoforge-Labs` |
| Repository name | `ligase` |
| Workflow name | `release.yml` |
| Environment name | `pypi` |

Repeat it on <https://test.pypi.org/manage/account/publishing/> with the
environment `testpypi` if you want to rehearse against TestPyPI.

Then create the matching GitHub environments (Settings → Environments):
`pypi`, `testpypi`, and `crates-io`. Adding required reviewers to `pypi` and
`crates-io` is worth it — publishing cannot be undone.

### 2. A crates.io token

crates.io has no OIDC equivalent, so this one is a secret. Generate a token
scoped to `publish-new` and `publish-update` at
<https://crates.io/settings/tokens>, then add it as `CARGO_REGISTRY_TOKEN` in
the `crates-io` environment — not as a repository-wide secret, so it is only
readable by the job that publishes.

If you would rather not store a token at all, skip this and run
`cargo publish -p ligase` from your own machine after `cargo login`. The
workflow's rehearsal step still checks the crate packages correctly.

## Releasing

1. Make sure `main` is green, including the `demo` job.
2. Move the `CHANGELOG.md` entries under `[Unreleased]` into a new dated
   version section, and add its link at the bottom.
3. Bump `version` in the workspace `[workspace.package]`. It is the single
   source of truth: the crate, the Python distribution and `ligase.__version__`
   all inherit it, and a test asserts they agree.
4. Rehearse. Run the workflow with **`dry-run`**: it builds every wheel, the
   sdist, verifies the wheel needs no toolchain, and runs
   `cargo publish --dry-run`, uploading nothing.
5. Optionally run it with **`testpypi`** and install from there.
6. Tag and push: `git tag -a v0.1.0 -m "0.1.0" && git push origin v0.1.0`.
   The tag publishes to both registries.

A manual run with **`publish`** does the same thing without a tag, for the
case where a tag already exists.

## What cannot be undone

Both registries are append-only. crates.io has no delete, only `cargo yank`,
which stops new dependents resolving to a version but leaves it downloadable.
PyPI deletion is possible but the version number is burned forever: `0.1.0`
can never be uploaded again, even deleted.

So the version number is spent the moment it uploads. Rehearse with `dry-run`
first — that is what it is for.
