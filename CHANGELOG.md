# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

**M0 — Scaffold.** The project builds, installs and checks itself. No pipeline
functionality yet; the engine's only behaviour is reporting its own version.

- Cargo workspace with `crates/ligase-core` (published as the `ligase` crate)
  and `crates/ligase-py` (PyO3 bindings compiled to the private
  `ligase._core`), sharing one version and one lint configuration.
- Python package at `python/ligase`, distributed as `ligase-bio` and imported
  as `ligase`. Built with maturin against `abi3-py311`, so one compiled
  artifact serves Python 3.11 and later. Ships `py.typed` and a stub for the
  extension module, so `mypy --strict` passes for users too.
- `__version__` is read from the compiled Rust rather than hardcoded, with
  tests asserting it matches the workspace manifest — a wheel cannot report a
  version it was not built from.
- GitHub Actions CI: `cargo fmt`, `cargo clippy -D warnings` and `cargo test`;
  the Python gate (`pip install -e .`, `ruff check`, `ruff format --check`,
  `mypy --strict`, `pytest`, `import ligase`) across Python 3.11 and 3.14 on
  Linux and macOS; `mkdocs build --strict`; and `shellcheck` over `scripts/`.
- Clippy denies `unwrap_used` and `expect_used` workspace-wide, turning the
  "no `unwrap` outside tests" rule into a build error. `missing_docs` is a
  warning locally and an error in CI.
- `scripts/fetch_test_data.sh` fetches the HIV-1 reference pipeline inputs
  into a gitignored `data/`: HXB2 (NCBI `K03455.1`) and the four smallest
  paired runs from each of PRJNA517147 and PRJNA1218827. Reads are verified
  against ENA's per-file MD5 and come over HTTPS, so no SRA toolkit is
  needed. The reference is pinned by the SHA-256 of its canonical sequence
  rather than of the raw file, so an upstream reflow or defline change does
  not break the build while a real sequence change still fails.
- MkDocs Material documentation site, with the design docs published
  alongside the concepts, guide and architecture pages.
- Dual MIT / Apache-2.0 licensing.

### Changed

- CLAUDE.md gains an engineering rule: re-read the design docs before each
  milestone, and work one milestone per session as plan, failing tests,
  implementation, docs.

[Unreleased]: https://github.com/Neoforge-Labs/ligase/commits/main
