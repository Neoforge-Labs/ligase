"""M13 acceptance: what `ligase.steps` claims, and whether it is true.

Two halves, and they are gated differently on purpose.

The first half is **the admission criteria of D42**, checked by iterating
`ligase.steps.LIBRARY`. It needs no tool installed and no data fetched, so
it runs everywhere a contributor runs the suite. That is the whole point of
it: an admission criterion only holds people if it fails on the machine
they are working on, and a criterion checked on one machine in the world is
a comment.

The second half **runs each step on the fixtures** and reads back what came
out. That needs the tools, so it skips where they are missing — except
where `LIGASE_DEMO_REQUIRED` says the machine was prepared, which is CI's
`demo` job, and there a skip is a failure. `.github/demo-tools.yml`
installs all four tools the library names for exactly that reason: a
fixture test that skips everywhere would be worth nothing.

The fixtures are not the tiny ones the rest of the suite uses. Those are
20 bp contigs and 10 bp reads, below the seed length of every aligner in
the field, and no amount of care makes minimap2 find a minimizer in them.
`crates/ligase-core/tests/fixtures/generate.py` writes a 2 kb reference and
100 bp read pairs over it with one substitution planted at position 1001,
which is the smallest thing these four tools can honestly be run on.
"""

from __future__ import annotations

import shutil
from pathlib import Path

import pytest

import ligase as lg
import ligase.types
from ligase import _core
from ligase.steps import LIBRARY, LibraryStep
from tests.conftest import REPO_ROOT, fixture, unavailable

#: The planted substitution: the contig, where it is, and what the reads
#: carry there. `generate.py` holds the same three values; a test that read
#: them out of the output would be asserting the output against itself.
CONTIG = "steps1"
VARIANT_AT = 1001
VARIANT_TO = "G"

#: The organism these fixtures declare. Synthetic, and labelled as such:
#: nothing here is a real sequence and a test should not imply one.
ORGANISM = "synthetic"

#: The sample the alignment steps are told to write.
SAMPLE = "steps_sample"


def recorded(sample: str) -> str:
    """What the engine currently writes into a file for a string parameter.

    Which is `repr(sample)`, quotes and all, and that is a defect rather
    than a convention. `python/ligase/graph.py` serialises every plain
    parameter with `repr()` before handing it to the engine -- right for a
    cache key, where it is what keeps `f(1)` and `f("1")` apart, and wrong
    for shell substitution, where only the value was wanted. So a BAM this
    library writes carries `@RG SM:'steps_sample'` and the VCF below it
    names its sample column the same way.

    The tests below assert against this rather than against the truth,
    because asserting the truth here would make four tests fail for one
    reason somewhere else. The truth is asserted once, as a strict xfail,
    in `test_a_string_parameter_should_reach_the_shell_as_its_value` -- so
    this helper disappears the day that passes, and until then nothing can
    quietly come to depend on the quotes.

    `graph.py` is M11's while that milestone is in flight, and the real fix
    is not a one-liner there: a parameter needs to travel as both the form
    that goes in the key and the form that goes in the command, which is a
    change to the task shape across the Python layer, the bindings and
    `exec.rs`.
    """
    return repr(sample)


#: Where a step's guide page has to be, for the clause that says it has one.
GUIDE = REPO_ROOT / "web" / "content" / "guide" / "steps"


# ------------------------------------------------ the admission criteria --


def ids() -> list[str]:
    """Every library step's name, for a readable test identifier."""
    return [entry.name for entry in LIBRARY]


@pytest.fixture(params=LIBRARY, ids=ids())
def entry(request: pytest.FixtureRequest) -> LibraryStep:
    """One library step, with what it claims about itself."""
    assert isinstance(request.param, LibraryStep)
    return request.param


def test_a_library_step_names_the_tools_it_runs(entry: LibraryStep) -> None:
    """`tool=` is not optional here, because the cache key depends on it.

    A step that shells out and names no tool is served from the cache
    across a tool upgrade that changed its answer. For a step somebody
    wrote themselves that is their business; for one this project ships it
    is this project's.
    """
    assert entry.step.tools, (
        f"{entry.name} runs a program and names none, so nothing puts that "
        "program's version into its cache key"
    )


