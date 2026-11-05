# /// script
# requires-python = ">=3.11"
# dependencies = ["marimo", "ligase-bio"]
# ///
"""A cohort pipeline, run four times, with a different change before each run."""

import marimo

__generated_with = "0.25.2"
app = marimo.App(width="medium")


@app.cell
def _():
    import atexit
    import hashlib
    import random
    import shutil
    import tempfile
    from pathlib import Path

    import marimo as mo

    import ligase as lg

    # Everything this notebook writes, the cache included, lives in one fresh
    # directory. A cache kept anywhere that outlives the run would make the
    # second build of this page say "from cache" where the first said "ran".
    work = Path(tempfile.mkdtemp(prefix="ligase-cohort-"))
    atexit.register(shutil.rmtree, work, ignore_errors=True)
    cache = work / "cache"

    def shown(text):
        """Diagnostics name the files they read; the page must not name ours."""
        for form in {str(work), str(work.resolve())}:
            text = text.replace(form, "work")
        return text

    return cache, hashlib, lg, mo, random, shown, work


@app.cell
def _(mo):
    mo.md(
        """
        # 5 — A cohort, and what changed

        The earlier notebooks stopped before anything ran. This one runs a
        whole pipeline, four times, and changes one thing about the input
        before each run. What the engine does about each change is the
        subject: recompute, reuse, or notice that a rerun made no difference.

        The pipeline starts from **alignments**, not reads. Aligning needs an
        aligner, and a docs build has none, so the cohort arrives as SAM
        files and the work begins where plain Python can honestly begin:
        counting reads in regions.
        """
    )
    return


@app.cell
def _(hashlib, random, work):
    # Two contigs, three samples, thirty reads each, all from fixed seeds so
    # the page is the same on every build. Contigs are named `1` and `2`, not
    # `chr1` and `chr2`, which matters at the end.
    _rng = random.Random(11)
    contigs = {"1": 400, "2": 300}
    sequences = {
        name: "".join(_rng.choice("ACGT") for _ in range(length))
        for name, length in contigs.items()
    }
    fasta = work / "reference.fasta"
    fasta.write_text(
        "".join(f">{n}\n{s}\n" for n, s in sequences.items()), encoding="utf-8"
    )

    # Regions are GFF, not BED: GFF is 1-based and closed, as SAM is. The BED
    # comes back at the end, as a mistake.
    genes = [("1", 21, 100), ("1", 201, 300), ("2", 31, 100), ("2", 151, 250)]
    regions_gff = work / "regions.gff"
    regions_gff.write_text(
        "##gff-version 3\n"
        + "".join(
            f"{c}\tdemo\tgene\t{s}\t{e}\t.\t+\t.\tID=gene{i}\n"
            for i, (c, s, e) in enumerate(genes, start=1)
        ),
        encoding="utf-8",
    )

    def sam_lines(index, sample):
        lines = ["@HD\tVN:1.6\tSO:unsorted"]
        for name, length in contigs.items():
            md5 = hashlib.md5(sequences[name].encode()).hexdigest()
            lines.append(f"@SQ\tSN:{name}\tLN:{length}\tM5:{md5}")
        lines.append(f"@RG\tID:{sample}\tSM:{sample}\tPL:ILLUMINA")
        rng = random.Random(100 + index)
        for n in range(30):
            contig = rng.choice(list(contigs))
            pos = rng.randint(1, contigs[contig] - 49)
            bases = sequences[contig][pos - 1 : pos + 49]
            fields = [f"read{n:02d}", "0", contig, str(pos), "60", "50M"]
            fields += ["*", "0", "0", bases, "I" * 50, f"RG:Z:{sample}"]
            lines.append("\t".join(fields))
        return lines

    sample_names = ["sample_a", "sample_b", "sample_c"]
    sam_paths = {}
    for _i, _sample in enumerate(sample_names):
        sam_paths[_sample] = work / f"{_sample}.sam"
        sam_paths[_sample].write_text(
            "\n".join(sam_lines(_i, _sample)) + "\n", encoding="utf-8"
        )

    sample_names
    return fasta, genes, regions_gff, sam_paths, sample_names


