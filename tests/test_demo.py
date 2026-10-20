"""M3 and M4 acceptance: the criteria, on the real HIV-1 pipeline.

`docs/milestones.md` signs these milestones off on the demo rather than on a
contrivance. For M3:

- the demo runs end to end on 3-4 runs from PRJNA517147,
- a second run completes entirely from cache,
- changing `min_freq` reruns only `call` and `resistance`,
- adding one sample processes only that sample.

For M4: rerunning alignment with a different thread count produces the same
canonical hash, and the downstream steps are skipped. That one needs the
real aligner rather than a stand-in, because what makes it hard is what
minimap2 actually writes into a BAM — and what it writes is its whole
command line, `-t 8` and output path and all.

These run real minimap2, samtools and bcftools over real reads, so they skip
rather than fail wherever the data has not been fetched or a tool is not
installed. The one machine where neither is true is the `demo` job in CI,
which installs the three tools, fetches PRJNA517147 and sets
`LIGASE_DEMO_REQUIRED` — under which a skip here fails the build, because a
green run has to mean the pipeline ran. Every property here is also tested
without the tools, in `crates/ligase-core/tests/execution.rs`,
`tests/test_run.py`, `crates/ligase-core/tests/canon.rs` and
`crates/ligase-core/tests/audit.rs`; what these add is the evidence that
they hold for an aligner and a variant caller rather than for `cat`.

The tests share one cache directory, so each pays only for the alignments
no earlier test has already made. Three thread counts appear between them,
and an alignment is its thread count's own — so expect a few minutes.
"""

from __future__ import annotations

import importlib.util
import shutil
from pathlib import Path
from types import ModuleType

import pytest

import ligase as lg
from tests.conftest import DATA, REPO_ROOT, unavailable

#: Three runs for the demo, and a fourth to add to it.
RUNS = ("SRR8525904", "SRR8525908", "SRR8525920")
ADDED = "SRR8525942"

#: What the demo shells out to.
TOOLS = ("minimap2", "samtools", "bcftools")

#: Where the demo pipeline lives. Loaded by path: its directory is named for
#: a human, not for an importer.
DEMO = REPO_ROOT / "examples" / "hiv-resistance" / "pipeline.py"


def _require_runnable() -> None:
    """Establish that the tools and the data are both here."""
    missing = [tool for tool in TOOLS if shutil.which(tool) is None]
    if missing:
        unavailable(f"not installed: {', '.join(missing)}")

    if not (DATA / "refs" / "HXB2.fasta").is_file():
        unavailable("data/refs/HXB2.fasta is missing; run scripts/fetch_test_data.sh")

    reads = DATA / "reads" / "PRJNA517147"
    for run in (*RUNS, ADDED):
        if not (reads / f"{run}_1.fastq.gz").is_file():
            unavailable(
                f"{run} is missing; run "
                "scripts/fetch_test_data.sh --project PRJNA517147"
            )


@pytest.fixture(scope="module")
def demo() -> ModuleType:
    """The demo pipeline module, loaded from its path."""
    _require_runnable()
    specification = importlib.util.spec_from_file_location("ligase_demo", DEMO)
    assert specification is not None
    assert specification.loader is not None
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


@pytest.fixture(scope="module")
def shared(tmp_path_factory: pytest.TempPathFactory) -> Path:
    """One cache for the tests that need a run to already have happened.

    Each of them runs the baseline first, which is what makes them
    independent of the order they happen to run in: a cache only ever gains
    entries.
    """
    return tmp_path_factory.mktemp("demo-cache")


def outcomes(report: lg.RunReport) -> dict[tuple[str, str | None], str]:
    """What happened to every step, by step name and sample."""
    return {(step.step, step.sample): step.outcome for step in report.steps}


def by_step(report: lg.RunReport, name: str) -> list[str]:
    """The outcomes of one step across every sample, in graph order."""
    return [step.outcome for step in report.steps if step.step == name]


# ------------------------------------------------- the demo runs at all --