def test_a_library_steps_citation_resolves(entry: LibraryStep) -> None:
    """Every tool a library step names has a curated reference.

    Not a `cite=` string on the step. `cite=` is how a user's own step says
    what it wants cited; a reference for a tool the project ships belongs
    in the engine's table, where it is one promise kept in one place (D24).

    This is also the test that made the table grow: `bwa` was not in it,
    and `ligase.steps.bwa.mem` could not have been admitted until it was.
    """
    for tool in entry.step.tools:
        assert _core.citation(tool) is not None, (
            f"{entry.name} names `{tool}`, which resolves to no reference. "
            "A library step whose tool cannot be cited would put a gap in "
            "the methods section of every pipeline that used it; add a "
            "checked entry to crates/ligase-core/src/prov/cite.rs"
        )


def test_a_library_step_declares_a_kind_the_engine_has(entry: LibraryStep) -> None:
    """The output kind is read off the return annotation, so it is real."""
    assert entry.step.output in lg.types.KINDS, entry.step.output


def test_a_library_step_is_documented_in_prose(entry: LibraryStep) -> None:
    """Both the step and the module it lives in say what they are for.

    The API reference is generated from these, and `generate_api.py` fails
    the build on an exported name without one. This fails earlier and says
    which step.
    """
    doc = entry.step.function.__doc__ or ""
    assert len(doc.split()) > 40, (
        f"{entry.name} has no real docstring. The reference is generated "
        "from it, so this is published prose rather than a comment"
    )


def test_every_library_step_has_a_guide_page(entry: LibraryStep) -> None:
    """The clause says every library step has a page under `guide/steps/`.

    Asserted rather than trusted, because a documentation clause nothing
    checks is a clause that decays on the first step somebody adds in a
    hurry.
    """
    slug = entry.page.strip("/").rsplit("/", 1)[-1]
    page = GUIDE / f"{slug}.mdx"
    assert page.is_file(), f"{entry.name} claims {entry.page}, which is not {page}"

    meta = (GUIDE / "_meta.ts").read_text()
    assert f"'{slug}'" in meta or f"{slug}:" in meta, (
        f"{page.name} exists but is in no _meta.ts entry, so Nextra will not show it"
    )


def test_the_two_aligners_are_not_the_same_step() -> None:
    """One output kind, two tools, and nothing shared but the kind.

    They are interchangeable in a pipeline and must not be
    interchangeable in provenance. This is the declaration half; the cache
    half is `test_swapping_the_aligner_changes_the_cache_key` below, which
    needs both of them installed.
    """
    aligners = [entry for entry in LIBRARY if entry.step.output == "Alignment"]
    kinds = {entry.step.output for entry in aligners}
    assert kinds == {"Alignment"}

    first = {entry.step.tools[0] for entry in aligners}
    assert first == {"minimap2", "bwa", "samtools"}, first


def test_the_library_ships_no_step_that_cannot_say_what_it_produces() -> None:
    """D42's clause, as the two things it has kept out.

    `samtools index` writes an index and not an alignment, and `fastp`'s
    paired-end trimming cannot be a step at all because a step's output is
    one file and paired reads are two. Both are argued in
    `web/content/project/architecture/step-library.mdx`.

    This test is here so that adding either of them back is a deliberate
    act with a test to change, rather than a plausible-looking commit.
    """
    names = {entry.name for entry in LIBRARY}
    assert "samtools.index" not in names, (
        "an index is not an alignment: declared `Alignment -> Alignment`, "
        "its output would be hashed as an alignment's meaning and accepted "
        "as one downstream"
    )
    assert not any(entry.name.startswith("fastp.") for entry in LIBRARY), (
        "a step's output is one file, so no step can produce paired reads; "
        "see the architecture page before adding one"
    )


def test_a_file_that_is_not_its_kind_is_hashed_as_bytes() -> None:
    """Why the canonical form has to be checked at all.

    The engine does not fail on a file that will not parse as the kind it
    was read as: it hashes the bytes instead and tags them as such, which
    is the conservative direction and the right one. The cost is that it is
    *silent*, so a step whose output is not what the step says it is loses
    early cutoff for everything below it and nothing says why.

    A reference read as an alignment stands in here for the BAM index that
    `samtools.index` would have produced.
    """
    reference = fixture("steps_ref.fasta")
    assert _core.canonical_form("Reference", reference) == "canonical"
    assert _core.canonical_form("Alignment", reference) == "raw"


# -------------------------------------------------- running them for real --

#: What the library shells out to, across all four steps.
TOOLS = ("minimap2", "bwa", "samtools", "bcftools")


def require(*tools: str) -> None:
    """Establish that these tools are installed, or skip."""
    missing = [tool for tool in tools if shutil.which(tool) is None]
    if missing:
        unavailable(f"not installed: {', '.join(missing)}")


