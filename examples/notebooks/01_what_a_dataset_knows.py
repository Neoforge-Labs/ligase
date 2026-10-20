# /// script
# requires-python = ">=3.11"
# dependencies = ["marimo", "ligase-bio"]
# ///
"""What a dataset knows, and where it learned it.

Run it:      marimo edit examples/notebooks/01_what_a_dataset_knows.py
Or as code:  python examples/notebooks/01_what_a_dataset_knows.py
"""

import marimo

__generated_with = "0.25.2"
app = marimo.App(width="medium")


@app.cell
def _():
    from pathlib import Path

    import marimo as mo

    import ligase as lg

    # The tiny fixtures committed with the test suite, so this notebook needs
    # no downloads and no aligner.
    fixtures = Path(__file__).resolve().parents[2] / "crates/ligase-core/tests/fixtures"
    return fixtures, lg, mo


@app.cell
def _(mo):
    mo.md(
        """
        # 1 — What a dataset knows

        Loading a file in Ligase does not just open it. The loader reads the
        headers and works out the file's **dimensions**: which organism, which
        reference build, which naming style, which coordinate convention,
        which samples.

        Crucially, it also records *where it learned each one*. That is what
        makes the error messages later worth reading.
        """
    )
    return


@app.cell
def _(fixtures, lg):
    reference = lg.reference(str(fixtures / "tiny_bare.fasta"), organism="test")
    reference
    return (reference,)


@app.cell
def _(mo, reference):
    rows = "\n".join(
        f"| `{d.name}` | {d.value} | {d.source} |"
        for d in reference.dimensions.values()
    )
    mo.md(
        f"""
        ## What it worked out

        | dimension | value | learned from |
        |---|---|---|
        {rows}

        Note the difference in the last column. `organism` says **declared** —
        no FASTA states its species, so a human had to. But `reference` and
        `contig_naming` were *inferred from the file itself*.

        This matters for a reason the project cares about: a filename is not
        evidence. `hg38.fa` may hold anything. So reference identity comes from
        the contigs — their names, lengths and MD5s.
        """
    )
    return


@app.cell
def _(mo, reference):
    contigs = "\n".join(
        f"| `{c.name}` | {c.length:,} bp | `{c.md5[:16]}…` |" for c in reference.contigs
    )
    mo.md(
        f"""
        ## The evidence behind `reference`

        | contig | length | md5 |
        |---|---|---|
        {contigs}

        Two references match when this table matches — not when their
        filenames do.
        """
    )
    return


if __name__ == "__main__":
    app.run()
