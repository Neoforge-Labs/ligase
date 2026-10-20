# /// script
# requires-python = ">=3.11"
# dependencies = ["marimo", "ligase-bio"]
# ///
"""The mistake Ligase exists to catch, caught before anything runs."""

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
        # 2 — An error before compute

        Here are two files describing the *same two contigs, at the same
        lengths*. One calls them `chr1`-style, the other bare. Every base
        agrees. Only the naming differs.

        This is the class of bug that produces plausible, wrong numbers and no
        crash — and it is why `check()` runs before anything computes.
        """
    )
    return


@app.cell
def _(fixtures, lg):
    bare = lg.reference(str(fixtures / "tiny_bare.fasta"), organism="test")
    aligned_prefixed = lg.alignment(
        str(fixtures / "tiny_prefixed.sam"), organism="test"
    )

    naming = (
        bare.dimension("contig_naming").value,
        aligned_prefixed.dimension("contig_naming").value,
    )
    naming
    return aligned_prefixed, bare


@app.cell
def _(aligned_prefixed, bare, lg):
    @lg.step(tool="bcftools")
    def call(ref: lg.Reference, aln: lg.Alignment) -> lg.Variants:
        return lg.sh("bcftools mpileup -f {ref} {aln} | bcftools call -mv -Oz -o {out}")

    # Nothing has run. No bcftools, no temporary files — just the question
    # "do these fit together?"
    problems = lg.dimension_errors("call", {"ref": bare, "aln": aligned_prefixed})
    return (problems,)


@app.cell
def _(mo, problems):
    mo.md(
        f"""
        ## What it says

        ```text
        {problems[0]}
        ```

        Three things worth noticing.

        It names **where each value came from** — the FASTA deflines and the
        SAM `@SQ` header — so you can go and look rather than guess.

        It says **the contigs are otherwise identical**, which tells you this
        is a naming problem and not the wrong genome.

        And its type is `{type(problems[0]).__name__}`, not a generic error. A
        caller can branch on a naming mismatch without parsing English, which
        is what makes the v2 plan — inserting the renaming step automatically —
        possible at all.
        """
    )
    return


if __name__ == "__main__":
    app.run()
