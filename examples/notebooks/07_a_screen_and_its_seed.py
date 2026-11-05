# /// script
# requires-python = ">=3.11"
# dependencies = ["marimo", "ligase-bio"]
# ///
"""Dock a library, cut the top of the ranking, and audit what the seed is worth."""

import marimo

__generated_with = "0.25.2"
app = marimo.App(width="medium")


@app.cell
def _():
    import atexit
    import pathlib
    import shutil
    import tempfile

    import marimo as mo

    import ligase as lg

    # Everything this notebook writes, the cache included, lives in one fresh
    # directory. A cache kept anywhere that outlives the run would make the
    # second build of this page say "from cache" where the first said "ran".
    work = pathlib.Path(tempfile.mkdtemp(prefix="ligase-screen-"))
    atexit.register(shutil.rmtree, work, ignore_errors=True)
    cache = work / "cache"

    # The molecule tables are the engine's own test fixtures, copied unchanged.
    # Nothing here invents a compound: an InChIKey made up for a demo is a key
    # that names no substance, and the page would be teaching on it.
    _fixtures = (
        pathlib.Path(__file__)
        .resolve()
        .parents[2]
        .joinpath("crates", "ligase-core", "tests", "fixtures")
    )
    for _name in ("library", "hits_salts"):
        shutil.copy(_fixtures / f"molecules_{_name}.tsv", work / f"{_name}.tsv")

    def shown(text):
        """Diagnostics name the files they read; the page must not name ours."""
        for form in {str(work), str(work.resolve())}:
            text = text.replace(form, "work")
        return text

    return cache, lg, mo, pathlib, shown, work


@app.cell
def _(mo):
    mo.md(
        """
        # 7 — A screen, and its seed

        A virtual screen docks a library of compounds against a target, ranks
        them, and buys the top of the list. Two things in that path fail
        without a sound. The first is **what a compound's name means**: the
        same substance has a different InChIKey depending on what was done to
        it before the key was computed. The second is **the dice**: docking
        draws on a random seed, and a result nobody can regenerate is not
        quite a result.

        This notebook declares the first, refuses a set that disagrees about
        it, and then runs a seeded screen: cached, rerun when the seed
        moves, and audited for what a seed is worth.
        """
    )
    return


@app.cell
def _(lg, work):
    library = lg.molecule_set(str(work / "library.tsv"), organism="Homo sapiens")
    hits = lg.molecule_set(str(work / "hits_salts.tsv"), organism="Homo sapiens")
    library_representation = library.dimensions["representation"]
    hits_representation = hits.dimensions["representation"]
    return hits, hits_representation, library, library_representation


@app.cell
def _(hits_representation, library_representation, mo, work):
    _line = (work / "library.tsv").read_text().splitlines()[0]
    mo.md(
        f"""
        ## A name is only as good as its preparation

        The first line of the library file is not data:

        `{_line}`

        That is where the `representation` dimension is read from. Ligase
        reports it as **{library_representation.value}**. The table of hits
        declares itself differently: **{hits_representation.value}**. Both
        are columns of twenty-seven-character keys, and nothing in the
        columns says which was prepared how. The line above the header is the
        only place that is written down.

        A table with no such line is not assumed to be raw. It is **unknown**,
        because a ChEMBL download is standardised and a vendor list is not,
        and guessing the weaker state would be inventing the most dangerous
        of the possible answers.
        """
    )
    return


@app.cell
def _(hits, lg, library, shown):
    @lg.step
    def enrichment(library: lg.MoleculeSet, hits: lg.MoleculeSet) -> lg.MoleculeSet:
        """The compounds both tables name."""
        import pathlib

        def produce(out: pathlib.Path) -> None:
            def keys(table):
                rows = pathlib.Path(table.file).read_text().splitlines()
                rows = [row for row in rows if not row.startswith("#")][1:]
                return {row.split("\t")[0] for row in rows}

            both = sorted(keys(library) & keys(hits))
            out.write_text("inchikey\n" + "".join(f"{key}\n" for key in both))

        return lg.write(produce)

    refused = lg.Pipeline(enrichment(library, hits)).errors()
    print("refusal, representations:")
    print(shown(str(refused[0])))
    return (refused,)


@app.cell
def _(mo, refused):
    mo.md(
        f"""
        ## Two sets prepared differently are not joined

        The library is salt-stripped and charge-neutralised. The hits are as
        they came: aspirin as its sodium salt, metformin as its
        hydrochloride. The engine found **{len(refused)} problem** and ran
        nothing.

        Without the check, `enrichment` would run, find that aspirin's key
        in one table is not aspirin's key in the other, and report a hit
        rate that is small and not zero. That reads as poor enrichment. It is
        a measurement of the standardiser.

        The engine does not fix this. The fix is
        `lg.chem.standardize`, which writes the state it arrived at onto its
        own output, so that the next step is checked against something true
        rather than a claim nobody verified. It needs RDKit, which is the
        `chem` extra, and this page does not call it: loading, checking and
        hashing molecule tables all work on a default install, and producing
        a standardised set does not.
        """
    )
    return