def test_the_demo_runs_end_to_end_on_three_runs(
    demo: ModuleType, tmp_path: Path
) -> None:
    # A cache of its own, so this is a cold run whatever else has happened.
    report = demo.pipeline(runs=RUNS).run(cache=tmp_path)

    assert len(report.steps) == 9, report.text
    assert set(by_step(report, "align")) == {"ran"}
    assert report.summary == "9 steps: 9 ran."

    # Every sample came out with an interpreted report: one row per drug,
    # naming the gene it was scored against and the algorithm that scored
    # it. What those rows should *say* is checked against published
    # results in `tests/test_validation.py`; what matters here is that the
    # pipeline produced them at all.
    for output in report.outputs("resistance"):
        rows = Path(output).read_text().splitlines()
        assert rows[0].split("\t")[:4] == ["gene", "drug_class", "drug", "drug_name"]
        assert len(rows) == 28, f"27 drugs and a header, not {len(rows)}: {output}"
        genes = {row.split("\t")[0] for row in rows[1:]}
        assert genes == {"PR", "RT", "IN", "CA"}, f"unexpected genes: {genes}"
        assert all("HIVDB" in row.split("\t")[9] for row in rows[1:]), output


def test_every_alignment_is_against_hxb2(demo: ModuleType, shared: Path) -> None:
    # The dimensions the graph propagated are a claim about files that now
    # exist, so it is worth checking the claim against them.
    report = demo.pipeline(runs=RUNS).run(cache=shared)

    for output in report.outputs("align"):
        aligned = lg.alignment(output, organism="HIV-1")
        contigs = aligned.contigs or []
        assert [contig.name for contig in contigs] == ["K03455.1"]


# ------------------------------------------------------ second run free --


def test_a_second_run_comes_entirely_from_cache(demo: ModuleType, shared: Path) -> None:
    demo.pipeline(runs=RUNS).run(cache=shared)

    again = demo.pipeline(runs=RUNS).run(cache=shared)

    assert set(outcomes(again).values()) == {"cached"}
    assert again.summary == "9 steps: 9 from cache."
    # And it did not reread a single input to find that out.
    assert again.hashed == 0


# ------------------------------------------- a parameter moves downstream --


def test_changing_min_freq_reruns_only_call_and_resistance(
    demo: ModuleType, shared: Path
) -> None:
    demo.pipeline(runs=RUNS).run(cache=shared)

    changed = demo.pipeline(runs=RUNS, min_freq=0.95).run(cache=shared)

    assert set(by_step(changed, "align")) == {"cached"}
    assert set(by_step(changed, "call")) == {"ran"}
    # `resistance` is reconsidered rather than served: it ran wherever the
    # stricter threshold changed the calls, and was cut off wherever it did
    # not, which is the cache working rather than failing to.
    assert set(by_step(changed, "resistance")) <= {"ran", "skipped"}
    assert "ran" in by_step(changed, "resistance")


# ---------------------------------------------------- one more sample --


def test_adding_one_sample_processes_only_that_sample(
    demo: ModuleType, shared: Path
) -> None:
    demo.pipeline(runs=RUNS).run(cache=shared)

    bigger = demo.pipeline(runs=(*RUNS, ADDED)).run(cache=shared)

    assert len(bigger.steps) == 12, bigger.text
    ran = {step.sample for step in bigger.ran}
    assert ran == {ADDED}, bigger.text
    assert len(bigger.cached) == 9


# -------------------------------------------- M4: a different -t, same answer --


def test_a_different_thread_count_is_the_same_alignment(
    demo: ModuleType, shared: Path
) -> None:
    """The M4 acceptance criterion, on the real aligner.

    `-t 8` makes minimap2 write a different `@PG CL` line — and that line
    holds the output path, which is the staging file named after the step's
    own key. So the BAM's bytes move twice over, while the alignments in it
    do not move at all.
    """
    baseline = demo.pipeline(runs=RUNS, threads=1).run(cache=shared)
    before = {step.sample: step for step in baseline.steps if step.step == "align"}

    wider = demo.pipeline(runs=RUNS, threads=8).run(cache=shared)
    after = {step.sample: step for step in wider.steps if step.step == "align"}

    # `align` reran: a thread count is a parameter, and the engine cannot
    # know in advance that it will not change the answer.
    assert set(by_step(wider, "align")) == {"ran"}, wider.text

    for sample, aligned in after.items():
        # The bytes moved...
        assert aligned.hash != before[sample].hash, sample
        # ...and the alignments did not. This is the criterion.
        assert aligned.canonical == before[sample].canonical, sample

    # So nothing below it had anything to redo.
    assert set(by_step(wider, "call")) == {"skipped"}, wider.text
    assert set(by_step(wider, "resistance")) == {"skipped"}, wider.text
    assert wider.summary == "9 steps: 3 ran, 6 skipped."


