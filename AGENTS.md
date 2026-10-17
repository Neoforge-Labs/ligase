# AGENTS.md — Ligase

Ligase is a Rust core with Python bindings (PyO3/maturin) for type-safe, reproducible computational biology pipelines. Read CLAUDE.md for the full vision, MVP scope and design rules.

## Setup
- `maturin develop` to build bindings
- `cargo test` and `pytest` must both pass

## Rules
- Follow the MVP build order in CLAUDE.md; don't implement out-of-scope features.
- Run `cargo fmt`, `cargo clippy -D warnings`, `ruff`, `mypy --strict` before finishing.
- Every feature includes concepts, guide and architecture docs plus a CHANGELOG entry.
- Validate everything against the HIV-1 reference demo pipeline.
- Ask before adding dependencies or changing public APIs.