@app.cell
def _(lg):
    @lg.step(coordinates="0-based half-open")
    def depth(aln: lg.Alignment, regions: lg.Intervals) -> lg.Intervals:
        """Count the reads that touch each region."""
        from pathlib import Path

        def produce(out: Path) -> None:
            spans = []
            for line in Path(regions.file).read_text().splitlines():
                if line and not line.startswith("#"):
                    f = line.split("\t")
                    spans.append((f[0], int(f[3]), int(f[4])))
            reads = []
            for line in Path(aln.file).read_text().splitlines():
                if line and not line.startswith("@"):
                    f = line.split("\t")
                    reads.append((f[2], int(f[3]), int(f[3]) + len(f[9]) - 1))
            rows = []
            for contig, start, end in spans:
                n = sum(
                    1 for c, s, e in reads if c == contig and s <= end and e >= start
                )
                # BED is 0-based half-open, so the 1-based closed start moves.
                rows.append(f"{contig}\t{start - 1}\t{end}\t{n}")
            out.write_text("\n".join(rows) + "\n")

        return lg.write(produce)

    @lg.step
    def merge(a: lg.Intervals, b: lg.Intervals, c: lg.Intervals) -> lg.Intervals:
        """Put the three samples' counts side by side, one row per region."""
        from pathlib import Path

        def produce(out: Path) -> None:
            tables = [Path(x.file).read_text().splitlines() for x in (a, b, c)]
            rows = []
            for lines in zip(*tables, strict=True):
                fields = [line.split("\t") for line in lines]
                counts = [f[3] for f in fields]
                rows.append("\t".join([*fields[0][:3], ",".join(counts)]))
            out.write_text("\n".join(rows) + "\n")

        return lg.write(produce)

    return depth, merge


@app.cell
def _(depth, fasta, lg, merge, regions_gff, sam_paths, sample_names):
    reference = lg.reference(str(fasta), organism="demo")
    regions = lg.intervals(str(regions_gff), organism="demo")
    alns = [lg.alignment(str(sam_paths[s]), organism="demo") for s in sample_names]

    pipeline = lg.Pipeline(merge(*[depth(a, regions) for a in alns]))
    problems = pipeline.errors()
    steps = pipeline.steps
    contig_names = [c.name for c in reference.contigs]
    assert all([c.name for c in a.contigs] == contig_names for a in alns)
    return alns, contig_names, pipeline, problems, steps


@app.cell
def _(contig_names, mo, problems, sample_names, steps):
    mo.md(
        f"""
        ## Three samples in, one table out

        The cohort is `{", ".join(sample_names)}`. The graph has
        **{len(steps)} steps** — three `depth` nodes and one `merge` — and
        `check()` found **{len(problems)} problems**, before any of them ran.
        The reference and every alignment name the same contigs:
        `{", ".join(contig_names)}`.

        The fan-in is written out by hand: `merge` takes three positional
        parameters, one per sample. A step's inputs are the datasets named in
        its signature, so the signature is where the graph's shape lives, and
        a list-of-datasets parameter would hide that from the checker.
        """
    )
    return


@app.cell
def _(cache, pipeline):
    run1 = pipeline.run(cache=cache)
    print("run 1, a cold cache:", run1.summary)
    print([(s.step, s.outcome) for s in run1.steps])
    return (run1,)


@app.cell
def _(cache, pipeline, run1, sam_paths):
    # The same records in the opposite order. The bytes differ; the alignment
    # does not. (`run1` is a parameter only so marimo runs this cell after it.)
    assert run1.steps
    _path = sam_paths["sample_a"]
    _lines = _path.read_text().splitlines()
    _header = [x for x in _lines if x.startswith("@")]
    _body = [x for x in _lines if not x.startswith("@")]
    _path.write_text("\n".join(_header + _body[::-1]) + "\n")

    run2 = pipeline.run(cache=cache)
    print("run 2, sample_a's records reordered:", run2.summary)
    return (run2,)


@app.cell
def _(mo, run1, run2):
    mo.md(
        f"""
        ## The bytes changed; the alignment did not

        Run 1 was cold: **{run1.summary}** Then `sample_a.sam` was rewritten
        with its records in the opposite order — a different file, by any
        checksum — and run 2 said **{run2.summary}**

        That is semantic hashing. Ligase hashes an alignment's *canonical*
        form, with the records sorted and the volatile header lines stripped,
        so a re-sorted file is the same input. A tool that hashed the bytes
        would have rerun everything, and a pipeline that does that on every
        harmless re-export is a pipeline people stop trusting the cache of.
        """
    )
    return


@app.cell
def _(cache, pipeline, run2, sam_paths):
    # Rewrite one read's bases in place: same contig, same position, same
    # length. A real change to the file that the depth counts cannot see.
    assert run2.steps
    _path = sam_paths["sample_a"]
    _lines = _path.read_text().splitlines()
    _i = next(n for n, x in enumerate(_lines) if not x.startswith("@"))
    _f = _lines[_i].split("\t")
    _f[9] = _f[9].translate(str.maketrans("ACGT", "CATG"))
    _lines[_i] = "\t".join(_f)
    _path.write_text("\n".join(_lines) + "\n")

    run3 = pipeline.run(cache=cache)
    print("run 3, one read's bases rewritten:", run3.summary)
    print([(s.step, s.outcome) for s in run3.steps])
    return (run3,)