# ------------------------------------------------------- M4: audit mode --


def test_auditing_the_aligner_finds_it_reproducible(
    demo: ModuleType, shared: Path
) -> None:
    """minimap2 is reproducible without being deterministic.

    It writes a different `@PG CL` every time — the output path changes
    between the two audit runs — and the same alignments. An audit that
    tested bytes alone would call the reference pipeline irreproducible.
    """
    report = demo.pipeline(runs=RUNS[:1]).audit("align", cache=shared)

    [audited] = report.steps
    assert audited.verdict == "reproducible", report.text
    assert audited.first != audited.second
    assert audited.first_canonical == audited.second_canonical
    assert report.reproducible


def test_auditing_the_caller_finds_it_reproducible(
    demo: ModuleType, shared: Path
) -> None:
    """The caller stamps the time of day into its header, and nothing else."""
    report = demo.pipeline(runs=RUNS[:1]).audit("call", cache=shared)

    [audited] = report.steps
    assert audited.verdict == "reproducible", report.text
    assert audited.first_canonical == audited.second_canonical


def test_auditing_the_report_finds_it_deterministic(
    demo: ModuleType, shared: Path
) -> None:
    """Our own writer has no volatile fields, so its bytes repeat exactly."""
    report = demo.pipeline(runs=RUNS[:1]).audit("resistance", cache=shared)

    [audited] = report.steps
    assert audited.verdict == "deterministic", report.text
    assert audited.first == audited.second


# -------------------------------------------- M5: how it was all produced --


def test_the_methods_paragraph_names_what_the_milestone_asks_for(
    demo: ModuleType, shared: Path
) -> None:
    """The four things `docs/milestones.md` signs M5 off on.

    Tool versions, parameters, reference accession and citations — on the
    real pipeline, with the real tools, so the versions are the ones that
    actually ran and the accession is the one HXB2's own defline carries.
    """
    pipe = demo.pipeline(runs=RUNS)
    pipe.run(cache=shared)

    methods = pipe.methods(cache=shared)
    text = methods.text

    # Tool versions, as the tools themselves reported them -- and, since
    # M6, the resistance algorithm, which is not a program to run but is
    # software whose version changes the answer.
    tools = dict(methods.tools)
    assert set(tools) == {"minimap2", "samtools", "bcftools", "hivdb"}
    for tool, version in tools.items():
        assert version.strip(), f"{tool} reported no version"
        assert version.split()[0] in text or tool in text

    # The algorithm by version, date and the digest of its rules. A
    # methods section that named "HIVdb" alone would not say which
    # HIVdb, and upstream republishes corrections under near-identical
    # version numbers.
    assert "9.8 2025-01-05" in tools["hivdb"]
    assert "blake3:" in tools["hivdb"]
    # It names what is shipped rather than what ran, because the sentence
    # renders as "ran with ...": claiming 10.2 for a step that named 9.8
    # would be a false statement in a methods section.
    assert tools["hivdb"].startswith("HIVdb rules as shipped:")

    # Parameters, as they were hashed.
    assert "min_freq = 0.01" in text
    assert "threads = " in text
    assert "algorithm = 'HIVDB_9.8'" in text
    assert "mixture = 0.15" in text

    # The reference accession, read from the FASTA rather than declared,
    # with the checksum the loader computed from the sequence.
    [reference] = methods.references
    assert reference.accession == "K03455.1"
    assert reference.length == 9719
    assert reference.checksum is not None
    assert "K03455.1" in text

    # Citations, for every tool that has one.
    assert any("Minimap2" in citation for citation in methods.citations)
    assert any("SAMtools and BCFtools" in citation for citation in methods.citations)
    # The interpretation algorithm is software too, and Stanford asks for
    # this reference by name. D24 forbids both guessing one and quietly
    # leaving one out.
    assert any("Rhee SY" in citation for citation in methods.citations)
    assert methods.uncited == [], methods.citations

    # And nothing unaccounted for.
    assert methods.accounted_for, text
    assert methods.summary == "9 steps, all accounted for."