@pytest.fixture
def reference() -> lg.types.Reference:
    """The step library's own reference, with its organism declared."""
    return lg.reference(fixture("steps_ref.fasta"), organism=ORGANISM, build="steps")


@pytest.fixture
def reads() -> lg.types.Reads:
    """The read pairs that tile it, with the substitution planted."""
    return lg.reads(
        fixture("steps_reads_1.fastq"),
        fixture("steps_reads_2.fastq"),
        organism=ORGANISM,
        sample=SAMPLE,
    )


def only(report: lg.RunReport, step: str) -> lg.StepRun:
    """The one run of a step, insisting that there was exactly one."""
    runs = [run for run in report.steps if run.step == step]
    assert len(runs) == 1, f"{len(runs)} runs of {step}: {report.text}"
    return runs[0]


def test_minimap2_align_produces_an_alignment_that_says_what_it_is(
    reads: lg.types.Reads, reference: lg.types.Reference, tmp_path: Path
) -> None:
    """The output parses as an alignment, on the reference, of the sample.

    Three claims the step makes and one it inherits. It says it produces an
    `Alignment`; it says the alignment carries a sample, which is the whole
    reason `sample_id` is a parameter; and the reference it was aligned to
    is read back out of the BAM's own `@SQ` lines rather than taken from
    the step's word for it.
    """
    require("minimap2", "samtools")
    from ligase.steps import minimap2

    aligned = minimap2.align(reads, reference, sample_id=SAMPLE, threads=2)
    report = lg.Pipeline([aligned]).run(cache=tmp_path / "cache")
    run = only(report, "align")
    assert run.outcome == "ran", report.text

    loaded = lg.alignment(run.output, organism=ORGANISM)
    assert loaded.samples == [recorded(SAMPLE)], (
        "the read group is why `sample_id` exists: without it bcftools names "
        f"the VCF's sample column after a cache path, not {loaded.samples}"
    )
    assert [contig.name for contig in loaded.contigs or []] == [CONTIG]

    # And it parses, so its hash is over what it means. A step whose output
    # did not would silently lose early cutoff for everything below it.
    assert _core.canonical_form("Alignment", run.output) == "canonical"


def test_bwa_mem_produces_the_same_kind_from_the_same_inputs(
    reads: lg.types.Reads, reference: lg.types.Reference, tmp_path: Path
) -> None:
    """The other aligner, and it indexes the reference itself.

    That last part is the thing worth testing rather than assuming: `bwa
    mem` cannot read a bare FASTA, and in a Ligase run the FASTA is an
    object in the content-addressed store, where a step must not write. So
    the step indexes into a temporary directory -- and if it ever stopped,
    this test would fail with bwa's own complaint rather than quietly
    writing five files next to a store object.
    """
    require("bwa", "samtools")
    from ligase.steps import bwa

    aligned = bwa.mem(reads, reference, sample_id=SAMPLE, threads=2)
    report = lg.Pipeline([aligned]).run(cache=tmp_path / "cache")
    run = only(report, "mem")
    assert run.outcome == "ran", report.text

    loaded = lg.alignment(run.output, organism=ORGANISM)
    assert loaded.samples == [recorded(SAMPLE)]
    assert [contig.name for contig in loaded.contigs or []] == [CONTIG]
    assert _core.canonical_form("Alignment", run.output) == "canonical"

    # Nothing was left beside the reference.
    beside = Path(reference.file).parent
    assert not list(beside.glob("steps_ref.fasta.*")), (
        "bwa's index files were written next to the reference; in a real run "
        "that directory is the content-addressed store"
    )


def test_swapping_the_aligner_changes_the_cache_key(
    reads: lg.types.Reads, reference: lg.types.Reference, tmp_path: Path
) -> None:
    """Two aligners, one kind, and never one cache key.

    They are deliberately interchangeable in a pipeline: both take reads
    and a reference and hand back an `Alignment`, so one can be swapped for
    the other without touching anything downstream. What must *not* be
    interchangeable is the result's identity. A key that did not move would
    serve minimap2's alignments for a pipeline that now says it used bwa,
    and the methods paragraph would say bwa over numbers minimap2 produced.

    Nothing arranges this. It falls out of `tool=` naming different
    programs, which is what the assertion is really checking -- so if
    somebody ever "simplifies" the two steps onto a shared body with a
    tool argument, this fails.
    """
    require("minimap2", "bwa", "samtools")
    from ligase.steps import bwa, minimap2

    cache = tmp_path / "cache"
    with_minimap2 = lg.Pipeline(
        [minimap2.align(reads, reference, sample_id=SAMPLE, threads=2)]
    ).run(cache=cache)
    with_bwa = lg.Pipeline(
        [bwa.mem(reads, reference, sample_id=SAMPLE, threads=2)]
    ).run(cache=cache)

    first = only(with_minimap2, "align")
    second = only(with_bwa, "mem")
    assert second.outcome == "ran", (
        "the second aligner was served from the first one's cache entry: "
        f"{with_bwa.text}"
    )
    assert first.key != second.key, (
        "two aligners share a cache key, so an alignment made by one would "
        "be served for a pipeline declaring the other"
    )


