# /// script
# requires-python = ">=3.11"
# dependencies = ["marimo", "ligase-bio"]
# ///
"""Bind a sentence to a number, write the methods, and replay a bundle."""

import marimo

__generated_with = "0.25.2"
app = marimo.App(width="medium")


@app.cell
def _():
    import atexit
    import json
    import pathlib
    import shutil
    import tempfile

    import marimo as mo

    import ligase as lg

    # Everything this notebook writes, the cache included, lives in one fresh
    # directory. A cache kept anywhere that outlives the run would make the
    # second build of this page say "from cache" where the first said "ran".
    work = pathlib.Path(tempfile.mkdtemp(prefix="ligase-paper-"))
    atexit.register(shutil.rmtree, work, ignore_errors=True)
    cache = work / "cache"

    def shown(text):
        """Reports name the files they read; the page must not name ours."""
        for form in {str(work), str(work.resolve())}:
            text = text.replace(form, "work")
        return text

    return cache, json, lg, mo, pathlib, shown, work


@app.cell
def _(mo):
    mo.md(
        """
        # 8 — What a paper needs

        The last mile of an analysis is a sentence. "The top candidate's
        score is such-and-such." It goes into a manuscript by hand, and the
        pipeline behind it goes on changing. Months later the sentence and
        the number disagree, and nothing marks the place.

        This notebook takes a result as far as a paper could carry it. It
        rebuilds a small version of the target-discovery pipeline from
        [from a graph to a shortlist](/notebooks/from-a-graph-to-a-shortlist/),
        binds a sentence to a number in the result, writes the methods
        paragraph the run supports, shows a claim the run does not support,
        and writes a bundle that can be replayed to check that the results
        come out the same.
        """
    )
    return


@app.cell
def _(json, work):
    # Twelve genes and thirty-six edges, so the page is the same on every
    # build. Which genes are joined is index arithmetic, not a random draw.
    ensembl = [f"ENSG{10000 + 137 * i:011d}" for i in range(12)]
    symbols = [f"GENE{i:02d}" for i in range(12)]
    edges = []
    for _i in range(36):
        _from = _i % 12
        # An offset of 1 to 11, so an edge never joins a gene to itself.
        _a, _b = ensembl[_from], ensembl[(_from + 1 + (_i * 5) % 11) % 12]
        edges.append(
            {
                "source": _a,
                "target": _b,
                "type": "interacts_with",
                "sources": ["open-targets"],
                "score": round(0.1 + ((_i * 3719) % 8900) / 10000, 4),
            }
        )
    (work / "targets.json").write_text(
        json.dumps(
            {
                "nodes": [
                    {"id": e, "type": "gene", "sources": ["open-targets"]}
                    for e in ensembl
                ],
                "edges": edges,
            }
        ),
        encoding="utf-8",
    )
    (work / "ensembl_to_hgnc.tsv").write_text(
        "id\tsymbol\n"
        + "".join(f"{e}\t{s}\n" for e, s in zip(ensembl, symbols, strict=True)),
        encoding="utf-8",
    )
    # A DEG ranking in HGNC symbols: eight of the twelve, best first. A stride
    # of 5 is coprime with 12, so the eight are distinct.
    _degs = [f"GENE{(5 * i) % 12:02d}" for i in range(8)]
    _degs_file = (work / "degs.tsv").write_text(
        "id\tscore\n" + "".join(f"{s}\t{8 - i}\n" for i, s in enumerate(_degs)),
        encoding="utf-8",
    )
    return