def test_a_fully_cached_run_still_says_how_it_was_produced(
    demo: ModuleType, shared: Path
) -> None:
    """The repair D22 asked provenance to make.

    M4 drops the `@PG` line before hashing, so a cache hit can no longer
    prove which command produced an object. Here every one of the nine steps
    comes from the cache — nothing runs — and the methods section still
    names the command, the versions and the reference for all of them.
    """
    pipe = demo.pipeline(runs=RUNS)
    pipe.run(cache=shared)

    report = pipe.run(cache=shared)
    assert set(by_step(report, "align")) == {"cached"}, report.text

    methods = pipe.methods(cache=shared)

    assert methods.accounted_for, methods.text
    assert "minimap2" in methods.text
    assert "bcftools" in methods.text
    assert "K03455.1" in methods.text


def test_a_bundle_of_the_demo_reproduces_it_on_a_clean_cache(
    demo: ModuleType, shared: Path, tmp_path: Path
) -> None:
    """The second half of the M5 criterion, as concretely as it can be put.

    A bundle cannot carry the reads — they are gigabytes, and fetching them
    is out of scope (D8) — so it names them by content hash and names HXB2
    by accession and checksum. What it claims is that a run elsewhere lands
    on the same canonical hashes, and "elsewhere" for this engine is an
    empty cache: nothing computed, nothing remembered, every tool probed
    again.

    So the bundle is written from one cache and checked against another,
    which recomputes all nine steps with the real aligner and caller.
    """
    pipe = demo.pipeline(runs=RUNS)
    pipe.run(cache=shared)
    into = tmp_path / "bundle"

    written = pipe.methods(bundle=into, include=[DEMO], cache=shared)
    assert written.bundle is not None
    assert written.bundle.steps == 9
    assert written.bundle.gaps == 0

    # The claim is in results.tsv, one row per step, and the data it was
    # built from is named rather than carried.
    results = (into / "results.tsv").read_text().splitlines()
    assert len(results) == 10, results
    references = (into / "references.tsv").read_text()
    assert "K03455.1" in references
    inputs = (into / "inputs.tsv").read_text()
    assert "SRR8525904_1.fastq.gz" in inputs
    # And the pipeline that produced it travels with it.
    assert (into / "pipeline" / "pipeline.py").is_file()

    # Now the check, against a cache that knows nothing.
    checked = pipe.methods(verify=into, cache=tmp_path / "clean").verification
    assert checked is not None
    assert checked.reproduced, checked.text
    assert len(checked.results) == 9
    # Every input is the one the bundle was built from, so a differing
    # result could not have been blamed on the data.
    assert checked.moved == [], checked.text


def test_a_bundle_notices_a_result_that_moved(
    demo: ModuleType, shared: Path, tmp_path: Path
) -> None:
    """A bundle that could not fail this way would not be worth writing.

    `min_freq` is raised far enough to change the variant calls, so `call`
    and `resistance` land somewhere else while the alignments do not.
    """
    pipe = demo.pipeline(runs=RUNS[:1])
    pipe.run(cache=shared)
    into = tmp_path / "bundle"
    pipe.methods(bundle=into, cache=shared)

    stricter = demo.pipeline(runs=RUNS[:1], min_freq=0.9)
    checked = stricter.methods(verify=into, cache=shared).verification

    assert checked is not None
    assert not checked.reproduced, checked.text
    moved = {check.step for check in checked.differing}
    assert "call" in moved, checked.text
    # The alignments are untouched by a calling threshold, and the bundle
    # says so rather than reporting the whole pipeline as changed.
    assert "align" not in moved, checked.text
