# Workspace architecture

How the M0 scaffold is wired, and what each choice costs.

## Shape

```text
Cargo.toml                 workspace root: shared version, lints, deps
├── crates/ligase-core/    [package] name = "ligase"   -- the engine
└── crates/ligase-py/      [lib] name = "_core"        -- cdylib bindings
pyproject.toml             maturin backend, dist "ligase-bio"
python/ligase/             python-source root
```

One version number lives in `[workspace.package]` and everything inherits it
with `version.workspace = true`. The Python distribution takes it too, via
`dynamic = ["version"]`, which maturin fills from the crate it builds. There
is exactly one place to bump a release, and no way for the Rust and Python
halves to disagree.

That last part is tested rather than assumed:

```python
def test_version_matches_workspace_manifest() -> None:
    import ligase
    assert ligase.__version__ == workspace_version()   # parsed from Cargo.toml


def test_core_version_comes_from_rust() -> None:
    import ligase
    assert ligase._core.version() == ligase.__version__
```

The second test is the one that matters. It would still pass if
`__version__` were hardcoded to the right string — so it is paired with the
first, which reads the manifest. Together they fail if the Python package
reports a version the engine was not built from.

## The build

`maturin` is the PEP 517 backend. The relevant configuration:

```toml
[tool.maturin]
manifest-path = "crates/ligase-py/Cargo.toml"
module-name = "ligase._core"
python-source = "python"
```

This is maturin's **mixed layout**: a hand-written Python package with a
compiled module dropped inside it. `python-source = "python"` tells maturin
the package root is `python/`, and `module-name` places the shared object at
`python/ligase/_core.abi3.so`. The result is that `pip install -e .` produces
one importable `ligase` whose `__init__.py` you can edit without rebuilding,
containing a `_core` you cannot.

### abi3

The bindings build against `abi3-py311` — Python's stable ABI, from 3.11
onwards. One compiled artifact works on 3.11, 3.12, 3.13, 3.14 and later.

The cost is real: abi3 closes off parts of the CPython C API, so some PyO3
features are unavailable, and a `#[pyclass]` under abi3 is marginally slower
than a version-specific build. We accept that. The alternative is a build
matrix with one wheel per Python version per platform, which multiplies CI
time and release surface by four or five. For a project whose hot path will
be hashing files and shelling out to aligners, the per-call overhead of a
stable-ABI class is not where the time goes.

The floor is 3.11 rather than 3.10 for two reasons: 3.10 reaches end of life
in October 2026, and `tomllib` is in the standard library from 3.11, which the
version test uses to read `Cargo.toml` without a dependency.

### Why the engine has no `pyo3` dependency

`ligase-core` does not depend on `pyo3` at all; `ligase-py` depends on both.
This is enforced by the manifests rather than by discipline.

The payoff is that the engine's test suite is plain `cargo test` with no
Python interpreter in the loop, which keeps the inner development loop fast
and means a type error in the engine surfaces in milliseconds. It also keeps
the engine usable from Rust, which matters for the conversion graph in v2.

The cost is a translation layer that must be written by hand, and the
temptation to let types leak across it for convenience. The leading
underscore on `_core` is the reminder that the layer is allowed to change.

## Lints as rules

CLAUDE.md says "no `unwrap` outside tests". Rather than leaving that to code
review, the workspace makes it a build error:

```toml
[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "deny"

[workspace.lints.rust]
missing_docs = "warn"
```

`missing_docs` is a warning locally and an error in CI, where clippy runs with
`-D warnings`. So an undocumented public item fails the build — the
documentation rule, applied to code.

Test files opt out explicitly, at the top of the file:

```rust
#![allow(clippy::unwrap_used, clippy::expect_used)]
```

That is the point of the design. An `unwrap` in a test is fine and an
`unwrap` in the engine is not, and the opt-out is visible in the diff when
someone adds a file that needs it.

## Trade-offs taken

**One repository, not three.** The engine, the bindings and the Python package
version and release together. A scientist installing Ligase should never have
to reason about which engine version their Python package wants. The cost is
that the Rust crate's releases are coupled to the Python package's, which
would matter if other Rust programs depended on `ligase` — none do yet.

**Source distribution only, for now.** Installing needs a Rust toolchain,
which is a real barrier for the target user. Pre-built wheels via
`maturin-action` are the fix, and belong with the first release rather than
with the scaffold; publishing wheels for a package that cannot yet check a
pipeline would be premature.

**`ruff` does not see `docs/`.** Recent ruff formats Python inside fenced
Markdown blocks, which means it wants to reformat
[the golden example](../golden-example.md) — adding blank lines between
decorated functions and de-aligning its comments. That document is the
specification the engine is built to satisfy, and its shape is deliberate, so
`extend-exclude = ["docs"]` keeps the formatter out of it. The trade is that
example code in the docs is not machine-checked. Once the API exists, the
right fix is doctests that actually run, not a formatter that rewrites prose.

**Four CI jobs, eight job-runs.** The Python gate runs on 3.11 and 3.14
across Linux and macOS. abi3 means the compiled artifact is not
version-specific, so the matrix is really testing that the *build* works
across interpreters and platforms, not the extension's behaviour on each.
Two versions at the ends of the supported range catch that; testing all four
would buy very little.

## What M0 deliberately leaves out

No dimension types, no graph, no cache, no execution. The only behaviour the
engine has is reporting its own version.

Dependencies were kept to the minimum that a scaffold needs: `pyo3` and
nothing else. `blake3`, `rusqlite` and a graph library arrive with the
milestones that need them ([M3](../milestones.md)), and the BAM and VCF
header parsing that [M1](../milestones.md) requires will need a format
library — `noodles` being the obvious candidate. Adding them now would mean
guessing at an interface before the thing that uses it exists.

## Related

- [Project layout](../concepts/project-layout.md) — the three layers and why
- [Installation](../guide/installation.md) — the developer loop
- [Decisions](../decisions.md) — D1 on the Rust/Python split, D4 on
  canonicalisation
