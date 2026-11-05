# /// script
# requires-python = ">=3.11"
# dependencies = ["marimo", "ligase-bio"]
# ///
"""Score a knowledge graph, change its identifiers, and cut a shortlist."""

import marimo

__generated_with = "0.25.2"
app = marimo.App(width="medium")


@app.cell
def _():
    import atexit
    import json
    import pathlib
    import random
    import shutil
    import tempfile

    import marimo as mo

    import ligase as lg

    # Everything this notebook writes, the cache included, lives in one fresh
    # directory. A cache kept anywhere that outlives the run would make the
    # second build of this page say "from cache" where the first said "ran".
    work = pathlib.Path(tempfile.mkdtemp(prefix="ligase-shortlist-"))
    atexit.register(shutil.rmtree, work, ignore_errors=True)
    cache = work / "cache"

    def shown(text):
        """Diagnostics name the files they read; the page must not name ours."""
        for form in {str(work), str(work.resolve())}:
            text = text.replace(form, "work")
        return text

    return cache, json, lg, mo, pathlib, random, shown, work


@app.cell
def _(mo):
    mo.md(
        """
        # 6 — From a graph to a shortlist

        A target-discovery analysis ends in a short list of genes, and gets
        there by crossing between naming schemes. A knowledge graph names its
        genes by Ensembl ID. A differential-expression table names them by
        HGNC symbol. Cutting a shortlist means joining the two, and a join on
        names from different schemes matches nothing and says nothing.

        This notebook runs that whole path: **score** a graph, **map** its
        ranking from Ensembl IDs to symbols, and **intersect** it with a
        DEG ranking. Then it shows what the engine refuses, what it reports
        about the mapping, and why a ranking is not a set.
        """
    )
    return


@app.cell
def _(json, random, work):
    # Thirty genes and ninety edges, so the page is the same on every build.
    # Which genes are joined is index arithmetic, not a random draw: the
    # standard library does not promise `random.sample` picks the same genes
    # on every Python version. Scores come from a seeded stream, which it does.
    # Ensembl IDs are the graph's node names.
    _rng = random.Random(6)
    ensembl = [f"ENSG{10000 + 137 * i:011d}" for i in range(30)]
    symbols = [f"GENE{i:02d}" for i in range(30)]

    edge_rows = []
    for _i in range(90):
        _from = _i % 30
        # An offset of 1 to 29, so an edge never joins a gene to itself.
        _a, _b = ensembl[_from], ensembl[(_from + 1 + (_i * 7) % 29) % 30]
        edge_rows.append((_a, _b, round(_rng.uniform(0.1, 0.99), 2)))

    # The two best-connected genes are the two that will collide below, so the
    # loss is at the top of the ranking, where it matters most.
    _totals = dict.fromkeys(ensembl, 0.0)
    for _a, _b, _s in edge_rows:
        _totals[_a] += _s
        _totals[_b] += _s
    _best = sorted(ensembl, key=lambda e: (-_totals[e], e))
    _first, _second = ensembl.index(_best[0]), ensembl.index(_best[1])
    symbols[_second] = symbols[_first]

    def graph_json(sources):
        nodes = [
            {"id": e, "type": "gene", "sources": ["open-targets"]} for e in ensembl
        ]
        edges = [
            {
                "source": a,
                "target": b,
                "type": "interacts_with",
                "sources": sources,
                "score": s,
            }
            for a, b, s in edge_rows
        ]
        return json.dumps({"nodes": nodes, "edges": edges})

    graph_path = work / "targets.json"
    graph_path.write_text(graph_json(["open-targets", "disgenet"]), encoding="utf-8")
    graph_narrow_path = work / "targets_one_source.json"
    graph_narrow_path.write_text(graph_json(["open-targets"]), encoding="utf-8")
    graph_reordered_path = work / "targets_reordered.json"
    graph_reordered_path.write_text(
        graph_json(["disgenet", "open-targets"]), encoding="utf-8"
    )

    # The mapping runs Ensembl -> HGNC. Two Ensembl IDs share one symbol.
    map_path = work / "ensembl_to_hgnc.tsv"
    map_path.write_text(
        "id\tsymbol\n"
        + "".join(f"{e}\t{s}\n" for e, s in zip(ensembl, symbols, strict=True)),
        encoding="utf-8",
    )

    # A DEG ranking in HGNC symbols: twenty of the thirty, best first. A
    # stride of 7 is coprime with 30, so the twenty are distinct.
    _degs = [f"GENE{(7 * i) % 30:02d}" for i in range(20)]
    degs_path = work / "degs.tsv"
    degs_path.write_text(
        "id\tscore\n" + "".join(f"{s}\t{20 - i}\n" for i, s in enumerate(_degs)),
        encoding="utf-8",
    )
    collided = (ensembl[_first], ensembl[_second], symbols[_first])
    return (
        collided,
        degs_path,
        graph_narrow_path,
        graph_path,
        graph_reordered_path,
        map_path,
    )