@app.cell
def _(cache, lg, pathlib, work):
    graph = lg.knowledge_graph(
        str(work / "targets.json"),
        organism="Homo sapiens",
        identifier_namespace="Ensembl gene",
        subject="target discovery",
        release="demo 1",
    )
    degs = lg.ranked_list(
        str(work / "degs.tsv"),
        organism="Homo sapiens",
        identifier_namespace="HGNC symbol",
    )
    mapping = lg.entity_set(
        str(work / "ensembl_to_hgnc.tsv"),
        organism="Homo sapiens",
        identifier_namespace="Ensembl gene",
    )

    @lg.step
    def score(graph: lg.KnowledgeGraph) -> lg.RankedList:
        """Rank nodes by the summed score of the edges they sit on."""
        import json

        def produce(out: pathlib.Path) -> None:
            totals = {}
            for edge in json.loads(pathlib.Path(graph.file).read_text())["edges"]:
                for end in (edge["source"], edge["target"]):
                    totals[end] = totals.get(end, 0.0) + edge["score"]
            ranked = sorted(totals.items(), key=lambda item: (-item[1], item[0]))
            rows = [f"{node}\t{total:.4f}" for node, total in ranked]
            out.write_text("id\tscore\n" + "\n".join(rows) + "\n")

        return lg.write(produce)

    @lg.step
    def shortlist(ranking: lg.RankedList, degs: lg.RankedList) -> lg.RankedList:
        """The genes both rankings name, in the graph ranking's order."""

        def produce(out: pathlib.Path) -> None:
            def rows(ranked):
                lines = pathlib.Path(ranked.file).read_text().splitlines()[1:]
                return [line.split("\t") for line in lines]

            wanted = {row[0] for row in rows(degs)}
            kept = [row for row in rows(ranking) if row[0] in wanted]
            out.write_text(
                "id\tscore\n" + "".join(f"{gene}\t{s}\n" for gene, s in kept)
            )

        return lg.write(produce)

    output = shortlist(
        lg.map_ranked_identifiers(
            score(graph),
            mapping,
            column="symbol",
            to="HGNC symbol",
            id_column="id",
            table_id_column="id",
        ),
        degs,
    )
    pipeline = lg.Pipeline(output)
    problems = pipeline.errors()
    report = pipeline.run(cache=cache)
    print("run:", report.summary)
    _written = pathlib.Path(
        next(s for s in report.steps if s.step == "shortlist").output
    )
    rows = [line.split("\t") for line in _written.read_text().splitlines()]
    print("the shortlist, as the step wrote it:")
    print("\n".join("  " + "\t".join(row) for row in rows[:4]))
    return output, pipeline, problems, report, rows


@app.cell
def _(mo, pipeline, problems, report, rows):
    mo.md(
        f"""
        ## A result

        The pipeline has **{len(pipeline.steps)} steps**, `score`,
        `map_ranked_identifiers` and `shortlist`, and `errors()` found
        **{len(problems)} problems**. The run said **{report.summary}**

        `shortlist` wrote **{len(rows) - 1} genes**, best first, each with the
        summed edge score the graph gave it. The first row of data is
        **{rows[1][0]}**, at **{rows[1][1]}**. That number is what a
        manuscript would quote, rounded, and the rest of this page is about
        the distance between a sentence that says it and a pipeline that can
        prove it.
        """
    )
    return


@app.cell
def _(cache, lg, output, rows):
    # What the paper prints: the engine's four decimals, rounded to two.
    published = round(float(rows[1][1]), 2)
    claim = lg.Claim(
        text=f"The top candidate's summed edge score is {published}.",
        step="shortlist",
        value=published,
        at=lg.cell(column="score", row=0),
        tolerance=0.005,
    )
    # The same claim with no tolerance, to see what the engine says.
    try:
        lg.Claim(
            text=claim.text,
            step="shortlist",
            value=published,
            at=lg.cell(column="score", row=0),
        )
        no_tolerance = "accepted"
    except ValueError as refusal:
        no_tolerance = str(refusal)
    print("the same claim with no tolerance:", no_tolerance)
    bound = lg.Pipeline(output, claims=[claim]).methods(cache=cache)
    check = bound.claims[0]
    print("verdict:", check.verdict)
    print("accounted for:", bound.accounted_for)
    print("summary:", bound.summary)
    return bound, check, claim


@app.cell
def _(bound, check, claim, mo, rows):
    mo.md(
        f"""
        ## A sentence bound to a cell

        `lg.Claim` takes the sentence as the paper words it, the step it
        comes from, and the cell of that step's output the number lives in.
        This one points at the `score` column of row 0, which is
        **{rows[1][0]}**, with `value={claim.value}` and
        `tolerance={claim.tolerance}`.

        The tolerance is the part to read. The step wrote
        **{rows[1][1]}**, and the paper prints **{claim.value}**. Those are
        not equal byte for byte. For the purposes of the sentence they are
        equal, because the sentence rounds to two decimals and the difference
        is {abs(float(rows[1][1]) - claim.value):.4f}, inside
        {claim.tolerance}. A tolerance states how much precision the
        sentence claims. The claim written without one is in the printed
        lines at the end of this page, with the engine's own words, because
        a default would invent a precision nobody stated.

        The engine read the cell and compared. The verdict is
        **{check.verdict}**, the write-up is accounted for:
        **{bound.accounted_for}**, and the claim rests on
        **{len(check.bound)} computation**.
        """
    )
    return


