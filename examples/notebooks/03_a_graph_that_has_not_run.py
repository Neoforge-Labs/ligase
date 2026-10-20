# /// script
# requires-python = ">=3.11"
# dependencies = ["marimo", "ligase-bio"]
# ///
"""A whole pipeline, checked without executing a single step."""

import marimo

__generated_with = "0.25.2"
app = marimo.App(width="medium")


@app.cell
def _():
    from pathlib import Path

    import marimo as mo

    import ligase as lg

    fixtures = Path(__file__).resolve().parents[2] / "crates/ligase-core/tests/fixtures"
    return fixtures, lg, mo


@app.cell
def _(mo):
    mo.md(
        """
        # 3 — A graph that has not run

        A Ligase pipeline is built from the *signatures* of its steps. The
        bodies are never called while the graph is assembled, so a pipeline
        can be wrong in a way you find out about immediately rather than
        after an overnight run.
        """
    )
    return


@app.cell
def _(fixtures, lg):
    samples = lg.samples(str(fixtures / "reads_*_1.fastq"), organism="test")
    samples.ids
    return (samples,)


@app.cell
def _(lg):
    calls = {"n": 0}

    @lg.step(tool="minimap2")
    def align(reads: lg.Reads, ref: lg.Reference) -> lg.Alignment:
        calls["n"] += 1  # never incremented: the body is not run
        return lg.sh("minimap2 -ax sr {ref} {reads} | samtools sort -o {out}")

    return align, calls


@app.cell
def _(fixtures, align, lg, samples):
    reference = lg.reference(str(fixtures / "tiny_bare.fasta"), organism="test")

    pipeline = lg.Pipeline(samples.map(lambda s: align(s, reference)))
    steps = pipeline.steps
    return pipeline, steps


@app.cell
def _(calls, mo, samples, steps):
    mo.md(
        f"""
        ## One branch per sample, none of them run

        The graph has **{len(steps)} step(s)** fanned out over
        **{len(samples.ids)} samples** — `{", ".join(samples.ids)}`.

        And the body's counter still reads **{calls["n"]}**. Not "zero because
        it failed": zero because building a graph does not call anything. That
        is what makes `check` cheap enough to run every time.
        """
    )
    return


@app.cell
def _(pipeline):
    # `errors` returns every problem in one pass rather than stopping at the
    # first, so a broken pipeline is one conversation, not twelve.
    problems = pipeline.errors()
    problems
    return (problems,)


@app.cell
def _(mo, problems):
    verdict = (
        "`check()` passes — every step's dimensions line up."
        if not problems
        else f"`check()` found {len(problems)} problem(s):\n\n```text\n"
        + "\n\n".join(str(p) for p in problems)
        + "\n```"
    )
    mo.md(f"## The verdict\n\n{verdict}")
    return


if __name__ == "__main__":
    app.run()