def test_sorting_changes_the_bytes_and_not_the_meaning(tmp_path: Path) -> None:
    """`samtools.sort` reorders records, which is not what a BAM is about.

    An alignment's canonical form sorts its records before hashing, so a
    sorted alignment and its unsorted original mean the same thing -- and
    the test for that is not an assertion about two hashes, it is the
    behaviour a user sees: insert a sort into a pipeline and the step below
    it is served from the cache.

    Here `downstream` runs once over the unsorted alignment, and then again
    over the same alignment *sorted*. The second time it does not run. Its
    key is built from its input's canonical hash, the sort did not change
    that hash, and so there was nothing below the sort left to do.

    `downstream` is a Python-bodied step on purpose: it needs no external
    tool, so what this test proves is about the engine rather than about
    whatever `cp` does on this operating system.
    """
    require("samtools")
    from ligase.steps import samtools

    @lg.step
    def downstream(aln: ligase.types.Alignment) -> ligase.types.Alignment:
        """Copy an alignment, so there is something below the sort."""

        def copy(out: Path) -> None:
            """Copy it once the sort above has produced."""
            shutil.copyfile(aln.file, out)

        return lg.write(copy)

    cache = tmp_path / "cache"
    unsorted = lg.alignment(fixture("steps_unsorted.sam"), organism=ORGANISM)

    direct = lg.Pipeline([downstream(unsorted)]).run(cache=cache)
    assert only(direct, "downstream").outcome == "ran", direct.text

    sorted_aln = samtools.sort(unsorted, threads=2)
    after = lg.Pipeline([downstream(sorted_aln)]).run(cache=cache)

    sort_run = only(after, "sort")
    assert sort_run.outcome == "ran", after.text
    # `skipped` rather than `cached`, and the distinction is the point:
    # cached means nothing above it moved, skipped means something above it
    # *did* and produced the same output anyway. That is early cutoff, and
    # it is the whole reason a sort is cheap to insert.
    assert only(after, "downstream").outcome == "skipped", (
        "sorting changed what the step below it was keyed on, so sorting "
        f"changed what the alignment means: {after.text}"
    )

    # And what the sort wrote is a BAM, not a reordered something else.
    produced = lg.alignment(sort_run.output, organism=ORGANISM)
    # Written by the fixture generator rather than by a step, so this one
    # is the sample identifier and not `recorded()` of it.
    assert produced.samples == [SAMPLE]
    assert _core.canonical_form("Alignment", sort_run.output) == "canonical"
    assert Path(sort_run.output).read_bytes() != Path(unsorted.file).read_bytes()


def test_mpileup_call_reports_the_planted_allele(
    reads: lg.types.Reads, reference: lg.types.Reference, tmp_path: Path
) -> None:
    """The library composes, and the calls are the fixture's own variant.

    Two steps in one pipeline, which is the thing the library is for. What
    comes out has to be the substitution `generate.py` planted and nothing
    else: the reads differ from the reference at exactly one position, so a
    caller reporting two variants is reporting one it invented.
    """
    require("minimap2", "samtools", "bcftools")
    from ligase.steps import bcftools, minimap2

    aligned = minimap2.align(reads, reference, sample_id=SAMPLE, threads=2)
    called = bcftools.mpileup_call(aligned, reference, min_freq=0.1)
    report = lg.Pipeline([called]).run(cache=tmp_path / "cache")
    run = only(report, "mpileup_call")
    assert run.outcome == "ran", report.text

    loaded = lg.variants(run.output, organism=ORGANISM)
    assert loaded.samples == [recorded(SAMPLE)]
    assert _core.canonical_form("Variants", run.output) == "canonical"

    positions = _called_positions(run.output)
    assert positions == [(CONTIG, VARIANT_AT)], (
        f"the reads differ from the reference at {VARIANT_AT} and nowhere "
        f"else, so these are not all real: {positions}"
    )


