# Ligase

Type-safe, reproducible computational biology pipelines. A Rust core with a
Python surface.

Biological data types carry **dimensions** — organism, reference build,
coordinate system, sample set — and the engine refuses to connect steps whose
dimensions do not match, the way a physics engine refuses to add metres to
seconds. The worst comp bio bugs are silent: hg19 vs hg38 mix-ups, 0- vs
1-based coordinates, `chr1` vs `1`, shuffled sample IDs. Workflow tools move
files; Ligase understands them.

Status: **pre-alpha**, building towards v0.1. See
[docs/milestones.md](docs/milestones.md).

## Install

```bash
pip install -e ".[dev]"   # needs a stable Rust toolchain
```

## Names

| Thing | Name |
|---|---|
| Rust core crate | `ligase` |
| PyPI distribution | `ligase-bio` |
| Python import | `ligase` |

## Licence

Dual-licensed under either [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option.