@app.cell
def _(mo, run3):
    mo.md(
        f"""
        ## A rerun that changed nothing

        Run 3 said **{run3.summary}** One read in `sample_a` had its bases
        rewritten, which is a real change: the alignment's canonical form
        moved, so `depth` had to run again for that sample. It wrote exactly
        the bytes it had written before, because a read's bases do not change
        how many reads touch a region.

        So `merge` was **skipped**, not rerun. This is early cutoff: a step
        is recomputed only when what it *reads* has changed, and `merge`
        reads the output of `depth`, not the alignment behind it. The other
        two samples' `depth` steps came from the cache, because nothing they
        read had moved.
        """
    )
    return


@app.cell
def _(cache, genes, pipeline, run3, sam_paths):
    # Now move a read so that it leaves every region. This time the change
    # is one the counts can see.
    assert run3.steps
    _path = sam_paths["sample_a"]
    _lines = _path.read_text().splitlines()
    for _n, _line in enumerate(_lines):
        if _line.startswith("@"):
            continue
        _f = _line.split("\t")
        _pos = int(_f[3])
        if any(c == _f[2] and s <= _pos + 49 and e >= _pos for c, s, e in genes):
            _f[2], _f[3] = "1", "120"  # 120-169 on contig 1: between the genes
            _lines[_n] = "\t".join(_f)
            break
    _path.write_text("\n".join(_lines) + "\n")

    run4 = pipeline.run(cache=cache)
    print("run 4, one read moved out of every region:", run4.summary)
    return (run4,)


@app.cell
def _(mo, run4):
    mo.md(
        f"""
        ## The same kind of edit, made to matter

        Run 4 said **{run4.summary}** A read in `sample_a` was moved to a
        position that no region covers, so a count really did fall by one.
        `depth` produced different bytes this time, and `merge`, whose input
        changed, ran after it. Two steps ran and the other two samples'
        `depth` steps were still read from the cache.

        Runs 3 and 4 differ only in whether the edit reached a count. The
        engine tells them apart by what the step *produced*, not by what it
        was *given*.
        """
    )
    return


@app.cell
def _(alns, depth, lg, merge, regions_gff, run4, shown):
    # Refusal one: the same regions, as a BED. BED is 0-based half-open and a
    # SAM is 1-based closed, so a read at position 10 and a region starting at
    # 10 would disagree by one base. Nothing runs.
    assert run4.steps
    _bed = regions_gff.with_suffix(".bed")
    _rows = []
    for _line in regions_gff.read_text().splitlines():
        if _line and not _line.startswith("#"):
            _f = _line.split("\t")
            _rows.append(f"{_f[0]}\t{int(_f[3]) - 1}\t{_f[4]}")
    _bed.write_text("\n".join(_rows) + "\n")
    _regions = lg.intervals(str(_bed), organism="demo")

    bed_problems = lg.Pipeline(merge(*[depth(a, _regions) for a in alns])).errors()
    print("refusal 1, coordinates:")
    print(shown(str(bed_problems[0])))
    return


@app.cell
def _(alns, depth, lg, merge, regions_gff, shown):
    # Refusal two: the regions as a GFF whose contigs are `chr1` and `chr2`,
    # against an alignment whose contigs are `1` and `2`.
    _prefixed = regions_gff.with_name("regions_chr.gff")
    _prefixed.write_text(
        regions_gff.read_text()
        .replace("\n1\t", "\nchr1\t")
        .replace("\n2\t", "\nchr2\t")
    )
    _regions = lg.intervals(str(_prefixed), organism="demo")

    chr_problems = lg.Pipeline(merge(*[depth(a, _regions) for a in alns])).errors()
    print("refusal 2, contig naming:")
    print(shown(str(chr_problems[0])))
    return


@app.cell
def _(mo):
    mo.md(
        """
        ## Two things the engine would not join

        Both pipelines above are the one you just watched run, with a single
        input changed, and neither ran. The first swapped the GFF for the same
        regions as a **BED**; the second renamed the GFF's contigs `chr1` and
        `chr2`.

        Coordinates and contig naming are semantics, not formatting. A BED
        region and a SAM read can name the same base and still disagree about
        it by one; `chr1` and `1` are the same chromosome to a person and two
        unrelated strings to a program. Joining them quietly gives a table
        that looks right and counts the wrong bases. That is why the regions
        here were a GFF, which counts as SAM does, and why `chr1` is not `1`
        until a step says it is.
        """
    )
    return


if __name__ == "__main__":
    app.run()
