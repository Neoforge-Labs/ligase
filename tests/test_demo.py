"""M3 acceptance: the four criteria, on the real HIV-1 pipeline.

`docs/milestones.md` signs M3 off on the demo rather than on a contrivance:

- the demo runs end to end on 3-4 runs from PRJNA517147,
- a second run completes entirely from cache,
- changing `min_freq` reruns only `call` and `resistance`,
- adding one sample processes only that sample.

These run real minimap2, samtools and bcftools over real reads, so they skip
rather than fail wherever the data has not been fetched or a tool is not
installed — which is every CI machine, since `data/` is gitignored. The same
four properties are tested without the tools in
`crates/ligase-core/tests/execution.rs` and `tests/test_run.py`; what these
add is the evidence that they hold for an aligner and a variant caller
rather than for `cat`.

The first run of whichever test gets there first pays for the alignments
(about fifteen seconds); the rest share one cache directory and are free.
"""

from __future__ import annotations

import importlib.util
import shutil
from pathlib import Path
from types import ModuleType

import pytest

import ligase as lg
from tests.conftest import DATA, REPO_ROOT

#: Three runs for the demo, and a fourth to add to it.
RUNS = ("SRR8525904", "SRR8525908", "SRR8525920")
ADDED = "SRR8525942"

#: What the demo shells out to.
TOOLS = ("minimap2", "samtools", "bcftools")

#: Where the demo pipeline lives. Loaded by path: its directory is named for
#: a human, not for an importer.
DEMO = REPO_ROOT / "examples" / "hiv-resistance" / "pipeline.py"


def _skip_unless_runnable() -> None:
    missing = [tool for tool in TOOLS if shutil.which(tool) is None]
    if missing:
        pytest.skip(f"not installed: {', '.join(missing)}")

    if not (DATA / "refs" / "HXB2.fasta").is_file():
        pytest.skip("run scripts/fetch_test_data.sh to fetch data/refs/HXB2.fasta")

    reads = DATA / "reads" / "PRJNA517147"
    for run in (*RUNS, ADDED):
        if not (reads / f"{run}_1.fastq.gz").is_file():
            pytest.skip(
                f"run scripts/fetch_test_data.sh --project PRJNA517147 to fetch {run}"
            )


@pytest.fixture(scope="module")
def demo() -> ModuleType:
    """The demo pipeline module, loaded from its path."""
    _skip_unless_runnable()
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

    # Every sample came out with a report, and the variants in it were
    # placed in the genes resistance is named by.
    for output in report.outputs("resistance"):
        rows = Path(output).read_text().splitlines()
        assert rows[0] == "contig\tposition\tref\talt\tgene\tcodon"
        assert len(rows) > 1, f"no variants at all in {output}"
        genes = {row.split("\t")[4] for row in rows[1:]}
        assert genes & {"PR", "RT", "IN"}, f"nothing placed in pol: {genes}"


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