@app.cell
def _(degs_path, graph_narrow_path, graph_path, graph_reordered_path, lg, map_path):
    graph = lg.knowledge_graph(
        str(graph_path),
        organism="Homo sapiens",
        identifier_namespace="Ensembl gene",
        subject="target discovery",
        release="demo 1",
    )
    graph_reordered = lg.knowledge_graph(
        str(graph_reordered_path),
        organism="Homo sapiens",
        identifier_namespace="Ensembl gene",
        subject="target discovery",
        release="demo 1",
    )
    graph_narrow = lg.knowledge_graph(
        str(graph_narrow_path),
        organism="Homo sapiens",
        identifier_namespace="Ensembl gene",
        subject="target discovery",
        release="demo 1",
    )
    degs = lg.ranked_list(
        str(degs_path),
        organism="Homo sapiens",
        identifier_namespace="HGNC symbol",
    )
    # The mapping table is a set whose own identifiers are the *source* side.
    mapping = lg.entity_set(
        str(map_path),
        organism="Homo sapiens",
        identifier_namespace="Ensembl gene",
    )
    sources = graph.dimensions["source_set"].value
    sources_reordered = graph_reordered.dimensions["source_set"].value
    return degs, graph, graph_narrow, mapping, sources, sources_reordered


@app.cell
def _(mo, sources, sources_reordered):
    mo.md(
        f"""
        ## A graph is defined by what went into it

        Nobody told the loader which evidence sources built this graph. It
        read them off the edges: **{sources}**. Add a source and edges
        appear, neighbourhoods widen and every propagated score moves, so two
        graphs assembled from different sources are not comparable, and
        nothing in the file says which is which.

        The same edges with their sources listed the other way round,
        `disgenet` before `open-targets`, give **{sources_reordered}**,
        the identical value. The order was never information: the source set
        is a set, so the graphs are the same graph. A real difference in
        sources is a different graph, and the next cell hands the engine one.
        """
    )
    return


@app.cell
def _(graph, graph_narrow, lg, pathlib, shown):
    @lg.step
    def overlap(a: lg.KnowledgeGraph, b: lg.KnowledgeGraph) -> lg.EntitySet:
        """The nodes two graphs share."""
        import json

        def produce(out: pathlib.Path) -> None:
            def nodes(g):
                doc = json.loads(pathlib.Path(g.file).read_text())
                return {node["id"] for node in doc["nodes"]}

            both = sorted(nodes(a) & nodes(b))
            out.write_text("id\n" + "".join(f"{node}\n" for node in both))

        return lg.write(produce)

    # The same genes and edges; one graph was built from `open-targets` alone.
    different = lg.Pipeline(overlap(graph, graph_narrow)).errors()
    print("refusal, source sets:")
    print(shown(str(different[0])))
    return


@app.cell
def _(mo):
    mo.md(
        """
        The second graph has the same genes and the same edges, built from
        `open-targets` alone. Comparing it with the first is refused before
        anything runs, for the reason the loader read the sources at all: the
        difference between two such graphs is a difference in evidence, and a
        comparison that reported it would present it as biology.
        """
    )
    return


@app.cell
def _(degs, graph, lg, mapping, pathlib):
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
            rows = [f"{node}\t{total:.2f}" for node, total in ranked]
            out.write_text("id\tscore\n" + "\n".join(rows) + "\n")

        return lg.write(produce)

    @lg.step
    def shortlist(ranking: lg.RankedList, degs: lg.RankedList) -> lg.EntitySet:
        """The genes both rankings name, as a set."""

        def produce(out: pathlib.Path) -> None:
            def ids(ranked):
                lines = pathlib.Path(ranked.file).read_text().splitlines()[1:]
                return [line.split("\t")[0] for line in lines]

            both = set(ids(ranking)) & set(ids(degs))
            out.write_text("id\n" + "".join(f"{gene}\n" for gene in sorted(both)))

        return lg.write(produce)

    ranking = score(graph)
    symbol_ranking = lg.map_ranked_identifiers(
        ranking,
        mapping,
        column="symbol",
        to="HGNC symbol",
        id_column="id",
        table_id_column="id",
    )
    pipeline = lg.Pipeline(shortlist(symbol_ranking, degs))
    problems = pipeline.errors()
    return pipeline, problems, ranking, shortlist


@app.cell
def _(mo, pipeline, problems):
    mo.md(
        f"""
        ## Three steps, checked before any ran

        `score` turns the graph into a ranking, in Ensembl IDs.
        `map_ranked_identifiers` is the conversion for a ranking, and it
        takes the mapping table as a dataset, so the table's identifier space
        is checked like any other input. `shortlist`
        intersects the result with the DEG ranking, which is in symbols.

        The graph has **{len(pipeline.steps)} steps** and `errors()` found
        **{len(problems)} problems**. The conversion is what makes that
        true: without it, `shortlist` would be handed Ensembl IDs and
        symbols.
        """
    )
    return


@app.cell
def _(cache, pipeline):
    first = pipeline.run(cache=cache)
    print("run 1, a cold cache:", first.summary)
    print([(s.step, s.outcome) for s in first.steps])
    return (first,)


