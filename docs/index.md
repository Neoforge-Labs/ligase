# Ligase

Type-safe, reproducible computational biology pipelines. A Rust core with a
Python surface.

## The idea

Biological data types carry **dimensions** — organism, reference build,
coordinate system, sample set — and the engine refuses to connect steps whose
dimensions do not match, the way a physics engine refuses to add metres to
seconds.

The worst comp bio bugs are silent. hg19 against hg38. 0-based against
1-based. `chr1` against `1`. Sample IDs that shifted by one somewhere in a
join. None of these crash; they produce plausible numbers that are wrong.
Workflow managers like Snakemake and Nextflow move files between steps very
well, but they do not understand what is in them. Ligase does, and it checks
before it computes.

```python
pipe.check()      # type-checks dimensions; no compute
pipe.run()        # parallel, cached, per-sample fan-out
pipe.methods()    # generated methods paragraph + reproducibility bundle
```

When a pipeline is wrong, you should find out in milliseconds rather than
after an overnight run:

```text
DimensionError in step `call`:
  aln.reference = HXB2 (from BAM @SQ header)
  ref.reference = hg38
  These must match. Did you pass the wrong reference?
```

## Status

**Pre-alpha**, building towards v0.1. Milestone **M0 (scaffold)** is complete:
the workspace builds, `pip install -e .` works, `import ligase` succeeds, and
CI runs the full gate.

Nothing in [the golden example](golden-example.md) works yet beyond version
reporting. The type system arrives in M1. See
[milestones](milestones.md) for the order and the acceptance test for each
step.

## Where to go

| If you want to | Read |
|---|---|
| Install it and run the checks | [Installation](guide/installation.md) |
| Understand how the project is put together | [Project layout](concepts/project-layout.md) |
| Know why the workspace is shaped this way | [Workspace architecture](architecture/workspace.md) |
| See the target API | [Golden example](golden-example.md) |
| Know which dimensions exist and how they are inferred | [Dimensions](dimensions.md) |
| Follow the build order | [Milestones](milestones.md) |
| Understand a design choice | [Decisions](decisions.md) |

## Names

| Thing | Name |
|---|---|
| Rust core crate | `ligase` |
| PyPI distribution | `ligase-bio` |
| Python import | `ligase` |

## Licence

Dual-licensed under either MIT or Apache-2.0, at your option.
