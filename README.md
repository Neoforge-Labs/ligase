# Ligase

Type-safe, reproducible computational biology pipelines. A Rust core with a
Python surface.

Biological data types carry **dimensions** — organism, reference build,
coordinate system, sample set — and the engine refuses to connect steps whose
dimensions do not match, the way a physics engine refuses to add metres to
seconds. The worst comp bio bugs are silent: hg19 vs hg38 mix-ups, 0- vs
1-based coordinates, `chr1` vs `1`, shuffled sample IDs. Workflow tools move
files; Ligase understands them.

Status: **v0.1.0**. All six v0.1 milestones are done — the type system, the
graph, the content-addressed cache, semantic hashing and audit mode,
provenance, and validation against published resistance results. See
[the milestones](https://ligase.neoforgelabs.tech/project/milestones/)
for what each one had to prove.

## Intended use

Ligase is **research software**. The drug-resistance calls in its reference
pipeline come from applying the Stanford HIVdb algorithm to variants Ligase
called, and they are **not for clinical use** — not for diagnosis, not for
treatment decisions, and not a substitute for a validated clinical assay or a
clinician's judgement.

The validation in
[M6](https://ligase.neoforgelabs.tech/project/validation/) checks Ligase's calls against
published results on two public datasets. That is evidence the implementation
is faithful to the algorithm. It is not evidence that it is fit for patient
care, and it was never meant to be.

## Install

```bash
pip install ligase-bio
```

Wheels cover Linux, macOS and Windows for Python 3.11 and newer, so this needs
no compiler. Building from source instead needs a Rust toolchain *and* a C
compiler, because SQLite is compiled from bundled source — see
[the installation guide](https://ligase.neoforgelabs.tech/start/installation/).

For development:

```bash
git clone git@github.com:Neoforge-Labs/ligase.git && cd ligase
pip install -e ".[dev]"
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