@app.cell
def _(collided, first, mo):
    _mapped = next(s for s in first.steps if s.step == "map_ranked_identifiers")
    _lines = "\n".join(f"- {note.splitlines()[0]}" for note in _mapped.notes)
    _a, _b, _symbol = collided
    mo.md(
        f"""
        ## Two genes, one name

        `{_a}` and `{_b}` are different genes, and the mapping table gives
        both the symbol `{_symbol}`. That is a collision: two rows cannot
        share a symbol, and the engine will not guess which gene the analysis
        meant. It leaves both out and says so. This is what the mapping step
        recorded in the run report:

        {_lines}

        These two were the best-connected genes in the graph, so the loss is
        at the head of the list: the last note says every rank below them
        moved up. A shortlist that silently lost its two top candidates would
        look the same as one that never had them.
        """
    )
    return


@app.cell
def _(cache, degs_path, first, pipeline):
    # The DEG ranking, reordered: the same rows with the same scores, in a
    # different order. (`first` is a parameter only so marimo runs this cell
    # after the first run.)
    assert first.steps
    _lines = degs_path.read_text().splitlines()
    degs_path.write_text("\n".join([_lines[0], *_lines[1:][::-1]]) + "\n")

    second = pipeline.run(cache=cache)
    print("run 2, the DEG ranking's rows reversed:", second.summary)
    print([(s.step, s.outcome) for s in second.steps])
    return (second,)


@app.cell
def _(mo, second):
    mo.md(
        f"""
        ## Reorder a ranking and it is a different ranking

        Run 2 said **{second.summary}** The DEG file has the same twenty
        genes and the same scores, in the opposite order, and `shortlist`
        ran again.

        That is deliberate. In a ranking the row order *is* the result: a
        table sorted best-first and the same table sorted worst-first answer
        different questions, so Ligase hashes the order in. Only the one step
        that reads the ranking reran. `score` and the mapping read neither
        the DEG file, so they were reused.
        """
    )
    return


@app.cell
def _(cache, first, lg, pathlib, work):
    # The shortlist is a set. Take the file the run wrote, load it as a
    # set of HGNC symbols, and hand it to a step that reads it.
    _out = next(s for s in first.steps if s.step == "shortlist").output
    shortlist_path = work / "shortlist.tsv"
    shortlist_path.write_text(pathlib.Path(_out).read_text())
    shortlist_set = lg.entity_set(
        str(shortlist_path),
        organism="Homo sapiens",
        identifier_namespace="HGNC symbol",
    )

    @lg.step
    def review(candidates: lg.EntitySet) -> lg.EntitySet:
        """Hand the shortlist on to the next stage, one symbol per line."""

        def produce(out: pathlib.Path) -> None:
            rows = pathlib.Path(candidates.file).read_text().splitlines()[1:]
            out.write_text("id\n" + "".join(f"{row}\n" for row in rows))

        return lg.write(produce)

    review_pipeline = lg.Pipeline(review(shortlist_set))
    third = review_pipeline.run(cache=cache)
    print("run 3, the shortlist set:", third.summary)

    _lines = shortlist_path.read_text().splitlines()
    shortlist_path.write_text("\n".join([_lines[0], *_lines[1:][::-1]]) + "\n")
    fourth = review_pipeline.run(cache=cache)
    print("run 4, the shortlist set's rows reversed:", fourth.summary)
    return fourth, third


@app.cell
def _(fourth, mo, third):
    mo.md(
        f"""
        ## Reorder a set and nothing happens

        The shortlist is a *set*: which genes, with no order. Handed to a
        further step and run cold, it said **{third.summary}** Then the
        file's rows were reversed, which makes it a different file, and the
        rerun said **{fourth.summary}**

        Ligase canonicalises a set before hashing it, so the reversed file is
        the same set and there is nothing to recompute. The pair of runs is
        the difference between the two kinds: a ranking whose order moved was
        recomputed, a set whose order moved was not. Getting that wrong in
        either direction is a silent failure, because a ranking treated as a
        set loses its order, and a set treated as a ranking reruns on every
        harmless re-export.
        """
    )
    return


@app.cell
def _(degs, lg, ranking, shortlist, shown):
    # The ranking from `score` is in Ensembl IDs, and `degs` is in symbols.
    # Nothing here maps one to the other.
    wrong = lg.Pipeline(shortlist(ranking, degs)).errors()
    print("refusal, identifier namespaces:")
    print(shown(str(wrong[0])))
    return


@app.cell
def _(mo):
    mo.md(
        """
        ## The join that matches nothing

        The pipeline above is the one that ran, with the conversion step cut
        out: the Ensembl ranking goes straight into `shortlist` beside a
        ranking in symbols. Nothing ran.

        Without this check the step would run, intersect two sets of strings
        that share none, and return an empty shortlist, which reads as "no
        overlap" rather than "wrong names". The identifier namespace is a
        dimension like any other here, and `map_ranked_identifiers` is how
        you cross it on purpose: it is a typed conversion, declared in the
        graph, with its losses on the record, not a rename that happens
        somewhere in a script.
        """
    )
    return


if __name__ == "__main__":
    app.run()