@app.cell
def _(lg, library, pathlib):
    def draw(table, out, seed):
        """Score every compound, with a jitter drawn from the seed."""
        import math
        import random

        rng = random.Random(seed)
        rows = pathlib.Path(table.file).read_text().splitlines()
        # The ranking has to say what it is a ranking *of*, or the next step
        # refuses it. The line is copied from the input, not written afresh.
        declaration = rows[0]
        header, *body = rows[1:]
        columns = header.split("\t")
        key, name, activity = (
            columns.index(c) for c in ("inchikey", "name", "activity_nm")
        )
        scored = []
        for row in body:
            cells = row.split("\t")
            score = -(10 - math.log10(float(cells[activity]))) + rng.uniform(-1, 1)
            scored.append((round(score, 2), cells[key], cells[name]))
        scored.sort(key=lambda item: (item[0], item[1]))
        lines = [declaration, "inchikey\tname\tscore"]
        lines += [f"{k}\t{n}\t{s}" for s, k, n in scored]
        out.write_text("\n".join(lines) + "\n")

    @lg.step(stochastic=True, seed="seed")
    def dock(library: lg.MoleculeSet, *, seed: int) -> lg.MoleculeRanking:
        """Rank the library, reproducibly, from its seed."""
        return lg.write(lambda out: draw(library, out, seed))

    @lg.step
    def top(ranking: lg.MoleculeRanking, *, k: int) -> lg.MoleculeSet:
        """The k best-ranked compounds, as a set."""

        def produce(out: pathlib.Path) -> None:
            rows = pathlib.Path(ranking.file).read_text().splitlines()
            declaration, _header, *body = rows
            chosen = sorted(row.split("\t")[0] for row in body[:k])
            out.write_text(
                declaration + "\ninchikey\n" + "".join(f"{key}\n" for key in chosen)
            )

        return lg.write(produce)

    def screen(seed):
        return lg.Pipeline(top(dock(library, seed=seed), k=2))

    return draw, dock, screen, top


@app.cell
def _(cache, screen):
    pipeline = screen(20260101)
    problems = pipeline.errors()
    first = pipeline.run(cache=cache)
    print("run 1, a cold cache:", first.summary)
    again = pipeline.run(cache=cache)
    print("run 2, the same seed:", again.summary)
    return again, first, pipeline, problems


@app.cell
def _(again, cache, first, screen):
    # `first` and `again` are parameters only so marimo runs this cell after
    # them: the seed is the only thing that changes.
    assert first.steps
    assert again.steps
    moved = screen(20260102).run(cache=cache)
    print("run 3, the seed changed:", moved.summary)
    print([(s.step, s.outcome) for s in moved.steps])
    return (moved,)


@app.cell
def _(again, first, mo, moved, pipeline, problems):
    mo.md(
        f"""
        ## The same declared seed is the same run

        The screen has **{len(pipeline.steps)} steps**, `dock` and `top`, and
        `errors()` found **{len(problems)} problems**. The step that
        `top` reads is checked too: `dock` writes the `representation` line
        itself, carried over from the library it was given, and without it the
        ranking would have no representation and `top` would refuse it.

        Run 1 said **{first.summary}** Run 2 repeated it and said
        **{again.summary}** The seed is a parameter like any other: it is in
        the step's cache key, so the same seed finds the same entry.

        Run 3 changed the seed and said **{moved.summary}** A different seed
        is a different run, so `dock` ran again. `top` reads the whole
        ranking `dock` wrote, so a ranking that moved reran it too. Had the
        new seed written the same ranking, early cutoff would have reused
        `top` and said so.
        """
    )
    return


@app.cell
def _(cache, pipeline):
    audit = pipeline.audit("dock", cache=cache)
    print("audit, a declared seed:")
    print(audit.text)
    return (audit,)


@app.cell
def _(audit, mo):
    mo.md(
        f"""
        ## A seeded step is reproducible, not deterministic

        `ligase audit` runs a step twice and compares. The verdict for `dock`
        is **{audit.verdict}**.

        It is not **deterministic**, though the two runs wrote the same bytes,
        and that is on purpose. A step that declares itself stochastic
        promises that its seed is what controls its randomness, and nothing
        the engine can observe proves it: a tool with a seed flag it ignores
        would write identical bytes on a lucky pair of runs too. So the best
        a declared step can earn is `reproducible`, which says *with this
        seed, you get this result*. It cannot earn the stronger word.
        """
    )
    return


@app.cell
def _(cache, draw, lg, library, shown):
    def undeclared_screen():
        @lg.step
        def dock(library: lg.MoleculeSet, *, seed: int) -> lg.MoleculeRanking:
            """The same work, with nothing said about where its randomness is."""
            return lg.write(lambda out: draw(library, out, seed))

        return lg.Pipeline(dock(library, seed=20260101))

    undeclared = undeclared_screen().audit("dock", cache=cache / "undeclared")
    print("audit, the same body with nothing declared:")
    print(shown(undeclared.text))
    return (undeclared,)


@app.cell
def _(mo, undeclared):
    mo.md(
        f"""
        The same body, the same seed, the same library, with the declaration
        removed, audits as **{undeclared.verdict}**.

        Nothing about the computation changed. What changed is what the
        author said. Undeclared, the engine takes the two identical runs at
        face value. Declared, it knows the step was written to be random, so
        it reports the weaker and more honest thing: not that nothing varies,
        but that the seed pins it. The pair is the whole of the difference
        between a claim and a guess, and it is what lets a methods section say
        "seeded with 20260101" and have the engine stand behind it.
        """
    )
    return


if __name__ == "__main__":
    app.run()