@app.cell
def _(bound, mo, shown):
    print("tools:", bound.tools)
    print("citations:", bound.citations)
    print("uncited:", bound.uncited)
    # Indented to the f-string's own margin, so the text stays inside the
    # fenced block once `mo.md` dedents the cell.
    _text = shown(bound.text).strip().replace("\n", "\n        ")
    mo.md(
        f"""
        ## The methods paragraph

        `Methods.text`, whole, as the engine wrote it:

        ```text
        {_text}
        ```

        Its first line, included. That line names the Ligase version and
        the platform the results were produced on, and the claim at the end
        carries the date it was recorded. This page is rebuilt on whichever
        machine builds it, and these lines say which. A methods section
        records the machine that did the work, so they are left as the
        engine wrote them.

        The record names **{len(bound.tools)} tool**,
        **{", ".join(name for name, _ in bound.tools)}**, and the reference
        list holds **{len(bound.citations)} citation**. The engine also
        reports tools it has no citation for, and reports
        **{len(bound.uncited)}** here.
        """
    )
    return


@app.cell
def _(bound, cache, claim, lg, mo, output, rows):
    # The same claim with its number rewritten, in a pipeline of its own so
    # that the good run above and this one are both on the page.
    _wrong = round(claim.value + 0.5, 2)
    broken_claim = lg.Claim(
        text=f"The top candidate's summed edge score is {_wrong}.",
        step="shortlist",
        value=_wrong,
        at=lg.cell(column="score", row=0),
        tolerance=0.005,
    )
    broken = lg.Pipeline(output, claims=[broken_claim]).methods(cache=cache)
    flagged = broken.unbacked
    print("verdict:", flagged[0].verdict)
    print("the run recorded:", flagged[0].recorded)
    print("accounted for:", broken.accounted_for)
    print("summary:", broken.summary)

    # What a CI job would do with it: assert the property, and see it fail.
    try:
        assert broken.accounted_for
        gate = "passed"
    except AssertionError:
        gate = "failed"
    print("an `assert methods.accounted_for` gate:", gate)
    mo.md(
        f"""
        ## A number the run does not support

        The same claim, with the number changed to **{_wrong}**, in a
        pipeline of its own over the same `shortlist`. The step still wrote
        **{rows[1][1]}**. `methods()` put **{len(flagged)} claim** in
        `unbacked`, with the verdict **{flagged[0].verdict}**, and the
        write-up is accounted for: **{broken.accounted_for}**. The good
        pipeline above says **{bound.accounted_for}**.

        `accounted_for` is a plain boolean, and a gate that asserts it
        {gate} here. The engine disagreed with the sentence before any
        reviewer read it.
        """
    )
    return (broken,)


@app.cell
def _(cache, claim, lg, mo, output, work):
    _recorded = lg.Pipeline(output, claims=[claim]).methods(
        bundle=str(work / "bundle"), cache=cache
    )
    bundle = _recorded.bundle
    _listing = "\n        ".join(sorted(bundle.files))
    mo.md(
        f"""
        ## A bundle

        `methods(bundle=...)` wrote **{len(bundle.files)} files** and
        accounts for **{bundle.steps} steps** and **{bundle.gaps} gaps**.
        This is what a reviewer receives, every path relative to the
        bundle's root:

        ```text
        {_listing}
        ```
        """
    )
    return (bundle,)


@app.cell
def _(bundle, cache, claim, lg, mo, output, work):
    assert bundle.files
    # An empty cache of its own: the replay recomputes rather than looking
    # up what the first run left behind.
    _fresh = work / "replay-cache"
    assert not _fresh.exists()
    replay = lg.Pipeline(output, claims=[claim]).methods(
        verify=str(work / "bundle"), cache=_fresh
    )
    verification = replay.verification
    print("verification:", verification.summary)
    print("verdict:", verification.verdict)
    print("reproduced:", verification.reproduced)
    print(
        "results:",
        [(r.step, r.verdict, r.same_key) for r in verification.results],
    )
    _same_hash = sum(r.expected == r.found for r in verification.results)
    _same_key = sum(r.same_key for r in verification.results)
    mo.md(
        f"""
        ## Replay

        `methods(verify=...)` takes the bundle and checks it against what
        this machine produces, here against an empty cache of its own. It
        said **{verification.summary}** Reproduced:
        **{verification.reproduced}**. The worst thing it found is the
        verdict **{verification.verdict}**.

        Of the {len(verification.results)} results, **{_same_hash}** have the
        canonical hash the bundle recorded and **{_same_key}** also have the
        same cache key.
        """
    )
    return


if __name__ == "__main__":
    app.run()
