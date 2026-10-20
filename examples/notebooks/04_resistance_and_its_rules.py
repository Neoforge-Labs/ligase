# /// script
# requires-python = ">=3.11"
# dependencies = ["marimo", "ligase-bio"]
# ///
"""Resistance interpretation, and why the rules are hashed rather than named.

This one runs the real interpretation when the HIV-1 demo data is present.
Without it, the rule metadata below is still the whole point.

    scripts/fetch_test_data.sh --project PRJNA1218827
"""

import marimo

__generated_with = "0.25.2"
app = marimo.App(width="medium")


@app.cell
def _():
    from pathlib import Path

    import marimo as mo

    import ligase as lg
    from ligase import hivdb

    root = Path(__file__).resolve().parents[2]
    return root, hivdb, lg, mo


@app.cell
def _(mo):
    mo.md(
        """
        # 4 — Resistance, and why the rules are hashed

        Interpreting a mutation means asking a published algorithm what it
        means. Ligase ships Stanford's HIVdb definitions **in the package**,
        pinned by checksum, and never calls a web service to find out.

        The reason is not convenience. An interpretation that reaches the
        network for its meaning is not reproducible: the answer depends on the
        day you ran it, and no cache key can see that.
        """
    )
    return


@app.cell
def _(hivdb):
    shipped = hivdb.ALGORITHMS, hivdb.DEFAULT
    shipped
    return


@app.cell
def _(hivdb, mo):
    mo.md(
        f"""
        ## What goes into the cache key

        ```text
        {hivdb.fingerprint()}
        ```

        That is a BLAKE3 hash of each ruleset's **contents**, not the string
        `"9.8"`. The distinction is the whole discipline of this project
        applied to one field: a version label is metadata you can derive from
        content, and metadata lies. Edit a rule without touching the version
        and the hash moves, so every result that depended on it is recomputed.
        Switch 9.8 to 10.2 and only the steps that read the rules rerun.

        The human-readable version still exists — it goes into the methods
        paragraph, where a reader needs "HIVdb 9.8" and a citation rather than
        a hash.
        """
    )
    return


@app.cell
def _(root):
    reference_path = root / "data/refs/HXB2.fasta"
    have_demo_data = reference_path.exists()
    have_demo_data
    return have_demo_data, reference_path


@app.cell
def _(have_demo_data, hivdb, lg, mo, reference_path):
    if have_demo_data:
        hxb2 = lg.reference(str(reference_path), organism="HIV-1", build="HXB2")
        detail = f"""
        The real HXB2 reference is here, so interpretation can run against it:

        - contigs: {[c.name for c in hxb2.contigs]}
        - length: {hxb2.contigs[0].length:,} bp
        - default algorithm: `{hivdb.DEFAULT}`

        A full run needs called variants too. The end-to-end pipeline —
        reads, alignment, calls, resistance — is
        `examples/hiv-resistance/pipeline.py`:

        ```bash
        ligase run examples/hiv-resistance/pipeline.py
        ```
        """
    else:
        detail = """
        The HIV-1 demo data is not on disk, so this cell is showing the rule
        metadata only. To run the interpretation for real:

        ```bash
        scripts/fetch_test_data.sh --project PRJNA1218827
        ligase run examples/hiv-resistance/pipeline.py
        ```
        """
    mo.md(f"## Running it for real\n{detail}")
    return


@app.cell
def _(mo):
    mo.md(
        """
        !!! warning "Research use"

            These calls are not for clinical use. Ligase's validation checks
            its calls against published results, which is evidence the
            implementation is faithful to the algorithm — not evidence that it
            is fit for patient care.
        """
    )
    return


if __name__ == "__main__":
    app.run()