def _called_positions(path: str) -> list[tuple[str, int]]:
    """Every record's contig and position, read out of a gzipped VCF."""
    import gzip

    with gzip.open(path, "rt") as handle:
        return [
            (fields[0], int(fields[1]))
            for fields in (line.split("\t") for line in handle if line[:1] != "#")
        ]


def test_every_library_tool_reports_a_version_that_is_a_version(
    entry: LibraryStep,
) -> None:
    """D42's version-capture clause, against the installed tools.

    The engine records the whole of a tool's version output and hashes it
    into the key, so what it must never record is the tool complaining
    about the question. `bwa --version` does exactly that -- it prints
    `[main] unrecognized command '--version'` and exits non-zero -- and the
    engine used to take that sentence as bwa's version. It is the same
    sentence for every bwa ever released, so upgrading the aligner moved no
    cache key at all.

    Checked here against the real tools rather than only in
    `exec.rs`'s unit tests, because the probe table is a claim about what
    these programs do and only these programs can settle it.
    """
    require(*entry.step.tools)

    for tool in entry.step.tools:
        version = _core.tool_version(tool)
        assert version, tool
        assert any(character.isdigit() for character in version), (
            f"`{tool}` reported `{version}` as its version, which holds no "
            "number; a version that cannot change is a cache key that cannot "
            "move"
        )
        for complaint in ("unrecognized", "unknown option", "usage:"):
            assert complaint not in version.lower(), (
                f"`{tool}` reported `{version}`, which is a complaint rather "
                "than a version; it needs an entry in the version-probe "
                "table in crates/ligase-core/src/exec.rs"
            )


@pytest.mark.xfail(
    strict=True,
    reason=(
        "graph.py serialises a plain parameter with repr(), so a string "
        "parameter reaches the shell with Python's quotes around it"
    ),
)
def test_a_string_parameter_should_reach_the_shell_as_its_value(
    reads: lg.types.Reads, reference: lg.types.Reference, tmp_path: Path
) -> None:
    """A read group should name the sample, not `'the sample'`.

    This is the defect `recorded()` exists to work around, asserted once,
    as the thing that should be true. It fails today and the day it stops
    failing this test fails the other way, which is what `strict=True`
    buys: nobody can fix it and leave the workaround behind.

    ## What is wrong

    `python/ligase/graph.py` hands the engine every plain parameter as
    `repr(value)`. For a number that is the value; for a string it is the
    value in quotes. The same text does two jobs — it goes into the cache
    key, where `repr` is exactly right because it is what tells `f(1)`
    from `f("1")`, and it is substituted into the shell command, where the
    quotes are not wanted and are not escaped away because they arrived
    *inside* the value.

    So this library writes `@RG ID:'steps_sample' SM:'steps_sample'`, and
    `bcftools` names the VCF's sample column `'steps_sample'` after it.

    ## Why it matters here of all places

    Because a sample set is a dimension, and the canonical form of a BAM
    and of a VCF **includes** their sample names — deliberately, since a
    sample identifier is part of what the file is about and is the one
    field canonicalisation refuses to drop. Every alignment and every set
    of calls this library produces therefore has its semantic hash taken
    over a sample identifier that is not the sample's identifier. Nothing
    breaks, because the quoting is perfectly reproducible; it is simply
    not true, and it is written into the hash.

    Downstream of that: a methods paragraph and a bundle name the sample
    with quotes, and anyone joining these calls to sample metadata by name
    matches nothing — which is the shuffled-sample-identifier failure this
    engine exists to catch, arriving from inside it.

    The science is unaffected, which is why it went unseen: Ligase takes
    sample identity from the declared dimension rather than from the file,
    so the reference demo's resistance calls are right and are attributed
    to the right runs.

    ## The fix, and why it is not here

    A parameter has to travel as two things: the form that goes in the key
    and the form that goes in the command. That is a change to the task
    shape across `graph.py`, `crates/ligase-py/src/lib.rs` and
    `exec.rs::resolve`, and it retires every cache key of every step with a
    string parameter. `graph.py` is also M11's while that milestone is in
    flight. So it is recorded rather than patched, and `str()` is not the
    fix: it would make `f(1)` and `f("1")` share a key.
    """
    require("minimap2", "samtools")
    from ligase.steps import minimap2

    aligned = minimap2.align(reads, reference, sample_id=SAMPLE, threads=2)
    report = lg.Pipeline([aligned]).run(cache=tmp_path / "cache")
    loaded = lg.alignment(only(report, "align").output, organism=ORGANISM)

    assert loaded.samples == [SAMPLE]
